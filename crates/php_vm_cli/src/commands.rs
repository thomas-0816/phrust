//! php-vm command implementation.
use php_bytecode_cache::{
    CacheArtifact, CacheFingerprint, CacheFingerprintInput, CacheHeader, CachedIrArtifact,
    PHP_TARGET_VERSION,
};
use php_diagnostics::{
    DebugEvent, DiagnosticEnvelope, DiagnosticLayer, DiagnosticOutputFormat, DiagnosticPhase,
};
use php_executor::{
    EngineProfileName, PhpCompileInput, PhpExecutionError, PhpExecutionOutput, PhpExecutionStatus,
    PhpExecutor, PhpExecutorOptions, PhpRequestExecutionInput, usage_diagnostic,
    write_diagnostic_envelope,
};
use php_ir::{
    LoweringOptions, VerificationDiagnosticContext, lower::LoweringDiagnosticContext,
    lower_frontend_result, module::IrUnit, verify_unit,
};
use php_optimizer::{OptimizationLevel, OptimizationReport, PassContext, PassPipeline};
use php_runtime::api::{
    ExitStatus, FilesystemCapabilities, RuntimeContext, RuntimeDiagnostic, RuntimeDiagnosticPayload,
};
use php_semantics::{FrontendResult, Severity, analyze_source, diagnostics::DiagnosticId};
use php_source::{SourceText, TextRange};
use php_vm::{
    api::{
        BytecodeLayoutMode, ExecutionFormat, IncludeLoader, JitBlacklistMode, JitMode,
        SuperinstructionMode, Vm, VmOptions, VmResult,
    },
    experimental::{
        BytecodeLayoutProfile, DenseBytecodeUnit, DenseOpcode, DenseOperands, InlineCacheMode,
        JitCompileDescriptor, PersistentFeedbackContext, PersistentFeedbackEpochs,
        PersistentFeedbackLoadReport, PersistentFeedbackStats, PersistentFeedbackStore,
        QuickeningMode, RegionProfile, TieringOptions, TieringStats, VmCounters,
        plan_dependency_units,
    },
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const EXIT_SUCCESS: i32 = 0;
const EXIT_COMPILE_ERROR: i32 = 2;
const EXIT_RUNTIME_ERROR: i32 = 3;
const EXIT_UNSUPPORTED: i32 = 4;
const EXIT_USAGE: i32 = 5;

pub(crate) fn main_entry() {
    let code = run(env::args().skip(1), &mut io::stdout(), &mut io::stderr());
    if code != EXIT_SUCCESS {
        std::process::exit(code);
    }
}

fn run<I, W, E>(args: I, stdout: &mut W, stderr: &mut E) -> i32
where
    I: IntoIterator<Item = String>,
    W: Write,
    E: Write,
{
    match run_inner(args, stdout, stderr) {
        Ok(code) => code,
        Err(error) => {
            let format = error_format_from_env("PHRUST_ERROR_FORMAT");
            let diagnostic = cli_usage_diagnostic_from_message(&error);
            let _ = write_diagnostic_envelope(stderr, &diagnostic, format);
            EXIT_USAGE
        }
    }
}

fn run_inner<I, W, E>(args: I, stdout: &mut W, stderr: &mut E) -> Result<i32, String>
where
    I: IntoIterator<Item = String>,
    W: Write,
    E: Write,
{
    let args: Vec<String> = args.into_iter().collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_usage(stdout)?;
        return Ok(EXIT_SUCCESS);
    }
    let Some(command) = args.first().map(String::as_str) else {
        print_usage(stdout)?;
        return Ok(EXIT_SUCCESS);
    };

    match command {
        "compile" => compile_command(&args[1..], stdout, stderr),
        "dump-ir" => dump_ir_command(&args[1..], stdout, stderr),
        "dump-bytecode-patterns" => dump_bytecode_patterns_command(&args[1..], stdout, stderr),
        "dump-rule-selection" => dump_rule_selection_command(&args[1..], stdout, stderr),
        "dump-dependency-units" => dump_dependency_units_command(&args[1..], stdout, stderr),
        "dump-baseline-native-stencil" => {
            dump_baseline_native_stencil_command(&args[1..], stdout, stderr)
        }
        "dump-copy-patch-stencils" => dump_copy_patch_stencils_command(&args[1..], stdout, stderr),
        "dump-mid-tier-plan" => dump_mid_tier_plan_command(&args[1..], stdout, stderr),
        "dump-cranelift-clif" => dump_cranelift_clif_command(&args[1..], stdout, stderr),
        "run" => run_command(&args[1..], stdout, stderr),
        "report" => report_command(&args[1..], stdout, stderr),
        "compare" => {
            writeln!(
                stderr,
                "compare is reserved until runtime-diff fixtures are implemented"
            )
            .map_err(|error| error.to_string())?;
            Ok(EXIT_UNSUPPORTED)
        }
        _ => Err(format!("unknown php-vm command `{command}`")),
    }
}

#[cfg(feature = "jit-cranelift")]
fn dump_cranelift_clif_command<W, E>(
    args: &[String],
    stdout: &mut W,
    _stderr: &mut E,
) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    if !args.is_empty() {
        return Err("dump-cranelift-clif does not accept arguments".to_string());
    }
    let result = php_jit::build_trivial_add_clif_smoke().map_err(|error| error.to_string())?;
    let output_dir = workspace_relative_path("target/performance/cranelift");
    fs::create_dir_all(&output_dir).map_err(|error| {
        format!(
            "{}: failed to create CLIF output directory: {error}",
            output_dir.display()
        )
    })?;
    let output_path = output_dir.join("trivial_add.clif");
    fs::write(&output_path, &result.clif)
        .map_err(|error| format!("{}: {error}", output_path.display()))?;
    writeln!(
        stdout,
        "ok backend=cranelift-experiment function={} verified={} blocks={} instructions={} path={}",
        result.function_name,
        result.stats.verified,
        result.stats.blocks_lowered,
        result.stats.instructions_lowered,
        output_path.display()
    )
    .map_err(|error| error.to_string())?;
    Ok(EXIT_SUCCESS)
}

#[cfg(not(feature = "jit-cranelift"))]
fn dump_cranelift_clif_command<W, E>(
    args: &[String],
    _stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    if !args.is_empty() {
        return Err("dump-cranelift-clif does not accept arguments".to_string());
    }
    writeln!(
        stderr,
        "dump-cranelift-clif requires the jit-cranelift feature"
    )
    .map_err(|error| error.to_string())?;
    Ok(EXIT_UNSUPPORTED)
}

fn compile_command<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    let options = parse_compile_args(args)?;
    let pipeline = match compile_pipeline_with_optimization(options.path, options.opt_level) {
        Ok(pipeline) => pipeline,
        Err(error) => {
            writeln!(stderr, "{error}").map_err(|io| io.to_string())?;
            return Ok(EXIT_COMPILE_ERROR);
        }
    };
    if options.json {
        writeln!(stdout, "{}", pipeline.compile_json()).map_err(|error| error.to_string())?;
    } else if pipeline.ok() {
        writeln!(
            stdout,
            "ok path={} functions={} constants={}",
            pipeline.path,
            pipeline.lowering.unit.functions.len(),
            pipeline.lowering.unit.constants.len()
        )
        .map_err(|error| error.to_string())?;
    } else {
        write_frontend_diagnostics(stderr, &pipeline)?;
        return Ok(EXIT_COMPILE_ERROR);
    }
    Ok(if pipeline.ok() {
        EXIT_SUCCESS
    } else {
        EXIT_COMPILE_ERROR
    })
}

fn dump_ir_command<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    let options = parse_dump_ir_args(args)?;
    let path = options.path;
    let pipeline = match compile_pipeline(path) {
        Ok(pipeline) => pipeline,
        Err(error) => {
            writeln!(stderr, "{error}").map_err(|io| io.to_string())?;
            return Ok(EXIT_COMPILE_ERROR);
        }
    };
    if !pipeline.ok() {
        write_frontend_diagnostics(stderr, &pipeline)?;
        return Ok(EXIT_COMPILE_ERROR);
    }
    if options.with_source {
        writeln!(stdout, "source path={}", path).map_err(|error| error.to_string())?;
        for (index, line) in pipeline.source.lines().enumerate() {
            writeln!(stdout, "source {:04}: {}", index + 1, line)
                .map_err(|error| error.to_string())?;
        }
        writeln!(stdout, "--- ir ---").map_err(|error| error.to_string())?;
    }
    write!(stdout, "{}", pipeline.lowering.unit.to_snapshot_text())
        .map_err(|error| error.to_string())?;
    Ok(EXIT_SUCCESS)
}

fn dump_bytecode_patterns_command<W, E>(
    args: &[String],
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    let (path, json) = parse_dump_bytecode_patterns_args(args)?;
    let pipeline = match compile_pipeline(path) {
        Ok(pipeline) => pipeline,
        Err(error) => {
            writeln!(stderr, "{error}").map_err(|io| io.to_string())?;
            return Ok(EXIT_COMPILE_ERROR);
        }
    };
    if !pipeline.ok() {
        write_frontend_diagnostics(stderr, &pipeline)?;
        return Ok(EXIT_COMPILE_ERROR);
    }
    let dense = match DenseBytecodeUnit::lower_from_ir(&pipeline.lowering.unit) {
        Ok(dense) => dense,
        Err(error) => {
            writeln!(
                stderr,
                "E_PHP_VM_DENSE_BYTECODE_UNSUPPORTED: {}",
                error.message
            )
            .map_err(|io| io.to_string())?;
            return Ok(EXIT_UNSUPPORTED);
        }
    };
    if let Err(errors) = dense.verify() {
        writeln!(
            stderr,
            "E_PHP_VM_DENSE_BYTECODE_VERIFY: dense bytecode verifier rejected unit with {} error(s)",
            errors.len()
        )
        .map_err(|io| io.to_string())?;
        return Ok(EXIT_UNSUPPORTED);
    }
    let report = collect_bytecode_patterns(&dense);
    if json {
        writeln!(stdout, "{}", bytecode_patterns_json(path, &dense, &report))
            .map_err(|error| error.to_string())?;
    } else {
        writeln!(
            stdout,
            "ok path={} functions={} blocks={} instructions={}",
            path,
            dense.functions.len(),
            report.blocks,
            report.instructions
        )
        .map_err(|error| error.to_string())?;
        for (pair, count) in &report.pairs {
            writeln!(stdout, "pair {count:>6} {pair}").map_err(|error| error.to_string())?;
        }
        for (triple, count) in &report.triples {
            writeln!(stdout, "triple {count:>4} {triple}").map_err(|error| error.to_string())?;
        }
    }
    Ok(EXIT_SUCCESS)
}

fn dump_rule_selection_command<W, E>(
    args: &[String],
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    let (path, json) = parse_dump_rule_selection_args(args)?;
    let pipeline = match compile_pipeline(path) {
        Ok(pipeline) => pipeline,
        Err(error) => {
            writeln!(stderr, "{error}").map_err(|io| io.to_string())?;
            return Ok(EXIT_COMPILE_ERROR);
        }
    };
    if !pipeline.ok() {
        write_frontend_diagnostics(stderr, &pipeline)?;
        return Ok(EXIT_COMPILE_ERROR);
    }
    let dense = match DenseBytecodeUnit::lower_from_ir(&pipeline.lowering.unit) {
        Ok(dense) => dense,
        Err(error) => {
            writeln!(
                stderr,
                "E_PHP_VM_DENSE_BYTECODE_UNSUPPORTED: {}",
                error.message
            )
            .map_err(|io| io.to_string())?;
            return Ok(EXIT_UNSUPPORTED);
        }
    };
    if let Err(errors) = dense.verify() {
        writeln!(
            stderr,
            "E_PHP_VM_DENSE_BYTECODE_VERIFY: dense bytecode verifier rejected unit with {} error(s)",
            errors.len()
        )
        .map_err(|io| io.to_string())?;
        return Ok(EXIT_UNSUPPORTED);
    }
    let report = dense.select_rule_metadata();
    if json {
        writeln!(stdout, "{}", rule_selection_json(path, &dense, &report))
            .map_err(|error| error.to_string())?;
    } else {
        write!(stdout, "{}", report.dump_text()).map_err(|error| error.to_string())?;
    }
    Ok(EXIT_SUCCESS)
}

fn dump_dependency_units_command<W, E>(
    args: &[String],
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    let (path, json) = parse_dump_dependency_units_args(args)?;
    let pipeline = match compile_pipeline(path) {
        Ok(pipeline) => pipeline,
        Err(error) => {
            writeln!(stderr, "{error}").map_err(|io| io.to_string())?;
            return Ok(EXIT_COMPILE_ERROR);
        }
    };
    if !pipeline.ok() {
        write_frontend_diagnostics(stderr, &pipeline)?;
        return Ok(EXIT_COMPILE_ERROR);
    }
    let report = plan_dependency_units(&pipeline.lowering.unit);
    if json {
        write!(stdout, "{}", report.to_json()).map_err(|error| error.to_string())?;
    } else {
        write!(stdout, "{}", report.to_markdown()).map_err(|error| error.to_string())?;
    }
    Ok(EXIT_SUCCESS)
}

fn dump_baseline_native_stencil_command<W, E>(
    args: &[String],
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    let (path, json) = parse_dump_baseline_native_stencil_args(args)?;
    let pipeline = match compile_pipeline(path) {
        Ok(pipeline) => pipeline,
        Err(error) => {
            writeln!(stderr, "{error}").map_err(|io| io.to_string())?;
            return Ok(EXIT_COMPILE_ERROR);
        }
    };
    if !pipeline.ok() {
        write_frontend_diagnostics(stderr, &pipeline)?;
        return Ok(EXIT_COMPILE_ERROR);
    }
    let dense = match DenseBytecodeUnit::lower_from_ir(&pipeline.lowering.unit) {
        Ok(dense) => dense,
        Err(error) => {
            writeln!(
                stderr,
                "E_PHP_VM_DENSE_BYTECODE_UNSUPPORTED: {}",
                error.message
            )
            .map_err(|io| io.to_string())?;
            return Ok(EXIT_UNSUPPORTED);
        }
    };
    if let Err(errors) = dense.verify() {
        writeln!(
            stderr,
            "E_PHP_VM_DENSE_BYTECODE_VERIFY: dense bytecode verifier rejected unit with {} error(s)",
            errors.len()
        )
        .map_err(|io| io.to_string())?;
        return Ok(EXIT_UNSUPPORTED);
    }

    let report = collect_baseline_native_stencil(&dense);
    if json {
        writeln!(
            stdout,
            "{}",
            baseline_native_stencil_json(path, &dense, &report)
        )
        .map_err(|error| error.to_string())?;
    } else {
        writeln!(
            stdout,
            "ok backend=baseline-native-stencil status=no-exec path={} functions={} blocks={} instructions={} stencilable={} unsupported={} helpers={} deopt_slots={} compile_cost={} code_size_estimate={}",
            path,
            report.functions,
            report.blocks,
            report.instructions,
            report.stencilable_instructions,
            report.unsupported_instructions,
            report.helper_calls,
            report.deopt_slots,
            report.compile_cost_units,
            report.code_size_bytes_estimate
        )
        .map_err(|error| error.to_string())?;
        for (reason, count) in &report.unsupported_by_reason {
            writeln!(stdout, "unsupported {count:>4} {reason}")
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(EXIT_SUCCESS)
}

fn dump_copy_patch_stencils_command<W, E>(
    args: &[String],
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    let (path, json) = parse_dump_copy_patch_stencils_args(args)?;
    let pipeline = match compile_pipeline(path) {
        Ok(pipeline) => pipeline,
        Err(error) => {
            writeln!(stderr, "{error}").map_err(|io| io.to_string())?;
            return Ok(EXIT_COMPILE_ERROR);
        }
    };
    if !pipeline.ok() {
        write_frontend_diagnostics(stderr, &pipeline)?;
        return Ok(EXIT_COMPILE_ERROR);
    }
    let mut dense = match DenseBytecodeUnit::lower_from_ir(&pipeline.lowering.unit) {
        Ok(dense) => dense,
        Err(error) => {
            writeln!(
                stderr,
                "E_PHP_VM_DENSE_BYTECODE_UNSUPPORTED: {}",
                error.message
            )
            .map_err(|io| io.to_string())?;
            return Ok(EXIT_UNSUPPORTED);
        }
    };
    let superinstructions = dense.select_superinstructions();
    if let Err(errors) = dense.verify() {
        writeln!(
            stderr,
            "E_PHP_VM_DENSE_BYTECODE_VERIFY: dense bytecode verifier rejected unit with {} error(s)",
            errors.len()
        )
        .map_err(|io| io.to_string())?;
        return Ok(EXIT_UNSUPPORTED);
    }

    let report = collect_copy_patch_stencils(&dense, superinstructions.emitted);
    if json {
        writeln!(
            stdout,
            "{}",
            copy_patch_stencils_json(path, &dense, &report)
        )
        .map_err(|error| error.to_string())?;
    } else {
        writeln!(
            stdout,
            "ok backend=copy-patch-stencil status=no-exec path={} functions={} blocks={} instructions={} stencils={} unsupported={} patch_sites={} helpers={} code_size_estimate={} compile_cost={} work_to_compile_ratio={}",
            path,
            report.functions,
            report.blocks,
            report.instructions,
            report.stencils.len(),
            report.unsupported_instructions,
            report.patch_sites,
            report.helper_calls,
            report.code_size_bytes_estimate,
            report.compile_cost_units,
            report.work_to_compile_ratio()
        )
        .map_err(|error| error.to_string())?;
        for stencil in &report.stencils {
            writeln!(
                stdout,
                "stencil function={} block={} instruction={} kind={} opcode={} patches={} helpers={} side_exit={}",
                stencil.function,
                stencil.block,
                stencil.instruction,
                stencil.kind,
                stencil.opcode,
                stencil.patch_sites.len(),
                stencil.helper_calls.len(),
                stencil.side_exit_target
            )
            .map_err(|error| error.to_string())?;
        }
        for (reason, count) in &report.unsupported_by_reason {
            writeln!(stdout, "unsupported {count:>4} {reason}")
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(EXIT_SUCCESS)
}

fn dump_mid_tier_plan_command<W, E>(
    args: &[String],
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    let (path, json) = parse_dump_mid_tier_plan_args(args)?;
    let pipeline = match compile_pipeline(path) {
        Ok(pipeline) => pipeline,
        Err(error) => {
            writeln!(stderr, "{error}").map_err(|io| io.to_string())?;
            return Ok(EXIT_COMPILE_ERROR);
        }
    };
    if !pipeline.ok() {
        write_frontend_diagnostics(stderr, &pipeline)?;
        return Ok(EXIT_COMPILE_ERROR);
    }
    let mut dense = match DenseBytecodeUnit::lower_from_ir(&pipeline.lowering.unit) {
        Ok(dense) => dense,
        Err(error) => {
            writeln!(
                stderr,
                "E_PHP_VM_DENSE_BYTECODE_UNSUPPORTED: {}",
                error.message
            )
            .map_err(|io| io.to_string())?;
            return Ok(EXIT_UNSUPPORTED);
        }
    };
    let superinstructions = dense.select_superinstructions();
    if let Err(errors) = dense.verify() {
        writeln!(
            stderr,
            "E_PHP_VM_DENSE_BYTECODE_VERIFY: dense bytecode verifier rejected unit with {} error(s)",
            errors.len()
        )
        .map_err(|io| io.to_string())?;
        return Ok(EXIT_UNSUPPORTED);
    }

    let report = collect_mid_tier_plan(&dense, superinstructions.emitted);
    if json {
        writeln!(stdout, "{}", mid_tier_plan_json(path, &dense, &report))
            .map_err(|error| error.to_string())?;
    } else {
        writeln!(
            stdout,
            "ok backend=php-mid-tier-plan status=metadata-only path={} functions={} eligible={} rejected={} candidate_optimizations={} guards={} helpers={} deopt_points={}",
            path,
            report.functions.len(),
            report.eligible_functions,
            report.rejected_functions,
            report.candidate_optimizations.len(),
            report.expected_guards.len(),
            report.required_helpers.len(),
            report.deopt_points
        )
        .map_err(|error| error.to_string())?;
        for function in &report.functions {
            writeln!(
                stdout,
                "function {} classification={} reason_count={} optimization_count={}",
                function.function,
                function.classification,
                function.rejection_reasons.len(),
                function.candidate_optimizations.len()
            )
            .map_err(|error| error.to_string())?;
        }
    }
    Ok(EXIT_SUCCESS)
}

fn run_command<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    let mut run_options = parse_run_args(args)?;
    if run_options.debug {
        run_options.trace = true;
        run_options.trace_runtime = true;
        run_options.trace_includes = true;
        emit_debug_event(
            stderr,
            &run_options,
            "D_PHRUST_CLI_PARSE_START",
            "parse",
            "CLI parse started",
            BTreeMap::from([("command".to_string(), "run".to_string())]),
        )?;
        emit_debug_event(
            stderr,
            &run_options,
            "D_PHRUST_CLI_PARSE_END",
            "parse",
            "CLI parse completed",
            BTreeMap::from([
                ("command".to_string(), "run".to_string()),
                ("path".to_string(), run_options.path.to_string()),
            ]),
        )?;
    }
    if run_options.region_profile_json.is_none() {
        run_options.region_profile_json = region_profile_json_from_env();
    }
    if run_options.jit_explicit
        && run_options.jit.requires_cranelift()
        && !cfg!(feature = "jit-cranelift")
    {
        writeln!(
            stderr,
            "run --jit=cranelift requires the jit-cranelift feature"
        )
        .map_err(|error| error.to_string())?;
        return Ok(EXIT_UNSUPPORTED);
    }
    let path = run_options.path;
    let mut cache_stats = BytecodeCacheStats::new(run_options.bytecode_cache.mode);
    let cache_context = prepare_bytecode_cache(path, &run_options, &mut cache_stats)?;
    if run_options.bytecode_cache.mode == BytecodeCacheMode::Off {
        return run_command_with_executor(path, &run_options, cache_stats, stdout, stderr);
    }
    // Bytecode-cache read/write still owns a CLI-local serialized IR artifact.
    // Keep that adapter here until the cache format can store executor artifacts.
    let cached = try_load_bytecode_cache(&run_options, cache_context.as_ref(), &mut cache_stats);
    let (unit, compiled_pipeline) = if let Some(CachedIrArtifact { unit, .. }) = cached {
        (unit, None)
    } else {
        let pipeline = match compile_pipeline_with_optimization(path, run_options.opt_level) {
            Ok(pipeline) => pipeline,
            Err(error) => {
                cache_stats.compile_error = true;
                if run_options.bytecode_cache.stats {
                    write_cache_stats_json(stderr, &cache_stats)?;
                }
                writeln!(stderr, "{error}").map_err(|io| io.to_string())?;
                return Ok(EXIT_COMPILE_ERROR);
            }
        };
        if !pipeline.ok() {
            cache_stats.compile_error = true;
            if run_options.bytecode_cache.stats {
                write_cache_stats_json(stderr, &cache_stats)?;
            }
            write_frontend_diagnostics_formatted(stderr, &pipeline, run_options.error_format)?;
            return Ok(EXIT_COMPILE_ERROR);
        }
        emit_debug_event(
            stderr,
            &run_options,
            "D_PHRUST_IR_LOWER_END",
            "lower",
            "IR lowering completed",
            BTreeMap::from([
                (
                    "lowering_diagnostic_count".to_string(),
                    pipeline.lowering.diagnostics.len().to_string(),
                ),
                (
                    "verification_error_count".to_string(),
                    pipeline
                        .lowering
                        .verification
                        .as_ref()
                        .err()
                        .map_or(0, Vec::len)
                        .to_string(),
                ),
            ]),
        )?;
        if let Some(context) = cache_context.as_ref()
            && run_options.bytecode_cache.mode.can_write()
        {
            store_bytecode_cache(context, &pipeline, &mut cache_stats);
        }
        let _optimizer_pass_count = pipeline.optimizer_pass_count();
        (pipeline.lowering.unit.clone(), Some(pipeline))
    };
    let include_loader = include_loader_for(path).ok();
    let runtime_context = runtime_context_for(
        path,
        run_options.script_args.clone(),
        run_options.env.clone(),
        include_loader.as_ref(),
    );
    let jit_eligibility_json = build_jit_eligibility_json(&unit, run_options.jit);
    let persistent_feedback = load_persistent_feedback(&run_options, path, &unit)?;
    let bytecode_layout_profile = load_bytecode_layout_profile(&run_options)?;
    let vm = Vm::with_options(VmOptions {
        include_loader,
        runtime_context,
        include_optimization_level: run_options.opt_level,
        trace: run_options.trace,
        trace_runtime: run_options.trace_runtime,
        trace_includes: run_options.trace_includes,
        collect_counters: run_options.counters_json.is_some()
            || run_options.jit_stats.is_json()
            || run_options.region_profile_json.is_some(),
        execution_format: run_options.execution_format,
        superinstructions: run_options.superinstructions,
        bytecode_layout: run_options.bytecode_layout,
        bytecode_layout_profile,
        quickening: run_options.quickening,
        inline_caches: run_options.inline_caches,
        jit: run_options.jit,
        jit_threshold: run_options.jit_threshold,
        jit_blacklist: run_options.jit_blacklist,
        jit_dump_clif: run_options.jit_dump_clif.as_ref().map(PathBuf::from),
        tiering: run_options.tiering.clone(),
        ..VmOptions::default()
    });
    let region_profile_unit = run_options
        .region_profile_json
        .as_ref()
        .map(|_| unit.clone());
    emit_debug_event(
        stderr,
        &run_options,
        "D_PHRUST_VM_EXECUTE_START",
        "execute",
        "VM execution started",
        BTreeMap::from([("path".to_string(), path.to_string())]),
    )?;
    let result = vm.execute(unit);
    emit_debug_event(
        stderr,
        &run_options,
        "D_PHRUST_VM_EXECUTE_END",
        "execute",
        "VM execution completed",
        BTreeMap::from([
            ("status".to_string(), result.status.to_string()),
            (
                "runtime_diagnostic_count".to_string(),
                result.diagnostics.len().to_string(),
            ),
        ]),
    )?;
    stdout
        .write_all(result.output.as_bytes())
        .map_err(|error| error.to_string())?;
    write_runtime_diagnostics(stderr, path, &result, run_options.error_format)?;
    if run_options.trace || run_options.trace_runtime || run_options.trace_includes {
        write_trace(stderr, &result.trace, &run_options)?;
    }
    if let Some(path) = &run_options.counters_json {
        let Some(counters) = &result.counters else {
            return Err("counters were requested but not collected".to_string());
        };
        write_counters_json(path.clone(), counters)?;
    }
    if let Some(path) = &run_options.region_profile_json {
        let Some(counters) = &result.counters else {
            return Err("region profile was requested but counters were not collected".to_string());
        };
        let Some(unit) = region_profile_unit.as_ref() else {
            return Err(
                "region profile was requested but IR metadata was not retained".to_string(),
            );
        };
        let profile = RegionProfile::from_unit_and_counters(unit, counters, run_options.path);
        write_region_profile_json(path.clone(), &profile)?;
    }
    if run_options.jit_stats.is_json()
        && let Some(counters) = result.counters.as_ref()
    {
        write_jit_stats_json(stderr, counters, &run_options, &jit_eligibility_json)?;
    }
    if let Some(path) = run_options.tiering_stats_json {
        let Some(stats) = &result.tiering_stats else {
            return Err("tiering stats were requested but not collected".to_string());
        };
        write_tiering_stats_json(path, stats)?;
    }
    if let Some(path) = run_options.persistent_feedback.stats_json {
        write_persistent_feedback_stats_json(path, &persistent_feedback.stats)?;
    }
    if run_options.bytecode_cache.stats {
        write_cache_stats_json(stderr, &cache_stats)?;
    }
    match result.status.exit_status() {
        ExitStatus::Success => Ok(EXIT_SUCCESS),
        ExitStatus::CompileError => {
            if let Some(pipeline) = compiled_pipeline.as_ref()
                && write_vm_compile_fatal_line(stderr, pipeline, &result.diagnostics)?
            {
                return Ok(EXIT_COMPILE_ERROR);
            }
            write_status(stderr, path, &result.status)?;
            Ok(EXIT_COMPILE_ERROR)
        }
        ExitStatus::RuntimeError | ExitStatus::Fatal => {
            write_status(stderr, path, &result.status)?;
            Ok(EXIT_RUNTIME_ERROR)
        }
        ExitStatus::Unsupported => {
            write_status(stderr, path, &result.status)?;
            Ok(EXIT_UNSUPPORTED)
        }
    }
}

fn run_command_with_executor<W, E>(
    path: &str,
    run_options: &RunOptions<'_>,
    cache_stats: BytecodeCacheStats,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    let collect_counters = run_options.counters_json.is_some()
        || run_options.jit_stats.is_json()
        || run_options.region_profile_json.is_some();
    let bytecode_layout_profile = load_bytecode_layout_profile(run_options)?;
    emit_debug_event(
        stderr,
        run_options,
        "D_PHRUST_SOURCE_READ_START",
        "source_read",
        "source read started",
        BTreeMap::from([("path".to_string(), path.to_string())]),
    )?;
    let (source, real_path, source_path) = php_executor::read_script(Path::new(path))?;
    emit_debug_event(
        stderr,
        run_options,
        "D_PHRUST_SOURCE_READ_END",
        "source_read",
        "source read completed",
        BTreeMap::from([
            ("path".to_string(), source_path.clone()),
            ("bytes".to_string(), source.len().to_string()),
        ]),
    )?;
    let executor = PhpExecutor::with_options(PhpExecutorOptions {
        optimization_level: run_options.opt_level,
        vm_options: VmOptions {
            trace: run_options.trace,
            trace_runtime: run_options.trace_runtime,
            trace_includes: run_options.trace_includes,
            collect_counters,
            include_optimization_level: run_options.opt_level,
            execution_format: run_options.execution_format,
            superinstructions: run_options.superinstructions,
            bytecode_layout: run_options.bytecode_layout,
            bytecode_layout_profile,
            quickening: run_options.quickening,
            inline_caches: run_options.inline_caches,
            jit: run_options.jit,
            jit_threshold: run_options.jit_threshold,
            jit_blacklist: run_options.jit_blacklist,
            jit_dump_clif: run_options.jit_dump_clif.as_ref().map(PathBuf::from),
            tiering: run_options.tiering.clone(),
            ..VmOptions::default()
        },
    });
    emit_debug_event(
        stderr,
        run_options,
        "D_PHRUST_FRONTEND_ANALYZE_START",
        "frontend",
        "frontend analysis started",
        BTreeMap::from([("path".to_string(), source_path.clone())]),
    )?;
    let compiled = match executor.compile_source(PhpCompileInput {
        source,
        source_path,
        optimization_level: Some(run_options.opt_level),
    }) {
        Ok(compiled) => compiled,
        Err(PhpExecutionError::Compile(output)) => {
            if run_options.bytecode_cache.stats {
                write_cache_stats_json(stderr, &cache_stats)?;
            }
            write_execution_output_diagnostics(stderr, path, &output, run_options.error_format)?;
            return Ok(EXIT_COMPILE_ERROR);
        }
        Err(PhpExecutionError::Engine(error)) => return Err(error),
    };
    emit_debug_event(
        stderr,
        run_options,
        "D_PHRUST_FRONTEND_ANALYZE_END",
        "frontend",
        "frontend analysis completed",
        BTreeMap::from([("status".to_string(), "ok".to_string())]),
    )?;
    emit_debug_event(
        stderr,
        run_options,
        "D_PHRUST_OPTIMIZER_END",
        "optimize",
        "optimization completed",
        BTreeMap::from([(
            "optimization_level".to_string(),
            run_options.opt_level.as_str().to_string(),
        )]),
    )?;
    let jit_eligibility_json = build_jit_eligibility_json(compiled.ir_unit(), run_options.jit);
    let persistent_feedback = load_persistent_feedback(run_options, path, compiled.ir_unit())?;
    let runtime_context = RuntimeContext::controlled_cli(path, run_options.script_args.clone())
        .with_env(run_options.env.clone());
    emit_debug_event(
        stderr,
        run_options,
        "D_PHRUST_VM_EXECUTE_START",
        "execute",
        "VM execution started",
        BTreeMap::from([("path".to_string(), path.to_string())]),
    )?;
    let output = executor.execute_compiled(
        &compiled,
        PhpRequestExecutionInput {
            real_path: Some(real_path),
            cwd: std::env::current_dir().map_err(|error| format!("current directory: {error}"))?,
            include_roots: Vec::new(),
            runtime_context,
            collect_counters,
        },
    );
    emit_debug_event(
        stderr,
        run_options,
        "D_PHRUST_VM_EXECUTE_END",
        "execute",
        "VM execution completed",
        BTreeMap::from([
            ("status".to_string(), format!("{:?}", output.status)),
            (
                "runtime_diagnostic_count".to_string(),
                output.runtime_diagnostics.len().to_string(),
            ),
        ]),
    )?;
    stdout
        .write_all(&output.stdout)
        .map_err(|error| error.to_string())?;
    if output.status == PhpExecutionStatus::Success {
        write_executor_success_runtime_diagnostics(
            stderr,
            path,
            &output,
            run_options.error_format,
        )?;
    } else if !output.diagnostics_text.is_empty()
        || !output.diagnostics.is_empty()
        || !output.runtime_diagnostics.is_empty()
    {
        write_execution_output_diagnostics(stderr, path, &output, run_options.error_format)?;
    }
    if run_options.trace || run_options.trace_runtime || run_options.trace_includes {
        write_trace(stderr, &output.trace, run_options)?;
    }
    if let Some(path) = &run_options.counters_json {
        let Some(counters) = &output.counters else {
            return Err("counters were requested but not collected".to_string());
        };
        write_counters_json(path.clone(), counters)?;
    }
    if let Some(path) = &run_options.region_profile_json {
        let Some(counters) = &output.counters else {
            return Err("region profile was requested but counters were not collected".to_string());
        };
        let profile =
            RegionProfile::from_unit_and_counters(compiled.ir_unit(), counters, run_options.path);
        write_region_profile_json(path.clone(), &profile)?;
    }
    if run_options.jit_stats.is_json()
        && let Some(counters) = output.counters.as_ref()
    {
        write_jit_stats_json(stderr, counters, run_options, &jit_eligibility_json)?;
    }
    if let Some(path) = run_options.tiering_stats_json.clone() {
        let Some(stats) = &output.tiering_stats else {
            return Err("tiering stats were requested but not collected".to_string());
        };
        write_tiering_stats_json(path, stats)?;
    }
    if let Some(path) = run_options.persistent_feedback.stats_json.clone() {
        write_persistent_feedback_stats_json(path, &persistent_feedback.stats)?;
    }
    if run_options.bytecode_cache.stats {
        write_cache_stats_json(stderr, &cache_stats)?;
    }
    Ok(match output.status {
        PhpExecutionStatus::Success => EXIT_SUCCESS,
        PhpExecutionStatus::CompileError => EXIT_COMPILE_ERROR,
        PhpExecutionStatus::RuntimeError | PhpExecutionStatus::Fatal => EXIT_RUNTIME_ERROR,
        PhpExecutionStatus::Unsupported => EXIT_UNSUPPORTED,
    })
}

fn write_executor_success_runtime_diagnostics<W: Write>(
    stderr: &mut W,
    path: &str,
    output: &PhpExecutionOutput,
    format: DiagnosticOutputFormat,
) -> Result<(), String> {
    let php_output = String::from_utf8_lossy(&output.stdout);
    for diagnostic in &output.runtime_diagnostics {
        if php_output.contains(diagnostic.message()) {
            continue;
        }
        match format {
            DiagnosticOutputFormat::Text => {
                writeln!(
                    stderr,
                    "{path}: runtime-diagnostic: {}",
                    diagnostic.to_json()
                )
                .map_err(|error| error.to_string())?;
            }
            DiagnosticOutputFormat::Json => {
                write_diagnostic_envelope(stderr, &diagnostic.to_diagnostic_envelope(), format)?;
            }
        }
    }
    Ok(())
}

fn report_command<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    let options = parse_report_args(args)?;
    let path = options.path;
    let pipeline = match compile_pipeline(path) {
        Ok(pipeline) => pipeline,
        Err(error) => {
            writeln!(stderr, "{error}").map_err(|io| io.to_string())?;
            return Ok(EXIT_COMPILE_ERROR);
        }
    };

    let vm_result = if pipeline.ok() {
        let include_loader = include_loader_for(path).ok();
        let runtime_context =
            runtime_context_for(path, Vec::new(), Vec::new(), include_loader.as_ref());
        let vm = Vm::with_options(VmOptions {
            include_loader,
            runtime_context,
            ..VmOptions::default()
        });
        Some(vm.execute(pipeline.lowering.unit.clone()))
    } else {
        None
    };

    let report = match options.format {
        ReportFormat::Markdown => render_markdown_report(&pipeline, vm_result.as_ref()),
        ReportFormat::Html => render_html_report(&pipeline, vm_result.as_ref()),
    };
    write!(stdout, "{report}").map_err(|error| error.to_string())?;

    if !pipeline.ok() {
        write_frontend_diagnostics(stderr, &pipeline)?;
        return Ok(EXIT_COMPILE_ERROR);
    }

    let Some(vm_result) = vm_result else {
        return Ok(EXIT_COMPILE_ERROR);
    };
    match vm_result.status.exit_status() {
        ExitStatus::Success => Ok(EXIT_SUCCESS),
        ExitStatus::CompileError => Ok(EXIT_COMPILE_ERROR),
        ExitStatus::RuntimeError | ExitStatus::Fatal => Ok(EXIT_RUNTIME_ERROR),
        ExitStatus::Unsupported => Ok(EXIT_UNSUPPORTED),
    }
}

struct Pipeline {
    path: String,
    source: String,
    frontend: FrontendResult,
    lowering: php_ir::LoweringResult,
    optimizer: Option<OptimizationReport>,
}

impl Pipeline {
    fn ok(&self) -> bool {
        !self.frontend.has_errors()
            && self.lowering.diagnostics.is_empty()
            && self.lowering.verification.is_ok()
    }

    fn compile_json(&self) -> String {
        to_json_string(&CompileJson::from_pipeline(self))
    }

    fn optimizer_pass_count(&self) -> usize {
        self.optimizer
            .as_ref()
            .map_or(0, OptimizationReport::enabled_pass_count)
    }
}

fn to_json_string<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("CLI JSON output should be serializable")
}

#[derive(Serialize)]
struct CompileJson<'a> {
    ok: bool,
    path: &'a str,
    source_bytes: usize,
    parser_diagnostics: Vec<ParserDiagnosticJson<'a>>,
    semantic_diagnostics: Vec<SemanticDiagnosticJson<'a>>,
    lowering_diagnostics: Vec<LoweringDiagnosticJson<'a>>,
    verification_diagnostics: Vec<VerificationDiagnosticJson<'a>>,
    ir: IrJson,
    optimizer: Option<OptimizerReportJson<'a>>,
}

impl<'a> CompileJson<'a> {
    fn from_pipeline(pipeline: &'a Pipeline) -> Self {
        Self {
            ok: pipeline.ok(),
            path: &pipeline.path,
            source_bytes: pipeline.source.len(),
            parser_diagnostics: parser_diagnostics_json(&pipeline.path, &pipeline.frontend),
            semantic_diagnostics: semantic_diagnostics_json(&pipeline.path, &pipeline.frontend),
            lowering_diagnostics: lowering_diagnostics_json(&pipeline.path, &pipeline.lowering),
            verification_diagnostics: verification_diagnostics_json(&pipeline.lowering),
            ir: IrJson {
                version: pipeline.lowering.unit.version,
                functions: pipeline.lowering.unit.functions.len(),
                constants: pipeline.lowering.unit.constants.len(),
                verified: pipeline.lowering.verification.is_ok(),
            },
            optimizer: pipeline.optimizer.as_ref().map(OptimizerReportJson::from),
        }
    }
}

#[derive(Serialize)]
struct ParserDiagnosticJson<'a> {
    path: &'a str,
    id: &'a str,
    message: &'a str,
    span: Option<RangeJson>,
}

#[derive(Serialize)]
struct SemanticDiagnosticJson<'a> {
    path: &'a str,
    id: &'a str,
    severity: &'a str,
    message: &'a str,
    span: Option<RangeJson>,
}

#[derive(Serialize)]
struct LoweringDiagnosticJson<'a> {
    path: &'a str,
    id: &'a str,
    message: &'a str,
    span: RangeJson,
}

#[derive(Serialize)]
struct VerificationDiagnosticJson<'a> {
    id: &'a str,
    message: &'a str,
}

#[derive(Clone, Copy, Serialize)]
struct RangeJson {
    start: usize,
    end: usize,
}

impl RangeJson {
    fn from_text_range(span: TextRange) -> Self {
        Self {
            start: span.start().to_usize(),
            end: span.end().to_usize(),
        }
    }
}

#[derive(Serialize)]
struct IrJson {
    version: u32,
    functions: usize,
    constants: usize,
    verified: bool,
}

#[derive(Serialize)]
struct OptimizerReportJson<'a> {
    level: &'a str,
    enabled_pass_count: usize,
    passes: Vec<OptimizerPassJson<'a>>,
}

impl<'a> From<&'a OptimizationReport> for OptimizerReportJson<'a> {
    fn from(report: &'a OptimizationReport) -> Self {
        Self {
            level: report.level.as_str(),
            enabled_pass_count: report.enabled_pass_count(),
            passes: report
                .passes
                .iter()
                .map(|pass| OptimizerPassJson {
                    name: pass.name,
                    phase: pass.phase.as_str(),
                    enabled: pass.enabled,
                    changed: pass.changed,
                    source_spans_preserved: pass.source_spans_preserved,
                    stats: &pass.stats,
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct OptimizerPassJson<'a> {
    name: &'a str,
    phase: &'a str,
    enabled: bool,
    changed: bool,
    source_spans_preserved: bool,
    stats: &'a BTreeMap<&'static str, u64>,
}

fn compile_pipeline_with_optimization(
    path: &str,
    opt_level: OptimizationLevel,
) -> Result<Pipeline, String> {
    let source = read_source_to_string(path)?;
    let frontend = analyze_source(&source);
    let source_path = fs::canonicalize(path)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string());
    let mut lowering = lower_frontend_result(
        &frontend,
        LoweringOptions {
            source_path,
            source_text: Some(source.clone()),
            ..LoweringOptions::default()
        },
    );
    let optimizer = if opt_level.runs_pipeline()
        && !frontend.has_errors()
        && lowering.diagnostics.is_empty()
        && lowering.verification.is_ok()
    {
        let report = PassPipeline::performance()
            .run(&mut lowering.unit, &PassContext::new(opt_level))
            .map_err(|error| format!("{path}: optimizer failed: {error}"))?;
        lowering.verification = verify_unit(&lowering.unit);
        Some(report)
    } else {
        None
    };
    if !frontend.has_errors() && lowering.verification.is_ok() {
        verify_unit(&lowering.unit).map_err(|errors| {
            format!("{path}: IR verification failed: {} error(s)", errors.len())
        })?;
    }
    Ok(Pipeline {
        path: path.to_string(),
        source,
        frontend,
        lowering,
        optimizer,
    })
}

fn compile_pipeline(path: &str) -> Result<Pipeline, String> {
    compile_pipeline_with_optimization(path, OptimizationLevel::O0)
}

fn read_source_to_string(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| source_io_error("read source file", path, &error))
}

fn read_source_bytes(path: &str) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| source_io_error("read source bytes", path, &error))
}

fn source_io_error(operation: &str, path: &str, error: &std::io::Error) -> String {
    let cwd = env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|cwd_error| format!("<unavailable: {cwd_error}>"));
    format!(
        "{operation} failed for path `{path}` from cwd `{cwd}`: {error}; suggestion: check that the file exists and is readable"
    )
}

fn include_loader_for(path: &str) -> Result<IncludeLoader, String> {
    let path = fs::canonicalize(path)
        .map_err(|error| source_io_error("canonicalize source path", path, &error))?;
    let root = path
        .parent()
        .ok_or_else(|| format!("{}: missing parent directory", path.display()))?;
    let cwd = std::env::current_dir().map_err(|error| format!("current directory: {error}"))?;
    IncludeLoader::new([root.to_path_buf(), cwd])
}

fn runtime_context_for(
    path: &str,
    script_args: Vec<String>,
    env: Vec<(String, String)>,
    include_loader: Option<&IncludeLoader>,
) -> RuntimeContext {
    let context = RuntimeContext::controlled_cli(path, script_args).with_env(env);
    let Some(loader) = include_loader else {
        return context;
    };
    context.with_filesystem_capabilities(
        FilesystemCapabilities::none().with_allowed_roots(loader.allowed_roots().to_vec()),
    )
}

fn write_frontend_diagnostics<W: Write>(stderr: &mut W, pipeline: &Pipeline) -> Result<(), String> {
    for diagnostic in pipeline.frontend.parser_diagnostics() {
        write_parser_diagnostic(
            stderr,
            &pipeline.path,
            &pipeline.source,
            diagnostic.span,
            diagnostic.id.as_str(),
            &diagnostic.message,
        )?;
    }
    for diagnostic in pipeline.frontend.semantic_diagnostics() {
        if diagnostic.severity() == Severity::Error {
            if let Some(span) = diagnostic.span() {
                if let Some(message) = semantic_diagnostic_php_fatal_message(
                    diagnostic.id(),
                    diagnostic.message(),
                    span,
                    &pipeline.lowering.unit,
                ) {
                    write_php_fatal_line(stderr, &pipeline.path, &pipeline.source, span, &message)?;
                    continue;
                }
                if semantic_diagnostic_uses_php_parse_error_line(diagnostic.id()) {
                    write_php_parse_error_line(
                        stderr,
                        &pipeline.path,
                        &pipeline.source,
                        span,
                        diagnostic.message(),
                    )?;
                    return Ok(());
                }
                if semantic_diagnostic_uses_php_fatal_line(diagnostic.id()) {
                    write_php_fatal_line(
                        stderr,
                        &pipeline.path,
                        &pipeline.source,
                        span,
                        diagnostic.message(),
                    )?;
                    if semantic_diagnostic_is_immediate_php_fatal(diagnostic.id()) {
                        return Ok(());
                    }
                    continue;
                }
                write_span_line(
                    stderr,
                    &pipeline.path,
                    span,
                    diagnostic.id().as_str(),
                    diagnostic.message(),
                )?;
            } else {
                writeln!(
                    stderr,
                    "{}: {}: {}",
                    pipeline.path,
                    diagnostic.id().as_str(),
                    diagnostic.message()
                )
                .map_err(|error| error.to_string())?;
            }
        }
    }
    for diagnostic in &pipeline.lowering.diagnostics {
        writeln!(
            stderr,
            "{}:{}..{}: {}: {}",
            pipeline.path,
            diagnostic.span.start,
            diagnostic.span.end,
            diagnostic.id,
            diagnostic.message
        )
        .map_err(|error| error.to_string())?;
    }
    if let Err(errors) = &pipeline.lowering.verification {
        writeln!(
            stderr,
            "{}: IR verification failed: {} error(s)",
            pipeline.path,
            errors.len()
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn write_status<W: Write>(
    stderr: &mut W,
    path: &str,
    status: &php_runtime::api::ExecutionStatus,
) -> Result<(), String> {
    writeln!(stderr, "{path}: {status}").map_err(|error| error.to_string())
}

fn write_vm_compile_fatal_line<W: Write>(
    stderr: &mut W,
    pipeline: &Pipeline,
    diagnostics: &[RuntimeDiagnostic],
) -> Result<bool, String> {
    let Some((payload, span)) = diagnostics.iter().find_map(|diagnostic| {
        let RuntimeDiagnosticPayload::VmCompile(payload) = diagnostic.payload()?;
        Some((payload, diagnostic.source_span()))
    }) else {
        return Ok(false);
    };
    if span.start == span.end {
        return Ok(false);
    }
    write_php_fatal_line(
        stderr,
        &pipeline.path,
        &pipeline.source,
        TextRange::new(span.start as usize, span.end as usize),
        &payload.php_fatal_message(),
    )?;
    Ok(true)
}

fn semantic_diagnostic_php_fatal_message(
    id: DiagnosticId,
    message: &str,
    span: TextRange,
    unit: &IrUnit,
) -> Option<String> {
    match id {
        DiagnosticId::InvalidConstExpr => {
            Some("Constant expression contains invalid operations".to_owned())
        }
        DiagnosticId::DuplicateClassMember => {
            let constant_name = message
                .strip_prefix("duplicate class constant `")?
                .strip_suffix('`')?;
            let class_name = class_display_name_containing_span(unit, span)?;
            Some(format!(
                "Cannot redefine class constant {class_name}::{constant_name}"
            ))
        }
        DiagnosticId::IncompatibleModifiers => match message {
            "`static` modifier is not allowed on class constant" => {
                Some("Cannot use the static modifier on a class constant".to_owned())
            }
            "`abstract` modifier is not allowed on class constant" => {
                Some("Cannot use the abstract modifier on a class constant".to_owned())
            }
            "method cannot be both abstract and final" => {
                Some("Cannot use the final modifier on an abstract method".to_owned())
            }
            _ => None,
        },
        _ => None,
    }
}

fn class_display_name_containing_span(unit: &IrUnit, span: TextRange) -> Option<&str> {
    let start = span.start().to_usize();
    let end = span.end().to_usize();
    unit.classes
        .iter()
        .filter(|class| class.span.start as usize <= start && end <= class.span.end as usize)
        .min_by_key(|class| class.span.end.saturating_sub(class.span.start))
        .map(|class| class.display_name.as_str())
}

fn write_runtime_diagnostics<W: Write>(
    stderr: &mut W,
    path: &str,
    result: &VmResult,
    format: DiagnosticOutputFormat,
) -> Result<(), String> {
    let php_output = result.output.to_string_lossy();
    for diagnostic in &result.diagnostics {
        if result.status.is_success() && php_output.contains(diagnostic.message()) {
            continue;
        }
        match format {
            DiagnosticOutputFormat::Text => {
                writeln!(
                    stderr,
                    "{path}: runtime-diagnostic: {}",
                    diagnostic.to_json()
                )
                .map_err(|error| error.to_string())?;
            }
            DiagnosticOutputFormat::Json => {
                write_diagnostic_envelope(stderr, &diagnostic.to_diagnostic_envelope(), format)?;
            }
        }
    }
    Ok(())
}

fn write_trace<W: Write>(
    stderr: &mut W,
    trace: &[String],
    options: &RunOptions<'_>,
) -> Result<(), String> {
    if options.debug {
        for (index, line) in trace.iter().enumerate() {
            emit_debug_event(
                stderr,
                options,
                "D_PHRUST_VM_TRACE",
                "execute",
                "VM trace event",
                BTreeMap::from([
                    ("index".to_string(), index.to_string()),
                    ("trace".to_string(), line.clone()),
                ]),
            )?;
        }
        return Ok(());
    }
    writeln!(stderr, "vm-trace:").map_err(|error| error.to_string())?;
    for line in trace {
        writeln!(stderr, "  {line}").map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn frontend_diagnostic_envelopes(pipeline: &Pipeline) -> Vec<DiagnosticEnvelope> {
    let source = SourceText::new(&pipeline.source);
    let mut diagnostics = Vec::new();
    for diagnostic in pipeline.frontend.parser_diagnostics() {
        diagnostics.push(diagnostic.to_diagnostic_envelope(
            Some(&source),
            None,
            Some(&pipeline.path),
        ));
    }
    for diagnostic in pipeline.frontend.semantic_diagnostics() {
        if diagnostic.severity() == Severity::Error {
            diagnostics.push(diagnostic.to_diagnostic_envelope(
                Some(&source),
                None,
                Some(&pipeline.path),
            ));
        }
    }
    for diagnostic in &pipeline.lowering.diagnostics {
        diagnostics.push(
            diagnostic.to_diagnostic_envelope(
                Some(&pipeline.path),
                &LoweringDiagnosticContext::default(),
            ),
        );
    }
    if let Err(errors) = &pipeline.lowering.verification {
        let context = VerificationDiagnosticContext {
            source_path: Some(pipeline.path.clone()),
            ..VerificationDiagnosticContext::default()
        };
        for error in errors {
            diagnostics.push(error.to_diagnostic_envelope(&context));
        }
    }
    diagnostics
}

fn write_frontend_diagnostics_formatted<W: Write>(
    stderr: &mut W,
    pipeline: &Pipeline,
    format: DiagnosticOutputFormat,
) -> Result<(), String> {
    match format {
        DiagnosticOutputFormat::Text => write_frontend_diagnostics(stderr, pipeline),
        DiagnosticOutputFormat::Json => {
            for diagnostic in frontend_diagnostic_envelopes(pipeline) {
                write_diagnostic_envelope(stderr, &diagnostic, format)?;
            }
            Ok(())
        }
    }
}

fn write_execution_output_diagnostics<W: Write>(
    stderr: &mut W,
    path: &str,
    output: &PhpExecutionOutput,
    format: DiagnosticOutputFormat,
) -> Result<(), String> {
    match format {
        DiagnosticOutputFormat::Text => stderr
            .write_all(output.diagnostics_text.as_bytes())
            .map_err(|error| error.to_string()),
        DiagnosticOutputFormat::Json => {
            if !output.diagnostics.is_empty() {
                for diagnostic in &output.diagnostics {
                    write_diagnostic_envelope(stderr, diagnostic, format)?;
                }
                return Ok(());
            }
            write_executor_success_runtime_diagnostics(stderr, path, output, format)
        }
    }
}

fn write_php_parse_error_line<W: Write>(
    stderr: &mut W,
    path: &str,
    source: &str,
    span: TextRange,
    message: &str,
) -> Result<(), String> {
    let line = SourceText::new(source).line_col(span.start()).line;
    writeln!(stderr, "Parse error: {message} in {path} on line {line}")
        .map_err(|error| error.to_string())
}

fn write_php_fatal_line<W: Write>(
    stderr: &mut W,
    path: &str,
    source: &str,
    span: TextRange,
    message: &str,
) -> Result<(), String> {
    let line = SourceText::new(source).line_col(span.start()).line;
    writeln!(stderr, "Fatal error: {message} in {path} on line {line}")
        .map_err(|error| error.to_string())
}

fn write_span_line<W: Write>(
    stderr: &mut W,
    path: &str,
    span: TextRange,
    id: &str,
    message: &str,
) -> Result<(), String> {
    writeln!(
        stderr,
        "{}:{}..{}: {}: {}",
        path,
        span.start().to_usize(),
        span.end().to_usize(),
        id,
        message
    )
    .map_err(|error| error.to_string())
}

fn write_parser_diagnostic<W: Write>(
    stderr: &mut W,
    path: &str,
    source: &str,
    span: TextRange,
    id: &str,
    message: &str,
) -> Result<(), String> {
    if message.starts_with("syntax error,") {
        write_php_parse_error_line(stderr, path, source, span, message)
    } else {
        write_span_line(stderr, path, span, id, message)
    }
}

fn semantic_diagnostic_uses_php_fatal_line(id: DiagnosticId) -> bool {
    matches!(
        id,
        DiagnosticId::ClosureUseDuplicatesParameter
            | DiagnosticId::DuplicateClosureUseVariable
            | DiagnosticId::ClosureUseAutoGlobal
            | DiagnosticId::ThisParameter
            | DiagnosticId::ThisReassignment
    )
}

fn semantic_diagnostic_uses_php_parse_error_line(id: DiagnosticId) -> bool {
    matches!(id, DiagnosticId::InvalidClassConstantWrite)
}

fn semantic_diagnostic_is_immediate_php_fatal(id: DiagnosticId) -> bool {
    matches!(
        id,
        DiagnosticId::ThisParameter | DiagnosticId::ThisReassignment
    )
}

fn debug_enabled_from_env(name: &str) -> bool {
    env::var(name)
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "on"))
}

fn error_format_from_env(name: &str) -> DiagnosticOutputFormat {
    env::var(name)
        .ok()
        .and_then(|value| parse_diagnostic_output_format(&value).ok())
        .unwrap_or(DiagnosticOutputFormat::Text)
}

fn parse_diagnostic_output_format(value: &str) -> Result<DiagnosticOutputFormat, String> {
    match value {
        "text" => Ok(DiagnosticOutputFormat::Text),
        "json" | "jsonl" => Ok(DiagnosticOutputFormat::Json),
        _ => Err(format!(
            "run --error-format requires text or json; got `{value}`"
        )),
    }
}

fn cli_usage_diagnostic_from_message(message: &str) -> php_diagnostics::DiagnosticEnvelope {
    let (command, argument, accepted_values, suggestion) = if let Some(command) = message
        .strip_prefix("unknown php-vm command `")
        .and_then(|rest| rest.strip_suffix('`'))
    {
        (
            Some("php-vm"),
            Some(command),
            Some(
                "compile, dump-ir, dump-bytecode-patterns, dump-rule-selection, dump-dependency-units, dump-baseline-native-stencil, dump-copy-patch-stencils, dump-mid-tier-plan, dump-cranelift-clif, run, report, compare",
            ),
            "run php-vm --help",
        )
    } else if message.starts_with("run ") {
        (
            Some("php-vm run"),
            message
                .split_whitespace()
                .nth(1)
                .filter(|value| value.starts_with("--") || *value == "requires"),
            Some("php-vm run [options] <path.php> [-- args...]"),
            "run php-vm run --help",
        )
    } else {
        (Some("php-vm"), None, None, "run php-vm --help")
    };

    usage_diagnostic(message, command, argument, accepted_values, suggestion)
}

fn emit_debug_event<W: Write>(
    stderr: &mut W,
    options: &RunOptions<'_>,
    code: &str,
    phase: &str,
    message: &str,
    context: BTreeMap<String, String>,
) -> Result<(), String> {
    if !options.debug {
        return Ok(());
    }
    let event = DebugEvent::new(
        code,
        debug_layer_for_phase(phase),
        DiagnosticPhase::new(phase),
        message,
    )
    .with_context(context);
    let rendered = match options.error_format {
        DiagnosticOutputFormat::Text => {
            let mut line = event.text_line();
            line.push('\n');
            line
        }
        DiagnosticOutputFormat::Json => event.json_line().map_err(|error| error.to_string())?,
    };
    if let Some(path) = options.debug_log.as_deref() {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| format!("{path}: {error}"))?;
        file.write_all(rendered.as_bytes())
            .map_err(|error| format!("{path}: {error}"))
    } else {
        stderr
            .write_all(rendered.as_bytes())
            .map_err(|error| error.to_string())
    }
}

fn debug_layer_for_phase(phase: &str) -> DiagnosticLayer {
    match phase {
        "parse" => DiagnosticLayer::cli(),
        "source_read" | "frontend" => DiagnosticLayer::executor(),
        "lower" => DiagnosticLayer::ir(),
        "optimize" => DiagnosticLayer::optimizer(),
        "execute" => DiagnosticLayer::vm(),
        _ => DiagnosticLayer::executor(),
    }
}

mod args;
use args::*;

fn prepare_bytecode_cache(
    path: &str,
    run_options: &RunOptions<'_>,
    stats: &mut BytecodeCacheStats,
) -> Result<Option<BytecodeCacheContext>, String> {
    if run_options.bytecode_cache.mode == BytecodeCacheMode::Off
        && !run_options.bytecode_cache.clear
    {
        return Ok(None);
    }

    let cache_dir = run_options
        .bytecode_cache
        .dir
        .clone()
        .unwrap_or_else(default_bytecode_cache_dir);
    if run_options.bytecode_cache.clear {
        clear_bytecode_cache_dir(&cache_dir)?;
        stats.cleared = true;
    }
    if run_options.bytecode_cache.mode == BytecodeCacheMode::Off {
        return Ok(None);
    }

    let source = match fs::read(path) {
        Ok(source) => source,
        Err(_) => {
            stats.miss = true;
            return Ok(None);
        }
    };
    let source_path = fs::canonicalize(path)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string());
    let fingerprint = CacheFingerprint::from_inputs(
        CacheFingerprintInput::new(source, env!("CARGO_PKG_VERSION"), rust_target_label())
            .with_source_path(source_path)
            .with_opt_level(run_options.opt_level.as_str())
            .with_feature_flag("bytecode_cache", true)
            .with_runtime_config("script_env_count", run_options.env.len().to_string()),
    )
    .map_err(|error| format!("bytecode cache fingerprint: {error}"))?;
    let cache_file = cache_file_for(&cache_dir, &fingerprint)?;
    stats.cache_file = Some(cache_file.clone());

    Ok(Some(BytecodeCacheContext {
        fingerprint,
        cache_file,
    }))
}

fn load_persistent_feedback(
    run_options: &RunOptions<'_>,
    path: &str,
    unit: &IrUnit,
) -> Result<PersistentFeedbackLoadReport, String> {
    let Some(feedback_path) = run_options.persistent_feedback.read.as_deref() else {
        return Ok(PersistentFeedbackLoadReport::new(
            PersistentFeedbackStore::default(),
            PersistentFeedbackStats::default(),
        ));
    };
    let context = persistent_feedback_context(path, run_options, unit)?;
    let bytes = match fs::read(feedback_path) {
        Ok(bytes) => bytes,
        Err(_) => {
            let stats = PersistentFeedbackStats {
                files_considered: 1,
                rejected_corrupt: 1,
                fallback_to_baseline: true,
                ..PersistentFeedbackStats::default()
            };
            return Ok(PersistentFeedbackLoadReport::new(
                PersistentFeedbackStore::default(),
                stats,
            ));
        }
    };
    Ok(context.validate_bytes(&bytes))
}

fn persistent_feedback_context(
    path: &str,
    run_options: &RunOptions<'_>,
    unit: &IrUnit,
) -> Result<PersistentFeedbackContext, String> {
    let source = read_source_bytes(path)?;
    let source_path = fs::canonicalize(path)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string());
    let fingerprint = CacheFingerprint::from_inputs(
        CacheFingerprintInput::new(source, env!("CARGO_PKG_VERSION"), rust_target_label())
            .with_source_path(source_path)
            .with_opt_level(run_options.opt_level.as_str())
            .with_feature_flag("persistent_feedback", true)
            .with_runtime_config(
                "compile_options",
                persistent_feedback_compile_options(run_options),
            )
            .with_runtime_config("script_env_count", run_options.env.len().to_string()),
    )
    .map_err(|error| format!("persistent feedback fingerprint: {error}"))?;
    Ok(PersistentFeedbackContext::new(
        fingerprint.digest,
        env!("CARGO_PKG_VERSION"),
        PHP_TARGET_VERSION,
        persistent_feedback_compile_options(run_options),
        stable_feedback_fingerprint(unit.to_snapshot_text().as_bytes()),
        PersistentFeedbackEpochs::default(),
        rust_target_label(),
    ))
}

fn persistent_feedback_compile_options(run_options: &RunOptions<'_>) -> String {
    format!(
        "opt={},exec={},super={},layout={},quickening={},inline_caches={},bytecode_cache={},jit={},tiering={}",
        run_options.opt_level.as_str(),
        run_options.execution_format.as_str(),
        run_options.superinstructions.as_str(),
        run_options.bytecode_layout.as_str(),
        on_off(run_options.quickening.enabled()),
        on_off(run_options.inline_caches.enabled()),
        run_options.bytecode_cache.mode.as_str(),
        run_options.jit.as_str(),
        on_off(run_options.tiering.enabled),
    )
}

fn stable_feedback_fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn on_off(enabled: bool) -> &'static str {
    if enabled { "on" } else { "off" }
}

fn try_load_bytecode_cache(
    run_options: &RunOptions<'_>,
    context: Option<&BytecodeCacheContext>,
    stats: &mut BytecodeCacheStats,
) -> Option<CachedIrArtifact> {
    if !run_options.bytecode_cache.mode.can_read() {
        return None;
    }
    let Some(context) = context else {
        stats.miss = true;
        return None;
    };
    let bytes = match fs::read(&context.cache_file) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            stats.miss = true;
            return None;
        }
        Err(error) => {
            stats.miss = true;
            stats.load_error = Some(error.to_string());
            return None;
        }
    };
    match CacheArtifact::load_ir_unit(&bytes, &rust_target_label(), &context.fingerprint) {
        Ok(cached) => {
            stats.hit = true;
            Some(cached)
        }
        Err(error) => {
            stats.miss = true;
            stats.load_error = Some(error.to_string());
            None
        }
    }
}

fn store_bytecode_cache(
    context: &BytecodeCacheContext,
    pipeline: &Pipeline,
    stats: &mut BytecodeCacheStats,
) {
    let Some(parent) = context.cache_file.parent() else {
        stats.store_error = Some("cache file has no parent directory".to_string());
        return;
    };
    if let Err(error) = fs::create_dir_all(parent) {
        stats.store_error = Some(format!("{}: {error}", parent.display()));
        return;
    }
    let header = CacheHeader::new(
        env!("CARGO_PKG_VERSION"),
        "performance-ir-cache-abi-1",
        rust_target_label(),
        context.fingerprint.clone(),
    );
    let artifact = match CacheArtifact::from_ir_unit(header, &pipeline.lowering.unit) {
        Ok(artifact) => artifact,
        Err(error) => {
            stats.store_error = Some(error.to_string());
            return;
        }
    };
    let bytes = match artifact.to_bytes() {
        Ok(bytes) => bytes,
        Err(error) => {
            stats.store_error = Some(error.to_string());
            return;
        }
    };
    match fs::write(&context.cache_file, bytes) {
        Ok(()) => stats.wrote = true,
        Err(error) => {
            stats.store_error = Some(format!("{}: {error}", context.cache_file.display()))
        }
    }
}

fn default_bytecode_cache_dir() -> PathBuf {
    PathBuf::from("target/performance/bytecode-cache")
}

fn cache_file_for(cache_dir: &Path, fingerprint: &CacheFingerprint) -> Result<PathBuf, String> {
    if !fingerprint.digest.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err("bytecode cache fingerprint digest is not hex".to_string());
    }
    Ok(cache_dir.join(format!("{}.phbc", fingerprint.digest)))
}

fn clear_bytecode_cache_dir(cache_dir: &Path) -> Result<(), String> {
    match fs::read_dir(cache_dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|error| format!("{}: {error}", cache_dir.display()))?;
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("phbc") {
                    fs::remove_file(&path)
                        .map_err(|error| format!("{}: {error}", path.display()))?;
                }
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("{}: {error}", cache_dir.display())),
    }
}

fn rust_target_label() -> String {
    format!("{}-{}", env::consts::ARCH, env::consts::OS)
}

fn write_cache_stats_json<W: Write>(
    stderr: &mut W,
    stats: &BytecodeCacheStats,
) -> Result<(), String> {
    writeln!(
        stderr,
        "{}",
        to_json_string(&BytecodeCacheStatsEnvelopeJson::from(stats))
    )
    .map_err(|error| error.to_string())
}

#[derive(Serialize)]
struct BytecodeCacheStatsEnvelopeJson<'a> {
    bytecode_cache: BytecodeCacheStatsJson<'a>,
}

impl<'a> From<&'a BytecodeCacheStats> for BytecodeCacheStatsEnvelopeJson<'a> {
    fn from(stats: &'a BytecodeCacheStats) -> Self {
        Self {
            bytecode_cache: BytecodeCacheStatsJson {
                mode: stats.mode.as_str(),
                hit: stats.hit,
                miss: stats.miss,
                wrote: stats.wrote,
                cleared: stats.cleared,
                compile_error: stats.compile_error,
                file: stats
                    .cache_file
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
                load_error: stats.load_error.as_deref(),
                store_error: stats.store_error.as_deref(),
            },
        }
    }
}

#[derive(Serialize)]
struct BytecodeCacheStatsJson<'a> {
    mode: &'a str,
    hit: bool,
    miss: bool,
    wrote: bool,
    cleared: bool,
    compile_error: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    load_error: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    store_error: Option<&'a str>,
}

fn write_counters_json(path: String, counters: &VmCounters) -> Result<(), String> {
    let path = Path::new(&path);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    fs::write(path, counters.to_json()).map_err(|error| format!("{}: {error}", path.display()))
}

fn write_region_profile_json(path: String, profile: &RegionProfile) -> Result<(), String> {
    let path = Path::new(&path);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    fs::write(path, profile.to_json()).map_err(|error| format!("{}: {error}", path.display()))
}

fn load_bytecode_layout_profile(
    options: &RunOptions<'_>,
) -> Result<Option<BytecodeLayoutProfile>, String> {
    let Some(path) = options.bytecode_layout_profile.as_ref() else {
        return Ok(None);
    };
    let text = fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("{path}: {error}"))?;
    let block_entries = json
        .get("block_entries")
        .or_else(|| json.get("dense_block_entry_counts"))
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            format!("{path}: expected object field `block_entries` or `dense_block_entry_counts`")
        })?;
    let mut profile = BytecodeLayoutProfile::default();
    for (key, value) in block_entries {
        let Some(count) = value.as_u64() else {
            return Err(format!(
                "{path}: block entry `{key}` is not a non-negative integer"
            ));
        };
        profile.block_entries.insert(key.clone(), count);
    }
    Ok(Some(profile))
}

fn write_jit_stats_json<W: Write>(
    stderr: &mut W,
    counters: &VmCounters,
    options: &RunOptions<'_>,
    eligibility: &serde_json::Value,
) -> Result<(), String> {
    let dump_clif = options.jit_dump_clif.as_deref().unwrap_or("");
    writeln!(
        stderr,
        "{}",
        to_json_string(&JitStatsEnvelopeJson {
            jit: JitStatsJson {
                mode: options.jit.as_str(),
                threshold: options.jit_threshold,
                eager: options.tiering.jit_eager,
                max_compile_us: options.tiering.jit_max_compile_us,
                max_functions: options.tiering.jit_max_functions,
                blacklist: options.jit_blacklist.as_str(),
                dump_clif,
                compile_attempts: counters.jit_compile_attempts,
                compiled: counters.jit_compiled,
                executed: counters.jit_executed,
                bailouts: counters.jit_bailouts,
                code_bytes: counters.jit_code_bytes,
                compile_time_nanos: counters.jit_compile_time_nanos,
                side_exits: counters.jit_side_exits,
                side_exit_reasons: &counters.jit_side_exit_reasons,
                guard_failures: counters.jit_guard_failures,
                blacklisted_regions: counters.jit_blacklisted_regions,
                blacklist_reasons: &counters.jit_blacklist_reasons,
                tiering_cold_functions: counters.jit_tiering_cold_functions,
                tiering_hot_functions: counters.jit_tiering_hot_functions,
                tiering_eager_functions: counters.jit_tiering_eager_functions,
                tiering_blacklist_rejections: counters.jit_tiering_blacklist_rejections,
                tiering_budget_rejections: counters.jit_tiering_budget_rejections,
                helper_calls: counters.jit_helper_calls,
                fast_path_hits: counters.jit_fast_path_hits,
                packed_fetch_fast_hits: counters.packed_fetch_fast_hits,
                packed_fetch_bounds_exits: counters.packed_fetch_bounds_exits,
                packed_fetch_layout_exits: counters.packed_fetch_layout_exits,
                packed_foreach_sum_fast_hits: counters.packed_foreach_sum_fast_hits,
                packed_foreach_sum_layout_exits: counters.packed_foreach_sum_layout_exits,
                packed_foreach_sum_overflow_exits: counters.packed_foreach_sum_overflow_exits,
                known_call_fast_hits: counters.known_call_fast_hits,
                known_call_guard_exits: counters.known_call_guard_exits,
                known_call_slow_calls: counters.known_call_slow_calls,
                direct_call_hits: counters.direct_call_hits,
                direct_call_fallbacks: counters.direct_call_fallbacks,
                property_load_fast_hits: counters.property_load_fast_hits,
                property_load_guard_exits: counters.property_load_guard_exits,
                property_load_layout_exits: counters.property_load_layout_exits,
                property_load_uninitialized_exits: counters.property_load_uninitialized_exits,
                property_load_slow_calls: counters.property_load_slow_calls,
                string_concat_fast_path_hits: counters.string_concat_fast_path_hits,
                string_concat_fast_path_misses: counters.string_concat_fast_path_misses,
                overflow_exits: counters.jit_overflow_exits,
                slow_path_calls: counters.jit_slow_path_calls,
                compile_cache_hits: counters.jit_compile_cache_hits,
                compile_cache_misses: counters.jit_compile_cache_misses,
                compile_cache_invalidations: counters.jit_compile_cache_invalidations,
                compile_descriptors: counters
                    .jit_compile_descriptors
                    .iter()
                    .map(JitCompileDescriptorJson::from)
                    .collect(),
                eligibility,
            },
        })
    )
    .map_err(|error| error.to_string())
}

#[derive(Serialize)]
struct JitStatsEnvelopeJson<'a> {
    jit: JitStatsJson<'a>,
}

#[derive(Serialize)]
struct JitStatsJson<'a> {
    mode: &'a str,
    threshold: u64,
    eager: bool,
    max_compile_us: u64,
    max_functions: u64,
    blacklist: &'a str,
    dump_clif: &'a str,
    compile_attempts: u64,
    compiled: u64,
    executed: u64,
    bailouts: u64,
    code_bytes: u64,
    compile_time_nanos: u64,
    side_exits: u64,
    side_exit_reasons: &'a BTreeMap<String, u64>,
    guard_failures: u64,
    blacklisted_regions: u64,
    blacklist_reasons: &'a BTreeMap<String, u64>,
    tiering_cold_functions: u64,
    tiering_hot_functions: u64,
    tiering_eager_functions: u64,
    tiering_blacklist_rejections: u64,
    tiering_budget_rejections: u64,
    helper_calls: u64,
    fast_path_hits: u64,
    packed_fetch_fast_hits: u64,
    packed_fetch_bounds_exits: u64,
    packed_fetch_layout_exits: u64,
    packed_foreach_sum_fast_hits: u64,
    packed_foreach_sum_layout_exits: u64,
    packed_foreach_sum_overflow_exits: u64,
    known_call_fast_hits: u64,
    known_call_guard_exits: u64,
    known_call_slow_calls: u64,
    direct_call_hits: u64,
    direct_call_fallbacks: u64,
    property_load_fast_hits: u64,
    property_load_guard_exits: u64,
    property_load_layout_exits: u64,
    property_load_uninitialized_exits: u64,
    property_load_slow_calls: u64,
    string_concat_fast_path_hits: u64,
    string_concat_fast_path_misses: u64,
    overflow_exits: u64,
    slow_path_calls: u64,
    compile_cache_hits: u64,
    compile_cache_misses: u64,
    compile_cache_invalidations: u64,
    compile_descriptors: Vec<JitCompileDescriptorJson<'a>>,
    eligibility: &'a serde_json::Value,
}

#[derive(Serialize)]
struct JitCompileDescriptorJson<'a> {
    function_id: u32,
    function_name: &'a str,
    ir_fingerprint: &'a str,
    code_bytes: u64,
    compile_time_nanos: u64,
    target_isa: &'a str,
    abi_hash: u64,
    config_hash: u64,
}

impl<'a> From<&'a JitCompileDescriptor> for JitCompileDescriptorJson<'a> {
    fn from(descriptor: &'a JitCompileDescriptor) -> Self {
        Self {
            function_id: descriptor.function_id,
            function_name: &descriptor.function_name,
            ir_fingerprint: &descriptor.ir_fingerprint,
            code_bytes: descriptor.code_bytes,
            compile_time_nanos: descriptor.compile_time_nanos,
            target_isa: &descriptor.target_isa,
            abi_hash: descriptor.abi_hash,
            config_hash: descriptor.config_hash,
        }
    }
}

#[cfg(feature = "jit-cranelift")]
fn build_jit_eligibility_json(unit: &php_ir::IrUnit, jit: JitMode) -> serde_json::Value {
    let mut reports = Vec::new();
    if jit.requires_cranelift() {
        for index in 0..unit.functions.len() {
            reports.push(php_jit::analyze_jit_eligibility(
                unit,
                php_ir::FunctionId::new(index as u32),
            ));
        }
    }
    jit_eligibility_reports_json(&reports)
}

#[cfg(not(feature = "jit-cranelift"))]
fn build_jit_eligibility_json(_unit: &php_ir::IrUnit, _jit: JitMode) -> serde_json::Value {
    empty_jit_eligibility_json()
}

#[cfg(feature = "jit-cranelift")]
fn jit_eligibility_reports_json(reports: &[php_jit::JitEligibilityReport]) -> serde_json::Value {
    let considered = reports.len();
    let eligible = reports
        .iter()
        .filter(|report| matches!(report.eligibility, php_jit::JitEligibility::Eligible))
        .count();
    let rejected = reports
        .iter()
        .filter(|report| matches!(report.eligibility, php_jit::JitEligibility::Rejected { .. }))
        .count();
    let unknown = reports
        .iter()
        .filter(|report| matches!(report.eligibility, php_jit::JitEligibility::Unknown { .. }))
        .count();
    let reports = reports
        .iter()
        .map(|report| {
            serde_json::from_str(&report.to_json()).expect("JIT eligibility report JSON is valid")
        })
        .collect::<Vec<serde_json::Value>>();
    serde_json::json!({
        "considered": considered,
        "eligible": eligible,
        "non_eligible": rejected + unknown,
        "rejected": rejected,
        "unknown": unknown,
        "reports": reports,
    })
}

#[cfg(not(feature = "jit-cranelift"))]
fn empty_jit_eligibility_json() -> serde_json::Value {
    serde_json::json!({
        "considered": 0,
        "eligible": 0,
        "non_eligible": 0,
        "rejected": 0,
        "unknown": 0,
        "reports": [],
    })
}

fn write_tiering_stats_json(path: String, stats: &TieringStats) -> Result<(), String> {
    let path = Path::new(&path);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    fs::write(path, stats.to_json()).map_err(|error| format!("{}: {error}", path.display()))
}

fn write_persistent_feedback_stats_json(
    path: String,
    stats: &PersistentFeedbackStats,
) -> Result<(), String> {
    let path = Path::new(&path);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    fs::write(path, stats.to_json()).map_err(|error| format!("{}: {error}", path.display()))
}

fn parse_env_assignment(value: &str) -> Result<(String, String), String> {
    let Some((key, value)) = value.split_once('=') else {
        return Err("run --env requires KEY=VALUE".to_string());
    };
    if key.is_empty() {
        return Err("run --env requires a non-empty key".to_string());
    }
    Ok((key.to_string(), value.to_string()))
}

fn parse_report_args(args: &[String]) -> Result<ReportOptions<'_>, String> {
    let mut path = None;
    let mut format = ReportFormat::Markdown;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if let Some(value) = arg.strip_prefix("--format=") {
            format = parse_report_format(value)?;
        } else if arg == "--format" {
            index += 1;
            let Some(value) = args.get(index) else {
                return Err("report --format requires markdown or html".to_string());
            };
            format = parse_report_format(value)?;
        } else if path.is_none() {
            path = Some(arg);
        } else {
            return Err(format!("unexpected report argument `{arg}`"));
        }
        index += 1;
    }
    let Some(path) = path else {
        return Err("report requires <path.php>".to_string());
    };
    Ok(ReportOptions { path, format })
}

fn parse_report_format(value: &str) -> Result<ReportFormat, String> {
    match value {
        "markdown" | "md" => Ok(ReportFormat::Markdown),
        "html" => Ok(ReportFormat::Html),
        _ => Err(format!(
            "unsupported report format `{value}`; expected markdown or html"
        )),
    }
}

fn region_profile_json_from_env() -> Option<String> {
    env::var("PHRUST_REGION_PROFILE_JSON")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn print_usage<W: Write>(stdout: &mut W) -> Result<(), String> {
    writeln!(
        stdout,
        "Usage:\n  php-vm compile <file> [--json] [--opt-level 0|1|2]\n  php-vm dump-ir <file> [--with-source]\n  php-vm dump-bytecode-patterns <file> [--json]\n  php-vm dump-rule-selection <file> [--json]\n  php-vm dump-dependency-units <file> [--json]\n  php-vm dump-baseline-native-stencil <file> [--json]\n  php-vm dump-copy-patch-stencils <file> [--json]\n  php-vm dump-mid-tier-plan <file> [--json]\n  php-vm dump-cranelift-clif\n  php-vm run [--engine-preset baseline|default|fast|experimental-jit] [--trace] [--trace-runtime] [--env KEY=VALUE] [--bytecode-cache=off|read|write|read-write] [--bytecode-cache-dir <path>] [--bytecode-cache-stats] [--clear-bytecode-cache] [developer engine flags] <file> [-- arg ...]\n  php-vm report <file> [--format markdown|html]\n  php-vm compare <file>\n\nEngine presets:\n  default          managed fast runtime with guarded native tier where available; also accepted as fast\n  baseline         compatibility/debug oracle with adaptive VM features off\n  experimental-jit developer native diagnostics profile using the same guarded tier\n\nAdvanced engine flags (developer diagnostics):\n  --opt-level 0|1|2 --exec-format ir|auto|bytecode --superinstructions off|on --bytecode-layout source|profiled --bytecode-layout-profile <path> --quickening off|on --inline-caches off|on --jit off|noop|cranelift --jit-threshold N --jit-max-compile-us N --jit-max-functions N --jit-eager --jit-blacklist off|on --jit-dump-clif PATH --jit-stats json --tiering off|on --tiering-function-threshold N --tiering-loop-threshold N --tiering-ic-stability-threshold N --tiering-guard-failure-threshold N --tiering-stats-json <path> --persistent-feedback-read <path> --persistent-feedback-stats-json <path> --counters-json <path> --region-profile-json <path>"
    )
    .map_err(|error| error.to_string())
}

fn parser_diagnostics_json<'a>(
    path: &'a str,
    frontend: &'a FrontendResult,
) -> Vec<ParserDiagnosticJson<'a>> {
    frontend
        .parser_diagnostics()
        .iter()
        .map(|diagnostic| ParserDiagnosticJson {
            path,
            id: diagnostic.id.as_str(),
            message: &diagnostic.message,
            span: Some(RangeJson::from_text_range(diagnostic.span)),
        })
        .collect()
}

fn semantic_diagnostics_json<'a>(
    path: &'a str,
    frontend: &'a FrontendResult,
) -> Vec<SemanticDiagnosticJson<'a>> {
    frontend
        .semantic_diagnostics()
        .iter()
        .map(|diagnostic| SemanticDiagnosticJson {
            path,
            id: diagnostic.id().as_str(),
            severity: diagnostic.severity().as_str(),
            message: diagnostic.message(),
            span: diagnostic.span().map(RangeJson::from_text_range),
        })
        .collect()
}

fn collect_bytecode_patterns(dense: &DenseBytecodeUnit) -> BytecodePatternReport {
    let mut report = BytecodePatternReport::default();
    for function in &dense.functions {
        for block in &function.blocks {
            let start = block.first_instruction as usize;
            let end = start + block.instruction_len as usize;
            let Some(instructions) = function.instructions.get(start..end) else {
                continue;
            };
            report.blocks += 1;
            report.instructions += instructions.len() as u64;
            for pair in instructions.windows(2) {
                let key = format!("{} {}", pair[0].opcode.as_str(), pair[1].opcode.as_str());
                *report.pairs.entry(key).or_default() += 1;
            }
            for triple in instructions.windows(3) {
                let key = format!(
                    "{} {} {}",
                    triple[0].opcode.as_str(),
                    triple[1].opcode.as_str(),
                    triple[2].opcode.as_str()
                );
                *report.triples.entry(key).or_default() += 1;
            }
        }
    }
    report
}

fn collect_baseline_native_stencil(dense: &DenseBytecodeUnit) -> BaselineNativeStencilReport {
    let mut report = BaselineNativeStencilReport {
        functions: dense.functions.len() as u64,
        ..BaselineNativeStencilReport::default()
    };
    for function in &dense.functions {
        for block in &function.blocks {
            let start = block.first_instruction as usize;
            let end = start + block.instruction_len as usize;
            let Some(instructions) = function.instructions.get(start..end) else {
                continue;
            };
            report.blocks += 1;
            for instruction in instructions {
                report.instructions += 1;
                *report
                    .opcode_counts
                    .entry(instruction.opcode.as_str().to_string())
                    .or_default() += 1;
                let class = classify_baseline_stencil_instruction(instruction.opcode);
                report.helper_calls += class.helper_calls;
                report.deopt_slots += class.deopt_slots;
                report.compile_cost_units += class.compile_cost_units;
                report.code_size_bytes_estimate += class.code_size_bytes_estimate;
                if let Some(reason) = class.unsupported_reason {
                    report.unsupported_instructions += 1;
                    *report
                        .unsupported_by_reason
                        .entry(reason.to_string())
                        .or_default() += 1;
                } else {
                    report.stencilable_instructions += 1;
                }
            }
        }
    }
    report
}

fn classify_baseline_stencil_instruction(opcode: DenseOpcode) -> BaselineStencilClass {
    match opcode {
        DenseOpcode::Nop => BaselineStencilClass {
            helper_calls: 0,
            deopt_slots: 0,
            compile_cost_units: 1,
            code_size_bytes_estimate: 1,
            unsupported_reason: None,
        },
        DenseOpcode::LoadConst
        | DenseOpcode::Move
        | DenseOpcode::LoadLocal
        | DenseOpcode::StoreLocal
        | DenseOpcode::StoreLocalDiscard
        | DenseOpcode::LoadConstEcho
        | DenseOpcode::LoadLocalEcho
        | DenseOpcode::Echo
        | DenseOpcode::Return
        | DenseOpcode::Exit
        | DenseOpcode::Discard => BaselineStencilClass {
            helper_calls: 0,
            deopt_slots: 1,
            compile_cost_units: 1,
            code_size_bytes_estimate: 8,
            unsupported_reason: None,
        },
        DenseOpcode::Jump
        | DenseOpcode::JumpIfFalse
        | DenseOpcode::JumpIfTrue
        | DenseOpcode::JumpIf => BaselineStencilClass {
            helper_calls: 0,
            deopt_slots: 1,
            compile_cost_units: 2,
            code_size_bytes_estimate: 12,
            unsupported_reason: None,
        },
        DenseOpcode::BinaryAdd
        | DenseOpcode::BinarySub
        | DenseOpcode::BinaryMul
        | DenseOpcode::BinaryDiv
        | DenseOpcode::BinaryMod
        | DenseOpcode::BinaryConcat
        | DenseOpcode::BinaryPow
        | DenseOpcode::BinaryBitAnd
        | DenseOpcode::BinaryBitOr
        | DenseOpcode::BinaryBitXor
        | DenseOpcode::BinaryShiftLeft
        | DenseOpcode::BinaryShiftRight
        | DenseOpcode::CompareEqual
        | DenseOpcode::CompareNotEqual
        | DenseOpcode::CompareIdentical
        | DenseOpcode::CompareNotIdentical
        | DenseOpcode::CompareLess
        | DenseOpcode::CompareLessEqual
        | DenseOpcode::CompareGreater
        | DenseOpcode::CompareGreaterEqual
        | DenseOpcode::CompareSpaceship
        | DenseOpcode::UnaryPlus
        | DenseOpcode::UnaryMinus
        | DenseOpcode::UnaryNot
        | DenseOpcode::UnaryBitNot
        | DenseOpcode::BinaryConcatEcho => BaselineStencilClass {
            helper_calls: 1,
            deopt_slots: 1,
            compile_cost_units: 3,
            code_size_bytes_estimate: 16,
            unsupported_reason: None,
        },
        DenseOpcode::CallFunction | DenseOpcode::CallMethod | DenseOpcode::CallStaticMethod => {
            BaselineStencilClass {
                helper_calls: 1,
                deopt_slots: 1,
                compile_cost_units: 5,
                code_size_bytes_estimate: 0,
                unsupported_reason: Some("call_frame_and_userland_side_effect_state"),
            }
        }
        DenseOpcode::FetchProperty | DenseOpcode::AssignProperty => BaselineStencilClass {
            helper_calls: 1,
            deopt_slots: 1,
            compile_cost_units: 5,
            code_size_bytes_estimate: 0,
            unsupported_reason: None,
        },
        DenseOpcode::NewArray
        | DenseOpcode::ArrayInsert
        | DenseOpcode::FetchDim
        | DenseOpcode::AssignDim
        | DenseOpcode::AppendDim => BaselineStencilClass {
            helper_calls: 1,
            deopt_slots: 1,
            compile_cost_units: 5,
            code_size_bytes_estimate: 0,
            unsupported_reason: Some("array_reference_cow_and_key_state"),
        },
        DenseOpcode::ForeachInit | DenseOpcode::ForeachNext => BaselineStencilClass {
            helper_calls: 1,
            deopt_slots: 1,
            compile_cost_units: 5,
            code_size_bytes_estimate: 0,
            unsupported_reason: Some("foreach_iterator_state"),
        },
    }
}

fn collect_copy_patch_stencils(
    dense: &DenseBytecodeUnit,
    quickened_superinstructions: u64,
) -> CopyPatchStencilReport {
    let mut report = CopyPatchStencilReport {
        functions: dense.functions.len() as u64,
        quickened_superinstructions,
        ..CopyPatchStencilReport::default()
    };
    for (function_index, function) in dense.functions.iter().enumerate() {
        for block in &function.blocks {
            let start = block.first_instruction as usize;
            let end = start + block.instruction_len as usize;
            let Some(instructions) = function.instructions.get(start..end) else {
                continue;
            };
            report.blocks += 1;
            for (offset, instruction) in instructions.iter().enumerate() {
                report.instructions += 1;
                let instruction_index = start + offset;
                let class = classify_copy_patch_stencil_instruction(
                    dense,
                    instruction.opcode,
                    &instruction.operands,
                );
                report.patch_sites += class.patch_sites.len() as u64;
                report.helper_calls += class.helper_calls.len() as u64;
                report.live_state_slots += class.live_state_requirements.len() as u64;
                if class.side_exit_target != "none" {
                    report.deopt_points += 1;
                }
                report.compile_cost_units += class.compile_cost_units;
                report.code_size_bytes_estimate += class.code_size_bytes_estimate;
                if let Some(reason) = class.unsupported_reason {
                    report.unsupported_instructions += 1;
                    *report
                        .unsupported_by_reason
                        .entry(reason.to_string())
                        .or_default() += 1;
                    continue;
                }
                *report
                    .stencil_kinds
                    .entry(class.kind.to_string())
                    .or_default() += 1;
                report.stencils.push(CopyPatchStencil {
                    function: function_index as u32,
                    block: block.id,
                    instruction: instruction_index as u32,
                    opcode: instruction.opcode.as_str(),
                    kind: class.kind,
                    patch_sites: class.patch_sites.to_vec(),
                    guard_dependencies: class.guard_dependencies.to_vec(),
                    helper_calls: class.helper_calls.to_vec(),
                    live_state_requirements: class.live_state_requirements.to_vec(),
                    side_exit_target: class.side_exit_target,
                    code_size_bytes_estimate: class.code_size_bytes_estimate,
                    compile_cost_units: class.compile_cost_units,
                });
            }
        }
    }
    if !dense
        .functions
        .iter()
        .flat_map(|function| &function.instructions)
        .any(|instruction| matches!(instruction.opcode, DenseOpcode::FetchProperty))
    {
        report
            .unsupported_by_reason
            .entry("object_shape_property_load_dense_opcode_absent".to_string())
            .or_insert(1);
    }
    report
}

fn classify_copy_patch_stencil_instruction(
    dense: &DenseBytecodeUnit,
    opcode: DenseOpcode,
    operands: &DenseOperands,
) -> CopyPatchStencilClass {
    match opcode {
        DenseOpcode::LoadLocal | DenseOpcode::LoadLocalEcho => CopyPatchStencilClass {
            kind: "load_local",
            patch_sites: &["frame_local_slot", "destination_register"],
            guard_dependencies: &["frame_layout_epoch"],
            helper_calls: &[],
            live_state_requirements: &["destination_register", "source_span"],
            side_exit_target: "none",
            code_size_bytes_estimate: 8,
            compile_cost_units: 1,
            unsupported_reason: None,
        },
        DenseOpcode::BinaryAdd | DenseOpcode::BinarySub | DenseOpcode::BinaryMul => {
            let helper_calls: &'static [&'static str] = match opcode {
                DenseOpcode::BinaryAdd => &["phrust_jit_i64_add_checked"],
                DenseOpcode::BinaryMul => &["phrust_jit_i64_mul_checked"],
                _ => &["inline_i64_sub_checked"],
            };
            CopyPatchStencilClass {
                kind: "guarded_int_arithmetic",
                patch_sites: &[
                    "lhs_register",
                    "rhs_register",
                    "destination_register",
                    "overflow_exit",
                ],
                guard_dependencies: &["lhs_is_int", "rhs_is_int"],
                helper_calls,
                live_state_requirements: &[
                    "operand_registers",
                    "destination_register",
                    "source_span",
                    "resume_instruction",
                ],
                side_exit_target: "interpreter_overflow_or_type_exit",
                code_size_bytes_estimate: 32,
                compile_cost_units: 3,
                unsupported_reason: None,
            }
        }
        DenseOpcode::FetchDim => CopyPatchStencilClass {
            kind: "packed_array_guard_fetch",
            patch_sites: &[
                "array_register",
                "key_register",
                "destination_register",
                "oob_exit",
            ],
            guard_dependencies: &["array_is_packed", "key_is_int", "no_by_ref_element"],
            helper_calls: &[
                "php_jit_array_is_packed_ints",
                "php_jit_array_fetch_int_slow",
            ],
            live_state_requirements: &[
                "array_value",
                "key_value",
                "destination_register",
                "diagnostic_order",
                "resume_instruction",
            ],
            side_exit_target: "interpreter_array_fetch_exit",
            code_size_bytes_estimate: 48,
            compile_cost_units: 5,
            unsupported_reason: None,
        },
        DenseOpcode::FetchProperty => CopyPatchStencilClass {
            kind: "guarded_property_fetch",
            patch_sites: &[
                "object_register",
                "property_name",
                "destination_register",
                "shape_guard_exit",
            ],
            guard_dependencies: &[
                "receiver_class_epoch",
                "property_layout_epoch",
                "visibility_scope",
            ],
            helper_calls: &["php_jit_property_fetch_slow"],
            live_state_requirements: &[
                "object_value",
                "destination_register",
                "diagnostic_order",
                "resume_instruction",
            ],
            side_exit_target: "interpreter_property_fetch_exit",
            code_size_bytes_estimate: 48,
            compile_cost_units: 5,
            unsupported_reason: None,
        },
        DenseOpcode::AssignProperty => CopyPatchStencilClass {
            kind: "guarded_property_assignment",
            patch_sites: &[
                "object_register",
                "value_register",
                "property_name",
                "shape_guard_exit",
            ],
            guard_dependencies: &[
                "receiver_class_epoch",
                "property_layout_epoch",
                "visibility_scope",
                "property_type",
            ],
            helper_calls: &["php_jit_property_assign_slow"],
            live_state_requirements: &[
                "object_value",
                "assigned_value",
                "diagnostic_order",
                "resume_instruction",
            ],
            side_exit_target: "interpreter_property_assign_exit",
            code_size_bytes_estimate: 56,
            compile_cost_units: 6,
            unsupported_reason: None,
        },
        DenseOpcode::CallFunction if is_known_builtin_copy_patch_call(dense, operands) => {
            CopyPatchStencilClass {
                kind: "known_builtin_call",
                patch_sites: &[
                    "function_symbol",
                    "argument_registers",
                    "destination_register",
                ],
                guard_dependencies: &["function_table_epoch", "builtin_identity", "argument_shape"],
                helper_calls: &["phrust_jit_strlen_known_or_count_known"],
                live_state_requirements: &[
                    "call_destination",
                    "arguments",
                    "diagnostic_order",
                    "resume_instruction",
                ],
                side_exit_target: "interpreter_builtin_fallback_exit",
                code_size_bytes_estimate: 40,
                compile_cost_units: 5,
                unsupported_reason: None,
            }
        }
        DenseOpcode::JumpIfFalse | DenseOpcode::JumpIfTrue | DenseOpcode::JumpIf => {
            CopyPatchStencilClass {
                kind: "branch_guard",
                patch_sites: &["condition_register", "taken_target", "fallthrough_target"],
                guard_dependencies: &["condition_is_bool", "branch_bias_feedback"],
                helper_calls: &[],
                live_state_requirements: &["condition_value", "source_span", "resume_block"],
                side_exit_target: "interpreter_branch_type_exit",
                code_size_bytes_estimate: 16,
                compile_cost_units: 2,
                unsupported_reason: None,
            }
        }
        DenseOpcode::Return => CopyPatchStencilClass {
            kind: "return",
            patch_sites: &["return_value", "caller_resume"],
            guard_dependencies: &["frame_is_current"],
            helper_calls: &[],
            live_state_requirements: &["return_value", "caller_frame", "destructor_order"],
            side_exit_target: "interpreter_return_slow_exit",
            code_size_bytes_estimate: 16,
            compile_cost_units: 2,
            unsupported_reason: None,
        },
        DenseOpcode::Exit => unsupported_copy_patch_class("script_exit_requires_request_state"),
        DenseOpcode::LoadConst
        | DenseOpcode::Move
        | DenseOpcode::StoreLocal
        | DenseOpcode::StoreLocalDiscard
        | DenseOpcode::LoadConstEcho
        | DenseOpcode::Echo
        | DenseOpcode::Discard
        | DenseOpcode::Nop => CopyPatchStencilClass {
            kind: "simple_value_move_or_output",
            patch_sites: &["value_slot"],
            guard_dependencies: &["frame_layout_epoch"],
            helper_calls: &[],
            live_state_requirements: &["source_span"],
            side_exit_target: "none",
            code_size_bytes_estimate: 8,
            compile_cost_units: 1,
            unsupported_reason: None,
        },
        DenseOpcode::Jump => CopyPatchStencilClass {
            kind: "direct_branch",
            patch_sites: &["target_block"],
            guard_dependencies: &["block_layout_epoch"],
            helper_calls: &[],
            live_state_requirements: &["resume_block"],
            side_exit_target: "none",
            code_size_bytes_estimate: 8,
            compile_cost_units: 1,
            unsupported_reason: None,
        },
        DenseOpcode::CallFunction | DenseOpcode::CallMethod | DenseOpcode::CallStaticMethod => {
            unsupported_copy_patch_class("dynamic_or_userland_call_requires_frame_and_symbol_state")
        }
        DenseOpcode::NewArray
        | DenseOpcode::ArrayInsert
        | DenseOpcode::AssignDim
        | DenseOpcode::AppendDim => unsupported_copy_patch_class(
            "array_mutation_requires_reference_cow_and_allocator_state",
        ),
        DenseOpcode::ForeachInit | DenseOpcode::ForeachNext => {
            unsupported_copy_patch_class("foreach_requires_iterator_mutation_and_resume_state")
        }
        DenseOpcode::BinaryDiv
        | DenseOpcode::BinaryMod
        | DenseOpcode::BinaryConcat
        | DenseOpcode::BinaryPow
        | DenseOpcode::BinaryBitAnd
        | DenseOpcode::BinaryBitOr
        | DenseOpcode::BinaryBitXor
        | DenseOpcode::BinaryShiftLeft
        | DenseOpcode::BinaryShiftRight
        | DenseOpcode::CompareEqual
        | DenseOpcode::CompareNotEqual
        | DenseOpcode::CompareIdentical
        | DenseOpcode::CompareNotIdentical
        | DenseOpcode::CompareLess
        | DenseOpcode::CompareLessEqual
        | DenseOpcode::CompareGreater
        | DenseOpcode::CompareGreaterEqual
        | DenseOpcode::CompareSpaceship
        | DenseOpcode::UnaryPlus
        | DenseOpcode::UnaryMinus
        | DenseOpcode::UnaryNot
        | DenseOpcode::UnaryBitNot
        | DenseOpcode::BinaryConcatEcho => {
            unsupported_copy_patch_class("opcode_needs_php_semantic_helper_or_string_state")
        }
    }
}

fn unsupported_copy_patch_class(reason: &'static str) -> CopyPatchStencilClass {
    CopyPatchStencilClass {
        kind: "unsupported",
        patch_sites: &[],
        guard_dependencies: &[],
        helper_calls: &[],
        live_state_requirements: &[],
        side_exit_target: "none",
        code_size_bytes_estimate: 0,
        compile_cost_units: 1,
        unsupported_reason: Some(reason),
    }
}

fn is_known_builtin_copy_patch_call(dense: &DenseBytecodeUnit, operands: &DenseOperands) -> bool {
    let DenseOperands::Call { name, .. } = operands else {
        return false;
    };
    dense
        .names
        .get(*name as usize)
        .map(|name| matches!(name.to_ascii_lowercase().as_str(), "strlen" | "count"))
        .unwrap_or(false)
}

fn collect_mid_tier_plan(
    dense: &DenseBytecodeUnit,
    quickened_superinstructions: u64,
) -> MidTierPlanReport {
    let mut report = MidTierPlanReport {
        quickened_superinstructions,
        ..MidTierPlanReport::default()
    };
    for (function_index, function) in dense.functions.iter().enumerate() {
        let mut plan = MidTierFunctionPlan {
            function: function_index as u32,
            ..MidTierFunctionPlan::default()
        };
        for instruction in &function.instructions {
            plan.instruction_count += 1;
            classify_mid_tier_instruction(
                dense,
                instruction.opcode,
                &instruction.operands,
                &mut plan,
            );
        }
        if plan.instruction_count <= 24
            && plan.rejection_reasons.is_empty()
            && !plan
                .candidate_optimizations
                .contains(&"tiny_leaf_method_inlining_candidate")
        {
            plan.candidate_optimizations
                .push("tiny_leaf_method_inlining_candidate");
        }
        if plan.rejection_reasons.is_empty() && !plan.candidate_optimizations.is_empty() {
            plan.classification = "eligible";
            report.eligible_functions += 1;
        } else {
            plan.classification = "ineligible";
            report.rejected_functions += 1;
        }
        plan.candidate_optimizations.sort_unstable();
        plan.candidate_optimizations.dedup();
        plan.rejection_reasons.sort_unstable();
        plan.rejection_reasons.dedup();
        plan.expected_guards.sort_unstable();
        plan.expected_guards.dedup();
        plan.required_helpers.sort_unstable();
        plan.required_helpers.dedup();

        for value in &plan.candidate_optimizations {
            *report
                .candidate_optimizations
                .entry((*value).to_string())
                .or_default() += 1;
        }
        for value in &plan.rejection_reasons {
            *report
                .rejection_reasons
                .entry((*value).to_string())
                .or_default() += 1;
        }
        for value in &plan.expected_guards {
            *report
                .expected_guards
                .entry((*value).to_string())
                .or_default() += 1;
        }
        for value in &plan.required_helpers {
            *report
                .required_helpers
                .entry((*value).to_string())
                .or_default() += 1;
        }
        report.deopt_points += plan.deopt_points;
        report.functions.push(plan);
    }
    if !report
        .candidate_optimizations
        .contains_key("method_property_shape_check_hoisting")
    {
        report
            .rejection_reasons
            .entry("method_property_shape_metadata_missing".to_string())
            .or_insert(1);
    }
    for reason in [
        "eval_include_mutation_requires_invalidation",
        "exceptions_try_finally_need_live_state_support",
        "generators_fibers_require_suspend_state",
        "destructor_sensitive_values_need_materialization",
    ] {
        report
            .rejection_reasons
            .entry(reason.to_string())
            .or_insert(1);
    }
    report
}

fn classify_mid_tier_instruction(
    dense: &DenseBytecodeUnit,
    opcode: DenseOpcode,
    operands: &DenseOperands,
    plan: &mut MidTierFunctionPlan,
) {
    match opcode {
        DenseOpcode::BinaryAdd | DenseOpcode::BinarySub | DenseOpcode::BinaryMul => {
            push_unique(
                &mut plan.candidate_optimizations,
                "numeric_string_guard_specialization",
            );
            push_unique(&mut plan.expected_guards, "int_or_numeric_string_operands");
            push_unique(&mut plan.expected_guards, "overflow_precision_guard");
            plan.deopt_points += 1;
        }
        DenseOpcode::FetchDim => {
            push_unique(
                &mut plan.candidate_optimizations,
                "packed_array_loop_specialization",
            );
            push_unique(&mut plan.expected_guards, "packed_array_shape");
            push_unique(&mut plan.expected_guards, "integer_array_key");
            push_unique(&mut plan.required_helpers, "php_jit_array_fetch_int_slow");
            plan.deopt_points += 1;
        }
        DenseOpcode::FetchProperty => {
            push_unique(
                &mut plan.candidate_optimizations,
                "property_shape_guard_specialization",
            );
            push_unique(&mut plan.expected_guards, "receiver_class_epoch");
            push_unique(&mut plan.expected_guards, "property_layout_epoch");
            push_unique(&mut plan.required_helpers, "php_jit_property_fetch_slow");
            plan.deopt_points += 1;
        }
        DenseOpcode::AssignProperty => {
            push_unique(
                &mut plan.candidate_optimizations,
                "property_assignment_guard_specialization",
            );
            push_unique(&mut plan.expected_guards, "receiver_class_epoch");
            push_unique(&mut plan.expected_guards, "property_layout_epoch");
            push_unique(&mut plan.expected_guards, "property_type");
            push_unique(&mut plan.required_helpers, "php_jit_property_assign_slow");
            plan.deopt_points += 1;
        }
        DenseOpcode::CallFunction if is_known_builtin_copy_patch_call(dense, operands) => {
            push_unique(
                &mut plan.candidate_optimizations,
                "builtin_intrinsic_inlining",
            );
            push_unique(&mut plan.expected_guards, "builtin_identity");
            push_unique(&mut plan.expected_guards, "argument_shape");
            push_unique(&mut plan.required_helpers, "known_builtin_helper");
            plan.deopt_points += 1;
        }
        DenseOpcode::CallFunction | DenseOpcode::CallMethod | DenseOpcode::CallStaticMethod => {
            push_unique(&mut plan.rejection_reasons, "magic_hooks_or_dynamic_calls");
            plan.deopt_points += 1;
        }
        DenseOpcode::JumpIfFalse | DenseOpcode::JumpIfTrue | DenseOpcode::JumpIf => {
            push_unique(&mut plan.candidate_optimizations, "branch_layout");
            push_unique(&mut plan.expected_guards, "branch_bias_feedback");
            push_unique(&mut plan.expected_guards, "bool_condition");
            plan.deopt_points += 1;
        }
        DenseOpcode::NewArray
        | DenseOpcode::ArrayInsert
        | DenseOpcode::AssignDim
        | DenseOpcode::AppendDim => {
            push_unique(&mut plan.rejection_reasons, "cow_mutation_ambiguity");
            push_unique(
                &mut plan.rejection_reasons,
                "references_or_unknown_aliasing",
            );
            plan.deopt_points += 1;
        }
        DenseOpcode::ForeachInit | DenseOpcode::ForeachNext => {
            push_unique(&mut plan.rejection_reasons, "cow_mutation_ambiguity");
            plan.deopt_points += 1;
        }
        DenseOpcode::BinaryConcat | DenseOpcode::BinaryConcatEcho | DenseOpcode::Echo => {
            push_unique(
                &mut plan.candidate_optimizations,
                "allocation_scratch_buffer_elision",
            );
            push_unique(&mut plan.expected_guards, "string_or_output_buffer_state");
            plan.deopt_points += 1;
        }
        DenseOpcode::Return => {
            push_unique(
                &mut plan.expected_guards,
                "destructor_sensitive_value_state",
            );
            plan.deopt_points += 1;
        }
        DenseOpcode::Exit => {
            push_unique(&mut plan.rejection_reasons, "script_exit_control_flow");
            plan.deopt_points += 1;
        }
        DenseOpcode::LoadConst
        | DenseOpcode::Move
        | DenseOpcode::LoadLocal
        | DenseOpcode::StoreLocal
        | DenseOpcode::StoreLocalDiscard
        | DenseOpcode::LoadConstEcho
        | DenseOpcode::LoadLocalEcho
        | DenseOpcode::Jump
        | DenseOpcode::Discard
        | DenseOpcode::Nop => {}
        DenseOpcode::BinaryDiv
        | DenseOpcode::BinaryMod
        | DenseOpcode::BinaryPow
        | DenseOpcode::BinaryBitAnd
        | DenseOpcode::BinaryBitOr
        | DenseOpcode::BinaryBitXor
        | DenseOpcode::BinaryShiftLeft
        | DenseOpcode::BinaryShiftRight
        | DenseOpcode::CompareEqual
        | DenseOpcode::CompareNotEqual
        | DenseOpcode::CompareIdentical
        | DenseOpcode::CompareNotIdentical
        | DenseOpcode::CompareLess
        | DenseOpcode::CompareLessEqual
        | DenseOpcode::CompareGreater
        | DenseOpcode::CompareGreaterEqual
        | DenseOpcode::CompareSpaceship
        | DenseOpcode::UnaryPlus
        | DenseOpcode::UnaryMinus
        | DenseOpcode::UnaryNot
        | DenseOpcode::UnaryBitNot => {
            push_unique(&mut plan.expected_guards, "php_scalar_semantics");
            plan.deopt_points += 1;
        }
    }
}

fn push_unique(values: &mut Vec<&'static str>, value: &'static str) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn bytecode_patterns_json(
    path: &str,
    dense: &DenseBytecodeUnit,
    report: &BytecodePatternReport,
) -> String {
    to_json_string(&BytecodePatternsJson {
        ok: true,
        path,
        functions: dense.functions.len(),
        blocks: report.blocks,
        instructions: report.instructions,
        pairs: &report.pairs,
        triples: &report.triples,
    })
}

fn rule_selection_json(
    path: &str,
    dense: &DenseBytecodeUnit,
    report: &php_ir::RuleSelectionReport,
) -> String {
    to_json_string(&serde_json::json!({
        "ok": true,
        "path": path,
        "functions": dense.functions.len(),
        "rule_selection_candidates": report.rule_selection_candidates,
        "rule_selection_selected": report.rule_selection_selected,
        "rule_selection_fused": report.rule_selection_fused,
        "rule_selection_skipped": report.rule_selection_skipped,
        "rule_selection_by_kind": report.rule_selection_by_kind,
        "selections": report.selections.iter().map(|selection| {
            serde_json::json!({
                "id": selection.id.raw(),
                "kind": selection.kind.as_str(),
                "source_indexes": &selection.source_indexes,
                "parent": selection.parent.map(php_ir::RuleId::raw),
                "reason": &selection.reason,
                "operand_constraints": selection.operand_constraints.iter().map(|constraint| {
                    serde_json::json!({
                        "operand_index": constraint.operand_index,
                        "constraint": &constraint.constraint,
                    })
                }).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    }))
}

#[derive(Serialize)]
struct BytecodePatternsJson<'a> {
    ok: bool,
    path: &'a str,
    functions: usize,
    blocks: u64,
    instructions: u64,
    pairs: &'a BTreeMap<String, u64>,
    triples: &'a BTreeMap<String, u64>,
}

fn baseline_native_stencil_json(
    path: &str,
    dense: &DenseBytecodeUnit,
    report: &BaselineNativeStencilReport,
) -> String {
    to_json_string(&BaselineNativeStencilJson {
        ok: true,
        schema_version: 1,
        backend: "baseline-native-stencil",
        status: "no-exec",
        native_execution: false,
        executable_memory: false,
        path,
        dense_bytecode_version: dense.version,
        functions: report.functions,
        blocks: report.blocks,
        instructions: report.instructions,
        stencilable_instructions: report.stencilable_instructions,
        unsupported_instructions: report.unsupported_instructions,
        helper_calls_estimate: report.helper_calls,
        required_deopt_slots: report.deopt_slots,
        compile_cost_units: report.compile_cost_units,
        code_size_bytes_estimate: report.code_size_bytes_estimate,
        cache_policy: "no native cache; future cache must key ABI/config/ISA/epoch",
        opcode_counts: &report.opcode_counts,
        unsupported_by_reason: &report.unsupported_by_reason,
    })
}

#[derive(Serialize)]
struct BaselineNativeStencilJson<'a> {
    ok: bool,
    schema_version: u64,
    backend: &'a str,
    status: &'a str,
    native_execution: bool,
    executable_memory: bool,
    path: &'a str,
    dense_bytecode_version: u32,
    functions: u64,
    blocks: u64,
    instructions: u64,
    stencilable_instructions: u64,
    unsupported_instructions: u64,
    helper_calls_estimate: u64,
    required_deopt_slots: u64,
    compile_cost_units: u64,
    code_size_bytes_estimate: u64,
    cache_policy: &'a str,
    opcode_counts: &'a BTreeMap<String, u64>,
    unsupported_by_reason: &'a BTreeMap<String, u64>,
}

fn copy_patch_stencils_json(
    path: &str,
    dense: &DenseBytecodeUnit,
    report: &CopyPatchStencilReport,
) -> String {
    let stencils: Vec<_> = report
        .stencils
        .iter()
        .map(|stencil| {
            serde_json::json!({
                "function": stencil.function,
                "block": stencil.block,
                "instruction": stencil.instruction,
                "opcode": stencil.opcode,
                "instruction_kind": stencil.kind,
                "patch_sites": stencil.patch_sites,
                "guard_dependencies": stencil.guard_dependencies,
                "helper_calls": stencil.helper_calls,
                "live_state_requirements": stencil.live_state_requirements,
                "side_exit_target": stencil.side_exit_target,
                "code_size_bytes_estimate": stencil.code_size_bytes_estimate,
                "compile_cost_units": stencil.compile_cost_units,
            })
        })
        .collect();
    to_json_string(&serde_json::json!({
        "ok": true,
        "schema_version": 1,
        "backend": "copy-patch-stencil",
        "status": "no-exec",
        "native_execution": false,
        "executable_memory": false,
        "path": path,
        "dense_bytecode_version": dense.version,
        "input": "quickened dense bytecode metadata",
        "functions": report.functions,
        "blocks": report.blocks,
        "instructions": report.instructions,
        "quickened_superinstructions": report.quickened_superinstructions,
        "stencil_count": report.stencils.len(),
        "unsupported_instructions": report.unsupported_instructions,
        "estimated_code_size_bytes": report.code_size_bytes_estimate,
        "patch_sites": report.patch_sites,
        "helper_calls": report.helper_calls,
        "live_state_slots": report.live_state_slots,
        "deopt_points": report.deopt_points,
        "compile_cost_units": report.compile_cost_units,
        "work_to_compile_ratio": report.work_to_compile_ratio(),
        "stencil_kinds": report.stencil_kinds,
        "unsupported_by_reason": report.unsupported_by_reason,
        "stencils": stencils,
    }))
}

fn mid_tier_plan_json(path: &str, dense: &DenseBytecodeUnit, report: &MidTierPlanReport) -> String {
    let functions: Vec<_> = report
        .functions
        .iter()
        .map(|function| {
            serde_json::json!({
                "function": function.function,
                "instruction_count": function.instruction_count,
                "classification": function.classification,
                "candidate_optimizations": function.candidate_optimizations,
                "rejection_reasons": function.rejection_reasons,
                "expected_guards": function.expected_guards,
                "required_helpers": function.required_helpers,
                "deopt_points": function.deopt_points,
            })
        })
        .collect();
    to_json_string(&serde_json::json!({
        "ok": true,
        "schema_version": 1,
        "backend": "php-mid-tier-plan",
        "status": "metadata-only",
        "native_execution": false,
        "executable_memory": false,
        "path": path,
        "dense_bytecode_version": dense.version,
        "tier_kind": "PHP-semantics-aware-mid-tier",
        "input_metadata": [
            "quickened_dense_bytecode",
            "inline_cache_feedback",
            "array_object_shapes",
            "numeric_string_classifications",
            "alias_reference_state",
            "branch_bias",
            "persistent_feedback",
            "deopt_live_state_maps"
        ],
        "output": "pseudo-ir-or-report-only",
        "quickened_superinstructions": report.quickened_superinstructions,
        "eligible_functions": report.eligible_functions,
        "rejected_functions": report.rejected_functions,
        "deopt_points": report.deopt_points,
        "candidate_optimizations": report.candidate_optimizations,
        "rejection_reasons": report.rejection_reasons,
        "expected_guards": report.expected_guards,
        "required_helpers": report.required_helpers,
        "functions": functions,
    }))
}

fn lowering_diagnostics_json<'a>(
    path: &'a str,
    lowering: &'a php_ir::LoweringResult,
) -> Vec<LoweringDiagnosticJson<'a>> {
    lowering
        .diagnostics
        .iter()
        .map(|diagnostic| LoweringDiagnosticJson {
            path,
            id: &diagnostic.id,
            message: &diagnostic.message,
            span: RangeJson {
                start: diagnostic.span.start as usize,
                end: diagnostic.span.end as usize,
            },
        })
        .collect()
}

fn verification_diagnostics_json(
    lowering: &php_ir::LoweringResult,
) -> Vec<VerificationDiagnosticJson<'_>> {
    match &lowering.verification {
        Ok(()) => Vec::new(),
        Err(errors) => errors
            .iter()
            .map(|error| VerificationDiagnosticJson {
                id: error.diagnostic_id(),
                message: &error.message,
            })
            .collect(),
    }
}

fn render_markdown_report(pipeline: &Pipeline, vm_result: Option<&VmResult>) -> String {
    let mut out = String::new();
    out.push_str("# PHP VM Report\n\n");
    out.push_str("## Source\n\n");
    out.push_str("- Path: `");
    out.push_str(&pipeline.path);
    out.push_str("`\n");
    out.push_str("- Source bytes: ");
    out.push_str(&pipeline.source.len().to_string());
    out.push_str("\n\n");
    push_fenced_block(&mut out, "php", &pipeline.source);

    out.push_str("## Diagnostics\n\n");
    push_diagnostics_markdown(&mut out, pipeline);

    out.push_str("## HIR Summary\n\n");
    push_hir_summary_markdown(&mut out, pipeline);

    out.push_str("## IR Dump\n\n");
    push_fenced_block(&mut out, "text", &pipeline.lowering.unit.to_snapshot_text());

    out.push_str("## VM Output\n\n");
    match vm_result {
        Some(result) => push_fenced_block(&mut out, "text", &result.output.to_string_lossy()),
        None => {
            out.push_str("VM execution skipped because compile-time diagnostics are present.\n\n")
        }
    }

    out.push_str("## Runtime Diagnostics\n\n");
    push_runtime_diagnostics_markdown(&mut out, vm_result);

    out.push_str("## Known-Gap Status\n\n");
    push_known_gap_status_markdown(&mut out, pipeline, vm_result);
    out
}

fn render_html_report(pipeline: &Pipeline, vm_result: Option<&VmResult>) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<title>PHP VM Report</title>\n");
    out.push_str("<style>body{font-family:system-ui,sans-serif;line-height:1.4;margin:2rem;max-width:72rem}pre{background:#f5f5f5;padding:1rem;overflow:auto}code{background:#f5f5f5;padding:.1rem .2rem}</style>\n");
    out.push_str("</head>\n<body>\n");
    out.push_str("<h1>PHP VM Report</h1>\n");
    html_section_with_pre(&mut out, "Source", &pipeline.source);
    html_section_with_pre(&mut out, "Diagnostics", &diagnostics_text(pipeline));
    html_section_with_pre(&mut out, "HIR Summary", &hir_summary_text(pipeline));
    html_section_with_pre(
        &mut out,
        "IR Dump",
        &pipeline.lowering.unit.to_snapshot_text(),
    );
    let output = vm_result
        .map(|result| result.output.to_string_lossy())
        .unwrap_or_else(|| {
            "VM execution skipped because compile-time diagnostics are present.".to_string()
        });
    html_section_with_pre(&mut out, "VM Output", &output);
    html_section_with_pre(
        &mut out,
        "Runtime Diagnostics",
        &runtime_diagnostics_text(vm_result),
    );
    html_section_with_pre(
        &mut out,
        "Known-Gap Status",
        &known_gap_status_text(pipeline, vm_result),
    );
    out.push_str("</body>\n</html>\n");
    out
}

fn push_diagnostics_markdown(out: &mut String, pipeline: &Pipeline) {
    let text = diagnostics_text(pipeline);
    if text == "none" {
        out.push_str("none\n\n");
    } else {
        push_fenced_block(out, "text", &text);
    }
}

fn diagnostics_text(pipeline: &Pipeline) -> String {
    let mut lines = Vec::new();
    for diagnostic in pipeline.frontend.parser_diagnostics() {
        lines.push(format!(
            "{} {}..{} {}",
            diagnostic.id.as_str(),
            diagnostic.span.start().to_usize(),
            diagnostic.span.end().to_usize(),
            diagnostic.message
        ));
    }
    for diagnostic in pipeline.frontend.semantic_diagnostics() {
        lines.push(format!(
            "{} {} {}",
            diagnostic.id().as_str(),
            diagnostic.severity().as_str(),
            diagnostic.message()
        ));
    }
    for diagnostic in &pipeline.lowering.diagnostics {
        lines.push(format!(
            "{} {}..{} {}",
            diagnostic.id, diagnostic.span.start, diagnostic.span.end, diagnostic.message
        ));
    }
    if let Err(errors) = &pipeline.lowering.verification {
        lines.push(format!("IR verification failed: {} error(s)", errors.len()));
        for error in errors {
            lines.push(format!("{} {}", error.diagnostic_id(), error.message));
        }
    }
    if lines.is_empty() {
        "none".to_string()
    } else {
        lines.join("\n")
    }
}

fn push_hir_summary_markdown(out: &mut String, pipeline: &Pipeline) {
    out.push_str(&hir_summary_text(pipeline));
    out.push('\n');
}

fn hir_summary_text(pipeline: &Pipeline) -> String {
    let summary = pipeline.frontend.module();
    let mut out = String::new();
    out.push_str(&format!("- Module ID: {}\n", summary.module_id().raw()));
    out.push_str(&format!("- Root kind: `{}`\n", summary.root_kind()));
    out.push_str(&format!("- Source bytes: {}\n", summary.source_bytes()));
    if let Some(module) = pipeline.frontend.database().module(summary.module_id()) {
        out.push_str(&format!("- Namespaces: {}\n", module.namespaces().len()));
        out.push_str(&format!(
            "- Declarations: {}\n",
            module.declarations().len()
        ));
        out.push_str(&format!("- Statements: {}\n", module.statements().len()));
        out.push_str(&format!("- Expressions: {}\n", module.expressions().len()));
        out.push_str(&format!("- Types: {}\n", module.types().len()));
        out.push_str(&format!(
            "- Const expressions: {}\n",
            module.const_exprs().len()
        ));
        out.push_str(&format!("- Signatures: {}\n", module.signatures().len()));
        out.push_str(&format!("- Attributes: {}\n", module.attributes().len()));
        out.push_str(&format!(
            "- Class-like declarations: {}\n",
            module.class_likes().len()
        ));
        out.push_str(&format!("- Methods: {}\n", module.methods().len()));
        out.push_str(&format!("- Properties: {}\n", module.properties().len()));
        out.push_str(&format!(
            "- Class constants: {}",
            module.class_consts().len()
        ));
    } else {
        out.push_str("- Module detail: missing from frontend database");
    }
    out
}

fn push_runtime_diagnostics_markdown(out: &mut String, vm_result: Option<&VmResult>) {
    let text = runtime_diagnostics_text(vm_result);
    if text == "none" {
        out.push_str("none\n\n");
    } else {
        push_fenced_block(out, "json", &text);
    }
}

fn runtime_diagnostics_text(vm_result: Option<&VmResult>) -> String {
    let Some(result) = vm_result else {
        return "not run".to_string();
    };
    if result.diagnostics.is_empty() {
        return "none".to_string();
    }
    result
        .diagnostics
        .iter()
        .map(RuntimeDiagnostic::to_json)
        .collect::<Vec<_>>()
        .join("\n")
}

fn push_known_gap_status_markdown(
    out: &mut String,
    pipeline: &Pipeline,
    vm_result: Option<&VmResult>,
) {
    out.push_str(&known_gap_status_text(pipeline, vm_result));
    out.push_str("\n\n");
}

fn known_gap_status_text(pipeline: &Pipeline, vm_result: Option<&VmResult>) -> String {
    let mut ids = Vec::new();
    for diagnostic in &pipeline.lowering.diagnostics {
        if is_known_gap_id(&diagnostic.id) {
            ids.push(diagnostic.id.clone());
        }
    }
    if let Some(result) = vm_result {
        for diagnostic in &result.diagnostics {
            if is_known_gap_id(diagnostic.id()) {
                ids.push(diagnostic.id().to_string());
            }
        }
    }
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        "none detected".to_string()
    } else {
        ids.join("\n")
    }
}

fn is_known_gap_id(id: &str) -> bool {
    id.contains("UNSUPPORTED") || id.contains("KNOWN_GAP") || id.contains("GAP")
}

fn push_fenced_block(out: &mut String, lang: &str, body: &str) {
    out.push_str("```");
    out.push_str(lang);
    out.push('\n');
    out.push_str(body);
    if !body.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("```\n\n");
}

fn html_section_with_pre(out: &mut String, title: &str, body: &str) {
    out.push_str("<section>\n<h2>");
    out.push_str(&escape_html(title));
    out.push_str("</h2>\n<pre>");
    out.push_str(&escape_html(body));
    out.push_str("</pre>\n</section>\n");
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            c => escaped.push(c),
        }
    }
    escaped
}

#[allow(dead_code)]
fn path_exists(path: &str) -> bool {
    Path::new(path).exists()
}

#[cfg(feature = "jit-cranelift")]
fn workspace_relative_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate should be under workspace crates directory")
        .join(path)
}

#[cfg(test)]
mod tests {
    use super::{
        BytecodeCacheMode, EXIT_COMPILE_ERROR, EXIT_RUNTIME_ERROR, EXIT_SUCCESS, EXIT_USAGE,
        JitStatsMode, OptimizationLevel, PersistentFeedbackOptions, QuickeningMode, cache_file_for,
        compile_pipeline_with_optimization, parse_dump_dependency_units_args,
        parse_dump_rule_selection_args, parse_run_args, run,
    };
    use php_bytecode_cache::{CacheFingerprint, CacheFingerprintInput};
    use php_runtime::api::RuntimeContext;
    use php_vm::{
        api::{
            BytecodeLayoutMode, ExecutionFormat, JitBlacklistMode, JitMode, SuperinstructionMode,
        },
        experimental::InlineCacheMode,
    };
    use serde_json::Value;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::{env, fs};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn parse_json_bytes(bytes: &[u8]) -> Value {
        serde_json::from_slice(bytes).expect("valid JSON output")
    }

    fn parse_json_text(text: &str) -> Value {
        serde_json::from_str(text).expect("valid JSON output")
    }

    fn bytecode_cache_json(stderr: &str) -> Value {
        let line = stderr
            .lines()
            .find(|line| line.contains("\"bytecode_cache\""))
            .expect("bytecode cache JSON line");
        parse_json_text(line)["bytecode_cache"].clone()
    }

    fn restore_env(name: &str, previous: Option<String>) {
        unsafe {
            if let Some(value) = previous {
                env::set_var(name, value);
            } else {
                env::remove_var(name);
            }
        }
    }

    #[test]
    fn help_is_available() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(["--help".to_string()], &mut stdout, &mut stderr);

        assert_eq!(code, EXIT_SUCCESS);
        assert!(stderr.is_empty());
        assert!(String::from_utf8(stdout).unwrap().contains("dump-ir"));
    }

    #[test]
    fn unknown_command_writes_json_usage_diagnostic_from_env() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous = env::var("PHRUST_ERROR_FORMAT").ok();
        unsafe {
            env::set_var("PHRUST_ERROR_FORMAT", "json");
        }
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(["wat".to_string()], &mut stdout, &mut stderr);

        restore_env("PHRUST_ERROR_FORMAT", previous);
        assert_eq!(code, EXIT_USAGE);
        assert!(stdout.is_empty());
        let json = parse_json_bytes(&stderr);
        assert_eq!(json["code"], "E_PHRUST_CLI_USAGE");
        assert_eq!(json["layer"], "cli");
        assert_eq!(json["context"]["command"], "php-vm");
        assert_eq!(json["context"]["argument"], "wat");
    }

    #[test]
    fn missing_run_path_writes_text_usage_diagnostic() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(["run".to_string()], &mut stdout, &mut stderr);

        assert_eq!(code, EXIT_USAGE);
        assert!(stdout.is_empty());
        let stderr = String::from_utf8(stderr).expect("utf8");
        assert!(stderr.contains("E_PHRUST_CLI_USAGE"));
        assert!(stderr.contains("run requires <path.php>"));
        assert!(stderr.contains("php-vm run"));
    }

    #[test]
    fn run_debug_writes_timeline_to_stderr_without_changing_stdout() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                "--debug".to_string(),
                fixture("fixtures/runtime/valid/hello.php"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(String::from_utf8(stdout).unwrap(), "hello runtime\n");
        let stderr = String::from_utf8(stderr).expect("utf8");
        assert!(stderr.contains("D_PHRUST_CLI_PARSE_START"));
        assert!(stderr.contains("D_PHRUST_SOURCE_READ_START"));
        assert!(stderr.contains("D_PHRUST_VM_EXECUTE_END"));
        assert!(stderr.contains("D_PHRUST_VM_TRACE"));
    }

    #[test]
    fn run_uses_error_format_env_for_usage_diagnostics() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous = env::var("PHRUST_ERROR_FORMAT").ok();
        unsafe {
            env::set_var("PHRUST_ERROR_FORMAT", "json");
        }
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run(["run".to_string()], &mut stdout, &mut stderr);

        restore_env("PHRUST_ERROR_FORMAT", previous);
        assert_eq!(code, EXIT_USAGE);
        assert!(stdout.is_empty());
        let json = parse_json_bytes(&stderr);
        assert_eq!(json["code"], "E_PHRUST_CLI_USAGE");
        assert_eq!(json["context"]["command"], "php-vm run");
    }

    #[cfg(feature = "jit-cranelift")]
    #[test]
    fn dump_cranelift_clif_writes_verified_standalone_smoke() {
        let output = workspace_root().join("target/performance/cranelift/trivial_add.clif");
        let _ = fs::remove_file(&output);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            ["dump-cranelift-clif".to_string()],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert!(stderr.is_empty());
        let stdout = String::from_utf8(stdout).unwrap();
        assert!(stdout.contains("backend=cranelift-experiment"));
        assert!(stdout.contains("verified=true"));
        let clif = fs::read_to_string(output).expect("CLIF smoke dump should be written");
        assert!(clif.contains("function u0:0(i64, i64) -> i64"));
        assert!(clif.contains("iadd"));
        assert!(clif.contains("return"));
    }

    #[cfg(not(feature = "jit-cranelift"))]
    #[test]
    fn dump_cranelift_clif_reports_feature_requirement_when_disabled() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            ["dump-cranelift-clif".to_string()],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, super::EXIT_UNSUPPORTED);
        assert!(stdout.is_empty());
        assert!(
            String::from_utf8(stderr)
                .unwrap()
                .contains("requires the jit-cranelift feature")
        );
    }

    #[test]
    fn compile_json_reports_ir_metadata() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "compile".to_string(),
                fixture("fixtures/runtime/valid/hello.php"),
                "--json".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        let json = parse_json_bytes(&stdout);
        assert_eq!(json["ok"], true);
        assert_eq!(json["path"], fixture("fixtures/runtime/valid/hello.php"));
        assert!(json["source_bytes"].as_u64().unwrap() > 0);
        assert!(json["parser_diagnostics"].as_array().unwrap().is_empty());
        assert!(json["semantic_diagnostics"].as_array().unwrap().is_empty());
        assert_eq!(json["ir"]["verified"], true);
        assert!(json["ir"]["functions"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn compile_json_reports_optimizer_stats_when_requested() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "compile".to_string(),
                fixture("tests/fixtures/performance/optimizer/safe_folding.php"),
                "--json".to_string(),
                "--opt-level=1".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        let json = parse_json_bytes(&stdout);
        let optimizer = &json["optimizer"];
        assert_eq!(optimizer["level"], "1");
        let passes = optimizer["passes"].as_array().unwrap();
        let folding = passes
            .iter()
            .find(|pass| pass["name"] == "constant_folding_safe_subset")
            .expect("constant folding pass");
        assert!(
            folding["stats"]["transformations_attempted"]
                .as_u64()
                .unwrap()
                > 0
        );
    }

    #[test]
    fn dump_rule_selection_reports_stable_text_and_json() {
        let fixture = fixture("fixtures/bytecode/literals/valid/echo-multiple.php");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            ["dump-rule-selection".to_string(), fixture.clone()],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert!(stderr.is_empty());
        let text = String::from_utf8(stdout).unwrap();
        assert!(text.starts_with("rule-selection\n"));
        assert!(!text.contains("rule_selection"));
        assert!(text.contains("load_const_echo"));
        assert!(text.contains("sources=["));

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "dump-rule-selection".to_string(),
                fixture,
                "--json".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert!(stderr.is_empty());
        let json = parse_json_bytes(&stdout);
        assert_eq!(json["ok"], true);
        assert!(json["rule_selection_candidates"].as_u64().unwrap() > 0);
        assert!(json["rule_selection_selected"].as_u64().unwrap() > 0);
        assert!(
            json["rule_selection_by_kind"]
                .as_object()
                .unwrap()
                .contains_key("load_const_echo")
        );
        assert!(!json["selections"].as_array().unwrap().is_empty());
    }

    #[test]
    fn dump_rule_selection_parser_rejects_extra_args() {
        let args = vec!["file.php".to_string(), "extra".to_string()];
        let error = parse_dump_rule_selection_args(&args).expect_err("extra arg should fail");

        assert!(error.contains("unexpected dump-rule-selection argument"));
    }

    #[test]
    fn dump_dependency_units_reports_stable_text_and_json() {
        let fixture =
            fixture("tests/fixtures/performance/framework_smoke/composer_autoload_lookup.php");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            ["dump-dependency-units".to_string(), fixture.clone()],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert!(stderr.is_empty());
        let text = String::from_utf8(stdout).unwrap();
        assert!(text.starts_with("# Dependency Units\n"));
        assert!(text.contains("autoload_resolver"));
        assert!(text.contains("source_content_changed"));

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "dump-dependency-units".to_string(),
                fixture,
                "--json".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert!(stderr.is_empty());
        let json = parse_json_bytes(&stdout);
        assert!(json["counters"]["dependency_units"].as_u64().unwrap() > 0);
        assert!(json["counters"]["dependency_edges"].as_u64().unwrap() > 0);
        assert!(
            json["units"]
                .as_array()
                .unwrap()
                .iter()
                .any(|unit| unit["kind"] == "autoload_resolver")
        );
    }

    #[test]
    fn dump_dependency_units_parser_rejects_extra_args() {
        let args = vec!["file.php".to_string(), "extra".to_string()];
        let error = parse_dump_dependency_units_args(&args).expect_err("extra arg should fail");

        assert!(error.contains("unexpected dump-dependency-units argument"));
    }

    #[test]
    fn dump_baseline_native_stencil_is_report_only() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "dump-baseline-native-stencil".to_string(),
                fixture("tests/fixtures/performance/perf_smoke/arrays_packed.php"),
                "--json".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert!(stderr.is_empty());
        let json = parse_json_bytes(&stdout);
        assert_eq!(json["backend"], "baseline-native-stencil");
        assert_eq!(json["status"], "no-exec");
        assert_eq!(json["native_execution"], false);
        assert_eq!(json["executable_memory"], false);
        assert!(json["compile_cost_units"].as_u64().unwrap() > 0);
        assert!(json["code_size_bytes_estimate"].as_u64().unwrap() > 0);
        assert!(json["required_deopt_slots"].as_u64().unwrap() > 0);
        assert!(
            json["unsupported_by_reason"]
                .as_object()
                .unwrap()
                .contains_key("array_reference_cow_and_key_state")
        );
    }

    #[test]
    fn run_executes_hello_fixture() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                fixture("fixtures/runtime/valid/hello.php"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(stdout, b"hello runtime\n");
    }

    #[test]
    fn run_output_matches_php_executor_for_basic_fixture() {
        let fixture_path = fixture("fixtures/runtime/valid/hello.php");
        let mut cli_stdout = Vec::new();
        let mut cli_stderr = Vec::new();
        let code = run(
            ["run".to_string(), fixture_path.clone()],
            &mut cli_stdout,
            &mut cli_stderr,
        );

        let (source, real_path, source_path) =
            php_executor::read_script(Path::new(&fixture_path)).expect("read fixture");
        let executor = php_executor::PhpExecutor::new();
        let output = executor.execute_source(php_executor::PhpExecutionInput {
            source,
            source_path,
            real_path: Some(real_path),
            cwd: std::env::current_dir().expect("current directory"),
            include_roots: Vec::new(),
            runtime_context: RuntimeContext::controlled_cli(&fixture_path, Vec::new()),
            optimization_level: None,
            collect_counters: false,
        });

        assert_eq!(
            code,
            EXIT_SUCCESS,
            "{}",
            String::from_utf8_lossy(&cli_stderr)
        );
        assert!(cli_stderr.is_empty());
        assert_eq!(output.status, php_executor::PhpExecutionStatus::Success);
        assert_eq!(cli_stdout, output.stdout);
        assert!(output.diagnostics_text.is_empty());
    }

    #[test]
    fn opt_level_one_reports_perf_optimizer_passes() {
        let pipeline = compile_pipeline_with_optimization(
            &fixture("tests/fixtures/performance/perf_smoke/arithmetic.php"),
            OptimizationLevel::O1,
        )
        .expect("fixture should compile");

        assert!(pipeline.ok());
        let report = pipeline.optimizer.expect("level 1 should run optimizer");
        assert_eq!(report.level, OptimizationLevel::O1);
        assert_eq!(report.enabled_pass_count(), 7);
        assert_eq!(report.passes[0].name, "perf_pre_verify_noop");
        assert_eq!(report.passes[1].name, "constant_folding_safe_subset");
        assert_eq!(report.passes[2].name, "literal_compaction");
        assert_eq!(report.passes[3].name, "copy_propagation_register_subset");
        assert_eq!(report.passes[4].name, "peephole_simplify");
        assert_eq!(report.passes[5].name, "branch_simplify");
        assert_eq!(report.passes[6].name, "perf_post_verify_noop");
        assert!(report.passes.iter().all(|pass| pass.source_spans_preserved));
    }

    #[test]
    fn opt_level_zero_has_no_optimizer_report() {
        let pipeline = compile_pipeline_with_optimization(
            &fixture("tests/fixtures/performance/perf_smoke/arithmetic.php"),
            OptimizationLevel::O0,
        )
        .expect("fixture should compile");

        assert!(pipeline.ok());
        assert!(pipeline.optimizer.is_none());
    }

    #[test]
    fn run_opt_level_zero_and_one_are_identical_for_perf_fixtures() {
        for fixture in optimizer_fixture_paths() {
            let mut stdout_zero = Vec::new();
            let mut stderr_zero = Vec::new();
            let code_zero = run(
                [
                    "run".to_string(),
                    "--opt-level=0".to_string(),
                    fixture.clone(),
                ],
                &mut stdout_zero,
                &mut stderr_zero,
            );

            let mut stdout_one = Vec::new();
            let mut stderr_one = Vec::new();
            let code_one = run(
                [
                    "run".to_string(),
                    "--opt-level=1".to_string(),
                    fixture.clone(),
                ],
                &mut stdout_one,
                &mut stderr_one,
            );

            assert_eq!(code_one, code_zero, "{fixture}");
            assert_eq!(stdout_one, stdout_zero, "{fixture}");
            assert_eq!(stderr_one, stderr_zero, "{fixture}");
        }
    }

    #[test]
    fn invalid_opt_level_is_rejected() {
        let args = vec![
            "--opt-level=3".to_string(),
            "fixtures/runtime/valid/hello.php".to_string(),
        ];

        let error = match parse_run_args(&args) {
            Ok(_) => panic!("level 3 should be rejected"),
            Err(error) => error,
        };

        assert!(error.contains("expected 0, 1, or 2"));
    }

    #[test]
    fn run_args_default_to_shared_default_engine_profile() {
        let args = vec!["fixtures/runtime/valid/hello.php".to_string()];

        let options = parse_run_args(&args).expect("run args should parse");

        assert_eq!(options.bytecode_cache.mode, BytecodeCacheMode::Off);
        assert_eq!(options.opt_level, OptimizationLevel::O2);
        assert_eq!(options.execution_format, ExecutionFormat::Auto);
        assert_eq!(options.superinstructions, SuperinstructionMode::On);
        assert_eq!(options.bytecode_layout, BytecodeLayoutMode::Source);
        assert_eq!(options.bytecode_layout_profile, None);
        assert_eq!(options.quickening, QuickeningMode::On);
        assert_eq!(options.inline_caches, InlineCacheMode::On);
        assert_eq!(options.jit, JitMode::Cranelift);
        assert_eq!(options.jit_blacklist, JitBlacklistMode::On);
        assert!(options.tiering.enabled);
        assert_eq!(
            options.jit_threshold,
            options.tiering.function_entry_threshold
        );
    }

    #[test]
    fn run_args_accept_default_engine_preset() {
        let args = vec![
            "--engine-preset=default".to_string(),
            "fixtures/runtime/valid/hello.php".to_string(),
        ];

        let options = parse_run_args(&args).expect("run args should parse");

        assert_eq!(options.opt_level, OptimizationLevel::O2);
        assert_eq!(options.execution_format, ExecutionFormat::Auto);
        assert_eq!(options.quickening, QuickeningMode::On);
        assert_eq!(options.inline_caches, InlineCacheMode::On);
        assert_eq!(options.jit, JitMode::Cranelift);
        assert!(options.tiering.enabled);
    }

    #[test]
    fn run_args_accept_baseline_engine_preset() {
        let args = vec![
            "--engine-preset=baseline".to_string(),
            "fixtures/runtime/valid/hello.php".to_string(),
        ];

        let options = parse_run_args(&args).expect("run args should parse");

        assert_eq!(options.opt_level, OptimizationLevel::O0);
        assert_eq!(options.execution_format, ExecutionFormat::Ir);
        assert_eq!(options.quickening, QuickeningMode::Off);
        assert_eq!(options.inline_caches, InlineCacheMode::Off);
        assert_eq!(options.jit, JitMode::Off);
        assert!(!options.tiering.enabled);
    }

    #[test]
    fn run_args_accept_fast_engine_preset_alias() {
        let args = vec![
            "--engine-preset=fast".to_string(),
            "fixtures/runtime/valid/hello.php".to_string(),
        ];

        let options = parse_run_args(&args).expect("run args should parse");

        assert_eq!(options.bytecode_cache.mode, BytecodeCacheMode::Off);
        assert_eq!(options.opt_level, OptimizationLevel::O2);
        assert_eq!(options.execution_format, ExecutionFormat::Auto);
        assert_eq!(options.superinstructions, SuperinstructionMode::On);
        assert_eq!(options.bytecode_layout, BytecodeLayoutMode::Source);
        assert_eq!(options.bytecode_layout_profile, None);
        assert_eq!(options.quickening, QuickeningMode::On);
        assert_eq!(options.inline_caches, InlineCacheMode::On);
        assert_eq!(options.jit, JitMode::Cranelift);
        assert_eq!(options.jit_blacklist, JitBlacklistMode::On);
        assert!(options.tiering.enabled);
        assert_eq!(
            options.jit_threshold,
            options.tiering.function_entry_threshold
        );
    }

    #[test]
    fn run_args_engine_preset_accepts_later_overrides() {
        let args = vec![
            "--engine-preset=fast".to_string(),
            "--opt-level=1".to_string(),
            "--inline-caches=off".to_string(),
            "--bytecode-cache=read".to_string(),
            "fixtures/runtime/valid/hello.php".to_string(),
        ];

        let options = parse_run_args(&args).expect("run args should parse");

        assert_eq!(options.opt_level, OptimizationLevel::O1);
        assert_eq!(options.inline_caches, InlineCacheMode::Off);
        assert_eq!(options.bytecode_cache.mode, BytecodeCacheMode::Read);
        assert_eq!(options.quickening, QuickeningMode::On);
        assert_eq!(options.execution_format, ExecutionFormat::Auto);
        assert_eq!(options.bytecode_layout, BytecodeLayoutMode::Source);
    }

    #[test]
    fn run_args_accept_experimental_jit_engine_preset() {
        let args = vec![
            "--engine-preset".to_string(),
            "experimental-jit".to_string(),
            "fixtures/runtime/valid/hello.php".to_string(),
        ];

        let options = parse_run_args(&args).expect("run args should parse");

        assert_eq!(options.opt_level, OptimizationLevel::O2);
        assert_eq!(options.execution_format, ExecutionFormat::Auto);
        assert_eq!(options.bytecode_layout, BytecodeLayoutMode::Source);
        assert_eq!(options.quickening, QuickeningMode::On);
        assert_eq!(options.inline_caches, InlineCacheMode::On);
        assert_eq!(options.jit, JitMode::Cranelift);
        assert!(options.tiering.enabled);
    }

    #[test]
    fn invalid_engine_preset_is_rejected() {
        let args = vec![
            "--engine-preset=turbo".to_string(),
            "fixtures/runtime/valid/hello.php".to_string(),
        ];

        let error = match parse_run_args(&args) {
            Ok(_) => panic!("invalid engine preset should be rejected"),
            Err(error) => error,
        };

        assert!(error.contains("expected baseline, default, fast, or experimental-jit"));
    }

    #[test]
    fn invalid_quickening_mode_is_rejected() {
        let args = vec![
            "--quickening=maybe".to_string(),
            "fixtures/runtime/valid/hello.php".to_string(),
        ];

        let error = match parse_run_args(&args) {
            Ok(_) => panic!("invalid quickening mode should be rejected"),
            Err(error) => error,
        };

        assert!(error.contains("expected off or on"));
    }

    #[cfg(not(feature = "jit-cranelift"))]
    #[test]
    fn cranelift_jit_mode_without_feature_is_unsupported() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                "--jit=cranelift".to_string(),
                fixture("fixtures/runtime/valid/hello.php"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, super::EXIT_UNSUPPORTED);
        assert!(stdout.is_empty());
        assert!(
            String::from_utf8(stderr)
                .unwrap()
                .contains("requires the jit-cranelift feature")
        );
    }

    #[test]
    fn invalid_inline_cache_mode_is_rejected() {
        let args = vec![
            "--inline-caches=maybe".to_string(),
            "fixtures/runtime/valid/hello.php".to_string(),
        ];

        let error = match parse_run_args(&args) {
            Ok(_) => panic!("invalid inline-cache mode should be rejected"),
            Err(error) => error,
        };

        assert!(error.contains("expected off or on"));
    }

    #[test]
    fn invalid_bytecode_layout_mode_is_rejected() {
        let args = vec![
            "--bytecode-layout=sideways".to_string(),
            "fixtures/runtime/valid/hello.php".to_string(),
        ];

        let error = match parse_run_args(&args) {
            Ok(_) => panic!("invalid bytecode layout mode should be rejected"),
            Err(error) => error,
        };

        assert!(error.contains("expected source or profiled"));
    }

    #[test]
    fn invalid_jit_mode_is_rejected() {
        let args = vec![
            "--jit=maybe".to_string(),
            "fixtures/runtime/valid/hello.php".to_string(),
        ];

        let error = match parse_run_args(&args) {
            Ok(_) => panic!("invalid jit mode should be rejected"),
            Err(error) => error,
        };

        assert!(error.contains("expected off, noop, or cranelift"));
    }

    #[test]
    fn invalid_tiering_mode_is_rejected() {
        let args = vec![
            "--tiering=maybe".to_string(),
            "fixtures/runtime/valid/hello.php".to_string(),
        ];

        let error = match parse_run_args(&args) {
            Ok(_) => panic!("invalid tiering mode should be rejected"),
            Err(error) => error,
        };

        assert!(error.contains("expected off or on"));
    }

    #[test]
    fn invalid_tiering_threshold_is_rejected() {
        let args = vec![
            "--tiering-function-threshold=soon".to_string(),
            "fixtures/runtime/valid/hello.php".to_string(),
        ];

        let error = match parse_run_args(&args) {
            Ok(_) => panic!("invalid tiering threshold should be rejected"),
            Err(error) => error,
        };

        assert!(error.contains("non-negative integer"));
    }

    #[test]
    fn run_counters_json_writes_file_without_stdout_leak() {
        let path =
            std::env::temp_dir().join(format!("phrust-vm-counters-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                "--counters-json".to_string(),
                path.display().to_string(),
                fixture("fixtures/runtime/valid/hello.php"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(stdout, b"hello runtime\n");
        assert!(stderr.is_empty());
        let json = std::fs::read_to_string(&path).expect("counter JSON should be written");
        let _ = std::fs::remove_file(&path);
        assert!(json.contains("\"instructions_executed\""));
        assert!(json.contains("\"jit_mode\": \"cranelift\""));
        assert!(json.contains("\"jit_threshold\": 8"));
        assert!(json.contains("\"output_bytes\": 14"));
        assert!(json.contains("\"guard_failures\": 0"));
    }

    #[test]
    fn run_default_collects_managed_fast_path_counters() {
        let root = std::env::temp_dir().join(format!(
            "phrust-vm-default-counters-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create CLI counter fixture root");
        let script = root.join("index.php");
        let counters_path = root.join("counters.json");
        fs::write(&script, managed_fast_counter_source()).expect("write CLI counter fixture");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                "--counters-json".to_string(),
                counters_path.display().to_string(),
                script.display().to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(stdout, b"123512351235");
        assert!(stderr.is_empty());
        let json = fs::read_to_string(&counters_path).expect("counter JSON should be written");
        let _ = fs::remove_dir_all(&root);
        let counters = parse_json_text(&json);
        assert_eq!(counters["jit_mode"], "cranelift");
        assert_eq!(counters["native_executions"], counters["jit_executed"]);
        assert!(
            counters["bytecode_lower_attempts"].as_u64().unwrap() > 0,
            "{json}"
        );
        assert!(
            counters["quickening_attempts"].as_u64().unwrap() > 0,
            "{json}"
        );
        assert!(
            counters["inline_cache_observations"].as_u64().unwrap() > 0,
            "{json}"
        );
    }

    #[test]
    fn run_jit_noop_mode_is_visible_in_counter_json() {
        let path = std::env::temp_dir().join(format!(
            "phrust-vm-jit-noop-counters-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                "--jit=noop".to_string(),
                "--jit-threshold=7".to_string(),
                "--counters-json".to_string(),
                path.display().to_string(),
                fixture("fixtures/runtime/valid/hello.php"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(stdout, b"hello runtime\n");
        assert!(stderr.is_empty());
        let json = std::fs::read_to_string(&path).expect("counter JSON should be written");
        let _ = std::fs::remove_file(&path);
        assert!(json.contains("\"jit_mode\": \"noop\""));
        assert!(json.contains("\"jit_threshold\": 7"));
        assert!(json.contains("\"jit_compile_attempts\": 0"));
    }

    #[test]
    fn run_jit_stats_json_writes_compact_report_to_stderr() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                "--jit=noop".to_string(),
                "--jit-threshold=5".to_string(),
                "--jit-dump-clif=target/performance/cranelift/noop.clif".to_string(),
                "--jit-stats=json".to_string(),
                fixture("fixtures/runtime/valid/hello.php"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(stdout, b"hello runtime\n");
        let stderr = String::from_utf8(stderr).unwrap();
        let json = parse_json_text(stderr.trim());
        let jit = &json["jit"];
        assert_eq!(jit["mode"], "noop");
        assert_eq!(jit["threshold"], 5);
        assert_eq!(jit["eager"], false);
        assert_eq!(jit["max_compile_us"], u64::MAX);
        assert_eq!(jit["max_functions"], u64::MAX);
        assert_eq!(jit["blacklist"], "on");
        assert_eq!(jit["dump_clif"], "target/performance/cranelift/noop.clif");
        assert!(jit["side_exit_reasons"].as_object().unwrap().is_empty());
        assert_eq!(jit["blacklisted_regions"], 0);
        assert!(jit["blacklist_reasons"].as_object().unwrap().is_empty());
        assert_eq!(jit["tiering_cold_functions"], 0);
        assert_eq!(jit["tiering_hot_functions"], 0);
        assert_eq!(jit["tiering_eager_functions"], 0);
        assert_eq!(jit["tiering_blacklist_rejections"], 0);
        assert_eq!(jit["tiering_budget_rejections"], 0);
        assert_eq!(jit["fast_path_hits"], 0);
        assert_eq!(jit["packed_fetch_fast_hits"], 0);
        assert_eq!(jit["packed_fetch_bounds_exits"], 0);
        assert_eq!(jit["packed_fetch_layout_exits"], 0);
        assert_eq!(jit["packed_foreach_sum_fast_hits"], 0);
        assert_eq!(jit["packed_foreach_sum_layout_exits"], 0);
        assert_eq!(jit["packed_foreach_sum_overflow_exits"], 0);
        assert_eq!(jit["known_call_fast_hits"], 0);
        assert_eq!(jit["known_call_guard_exits"], 0);
        assert_eq!(jit["known_call_slow_calls"], 0);
        assert_eq!(jit["direct_call_hits"], 0);
        assert_eq!(jit["direct_call_fallbacks"], 0);
        assert_eq!(jit["property_load_fast_hits"], 0);
        assert_eq!(jit["property_load_guard_exits"], 0);
        assert_eq!(jit["property_load_layout_exits"], 0);
        assert_eq!(jit["property_load_uninitialized_exits"], 0);
        assert_eq!(jit["property_load_slow_calls"], 0);
        assert_eq!(jit["string_concat_fast_path_hits"], 0);
        assert_eq!(jit["string_concat_fast_path_misses"], 0);
        assert_eq!(jit["overflow_exits"], 0);
        assert_eq!(jit["slow_path_calls"], 0);
        assert_eq!(jit["compile_cache_hits"], 0);
        assert_eq!(jit["compile_cache_misses"], 0);
        assert_eq!(jit["compile_cache_invalidations"], 0);
        assert!(jit["compile_descriptors"].as_array().unwrap().is_empty());
        assert_eq!(jit["eligibility"]["considered"], 0);
    }

    #[cfg(feature = "jit-cranelift")]
    #[test]
    fn cranelift_jit_stats_reports_eligibility_json_for_fixtures() {
        let fixtures = [
            (
                "tests/fixtures/performance/cranelift/eligibility/eligible-int-leaf.php",
                "\"candidate_kind\":\"IntLeafCandidate\"",
            ),
            (
                "tests/fixtures/performance/cranelift/eligibility/rejected-array-op.php",
                "JIT_ELIGIBILITY_REJECT_ARRAY_OPCODE",
            ),
            (
                "tests/fixtures/performance/cranelift/eligibility/rejected-call.php",
                "JIT_ELIGIBILITY_REJECT_CALL_OPCODE",
            ),
            (
                "tests/fixtures/performance/cranelift/eligibility/rejected-untyped-param.php",
                "JIT_ELIGIBILITY_REJECT_UNTYPED_PARAM",
            ),
        ];

        for (fixture_path, expected_json) in fixtures {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let code = run(
                [
                    "run".to_string(),
                    "--jit=cranelift".to_string(),
                    "--jit-stats=json".to_string(),
                    fixture(fixture_path),
                ],
                &mut stdout,
                &mut stderr,
            );

            assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
            let stderr = String::from_utf8(stderr).unwrap();
            assert!(stderr.contains("\"eligibility\":{\"considered\":"));
            assert!(stderr.contains("\"reports\":["));
            assert!(stderr.contains(expected_json), "{stderr}");
        }
    }

    #[test]
    fn args_after_separator_initialize_argc_and_argv() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                fixture("fixtures/runtime/valid/superglobals/argv.php"),
                "--".to_string(),
                "alpha".to_string(),
                "beta".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(stdout, b"3|alpha|beta\n");
    }

    #[test]
    fn args_without_separator_are_rejected() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                fixture("fixtures/runtime/valid/superglobals/argv.php"),
                "alpha".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_ne!(code, EXIT_SUCCESS);
        assert!(stdout.is_empty());
        assert!(
            String::from_utf8(stderr)
                .unwrap()
                .contains("pass script arguments after `--`")
        );
    }

    #[test]
    fn run_args_accept_controlled_environment_entries() {
        let args = vec![
            "--env".to_string(),
            "PHP_APP_HOME=/tmp/app".to_string(),
            "--env=PHP_APP_CACHE_DIR=/tmp/cache".to_string(),
            "fixtures/runtime/valid/hello.php".to_string(),
            "--".to_string(),
            "script-arg".to_string(),
        ];

        let options = parse_run_args(&args).expect("run args should parse");

        assert_eq!(options.path, "fixtures/runtime/valid/hello.php");
        assert_eq!(options.script_args, vec!["script-arg"]);
        assert_eq!(options.counters_json, None);
        assert_eq!(options.bytecode_cache.mode, BytecodeCacheMode::Off);
        assert_eq!(options.opt_level, OptimizationLevel::O2);
        assert_eq!(options.execution_format, ExecutionFormat::Auto);
        assert_eq!(options.superinstructions, SuperinstructionMode::On);
        assert_eq!(options.bytecode_layout, BytecodeLayoutMode::Source);
        assert_eq!(options.bytecode_layout_profile, None);
        assert_eq!(options.quickening, QuickeningMode::On);
        assert_eq!(options.inline_caches, InlineCacheMode::On);
        assert_eq!(options.jit, JitMode::Cranelift);
        assert_eq!(options.jit_blacklist, JitBlacklistMode::On);
        assert!(options.tiering.enabled);
        assert!(!options.tiering.collect_stats);
        assert_eq!(options.tiering_stats_json, None);
        assert_eq!(
            options.persistent_feedback,
            PersistentFeedbackOptions::default()
        );
        assert_eq!(
            options.env,
            vec![
                ("PHP_APP_HOME".to_string(), "/tmp/app".to_string()),
                ("PHP_APP_CACHE_DIR".to_string(), "/tmp/cache".to_string())
            ]
        );
    }

    #[test]
    fn run_args_accept_bytecode_cache_options() {
        let args = vec![
            "--bytecode-cache=read-write".to_string(),
            "--bytecode-cache-dir".to_string(),
            "target/performance/cli-cache".to_string(),
            "--bytecode-cache-stats".to_string(),
            "--clear-bytecode-cache".to_string(),
            "--opt-level=1".to_string(),
            "--exec-format=bytecode".to_string(),
            "--superinstructions=on".to_string(),
            "--bytecode-layout=profiled".to_string(),
            "--bytecode-layout-profile".to_string(),
            "target/performance/bytecode-layout/block-frequency.json".to_string(),
            "--quickening=on".to_string(),
            "--inline-caches=on".to_string(),
            "--jit=cranelift".to_string(),
            "--jit-threshold=9".to_string(),
            "--jit-max-compile-us=1000".to_string(),
            "--jit-max-functions".to_string(),
            "2".to_string(),
            "--jit-eager".to_string(),
            "--jit-blacklist=off".to_string(),
            "--jit-dump-clif=target/performance/cranelift/run.clif".to_string(),
            "--jit-stats=json".to_string(),
            "--tiering=off".to_string(),
            "--tiering-function-threshold=3".to_string(),
            "--tiering-loop-threshold".to_string(),
            "4".to_string(),
            "--tiering-ic-stability-threshold=5".to_string(),
            "--tiering-guard-failure-threshold".to_string(),
            "6".to_string(),
            "--tiering-stats-json=target/performance/tiering.json".to_string(),
            "--persistent-feedback-read=target/performance/feedback/input.pff".to_string(),
            "--persistent-feedback-stats-json".to_string(),
            "target/performance/feedback/stats.json".to_string(),
            "fixtures/runtime/valid/hello.php".to_string(),
        ];

        let options = parse_run_args(&args).expect("run args should parse");

        assert_eq!(options.bytecode_cache.mode, BytecodeCacheMode::ReadWrite);
        assert_eq!(
            options.bytecode_cache.dir,
            Some(PathBuf::from("target/performance/cli-cache"))
        );
        assert!(options.bytecode_cache.stats);
        assert!(options.bytecode_cache.clear);
        assert_eq!(options.opt_level, OptimizationLevel::O1);
        assert_eq!(options.execution_format, ExecutionFormat::Bytecode);
        assert_eq!(options.superinstructions, SuperinstructionMode::On);
        assert_eq!(options.bytecode_layout, BytecodeLayoutMode::Profiled);
        assert_eq!(
            options.bytecode_layout_profile,
            Some("target/performance/bytecode-layout/block-frequency.json".to_string())
        );
        assert_eq!(options.quickening, QuickeningMode::On);
        assert_eq!(options.inline_caches, InlineCacheMode::On);
        assert_eq!(options.jit, JitMode::Cranelift);
        assert_eq!(options.jit_threshold, 1);
        assert_eq!(options.jit_blacklist, JitBlacklistMode::Off);
        assert_eq!(
            options.jit_dump_clif,
            Some("target/performance/cranelift/run.clif".to_string())
        );
        assert_eq!(options.jit_stats, JitStatsMode::Json);
        assert!(!options.tiering.enabled);
        assert!(options.tiering.collect_stats);
        assert!(options.tiering.jit_eager);
        assert_eq!(options.tiering.jit_max_compile_us, 1000);
        assert_eq!(options.tiering.jit_max_functions, 2);
        assert_eq!(options.tiering.function_entry_threshold, 3);
        assert_eq!(options.tiering.loop_backedge_threshold, 4);
        assert_eq!(options.tiering.ic_stability_threshold, 5);
        assert_eq!(options.tiering.guard_failure_threshold, 6);
        assert_eq!(
            options.tiering_stats_json,
            Some("target/performance/tiering.json".to_string())
        );
        assert_eq!(
            options.persistent_feedback.read,
            Some("target/performance/feedback/input.pff".to_string())
        );
        assert_eq!(
            options.persistent_feedback.stats_json,
            Some("target/performance/feedback/stats.json".to_string())
        );
    }

    #[test]
    fn run_tiering_stats_json_writes_file_without_stdout_leak() {
        let path =
            std::env::temp_dir().join(format!("phrust-vm-tiering-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                "--tiering-stats-json".to_string(),
                path.display().to_string(),
                fixture("fixtures/runtime/valid/hello.php"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(stdout, b"hello runtime\n");
        assert!(stderr.is_empty());
        let json = std::fs::read_to_string(&path).expect("tiering JSON should be written");
        let _ = std::fs::remove_file(&path);
        assert!(json.contains("\"function_entry_count\""));
        assert!(json.contains("\"tier0_interpreter_entries\""));
        assert!(json.contains("\"tiering_disabled_entries\""));
        assert!(json.contains("\"schema_version\": 2"));
        assert!(json.contains("\"exit_policy\""));
        assert!(json.contains("\"sites\""));
        assert!(json.contains("\"decisions\""));
    }

    #[test]
    fn run_persistent_feedback_stats_json_reports_corrupt_fallback_without_stdout_leak() {
        let base = std::env::temp_dir().join(format!(
            "phrust-vm-persistent-feedback-{}",
            std::process::id()
        ));
        let feedback_path = base.with_extension("pff");
        let stats_path = base.with_extension("json");
        let _ = std::fs::remove_file(&feedback_path);
        let _ = std::fs::remove_file(&stats_path);
        std::fs::write(&feedback_path, "not-valid-feedback\n").expect("feedback fixture");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                "--persistent-feedback-read".to_string(),
                feedback_path.display().to_string(),
                "--persistent-feedback-stats-json".to_string(),
                stats_path.display().to_string(),
                fixture("fixtures/runtime/valid/hello.php"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(stdout, b"hello runtime\n");
        assert!(stderr.is_empty());
        let json = std::fs::read_to_string(&stats_path).expect("feedback JSON should be written");
        let _ = std::fs::remove_file(&feedback_path);
        let _ = std::fs::remove_file(&stats_path);
        assert!(json.contains("\"advisory_only\": true"));
        assert!(json.contains("\"default_enabled\": false"));
        assert!(json.contains("\"rejected_corrupt\": 1"));
        assert!(json.contains("\"fallback_to_baseline\": true"));
    }

    #[test]
    fn run_bytecode_cache_first_write_then_second_read_hits() {
        let cache_dir = cache_test_dir("write-read");
        reset_dir(&cache_dir);
        let fixture = fixture("tests/fixtures/performance/bytecode_cache/simple.php");

        let first = run_cache_fixture_with_mode(&fixture, &cache_dir, "0", "write");
        assert_eq!(first.0, EXIT_SUCCESS, "{}", first.2);
        assert_eq!(first.1, b"cache:5\n");
        assert_eq!(bytecode_cache_json(&first.2)["wrote"], true);
        assert!(!cache_files(&cache_dir).is_empty());

        let second = run_cache_fixture_with_mode(&fixture, &cache_dir, "0", "read");
        assert_eq!(second.0, EXIT_SUCCESS, "{}", second.2);
        assert_eq!(second.1, b"cache:5\n");
        let cache = bytecode_cache_json(&second.2);
        assert_eq!(cache["hit"], true);
        assert!(cache.get("load_error").is_none(), "{}", second.2);
    }

    #[test]
    fn run_bytecode_cache_source_change_misses() {
        let cache_dir = cache_test_dir("source-change");
        reset_dir(&cache_dir);
        let source = cache_dir.join("source-change.php");
        fs::write(&source, "<?php echo \"one\\n\";").expect("write source");

        let first = run_cache_fixture(&source.display().to_string(), &cache_dir, "0");
        assert_eq!(first.0, EXIT_SUCCESS, "{}", first.2);
        assert_eq!(first.1, b"one\n");
        assert_eq!(bytecode_cache_json(&first.2)["wrote"], true);

        fs::write(&source, "<?php echo \"two\\n\";").expect("rewrite source");
        let second = run_cache_fixture(&source.display().to_string(), &cache_dir, "0");
        assert_eq!(second.0, EXIT_SUCCESS, "{}", second.2);
        assert_eq!(second.1, b"two\n");
        let cache = bytecode_cache_json(&second.2);
        assert_eq!(cache["miss"], true);
        assert_eq!(cache["hit"], false);
    }

    #[test]
    fn run_bytecode_cache_opt_level_change_misses() {
        let cache_dir = cache_test_dir("opt-level-change");
        reset_dir(&cache_dir);
        let fixture = fixture("tests/fixtures/performance/bytecode_cache/simple.php");

        let first = run_cache_fixture(&fixture, &cache_dir, "0");
        assert_eq!(first.0, EXIT_SUCCESS, "{}", first.2);
        assert_eq!(bytecode_cache_json(&first.2)["wrote"], true);

        let second = run_cache_fixture(&fixture, &cache_dir, "1");
        assert_eq!(second.0, EXIT_SUCCESS, "{}", second.2);
        assert_eq!(second.1, b"cache:5\n");
        let cache = bytecode_cache_json(&second.2);
        assert_eq!(cache["miss"], true);
        assert_eq!(cache["hit"], false);
    }

    #[test]
    fn run_bytecode_cache_corrupt_cache_does_not_block_execution() {
        let cache_dir = cache_test_dir("corrupt");
        reset_dir(&cache_dir);
        let fixture = fixture("tests/fixtures/performance/bytecode_cache/simple.php");

        let first = run_cache_fixture(&fixture, &cache_dir, "0");
        assert_eq!(first.0, EXIT_SUCCESS, "{}", first.2);
        for file in cache_files(&cache_dir) {
            fs::write(file, b"not a bytecode cache").expect("corrupt cache file");
        }

        let second = run_cache_fixture(&fixture, &cache_dir, "0");
        assert_eq!(second.0, EXIT_SUCCESS, "{}", second.2);
        assert_eq!(second.1, b"cache:5\n");
        let cache = bytecode_cache_json(&second.2);
        assert_eq!(cache["miss"], true);
        assert!(cache["load_error"].as_str().is_some());
    }

    #[test]
    fn run_bytecode_cache_stats_marks_compile_errors() {
        let cache_dir = cache_test_dir("compile-error");
        reset_dir(&cache_dir);
        let fixture = fixture("fixtures/semantic/invalid/missing-semicolon.php");

        let result = run_cache_fixture(&fixture, &cache_dir, "0");

        assert_eq!(result.0, EXIT_COMPILE_ERROR, "{}", result.2);
        assert!(result.1.is_empty());
        let cache = bytecode_cache_json(&result.2);
        assert_eq!(cache["miss"], true);
        assert_eq!(cache["hit"], false);
        assert_eq!(cache["compile_error"], true);
    }

    #[test]
    fn run_bytecode_cache_rejects_non_hex_digest_path_component() {
        let cache_dir = PathBuf::from("target/performance/cli-cache");
        let mut fingerprint = CacheFingerprint::from_inputs(
            CacheFingerprintInput::new(b"<?php echo 1;\n", env!("CARGO_PKG_VERSION"), "test")
                .with_feature_flag("bytecode_cache", true),
        )
        .expect("fingerprint");
        fingerprint.digest = "../outside".to_string();

        let error = cache_file_for(&cache_dir, &fingerprint).expect_err("digest must be rejected");

        assert_eq!(error, "bytecode cache fingerprint digest is not hex");
    }

    #[test]
    fn dump_ir_prints_textual_ir() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "dump-ir".to_string(),
                fixture("fixtures/runtime/valid/hello.php"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        let stdout = String::from_utf8(stdout).unwrap();
        assert!(stdout.contains("ir version=1"));
        assert!(stdout.contains("echo r0"));
    }

    #[test]
    fn dump_ir_with_source_prints_source_prelude() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "dump-ir".to_string(),
                fixture("fixtures/runtime/valid/hello.php"),
                "--with-source".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        let stdout = String::from_utf8(stdout).unwrap();
        assert!(stdout.contains("source path="));
        assert!(stdout.contains("source 0001: <?php"));
        assert!(stdout.contains("--- ir ---"));
        assert!(stdout.contains("ir version=1"));
    }

    #[test]
    fn report_markdown_contains_debug_sections() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "report".to_string(),
                fixture("fixtures/runtime/valid/hello.php"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert!(stderr.is_empty());
        let stdout = String::from_utf8(stdout).unwrap();
        assert!(stdout.contains("# PHP VM Report"));
        assert!(stdout.contains("## Source"));
        assert!(stdout.contains("## Diagnostics"));
        assert!(stdout.contains("## HIR Summary"));
        assert!(stdout.contains("## IR Dump"));
        assert!(stdout.contains("## VM Output"));
        assert!(stdout.contains("## Runtime Diagnostics"));
        assert!(stdout.contains("## Known-Gap Status"));
        assert!(stdout.contains("hello runtime"));
    }

    #[test]
    fn report_html_escapes_source_and_contains_sections() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "report".to_string(),
                fixture("fixtures/runtime/valid/hello.php"),
                "--format=html".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert!(stderr.is_empty());
        let stdout = String::from_utf8(stdout).unwrap();
        assert!(stdout.contains("<!doctype html>"));
        assert!(stdout.contains("<h1>PHP VM Report</h1>"));
        assert!(stdout.contains("<h2>HIR Summary</h2>"));
        assert!(stdout.contains("&lt;?php"));
    }

    #[test]
    fn report_runtime_error_returns_runtime_error_after_rendering() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "report".to_string(),
                fixture("fixtures/runtime/invalid/errors/undefined-function.php"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_RUNTIME_ERROR);
        assert!(stderr.is_empty());
        let stdout = String::from_utf8(stdout).unwrap();
        assert!(stdout.contains("## Runtime Diagnostics"));
        assert!(stdout.contains("E_PHP_RUNTIME_UNDEFINED_FUNCTION"));
    }

    #[test]
    fn run_trace_writes_stderr_without_changing_stdout() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                "--trace".to_string(),
                fixture("fixtures/runtime/valid/variables/assignment.php"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(stdout, b"1\n");
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("vm-trace:"), "{stderr}");
        assert!(stderr.contains("function=main(0)"), "{stderr}");
        assert!(stderr.contains("output_len="), "{stderr}");
    }

    #[test]
    fn run_trace_runtime_writes_stderr_without_changing_stdout() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                "--trace-runtime".to_string(),
                fixture("fixtures/runtime/valid/references/array-element-ref.php"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert!(!stdout.is_empty());
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("vm-trace:"), "{stderr}");
        assert!(stderr.contains("runtime lvalue"), "{stderr}");
        assert!(!stderr.contains("0x"), "{stderr}");
    }

    #[test]
    fn syntax_error_returns_compile_error_with_path_and_span() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                fixture("fixtures/semantic/invalid/missing-semicolon.php"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_COMPILE_ERROR);
        assert!(stdout.is_empty());
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("missing-semicolon.php"));
        assert!(stderr.contains(".."));
    }

    #[test]
    fn class_table_compile_errors_render_php_fatal_line() {
        let path = std::env::temp_dir().join(format!(
            "phrust-vm-cli-visibility-{}.php",
            std::process::id()
        ));
        fs::write(
            &path,
            "<?php\nclass Base { public function show() {} }\nclass Child extends Base {\n    protected function show() {}\n}\n",
        )
        .expect("write temporary PHP source");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            ["run".to_string(), path.display().to_string()],
            &mut stdout,
            &mut stderr,
        );
        let _ = fs::remove_file(&path);

        assert_eq!(code, EXIT_COMPILE_ERROR);
        assert!(stdout.is_empty());
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(
            stderr.contains(
                "Fatal error: Access level to child::show() must be public (as in class base)"
            ),
            "{stderr}"
        );
        assert!(stderr.contains(" on line 4"), "{stderr}");
        assert!(
            !stderr.contains("E_PHP_VM_METHOD_VISIBILITY_OVERRIDE"),
            "{stderr}"
        );
    }

    #[test]
    fn runtime_error_writes_structured_diagnostic() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                "--error-format".to_string(),
                "text".to_string(),
                fixture("fixtures/runtime/invalid/errors/undefined-function.php"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_RUNTIME_ERROR);
        assert!(stdout.is_empty());
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("runtime-diagnostic:"), "{stderr}");
        assert!(
            stderr.contains("\"id\":\"E_PHP_RUNTIME_UNDEFINED_FUNCTION\""),
            "{stderr}"
        );
        assert!(
            stderr.contains("\"stack\":[{\"function\":\"main\"}]"),
            "{stderr}"
        );
    }

    #[test]
    fn successful_warning_output_does_not_emit_internal_runtime_diagnostics() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                fixture("tests/fixtures/stdlib/_harness/stdlib/array_flip_warning.php"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        let stdout = String::from_utf8(stdout).unwrap();
        assert!(stdout.contains("Warning: array_flip()"), "{stdout}");
        assert!(!stdout.contains("runtime-diagnostic:"), "{stdout}");
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(!stderr.contains("runtime-diagnostic:"), "{stderr}");
    }

    #[test]
    fn successful_unrendered_warning_keeps_structured_diagnostic() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                "--error-format".to_string(),
                "text".to_string(),
                fixture("fixtures/runtime/valid/arrays/missing-key.php"),
            ],
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, EXIT_SUCCESS, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(stdout, b"x\n");
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("runtime-diagnostic:"), "{stderr}");
        assert!(
            stderr.contains("E_PHP_RUNTIME_UNDEFINED_ARRAY_KEY_WARNING"),
            "{stderr}"
        );
    }

    fn run_cache_fixture(
        path: &str,
        cache_dir: &std::path::Path,
        opt_level: &str,
    ) -> (i32, Vec<u8>, String) {
        run_cache_fixture_with_mode(path, cache_dir, opt_level, "read-write")
    }

    fn run_cache_fixture_with_mode(
        path: &str,
        cache_dir: &std::path::Path,
        opt_level: &str,
        mode: &str,
    ) -> (i32, Vec<u8>, String) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
                format!("--bytecode-cache={mode}"),
                "--bytecode-cache-dir".to_string(),
                cache_dir.display().to_string(),
                "--bytecode-cache-stats".to_string(),
                "--opt-level".to_string(),
                opt_level.to_string(),
                path.to_string(),
            ],
            &mut stdout,
            &mut stderr,
        );
        (code, stdout, String::from_utf8(stderr).unwrap())
    }

    fn cache_test_dir(name: &str) -> PathBuf {
        workspace_root().join(format!(
            "target/performance/cli-cache-tests/{}-{}",
            name,
            std::process::id()
        ))
    }

    fn reset_dir(path: &std::path::Path) {
        let _ = fs::remove_dir_all(path);
        fs::create_dir_all(path).expect("create cache test dir");
    }

    fn cache_files(path: &std::path::Path) -> Vec<PathBuf> {
        fs::read_dir(path)
            .expect("read cache dir")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("phbc"))
            .collect()
    }

    fn optimizer_fixture_paths() -> Vec<String> {
        let mut fixtures = Vec::new();
        for dir in [
            workspace_root().join("tests/fixtures/performance/perf_smoke"),
            workspace_root().join("tests/fixtures/performance/bytecode_cache"),
        ] {
            for entry in fs::read_dir(&dir).expect("read optimizer fixture dir") {
                let path = entry.expect("read optimizer fixture entry").path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("php")
                    && path.with_extension("php.out").is_file()
                {
                    fixtures.push(path.display().to_string());
                }
            }
        }
        fixtures.sort();
        fixtures
    }

    fn managed_fast_counter_source() -> &'static str {
        "<?php\n\
         function ic_f() { return 1; }\n\
         class ICSlotSmoke {\n\
             public $x = 3;\n\
             public function m() { return 2; }\n\
         }\n\
         $object = new ICSlotSmoke();\n\
         $items = [4, 5];\n\
         for ($i = 0; $i < 3; $i = $i + 1) {\n\
             echo ic_f(), $object->m(), $object->x, $items[1];\n\
         }\n"
    }

    fn fixture(path: &str) -> String {
        workspace_root().join(path).display().to_string()
    }

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("crate should be under workspace crates directory")
            .to_path_buf()
    }
}
