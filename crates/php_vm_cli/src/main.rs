//! VM CLI.

use php_bytecode_cache::{
    CacheArtifact, CacheFingerprint, CacheFingerprintInput, CacheHeader, CachedIrArtifact,
};
use php_ir::{
    LoweringOptions, lower_frontend_result,
    module::{IrUnit, normalize_class_name},
    verify_unit,
};
use php_optimizer::{OptimizationLevel, OptimizationReport, PassContext, PassPipeline};
use php_runtime::{ExitStatus, FilesystemCapabilities, RuntimeContext};
use php_semantics::{FrontendResult, Severity, analyze_source, diagnostics::DiagnosticId};
use php_source::{SourceText, TextRange};
use php_vm::{
    DenseBytecodeUnit, ExecutionFormat, IncludeLoader, InlineCacheMode, JitBlacklistMode, JitMode,
    QuickeningMode, SuperinstructionMode, TieringOptions, Vm, VmOptions,
};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

mod todo_cli;

const EXIT_SUCCESS: i32 = 0;
const EXIT_COMPILE_ERROR: i32 = 2;
const EXIT_RUNTIME_ERROR: i32 = 3;
const EXIT_UNSUPPORTED: i32 = 4;
const EXIT_USAGE: i32 = 5;

fn main() {
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
            let _ = writeln!(stderr, "{error}");
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

fn run_command<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    let run_options = parse_run_args(args)?;
    if run_options.jit.requires_cranelift() && !cfg!(feature = "jit-cranelift") {
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
    let cached = try_load_bytecode_cache(&run_options, cache_context.as_ref(), &mut cache_stats);
    let (unit, compiled_pipeline) = if let Some(CachedIrArtifact { unit, .. }) = cached {
        (unit, None)
    } else {
        let pipeline = match compile_pipeline_with_optimization(path, run_options.opt_level) {
            Ok(pipeline) => pipeline,
            Err(error) => {
                if run_options.bytecode_cache.stats {
                    write_cache_stats_json(stderr, &cache_stats)?;
                }
                writeln!(stderr, "{error}").map_err(|io| io.to_string())?;
                return Ok(EXIT_COMPILE_ERROR);
            }
        };
        if !pipeline.ok() {
            if run_options.bytecode_cache.stats {
                write_cache_stats_json(stderr, &cache_stats)?;
            }
            write_frontend_diagnostics(stderr, &pipeline)?;
            return Ok(EXIT_COMPILE_ERROR);
        }
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
    let vm = Vm::with_options(VmOptions {
        include_loader,
        runtime_context,
        trace: run_options.trace,
        trace_runtime: run_options.trace_runtime,
        collect_counters: run_options.counters_json.is_some() || run_options.jit_stats.is_json(),
        execution_format: run_options.execution_format,
        superinstructions: run_options.superinstructions,
        quickening: run_options.quickening,
        inline_caches: run_options.inline_caches,
        jit: run_options.jit,
        jit_threshold: run_options.jit_threshold,
        jit_blacklist: run_options.jit_blacklist,
        jit_dump_clif: run_options.jit_dump_clif.as_ref().map(PathBuf::from),
        tiering: run_options.tiering.clone(),
        ..VmOptions::default()
    });
    let result = vm.execute(unit);
    stdout
        .write_all(result.output.as_bytes())
        .map_err(|error| error.to_string())?;
    write_runtime_diagnostics(stderr, path, &result)?;
    if run_options.trace || run_options.trace_runtime {
        write_trace(stderr, &result.trace)?;
    }
    if let Some(path) = &run_options.counters_json {
        let Some(counters) = &result.counters else {
            return Err("counters were requested but not collected".to_string());
        };
        write_counters_json(path.clone(), counters)?;
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
    if run_options.bytecode_cache.stats {
        write_cache_stats_json(stderr, &cache_stats)?;
    }
    match result.status.exit_status() {
        ExitStatus::Success => Ok(EXIT_SUCCESS),
        ExitStatus::CompileError => {
            if let Some(pipeline) = compiled_pipeline.as_ref()
                && write_vm_compile_fatal_line(stderr, pipeline, &result.status)?
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
        let mut out = String::new();
        out.push_str("{\"ok\":");
        out.push_str(if self.ok() { "true" } else { "false" });
        out.push_str(",\"path\":\"");
        out.push_str(&escape_json(&self.path));
        out.push_str("\",\"source_bytes\":");
        out.push_str(&self.source.len().to_string());
        out.push_str(",\"parser_diagnostics\":");
        push_parser_diagnostics_json(&mut out, &self.path, &self.frontend);
        out.push_str(",\"semantic_diagnostics\":");
        push_semantic_diagnostics_json(&mut out, &self.path, &self.frontend);
        out.push_str(",\"lowering_diagnostics\":");
        push_lowering_diagnostics_json(&mut out, &self.path, &self.lowering);
        out.push_str(",\"ir\":{");
        out.push_str("\"version\":");
        out.push_str(&self.lowering.unit.version.to_string());
        out.push_str(",\"functions\":");
        out.push_str(&self.lowering.unit.functions.len().to_string());
        out.push_str(",\"constants\":");
        out.push_str(&self.lowering.unit.constants.len().to_string());
        out.push_str(",\"verified\":");
        out.push_str(if self.lowering.verification.is_ok() {
            "true"
        } else {
            "false"
        });
        out.push_str("},\"optimizer\":");
        push_optimizer_report_json(&mut out, self.optimizer.as_ref());
        out.push('}');
        out
    }

    fn optimizer_pass_count(&self) -> usize {
        self.optimizer
            .as_ref()
            .map_or(0, OptimizationReport::enabled_pass_count)
    }
}

fn compile_pipeline_with_optimization(
    path: &str,
    opt_level: OptimizationLevel,
) -> Result<Pipeline, String> {
    let source = fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
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

fn include_loader_for(path: &str) -> Result<IncludeLoader, String> {
    let path = fs::canonicalize(path).map_err(|error| format!("{path}: {error}"))?;
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
    status: &php_runtime::ExecutionStatus,
) -> Result<(), String> {
    writeln!(stderr, "{path}: {status}").map_err(|error| error.to_string())
}

fn write_vm_compile_fatal_line<W: Write>(
    stderr: &mut W,
    pipeline: &Pipeline,
    status: &php_runtime::ExecutionStatus,
) -> Result<bool, String> {
    let Some(message) = status.message() else {
        return Ok(false);
    };
    let Some(display_message) = vm_compile_error_php_fatal_message(message) else {
        return Ok(false);
    };
    let span = if let Some((class_name, method_name)) = vm_compile_error_interface_method(message) {
        class_method_span(&pipeline.lowering.unit, &class_name, &method_name)
    } else if let Some((class_name, _, _)) = vm_compile_error_interface_method_missing(message) {
        class_span(&pipeline.lowering.unit, &class_name)
    } else if let Some((class_name, constant_name)) = vm_compile_error_interface_constant(message) {
        class_constant_span(&pipeline.lowering.unit, &class_name, &constant_name)
    } else if vm_compile_error_interface_property(message) {
        pipeline
            .lowering
            .unit
            .classes
            .iter()
            .find(|class| class.flags.is_interface)
            .map(|class| class.span)
    } else if let Some((class_name, method_name)) = vm_compile_error_child_method(message) {
        class_method_span(&pipeline.lowering.unit, &class_name, &method_name)
    } else if let Some((class_name, _property_name)) = vm_compile_error_child_property(message) {
        class_span(&pipeline.lowering.unit, &class_name)
    } else if let Some((class_name, _constant_name)) = vm_compile_error_child_constant(message) {
        class_span(&pipeline.lowering.unit, &class_name)
    } else if let Some((parent_class, method_name)) = vm_compile_error_final_method(message) {
        overriding_method_span(&pipeline.lowering.unit, &parent_class, &method_name)
    } else if let Some(class_name) = vm_compile_error_traversable_direct(message) {
        class_span(&pipeline.lowering.unit, &class_name)
    } else if let Some(class_name) = vm_compile_error_child_class(message) {
        class_span(&pipeline.lowering.unit, &class_name)
    } else {
        None
    };
    let Some(span) = span else {
        return Ok(false);
    };
    write_php_fatal_line(
        stderr,
        &pipeline.path,
        &pipeline.source,
        TextRange::new(span.start as usize, span.end as usize),
        &display_message,
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

fn vm_compile_error_php_fatal_message(message: &str) -> Option<String> {
    if let Some((class_name, interface_name, method_name)) =
        vm_compile_error_interface_method_missing(message)
    {
        return Some(format!(
            "Class {class_name} contains 1 abstract method and must therefore be declared abstract or implement the remaining method ({interface_name}::{method_name})"
        ));
    }

    message
        .strip_prefix("E_PHP_VM_METHOD_VISIBILITY_OVERRIDE: ")
        .or_else(|| message.strip_prefix("E_PHP_VM_STATIC_METHOD_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_METHOD_SIGNATURE_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_VISIBILITY: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_BODY: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_SIGNATURE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_INTERFACE_PROPERTY: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_INTERFACE_CONSTANT_VISIBILITY: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_FINAL_CLASS_EXTEND: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_FINAL_METHOD_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_PROPERTY_STATIC_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_PROPERTY_VISIBILITY_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_CLASS_CONSTANT_VISIBILITY_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_CLASS_EXTENDS_INTERFACE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_IMPLEMENTS_NON_INTERFACE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_TRAVERSABLE_DIRECT_IMPLEMENTATION: "))
        .map(str::to_owned)
}

fn vm_compile_error_interface_method(message: &str) -> Option<(String, String)> {
    if let Some(rest) = message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_VISIBILITY: ") {
        let target = rest
            .strip_prefix("Access type for interface method ")?
            .split_once("()")?
            .0;
        return split_class_method(target);
    }
    if let Some(rest) = message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_BODY: ") {
        let target = rest
            .strip_prefix("Interface function ")?
            .split_once("()")?
            .0;
        return split_class_method(target);
    }
    None
}

fn vm_compile_error_interface_property(message: &str) -> bool {
    message.starts_with("E_PHP_VM_INTERFACE_PROPERTY: ")
}

fn vm_compile_error_interface_method_missing(message: &str) -> Option<(String, String, String)> {
    let rest = message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_MISSING: ")?;
    let rest = rest.strip_prefix("class ")?;
    let (class_name, target) = rest.split_once(" must implement ")?;
    let (interface_name, method_name) = target.split_once("::")?;
    Some((
        class_name.to_owned(),
        interface_name.to_owned(),
        method_name.to_owned(),
    ))
}

fn vm_compile_error_interface_constant(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("E_PHP_VM_INTERFACE_CONSTANT_VISIBILITY: ")?;
    let target = rest
        .strip_prefix("Access type for interface constant ")?
        .split_once(" must be public")?
        .0;
    split_class_method(target)
}

fn vm_compile_error_child_method(message: &str) -> Option<(String, String)> {
    if let Some(rest) = message.strip_prefix("E_PHP_VM_METHOD_VISIBILITY_OVERRIDE: ") {
        let target = rest.strip_prefix("Access level to ")?.split_once("()")?.0;
        return split_class_method(target);
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_STATIC_METHOD_OVERRIDE: ") {
        let (parent_method, class_name) = rest
            .strip_prefix("Cannot make static method ")
            .and_then(|rest| rest.split_once("() non static in class "))
            .or_else(|| {
                rest.strip_prefix("Cannot make non static method ")
                    .and_then(|rest| rest.split_once("() static in class "))
            })?;
        let (_, method_name) = split_class_method(parent_method)?;
        return Some((class_name.to_owned(), method_name));
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_METHOD_SIGNATURE_OVERRIDE: ") {
        let target = rest
            .strip_prefix("Declaration of ")?
            .split_once(" must be compatible with ")?
            .0;
        return split_class_method(target);
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_SIGNATURE: ") {
        let target = rest
            .strip_prefix("Declaration of ")?
            .split_once(" must be compatible with ")?
            .0;
        return split_class_method(target);
    }

    None
}

fn vm_compile_error_child_class(message: &str) -> Option<String> {
    if let Some(rest) = message.strip_prefix("E_PHP_VM_FINAL_CLASS_EXTEND: ") {
        return Some(
            rest.strip_prefix("Class ")?
                .split_once(" cannot extend final class ")?
                .0
                .to_owned(),
        );
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_CLASS_EXTENDS_INTERFACE: ") {
        return Some(
            rest.strip_prefix("Class ")?
                .split_once(" cannot extend interface ")?
                .0
                .to_owned(),
        );
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_IMPLEMENTS_NON_INTERFACE: ") {
        return Some(rest.split_once(" cannot implement ")?.0.to_owned());
    }

    None
}

fn vm_compile_error_final_method(message: &str) -> Option<(String, String)> {
    let target = message
        .strip_prefix("E_PHP_VM_FINAL_METHOD_OVERRIDE: ")?
        .strip_prefix("Cannot override final method ")?
        .split_once("()")?
        .0;
    split_class_method(target)
}

fn vm_compile_error_traversable_direct(message: &str) -> Option<String> {
    let rest = message.strip_prefix("E_PHP_VM_TRAVERSABLE_DIRECT_IMPLEMENTATION: ")?;
    Some(
        rest.strip_prefix("Class ")?
            .split_once(" must implement interface Traversable ")?
            .0
            .to_owned(),
    )
}

fn vm_compile_error_child_property(message: &str) -> Option<(String, String)> {
    if let Some(rest) = message.strip_prefix("E_PHP_VM_PROPERTY_VISIBILITY_OVERRIDE: ") {
        let target = rest
            .strip_prefix("Access level to ")?
            .split_once(" must be ")?
            .0;
        return split_class_property(target);
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_PROPERTY_STATIC_OVERRIDE: ") {
        let target = rest
            .split_once(" as static ")
            .or_else(|| rest.split_once(" as non static "))?
            .1;
        return split_class_property(target);
    }

    None
}

fn vm_compile_error_child_constant(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("E_PHP_VM_CLASS_CONSTANT_VISIBILITY_OVERRIDE: ")?;
    let target = rest
        .strip_prefix("Access level to ")?
        .split_once(" must be ")?
        .0;
    split_class_constant(target)
}

fn split_class_method(target: &str) -> Option<(String, String)> {
    let (class_name, method_name) = target.rsplit_once("::")?;
    let method_name = method_name
        .split_once('(')
        .map_or(method_name, |(name, _)| name);
    Some((class_name.to_owned(), method_name.to_owned()))
}

fn split_class_property(target: &str) -> Option<(String, String)> {
    let (class_name, property_name) = target.rsplit_once("::$")?;
    Some((class_name.to_owned(), property_name.to_owned()))
}

fn split_class_constant(target: &str) -> Option<(String, String)> {
    let (class_name, constant_name) = target.rsplit_once("::")?;
    Some((class_name.to_owned(), constant_name.to_owned()))
}

fn class_span(unit: &IrUnit, class_name: &str) -> Option<php_ir::IrSpan> {
    let normalized_class = normalize_class_name(class_name);
    unit.classes
        .iter()
        .find(|class| normalize_class_name(&class.name) == normalized_class)
        .map(|class| class.span)
}

fn class_method_span(unit: &IrUnit, class_name: &str, method_name: &str) -> Option<php_ir::IrSpan> {
    let normalized_class = normalize_class_name(class_name);
    let normalized_method = method_name.to_ascii_lowercase();
    let class = unit
        .classes
        .iter()
        .find(|class| normalize_class_name(&class.name) == normalized_class)?;
    let method = class
        .methods
        .iter()
        .find(|method| method.name.eq_ignore_ascii_case(&normalized_method))?;
    unit.functions
        .get(method.function.index())
        .map(|function| function.span)
}

fn class_constant_span(
    unit: &IrUnit,
    class_name: &str,
    constant_name: &str,
) -> Option<php_ir::IrSpan> {
    let normalized_class = normalize_class_name(class_name);
    let class = unit
        .classes
        .iter()
        .find(|class| normalize_class_name(&class.name) == normalized_class)?;
    class
        .constants
        .iter()
        .find(|constant| constant.name.eq_ignore_ascii_case(constant_name))
        .map(|constant| constant.span)
}

fn overriding_method_span(
    unit: &IrUnit,
    parent_class: &str,
    method_name: &str,
) -> Option<php_ir::IrSpan> {
    let normalized_method = method_name.to_ascii_lowercase();
    unit.classes.iter().find_map(|class| {
        if !unit_class_extends(unit, class, parent_class) {
            return None;
        }
        let method = class
            .methods
            .iter()
            .find(|method| method.name.eq_ignore_ascii_case(&normalized_method))?;
        unit.functions
            .get(method.function.index())
            .map(|function| function.span)
    })
}

fn unit_class_extends(unit: &IrUnit, class: &php_ir::ClassEntry, parent_class: &str) -> bool {
    let normalized_parent = normalize_class_name(parent_class);
    let mut next = class.parent.as_deref();
    while let Some(name) = next {
        if normalize_class_name(name) == normalized_parent {
            return true;
        }
        next = unit
            .classes
            .iter()
            .find(|candidate| normalize_class_name(&candidate.name) == normalize_class_name(name))
            .and_then(|candidate| candidate.parent.as_deref());
    }
    false
}

fn write_runtime_diagnostics<W: Write>(
    stderr: &mut W,
    path: &str,
    result: &php_vm::VmResult,
) -> Result<(), String> {
    let php_output = result.output.to_string_lossy();
    for diagnostic in &result.diagnostics {
        if result.status.is_success() && php_output.contains(diagnostic.message()) {
            continue;
        }
        writeln!(
            stderr,
            "{path}: runtime-diagnostic: {}",
            diagnostic.to_json()
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn write_trace<W: Write>(stderr: &mut W, trace: &[String]) -> Result<(), String> {
    writeln!(stderr, "vm-trace:").map_err(|error| error.to_string())?;
    for line in trace {
        writeln!(stderr, "  {line}").map_err(|error| error.to_string())?;
    }
    Ok(())
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

struct CompileOptions<'a> {
    path: &'a str,
    json: bool,
    opt_level: OptimizationLevel,
}

fn parse_compile_args(args: &[String]) -> Result<CompileOptions<'_>, String> {
    let mut path = None;
    let mut json = false;
    let mut opt_level = OptimizationLevel::O0;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => json = true,
            "--opt-level" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("compile --opt-level requires <level>".to_string());
                };
                opt_level = parse_optimization_level(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--opt-level=") => {
                opt_level = parse_optimization_level(value)?;
            }
            arg if path.is_none() => path = Some(arg),
            arg => return Err(format!("unexpected compile argument `{arg}`")),
        }
        index += 1;
    }
    let Some(path) = path else {
        return Err("compile requires <path.php>".to_string());
    };
    Ok(CompileOptions {
        path,
        json,
        opt_level,
    })
}

struct DumpIrOptions<'a> {
    path: &'a str,
    with_source: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct BytecodePatternReport {
    blocks: u64,
    instructions: u64,
    pairs: BTreeMap<String, u64>,
    triples: BTreeMap<String, u64>,
}

struct RunOptions<'a> {
    path: &'a str,
    script_args: Vec<String>,
    env: Vec<(String, String)>,
    trace: bool,
    trace_runtime: bool,
    counters_json: Option<String>,
    bytecode_cache: BytecodeCacheOptions,
    opt_level: OptimizationLevel,
    execution_format: ExecutionFormat,
    superinstructions: SuperinstructionMode,
    quickening: QuickeningMode,
    inline_caches: InlineCacheMode,
    jit: JitMode,
    jit_threshold: u64,
    jit_blacklist: JitBlacklistMode,
    jit_dump_clif: Option<String>,
    jit_stats: JitStatsMode,
    tiering: TieringOptions,
    tiering_stats_json: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EnginePreset {
    Baseline,
    Fast,
    ExperimentalJit,
}

impl EnginePreset {
    fn config(self) -> EnginePresetConfig {
        match self {
            Self::Baseline => EnginePresetConfig {
                bytecode_cache_mode: BytecodeCacheMode::Off,
                opt_level: OptimizationLevel::O0,
                execution_format: ExecutionFormat::Ir,
                superinstructions: SuperinstructionMode::Off,
                quickening: QuickeningMode::Off,
                inline_caches: InlineCacheMode::Off,
                jit: JitMode::Off,
                jit_blacklist: JitBlacklistMode::On,
                tiering: TieringOptions::default(),
            },
            Self::Fast => EnginePresetConfig {
                bytecode_cache_mode: BytecodeCacheMode::Off,
                opt_level: OptimizationLevel::O2,
                execution_format: ExecutionFormat::Auto,
                superinstructions: SuperinstructionMode::Off,
                quickening: QuickeningMode::On,
                inline_caches: InlineCacheMode::On,
                jit: JitMode::Off,
                jit_blacklist: JitBlacklistMode::On,
                tiering: TieringOptions::default(),
            },
            Self::ExperimentalJit => EnginePresetConfig {
                bytecode_cache_mode: BytecodeCacheMode::Off,
                opt_level: OptimizationLevel::O2,
                execution_format: ExecutionFormat::Auto,
                superinstructions: SuperinstructionMode::Off,
                quickening: QuickeningMode::On,
                inline_caches: InlineCacheMode::On,
                jit: JitMode::Cranelift,
                jit_blacklist: JitBlacklistMode::On,
                tiering: TieringOptions::default(),
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EnginePresetConfig {
    bytecode_cache_mode: BytecodeCacheMode,
    opt_level: OptimizationLevel,
    execution_format: ExecutionFormat,
    superinstructions: SuperinstructionMode,
    quickening: QuickeningMode,
    inline_caches: InlineCacheMode,
    jit: JitMode,
    jit_blacklist: JitBlacklistMode,
    tiering: TieringOptions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BytecodeCacheMode {
    Off,
    Read,
    Write,
    ReadWrite,
}

impl BytecodeCacheMode {
    fn can_read(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }

    fn can_write(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Read => "read",
            Self::Write => "write",
            Self::ReadWrite => "read-write",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BytecodeCacheOptions {
    mode: BytecodeCacheMode,
    dir: Option<PathBuf>,
    stats: bool,
    clear: bool,
}

impl Default for BytecodeCacheOptions {
    fn default() -> Self {
        Self {
            mode: BytecodeCacheMode::Off,
            dir: None,
            stats: false,
            clear: false,
        }
    }
}

#[derive(Clone, Debug)]
struct BytecodeCacheContext {
    fingerprint: CacheFingerprint,
    cache_file: PathBuf,
}

#[derive(Clone, Debug)]
struct BytecodeCacheStats {
    mode: BytecodeCacheMode,
    cache_file: Option<PathBuf>,
    hit: bool,
    miss: bool,
    wrote: bool,
    cleared: bool,
    load_error: Option<String>,
    store_error: Option<String>,
}

impl BytecodeCacheStats {
    fn new(mode: BytecodeCacheMode) -> Self {
        Self {
            mode,
            cache_file: None,
            hit: false,
            miss: false,
            wrote: false,
            cleared: false,
            load_error: None,
            store_error: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReportFormat {
    Markdown,
    Html,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum JitStatsMode {
    #[default]
    Off,
    Json,
}

impl JitStatsMode {
    fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

struct ReportOptions<'a> {
    path: &'a str,
    format: ReportFormat,
}

fn parse_dump_ir_args(args: &[String]) -> Result<DumpIrOptions<'_>, String> {
    let mut path = None;
    let mut with_source = false;
    for arg in args {
        if arg == "--with-source" {
            with_source = true;
        } else if path.is_none() {
            path = Some(arg.as_str());
        } else {
            return Err(format!("unexpected dump-ir argument `{arg}`"));
        }
    }
    let Some(path) = path else {
        return Err("dump-ir requires <path.php>".to_string());
    };
    Ok(DumpIrOptions { path, with_source })
}

fn parse_dump_bytecode_patterns_args(args: &[String]) -> Result<(&str, bool), String> {
    let mut path = None;
    let mut json = false;
    for arg in args {
        if arg == "--json" {
            json = true;
        } else if path.is_none() {
            path = Some(arg.as_str());
        } else {
            return Err(format!(
                "unexpected dump-bytecode-patterns argument `{arg}`"
            ));
        }
    }
    let Some(path) = path else {
        return Err("dump-bytecode-patterns requires <path.php>".to_string());
    };
    Ok((path, json))
}

fn parse_run_args(args: &[String]) -> Result<RunOptions<'_>, String> {
    let Some(_) = args.first() else {
        return Err("run requires <path.php>".to_string());
    };

    let mut path = None;
    let mut env = Vec::new();
    let mut trace = false;
    let mut trace_runtime = false;
    let mut counters_json = None;
    let mut bytecode_cache = BytecodeCacheOptions::default();
    let mut opt_level = OptimizationLevel::O0;
    let mut execution_format = ExecutionFormat::Ir;
    let mut superinstructions = SuperinstructionMode::Off;
    let mut quickening = QuickeningMode::Off;
    let mut inline_caches = InlineCacheMode::Off;
    let mut jit = JitMode::Off;
    let mut jit_threshold = TieringOptions::default().function_entry_threshold;
    let mut jit_blacklist = JitBlacklistMode::On;
    let mut jit_dump_clif = None;
    let mut jit_stats = JitStatsMode::Off;
    let mut tiering = TieringOptions::default();
    let mut tiering_stats_json = None;
    let mut tiering_function_threshold_explicit = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--trace" => trace = true,
            "--trace-runtime" => trace_runtime = true,
            "--engine-preset" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(
                        "run --engine-preset requires baseline, fast, or experimental-jit"
                            .to_string(),
                    );
                };
                let config = parse_engine_preset(value)?.config();
                bytecode_cache.mode = config.bytecode_cache_mode;
                opt_level = config.opt_level;
                execution_format = config.execution_format;
                superinstructions = config.superinstructions;
                quickening = config.quickening;
                inline_caches = config.inline_caches;
                jit = config.jit;
                jit_blacklist = config.jit_blacklist;
                tiering = config.tiering;
                jit_threshold = tiering.function_entry_threshold;
                tiering_function_threshold_explicit = false;
            }
            arg if let Some(value) = arg.strip_prefix("--engine-preset=") => {
                let config = parse_engine_preset(value)?.config();
                bytecode_cache.mode = config.bytecode_cache_mode;
                opt_level = config.opt_level;
                execution_format = config.execution_format;
                superinstructions = config.superinstructions;
                quickening = config.quickening;
                inline_caches = config.inline_caches;
                jit = config.jit;
                jit_blacklist = config.jit_blacklist;
                tiering = config.tiering;
                jit_threshold = tiering.function_entry_threshold;
                tiering_function_threshold_explicit = false;
            }
            "--bytecode-cache" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(
                        "run --bytecode-cache requires off, read, write, or read-write".to_string(),
                    );
                };
                bytecode_cache.mode = parse_bytecode_cache_mode(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--bytecode-cache=") => {
                bytecode_cache.mode = parse_bytecode_cache_mode(value)?;
            }
            "--bytecode-cache-dir" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --bytecode-cache-dir requires <path>".to_string());
                };
                bytecode_cache.dir = Some(PathBuf::from(value));
            }
            arg if let Some(value) = arg.strip_prefix("--bytecode-cache-dir=") => {
                bytecode_cache.dir = Some(PathBuf::from(value));
            }
            "--bytecode-cache-stats" => bytecode_cache.stats = true,
            "--clear-bytecode-cache" => bytecode_cache.clear = true,
            "--opt-level" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --opt-level requires <level>".to_string());
                };
                opt_level = parse_optimization_level(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--opt-level=") => {
                opt_level = parse_optimization_level(value)?;
            }
            "--exec-format" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --exec-format requires ir, auto, or bytecode".to_string());
                };
                execution_format = parse_execution_format(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--exec-format=") => {
                execution_format = parse_execution_format(value)?;
            }
            "--superinstructions" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --superinstructions requires off or on".to_string());
                };
                superinstructions = parse_superinstruction_mode(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--superinstructions=") => {
                superinstructions = parse_superinstruction_mode(value)?;
            }
            "--quickening" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --quickening requires off or on".to_string());
                };
                quickening = parse_quickening_mode(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--quickening=") => {
                quickening = parse_quickening_mode(value)?;
            }
            "--inline-caches" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --inline-caches requires off or on".to_string());
                };
                inline_caches = parse_inline_cache_mode(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--inline-caches=") => {
                inline_caches = parse_inline_cache_mode(value)?;
            }
            "--jit" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --jit requires off, noop, or cranelift".to_string());
                };
                jit = parse_jit_mode(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--jit=") => {
                jit = parse_jit_mode(value)?;
            }
            "--jit-threshold" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --jit-threshold requires <count>".to_string());
                };
                jit_threshold = parse_u64_option(value, "jit-threshold")?;
                if !tiering_function_threshold_explicit {
                    tiering.function_entry_threshold = jit_threshold;
                }
            }
            arg if let Some(value) = arg.strip_prefix("--jit-threshold=") => {
                jit_threshold = parse_u64_option(value, "jit-threshold")?;
                if !tiering_function_threshold_explicit {
                    tiering.function_entry_threshold = jit_threshold;
                }
            }
            "--jit-max-compile-us" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --jit-max-compile-us requires <microseconds>".to_string());
                };
                tiering.jit_max_compile_us = parse_u64_option(value, "jit-max-compile-us")?;
            }
            arg if let Some(value) = arg.strip_prefix("--jit-max-compile-us=") => {
                tiering.jit_max_compile_us = parse_u64_option(value, "jit-max-compile-us")?;
            }
            "--jit-max-functions" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --jit-max-functions requires <count>".to_string());
                };
                tiering.jit_max_functions = parse_u64_option(value, "jit-max-functions")?;
            }
            arg if let Some(value) = arg.strip_prefix("--jit-max-functions=") => {
                tiering.jit_max_functions = parse_u64_option(value, "jit-max-functions")?;
            }
            "--jit-eager" => {
                tiering.jit_eager = true;
                jit_threshold = 1;
                if !tiering_function_threshold_explicit {
                    tiering.function_entry_threshold = 1;
                }
            }
            "--jit-blacklist" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --jit-blacklist requires off or on".to_string());
                };
                jit_blacklist = parse_jit_blacklist_mode(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--jit-blacklist=") => {
                jit_blacklist = parse_jit_blacklist_mode(value)?;
            }
            "--jit-dump-clif" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --jit-dump-clif requires <path>".to_string());
                };
                jit_dump_clif = Some(value.clone());
            }
            arg if let Some(value) = arg.strip_prefix("--jit-dump-clif=") => {
                jit_dump_clif = Some(value.to_owned());
            }
            "--jit-stats" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --jit-stats requires json".to_string());
                };
                jit_stats = parse_jit_stats_mode(value)?;
            }
            arg if let Some(value) = arg.strip_prefix("--jit-stats=") => {
                jit_stats = parse_jit_stats_mode(value)?;
            }
            "--tiering" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --tiering requires off or on".to_string());
                };
                tiering.enabled = parse_on_off(value, "tiering")?;
            }
            arg if let Some(value) = arg.strip_prefix("--tiering=") => {
                tiering.enabled = parse_on_off(value, "tiering")?;
            }
            "--tiering-function-threshold" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --tiering-function-threshold requires <count>".to_string());
                };
                tiering_function_threshold_explicit = true;
                tiering.function_entry_threshold =
                    parse_u64_option(value, "tiering-function-threshold")?;
            }
            arg if let Some(value) = arg.strip_prefix("--tiering-function-threshold=") => {
                tiering_function_threshold_explicit = true;
                tiering.function_entry_threshold =
                    parse_u64_option(value, "tiering-function-threshold")?;
            }
            "--tiering-loop-threshold" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --tiering-loop-threshold requires <count>".to_string());
                };
                tiering.loop_backedge_threshold =
                    parse_u64_option(value, "tiering-loop-threshold")?;
            }
            arg if let Some(value) = arg.strip_prefix("--tiering-loop-threshold=") => {
                tiering.loop_backedge_threshold =
                    parse_u64_option(value, "tiering-loop-threshold")?;
            }
            "--tiering-ic-stability-threshold" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --tiering-ic-stability-threshold requires <score>".to_string());
                };
                tiering.ic_stability_threshold =
                    parse_i64_option(value, "tiering-ic-stability-threshold")?;
            }
            arg if let Some(value) = arg.strip_prefix("--tiering-ic-stability-threshold=") => {
                tiering.ic_stability_threshold =
                    parse_i64_option(value, "tiering-ic-stability-threshold")?;
            }
            "--tiering-guard-failure-threshold" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(
                        "run --tiering-guard-failure-threshold requires <count>".to_string()
                    );
                };
                tiering.guard_failure_threshold =
                    parse_u64_option(value, "tiering-guard-failure-threshold")?;
            }
            arg if let Some(value) = arg.strip_prefix("--tiering-guard-failure-threshold=") => {
                tiering.guard_failure_threshold =
                    parse_u64_option(value, "tiering-guard-failure-threshold")?;
            }
            "--tiering-stats-json" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --tiering-stats-json requires <path>".to_string());
                };
                tiering_stats_json = Some(value.clone());
                tiering.collect_stats = true;
            }
            arg if let Some(value) = arg.strip_prefix("--tiering-stats-json=") => {
                tiering_stats_json = Some(value.to_owned());
                tiering.collect_stats = true;
            }
            "--counters-json" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --counters-json requires <path>".to_string());
                };
                counters_json = Some(value.clone());
            }
            arg if let Some(value) = arg.strip_prefix("--counters-json=") => {
                counters_json = Some(value.to_owned());
            }
            "--env" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("run --env requires KEY=VALUE".to_string());
                };
                env.push(parse_env_assignment(value)?);
            }
            arg if let Some(value) = arg.strip_prefix("--env=") => {
                env.push(parse_env_assignment(value)?);
            }
            "--" => {
                let Some(path) = path else {
                    return Err("run requires <path.php> before `--`".to_string());
                };
                return Ok(RunOptions {
                    path,
                    script_args: args[index + 1..].to_vec(),
                    env,
                    trace,
                    trace_runtime,
                    counters_json,
                    bytecode_cache,
                    opt_level,
                    execution_format,
                    superinstructions,
                    quickening,
                    inline_caches,
                    jit,
                    jit_threshold,
                    jit_blacklist,
                    jit_dump_clif,
                    jit_stats,
                    tiering,
                    tiering_stats_json,
                });
            }
            arg if path.is_none() => path = Some(arg),
            unexpected => {
                return Err(format!(
                    "unexpected run argument `{unexpected}`; pass script arguments after `--`"
                ));
            }
        }
        index += 1;
    }
    let Some(path) = path else {
        return Err("run requires <path.php>".to_string());
    };
    Ok(RunOptions {
        path,
        script_args: Vec::new(),
        env,
        trace,
        trace_runtime,
        counters_json,
        bytecode_cache,
        opt_level,
        execution_format,
        superinstructions,
        quickening,
        inline_caches,
        jit,
        jit_threshold,
        jit_blacklist,
        jit_dump_clif,
        jit_stats,
        tiering,
        tiering_stats_json,
    })
}

fn parse_on_off(value: &str, flag: &str) -> Result<bool, String> {
    match value {
        "off" => Ok(false),
        "on" => Ok(true),
        _ => Err(format!(
            "unsupported {flag} mode `{value}`; expected off or on"
        )),
    }
}

fn parse_engine_preset(value: &str) -> Result<EnginePreset, String> {
    match value {
        "baseline" => Ok(EnginePreset::Baseline),
        "fast" => Ok(EnginePreset::Fast),
        "experimental-jit" => Ok(EnginePreset::ExperimentalJit),
        _ => Err(format!(
            "unsupported engine preset `{value}`; expected baseline, fast, or experimental-jit"
        )),
    }
}

fn parse_jit_blacklist_mode(value: &str) -> Result<JitBlacklistMode, String> {
    Ok(if parse_on_off(value, "jit-blacklist")? {
        JitBlacklistMode::On
    } else {
        JitBlacklistMode::Off
    })
}

fn parse_u64_option(value: &str, flag: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("run --{flag} requires a non-negative integer"))
}

fn parse_i64_option(value: &str, flag: &str) -> Result<i64, String> {
    value
        .parse::<i64>()
        .map_err(|_| format!("run --{flag} requires an integer"))
}

fn parse_quickening_mode(value: &str) -> Result<QuickeningMode, String> {
    match value {
        "off" => Ok(QuickeningMode::Off),
        "on" => Ok(QuickeningMode::On),
        _ => Err(format!(
            "unsupported quickening mode `{value}`; expected off or on"
        )),
    }
}

fn parse_execution_format(value: &str) -> Result<ExecutionFormat, String> {
    match value {
        "ir" => Ok(ExecutionFormat::Ir),
        "auto" => Ok(ExecutionFormat::Auto),
        "bytecode" => Ok(ExecutionFormat::Bytecode),
        _ => Err(format!(
            "unsupported exec-format mode `{value}`; expected ir, auto, or bytecode"
        )),
    }
}

fn parse_superinstruction_mode(value: &str) -> Result<SuperinstructionMode, String> {
    match value {
        "off" => Ok(SuperinstructionMode::Off),
        "on" => Ok(SuperinstructionMode::On),
        _ => Err(format!(
            "unsupported superinstructions mode `{value}`; expected off or on"
        )),
    }
}

fn parse_inline_cache_mode(value: &str) -> Result<InlineCacheMode, String> {
    match value {
        "off" => Ok(InlineCacheMode::Off),
        "on" => Ok(InlineCacheMode::On),
        _ => Err(format!(
            "unsupported inline-cache mode `{value}`; expected off or on"
        )),
    }
}

fn parse_jit_mode(value: &str) -> Result<JitMode, String> {
    match value {
        "off" => Ok(JitMode::Off),
        "noop" => Ok(JitMode::Noop),
        "cranelift" => Ok(JitMode::Cranelift),
        _ => Err(format!(
            "unsupported jit mode `{value}`; expected off, noop, or cranelift"
        )),
    }
}

fn parse_jit_stats_mode(value: &str) -> Result<JitStatsMode, String> {
    match value {
        "json" => Ok(JitStatsMode::Json),
        _ => Err(format!(
            "unsupported jit stats mode `{value}`; expected json"
        )),
    }
}

fn parse_bytecode_cache_mode(value: &str) -> Result<BytecodeCacheMode, String> {
    match value {
        "off" => Ok(BytecodeCacheMode::Off),
        "read" => Ok(BytecodeCacheMode::Read),
        "write" => Ok(BytecodeCacheMode::Write),
        "read-write" => Ok(BytecodeCacheMode::ReadWrite),
        _ => Err(format!(
            "unsupported bytecode cache mode `{value}`; expected off, read, write, or read-write"
        )),
    }
}

fn parse_optimization_level(value: &str) -> Result<OptimizationLevel, String> {
    value
        .parse()
        .map_err(|error: php_optimizer::ParseOptimizationLevelError| error.to_string())
}

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
    let mut json = String::new();
    json.push_str("{\"bytecode_cache\":{");
    json.push_str("\"mode\":\"");
    json.push_str(stats.mode.as_str());
    json.push_str("\",\"hit\":");
    json.push_str(if stats.hit { "true" } else { "false" });
    json.push_str(",\"miss\":");
    json.push_str(if stats.miss { "true" } else { "false" });
    json.push_str(",\"wrote\":");
    json.push_str(if stats.wrote { "true" } else { "false" });
    json.push_str(",\"cleared\":");
    json.push_str(if stats.cleared { "true" } else { "false" });
    if let Some(path) = &stats.cache_file {
        json.push_str(",\"file\":\"");
        json.push_str(&escape_json(&path.to_string_lossy()));
        json.push('"');
    }
    if let Some(error) = &stats.load_error {
        json.push_str(",\"load_error\":\"");
        json.push_str(&escape_json(error));
        json.push('"');
    }
    if let Some(error) = &stats.store_error {
        json.push_str(",\"store_error\":\"");
        json.push_str(&escape_json(error));
        json.push('"');
    }
    json.push_str("}}\n");
    stderr
        .write_all(json.as_bytes())
        .map_err(|error| error.to_string())
}

fn write_counters_json(path: String, counters: &php_vm::VmCounters) -> Result<(), String> {
    let path = Path::new(&path);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    fs::write(path, counters.to_json()).map_err(|error| format!("{}: {error}", path.display()))
}

fn write_jit_stats_json<W: Write>(
    stderr: &mut W,
    counters: &php_vm::VmCounters,
    options: &RunOptions<'_>,
    eligibility_json: &str,
) -> Result<(), String> {
    let dump_clif = options.jit_dump_clif.as_deref().unwrap_or("");
    let side_exit_reasons = write_string_u64_map_json(&counters.jit_side_exit_reasons);
    let blacklist_reasons = write_string_u64_map_json(&counters.jit_blacklist_reasons);
    let compile_descriptors = write_jit_compile_descriptors_json(&counters.jit_compile_descriptors);
    writeln!(
        stderr,
        "{{\"jit\":{{\"mode\":\"{}\",\"threshold\":{},\"eager\":{},\"max_compile_us\":{},\"max_functions\":{},\"blacklist\":\"{}\",\"dump_clif\":\"{}\",\"compile_attempts\":{},\"compiled\":{},\"executed\":{},\"bailouts\":{},\"code_bytes\":{},\"compile_time_nanos\":{},\"side_exits\":{},\"side_exit_reasons\":{},\"guard_failures\":{},\"blacklisted_regions\":{},\"blacklist_reasons\":{},\"tiering_cold_functions\":{},\"tiering_hot_functions\":{},\"tiering_eager_functions\":{},\"tiering_blacklist_rejections\":{},\"tiering_budget_rejections\":{},\"helper_calls\":{},\"fast_path_hits\":{},\"packed_fetch_fast_hits\":{},\"packed_fetch_bounds_exits\":{},\"packed_fetch_layout_exits\":{},\"packed_foreach_sum_fast_hits\":{},\"packed_foreach_sum_layout_exits\":{},\"packed_foreach_sum_overflow_exits\":{},\"known_call_fast_hits\":{},\"known_call_guard_exits\":{},\"known_call_slow_calls\":{},\"direct_call_hits\":{},\"direct_call_fallbacks\":{},\"property_load_fast_hits\":{},\"property_load_guard_exits\":{},\"property_load_layout_exits\":{},\"property_load_uninitialized_exits\":{},\"property_load_slow_calls\":{},\"string_concat_fast_path_hits\":{},\"string_concat_fast_path_misses\":{},\"overflow_exits\":{},\"slow_path_calls\":{},\"compile_cache_hits\":{},\"compile_cache_misses\":{},\"compile_cache_invalidations\":{},\"compile_descriptors\":{},\"eligibility\":{}}}}}",
        options.jit.as_str(),
        options.jit_threshold,
        options.tiering.jit_eager,
        options.tiering.jit_max_compile_us,
        options.tiering.jit_max_functions,
        options.jit_blacklist.as_str(),
        escape_json(dump_clif),
        counters.jit_compile_attempts,
        counters.jit_compiled,
        counters.jit_executed,
        counters.jit_bailouts,
        counters.jit_code_bytes,
        counters.jit_compile_time_nanos,
        counters.jit_side_exits,
        side_exit_reasons,
        counters.jit_guard_failures,
        counters.jit_blacklisted_regions,
        blacklist_reasons,
        counters.jit_tiering_cold_functions,
        counters.jit_tiering_hot_functions,
        counters.jit_tiering_eager_functions,
        counters.jit_tiering_blacklist_rejections,
        counters.jit_tiering_budget_rejections,
        counters.jit_helper_calls,
        counters.jit_fast_path_hits,
        counters.packed_fetch_fast_hits,
        counters.packed_fetch_bounds_exits,
        counters.packed_fetch_layout_exits,
        counters.packed_foreach_sum_fast_hits,
        counters.packed_foreach_sum_layout_exits,
        counters.packed_foreach_sum_overflow_exits,
        counters.known_call_fast_hits,
        counters.known_call_guard_exits,
        counters.known_call_slow_calls,
        counters.direct_call_hits,
        counters.direct_call_fallbacks,
        counters.property_load_fast_hits,
        counters.property_load_guard_exits,
        counters.property_load_layout_exits,
        counters.property_load_uninitialized_exits,
        counters.property_load_slow_calls,
        counters.string_concat_fast_path_hits,
        counters.string_concat_fast_path_misses,
        counters.jit_overflow_exits,
        counters.jit_slow_path_calls,
        counters.jit_compile_cache_hits,
        counters.jit_compile_cache_misses,
        counters.jit_compile_cache_invalidations,
        compile_descriptors,
        eligibility_json
    )
    .map_err(|error| error.to_string())
}

fn write_string_u64_map_json(values: &std::collections::BTreeMap<String, u64>) -> String {
    let mut json = String::from("{");
    for (index, (key, value)) in values.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push('"');
        json.push_str(&escape_json(key));
        json.push_str("\":");
        json.push_str(&value.to_string());
    }
    json.push('}');
    json
}

fn write_jit_compile_descriptors_json(values: &[php_vm::JitCompileDescriptor]) -> String {
    let mut json = String::from("[");
    for (index, descriptor) in values.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push('{');
        json.push_str("\"function_id\":");
        json.push_str(&descriptor.function_id.to_string());
        json.push_str(",\"function_name\":\"");
        json.push_str(&escape_json(&descriptor.function_name));
        json.push_str("\",\"ir_fingerprint\":\"");
        json.push_str(&escape_json(&descriptor.ir_fingerprint));
        json.push_str("\",\"code_bytes\":");
        json.push_str(&descriptor.code_bytes.to_string());
        json.push_str(",\"compile_time_nanos\":");
        json.push_str(&descriptor.compile_time_nanos.to_string());
        json.push_str(",\"target_isa\":\"");
        json.push_str(&escape_json(&descriptor.target_isa));
        json.push_str("\",\"abi_hash\":");
        json.push_str(&descriptor.abi_hash.to_string());
        json.push_str(",\"config_hash\":");
        json.push_str(&descriptor.config_hash.to_string());
        json.push('}');
    }
    json.push(']');
    json
}

#[cfg(feature = "jit-cranelift")]
fn build_jit_eligibility_json(unit: &php_ir::IrUnit, jit: JitMode) -> String {
    let mut reports = Vec::new();
    if jit.requires_cranelift() {
        for index in 0..unit.functions.len() {
            reports.push(php_jit::analyze_jit_eligibility(
                unit,
                php_ir::FunctionId::new(index as u32),
            ));
        }
    }
    write_jit_eligibility_reports_json(&reports)
}

#[cfg(not(feature = "jit-cranelift"))]
fn build_jit_eligibility_json(_unit: &php_ir::IrUnit, _jit: JitMode) -> String {
    write_empty_jit_eligibility_json()
}

#[cfg(feature = "jit-cranelift")]
fn write_jit_eligibility_reports_json(reports: &[php_jit::JitEligibilityReport]) -> String {
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
    let mut json = String::new();
    json.push('{');
    json.push_str("\"considered\":");
    json.push_str(&considered.to_string());
    json.push_str(",\"eligible\":");
    json.push_str(&eligible.to_string());
    json.push_str(",\"non_eligible\":");
    json.push_str(&(rejected + unknown).to_string());
    json.push_str(",\"rejected\":");
    json.push_str(&rejected.to_string());
    json.push_str(",\"unknown\":");
    json.push_str(&unknown.to_string());
    json.push_str(",\"reports\":[");
    for (index, report) in reports.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&report.to_json());
    }
    json.push_str("]}");
    json
}

#[cfg(not(feature = "jit-cranelift"))]
fn write_empty_jit_eligibility_json() -> String {
    "{\"considered\":0,\"eligible\":0,\"non_eligible\":0,\"rejected\":0,\"unknown\":0,\"reports\":[]}"
        .to_owned()
}

fn write_tiering_stats_json(path: String, stats: &php_vm::TieringStats) -> Result<(), String> {
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

fn print_usage<W: Write>(stdout: &mut W) -> Result<(), String> {
    writeln!(
        stdout,
        "Usage:\n  php-vm compile <file> [--json] [--opt-level 0|1|2]\n  php-vm dump-ir <file> [--with-source]\n  php-vm dump-bytecode-patterns <file> [--json]\n  php-vm dump-cranelift-clif\n  php-vm run [--trace] [--trace-runtime] [--env KEY=VALUE] [--engine-preset baseline|fast|experimental-jit] [--bytecode-cache=off|read|write|read-write] [--bytecode-cache-dir <path>] [--bytecode-cache-stats] [--clear-bytecode-cache] [--opt-level 0|1|2] [--exec-format ir|auto|bytecode] [--superinstructions off|on] [--quickening off|on] [--inline-caches off|on] [--jit off|noop|cranelift] [--jit-threshold N] [--jit-max-compile-us N] [--jit-max-functions N] [--jit-eager] [--jit-blacklist off|on] [--jit-dump-clif PATH] [--jit-stats json] [--tiering off|on] [--tiering-function-threshold N] [--tiering-loop-threshold N] [--tiering-ic-stability-threshold N] [--tiering-guard-failure-threshold N] [--tiering-stats-json <path>] <file> [-- arg ...]\n  php-vm report <file> [--format markdown|html]\n  php-vm compare <file>\n\nStatus:\n  {}\n  {}\n  {}\n  {}",
        php_ir::ir_skeleton_status(),
        php_runtime::runtime_skeleton_status(),
        php_vm::vm_skeleton_status(),
        todo_cli::cli_skeleton_status()
    )
    .map_err(|error| error.to_string())
}

fn push_parser_diagnostics_json(out: &mut String, path: &str, frontend: &FrontendResult) {
    out.push('[');
    for (index, diagnostic) in frontend.parser_diagnostics().iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"path\":\"");
        out.push_str(&escape_json(path));
        out.push_str("\",\"id\":\"");
        out.push_str(diagnostic.id.as_str());
        out.push_str("\",\"message\":\"");
        out.push_str(&escape_json(&diagnostic.message));
        out.push_str("\",\"span\":");
        push_range_json(out, Some(diagnostic.span));
        out.push('}');
    }
    out.push(']');
}

fn push_semantic_diagnostics_json(out: &mut String, path: &str, frontend: &FrontendResult) {
    out.push('[');
    for (index, diagnostic) in frontend.semantic_diagnostics().iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"path\":\"");
        out.push_str(&escape_json(path));
        out.push_str("\",\"id\":\"");
        out.push_str(diagnostic.id().as_str());
        out.push_str("\",\"severity\":\"");
        out.push_str(diagnostic.severity().as_str());
        out.push_str("\",\"message\":\"");
        out.push_str(&escape_json(diagnostic.message()));
        out.push_str("\",\"span\":");
        push_range_json(out, diagnostic.span());
        out.push('}');
    }
    out.push(']');
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

fn bytecode_patterns_json(
    path: &str,
    dense: &DenseBytecodeUnit,
    report: &BytecodePatternReport,
) -> String {
    let mut out = String::new();
    out.push_str("{\"ok\":true");
    out.push_str(",\"path\":\"");
    out.push_str(&escape_json(path));
    out.push('"');
    out.push_str(",\"functions\":");
    out.push_str(&dense.functions.len().to_string());
    out.push_str(",\"blocks\":");
    out.push_str(&report.blocks.to_string());
    out.push_str(",\"instructions\":");
    out.push_str(&report.instructions.to_string());
    out.push_str(",\"pairs\":");
    push_string_u64_map_json(&mut out, &report.pairs);
    out.push_str(",\"triples\":");
    push_string_u64_map_json(&mut out, &report.triples);
    out.push('}');
    out
}

fn push_string_u64_map_json(out: &mut String, values: &BTreeMap<String, u64>) {
    out.push('{');
    for (index, (key, value)) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(&escape_json(key));
        out.push_str("\":");
        out.push_str(&value.to_string());
    }
    out.push('}');
}

fn push_optimizer_report_json(out: &mut String, report: Option<&OptimizationReport>) {
    let Some(report) = report else {
        out.push_str("null");
        return;
    };
    out.push_str("{\"level\":\"");
    out.push_str(report.level.as_str());
    out.push_str("\",\"enabled_pass_count\":");
    out.push_str(&report.enabled_pass_count().to_string());
    out.push_str(",\"passes\":[");
    for (index, pass) in report.passes.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":\"");
        out.push_str(&escape_json(pass.name));
        out.push_str("\",\"phase\":\"");
        out.push_str(pass.phase.as_str());
        out.push_str("\",\"enabled\":");
        out.push_str(if pass.enabled { "true" } else { "false" });
        out.push_str(",\"changed\":");
        out.push_str(if pass.changed { "true" } else { "false" });
        out.push_str(",\"source_spans_preserved\":");
        out.push_str(if pass.source_spans_preserved {
            "true"
        } else {
            "false"
        });
        out.push_str(",\"stats\":{");
        for (stat_index, (key, value)) in pass.stats.iter().enumerate() {
            if stat_index > 0 {
                out.push(',');
            }
            out.push('"');
            out.push_str(&escape_json(key));
            out.push_str("\":");
            out.push_str(&value.to_string());
        }
        out.push_str("}}");
    }
    out.push_str("]}");
}

fn push_lowering_diagnostics_json(out: &mut String, path: &str, lowering: &php_ir::LoweringResult) {
    out.push('[');
    for (index, diagnostic) in lowering.diagnostics.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"path\":\"");
        out.push_str(&escape_json(path));
        out.push_str("\",\"id\":\"");
        out.push_str(&escape_json(&diagnostic.id));
        out.push_str("\",\"message\":\"");
        out.push_str(&escape_json(&diagnostic.message));
        out.push_str("\",\"span\":{\"start\":");
        out.push_str(&diagnostic.span.start.to_string());
        out.push_str(",\"end\":");
        out.push_str(&diagnostic.span.end.to_string());
        out.push_str("}}");
    }
    out.push(']');
}

fn render_markdown_report(pipeline: &Pipeline, vm_result: Option<&php_vm::VmResult>) -> String {
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

fn render_html_report(pipeline: &Pipeline, vm_result: Option<&php_vm::VmResult>) -> String {
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
            "{} {:?} {}",
            diagnostic.id().as_str(),
            diagnostic.severity(),
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

fn push_runtime_diagnostics_markdown(out: &mut String, vm_result: Option<&php_vm::VmResult>) {
    let text = runtime_diagnostics_text(vm_result);
    if text == "none" {
        out.push_str("none\n\n");
    } else {
        push_fenced_block(out, "json", &text);
    }
}

fn runtime_diagnostics_text(vm_result: Option<&php_vm::VmResult>) -> String {
    let Some(result) = vm_result else {
        return "not run".to_string();
    };
    if result.diagnostics.is_empty() {
        return "none".to_string();
    }
    result
        .diagnostics
        .iter()
        .map(php_runtime::RuntimeDiagnostic::to_json)
        .collect::<Vec<_>>()
        .join("\n")
}

fn push_known_gap_status_markdown(
    out: &mut String,
    pipeline: &Pipeline,
    vm_result: Option<&php_vm::VmResult>,
) {
    out.push_str(&known_gap_status_text(pipeline, vm_result));
    out.push_str("\n\n");
}

fn known_gap_status_text(pipeline: &Pipeline, vm_result: Option<&php_vm::VmResult>) -> String {
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

fn push_range_json(out: &mut String, span: Option<TextRange>) {
    match span {
        Some(span) => {
            out.push_str("{\"start\":");
            out.push_str(&span.start().to_usize().to_string());
            out.push_str(",\"end\":");
            out.push_str(&span.end().to_usize().to_string());
            out.push('}');
        }
        None => out.push_str("null"),
    }
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if c.is_control() => escaped.push_str(&format!("\\u{:04x}", c as u32)),
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
        BytecodeCacheMode, EXIT_COMPILE_ERROR, EXIT_RUNTIME_ERROR, EXIT_SUCCESS, JitStatsMode,
        OptimizationLevel, QuickeningMode, cache_file_for, compile_pipeline_with_optimization,
        parse_run_args, run, vm_compile_error_child_constant, vm_compile_error_child_method,
        vm_compile_error_child_property, vm_compile_error_interface_constant,
        vm_compile_error_interface_method_missing,
    };
    use php_bytecode_cache::{CacheFingerprint, CacheFingerprintInput};
    use php_vm::{
        ExecutionFormat, InlineCacheMode, JitBlacklistMode, JitMode, SuperinstructionMode,
    };
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn help_is_available() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(["--help".to_string()], &mut stdout, &mut stderr);

        assert_eq!(code, EXIT_SUCCESS);
        assert!(stderr.is_empty());
        assert!(String::from_utf8(stdout).unwrap().contains("dump-ir"));
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
        let stdout = String::from_utf8(stdout).unwrap();
        assert!(stdout.contains("\"ok\":true"));
        assert!(stdout.contains("\"ir\""));
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
        let stdout = String::from_utf8(stdout).unwrap();
        assert!(stdout.contains("\"optimizer\":{\"level\":\"1\""));
        assert!(stdout.contains("\"constant_folding_safe_subset\""));
        assert!(stdout.contains("\"transformations_attempted\""));
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
    fn run_args_accept_fast_engine_preset() {
        let args = vec![
            "--engine-preset=fast".to_string(),
            "fixtures/runtime/valid/hello.php".to_string(),
        ];

        let options = parse_run_args(&args).expect("run args should parse");

        assert_eq!(options.bytecode_cache.mode, BytecodeCacheMode::Off);
        assert_eq!(options.opt_level, OptimizationLevel::O2);
        assert_eq!(options.execution_format, ExecutionFormat::Auto);
        assert_eq!(options.superinstructions, SuperinstructionMode::Off);
        assert_eq!(options.quickening, QuickeningMode::On);
        assert_eq!(options.inline_caches, InlineCacheMode::On);
        assert_eq!(options.jit, JitMode::Off);
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

        assert!(error.contains("expected baseline, fast, or experimental-jit"));
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
        assert!(json.contains("\"jit_mode\": \"off\""));
        assert!(json.contains("\"jit_threshold\": 8"));
        assert!(json.contains("\"echo\": 1"));
        assert!(json.contains("\"guard_failures\": 0"));
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
        assert!(stderr.contains("\"mode\":\"noop\""));
        assert!(stderr.contains("\"threshold\":5"));
        assert!(stderr.contains("\"eager\":false"));
        assert!(stderr.contains("\"max_compile_us\":18446744073709551615"));
        assert!(stderr.contains("\"max_functions\":18446744073709551615"));
        assert!(stderr.contains("\"blacklist\":\"on\""));
        assert!(stderr.contains("\"dump_clif\":\"target/performance/cranelift/noop.clif\""));
        assert!(stderr.contains("\"side_exit_reasons\":{}"));
        assert!(stderr.contains("\"blacklisted_regions\":0"));
        assert!(stderr.contains("\"blacklist_reasons\":{}"));
        assert!(stderr.contains("\"tiering_cold_functions\":0"));
        assert!(stderr.contains("\"tiering_hot_functions\":0"));
        assert!(stderr.contains("\"tiering_eager_functions\":0"));
        assert!(stderr.contains("\"tiering_blacklist_rejections\":0"));
        assert!(stderr.contains("\"tiering_budget_rejections\":0"));
        assert!(stderr.contains("\"fast_path_hits\":0"));
        assert!(stderr.contains("\"packed_fetch_fast_hits\":0"));
        assert!(stderr.contains("\"packed_fetch_bounds_exits\":0"));
        assert!(stderr.contains("\"packed_fetch_layout_exits\":0"));
        assert!(stderr.contains("\"packed_foreach_sum_fast_hits\":0"));
        assert!(stderr.contains("\"packed_foreach_sum_layout_exits\":0"));
        assert!(stderr.contains("\"packed_foreach_sum_overflow_exits\":0"));
        assert!(stderr.contains("\"known_call_fast_hits\":0"));
        assert!(stderr.contains("\"known_call_guard_exits\":0"));
        assert!(stderr.contains("\"known_call_slow_calls\":0"));
        assert!(stderr.contains("\"direct_call_hits\":0"));
        assert!(stderr.contains("\"direct_call_fallbacks\":0"));
        assert!(stderr.contains("\"property_load_fast_hits\":0"));
        assert!(stderr.contains("\"property_load_guard_exits\":0"));
        assert!(stderr.contains("\"property_load_layout_exits\":0"));
        assert!(stderr.contains("\"property_load_uninitialized_exits\":0"));
        assert!(stderr.contains("\"property_load_slow_calls\":0"));
        assert!(stderr.contains("\"string_concat_fast_path_hits\":0"));
        assert!(stderr.contains("\"string_concat_fast_path_misses\":0"));
        assert!(stderr.contains("\"overflow_exits\":0"));
        assert!(stderr.contains("\"slow_path_calls\":0"));
        assert!(stderr.contains("\"compile_cache_hits\":0"));
        assert!(stderr.contains("\"compile_cache_misses\":0"));
        assert!(stderr.contains("\"compile_cache_invalidations\":0"));
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
            "COMPOSER_HOME=/tmp/composer".to_string(),
            "--env=COMPOSER_CACHE_DIR=/tmp/cache".to_string(),
            "fixtures/runtime/valid/hello.php".to_string(),
            "--".to_string(),
            "script-arg".to_string(),
        ];

        let options = parse_run_args(&args).expect("run args should parse");

        assert_eq!(options.path, "fixtures/runtime/valid/hello.php");
        assert_eq!(options.script_args, vec!["script-arg"]);
        assert_eq!(options.counters_json, None);
        assert_eq!(options.bytecode_cache.mode, BytecodeCacheMode::Off);
        assert_eq!(options.opt_level, OptimizationLevel::O0);
        assert_eq!(options.execution_format, ExecutionFormat::Ir);
        assert_eq!(options.superinstructions, SuperinstructionMode::Off);
        assert_eq!(options.quickening, QuickeningMode::Off);
        assert_eq!(options.inline_caches, InlineCacheMode::Off);
        assert_eq!(options.jit, JitMode::Off);
        assert_eq!(options.jit_blacklist, JitBlacklistMode::On);
        assert!(options.tiering.enabled);
        assert!(!options.tiering.collect_stats);
        assert_eq!(options.tiering_stats_json, None);
        assert_eq!(
            options.env,
            vec![
                ("COMPOSER_HOME".to_string(), "/tmp/composer".to_string()),
                ("COMPOSER_CACHE_DIR".to_string(), "/tmp/cache".to_string())
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
    }

    #[test]
    fn run_bytecode_cache_first_write_then_second_read_hits() {
        let cache_dir = cache_test_dir("write-read");
        reset_dir(&cache_dir);
        let fixture = fixture("tests/fixtures/performance/bytecode_cache/simple.php");

        let first = run_cache_fixture_with_mode(&fixture, &cache_dir, "0", "write");
        assert_eq!(first.0, EXIT_SUCCESS, "{}", first.2);
        assert_eq!(first.1, b"cache:5\n");
        assert!(first.2.contains("\"wrote\":true"), "{}", first.2);
        assert!(!cache_files(&cache_dir).is_empty());

        let second = run_cache_fixture_with_mode(&fixture, &cache_dir, "0", "read");
        assert_eq!(second.0, EXIT_SUCCESS, "{}", second.2);
        assert_eq!(second.1, b"cache:5\n");
        assert!(second.2.contains("\"hit\":true"), "{}", second.2);
        assert!(!second.2.contains("load_error"), "{}", second.2);
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
        assert!(first.2.contains("\"wrote\":true"), "{}", first.2);

        fs::write(&source, "<?php echo \"two\\n\";").expect("rewrite source");
        let second = run_cache_fixture(&source.display().to_string(), &cache_dir, "0");
        assert_eq!(second.0, EXIT_SUCCESS, "{}", second.2);
        assert_eq!(second.1, b"two\n");
        assert!(second.2.contains("\"miss\":true"), "{}", second.2);
        assert!(!second.2.contains("\"hit\":true"), "{}", second.2);
    }

    #[test]
    fn run_bytecode_cache_opt_level_change_misses() {
        let cache_dir = cache_test_dir("opt-level-change");
        reset_dir(&cache_dir);
        let fixture = fixture("tests/fixtures/performance/bytecode_cache/simple.php");

        let first = run_cache_fixture(&fixture, &cache_dir, "0");
        assert_eq!(first.0, EXIT_SUCCESS, "{}", first.2);
        assert!(first.2.contains("\"wrote\":true"), "{}", first.2);

        let second = run_cache_fixture(&fixture, &cache_dir, "1");
        assert_eq!(second.0, EXIT_SUCCESS, "{}", second.2);
        assert_eq!(second.1, b"cache:5\n");
        assert!(second.2.contains("\"miss\":true"), "{}", second.2);
        assert!(!second.2.contains("\"hit\":true"), "{}", second.2);
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
        assert!(second.2.contains("\"miss\":true"), "{}", second.2);
        assert!(second.2.contains("load_error"), "{}", second.2);
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
    fn vm_compile_error_parser_finds_child_method() {
        assert_eq!(
            vm_compile_error_child_method(
                "E_PHP_VM_STATIC_METHOD_OVERRIDE: Cannot make static method base::show() non static in class child",
            ),
            Some(("child".to_owned(), "show".to_owned()))
        );
        assert_eq!(
            vm_compile_error_child_method(
                "E_PHP_VM_METHOD_SIGNATURE_OVERRIDE: Declaration of D::f(array $a) must be compatible with C::f($a)",
            ),
            Some(("D".to_owned(), "f".to_owned()))
        );
        assert_eq!(
            vm_compile_error_child_method(
                "E_PHP_VM_INTERFACE_METHOD_SIGNATURE: Declaration of D::f(array $a) must be compatible with I::f($a)",
            ),
            Some(("D".to_owned(), "f".to_owned()))
        );
        assert_eq!(
            vm_compile_error_child_property(
                "E_PHP_VM_PROPERTY_STATIC_OVERRIDE: Cannot redeclare static A::$p as non static B::$p",
            ),
            Some(("B".to_owned(), "p".to_owned()))
        );
        assert_eq!(
            vm_compile_error_child_constant(
                "E_PHP_VM_CLASS_CONSTANT_VISIBILITY_OVERRIDE: Access level to B::TOKEN must be public (as in class A)",
            ),
            Some(("B".to_owned(), "TOKEN".to_owned()))
        );
        assert_eq!(
            vm_compile_error_interface_constant(
                "E_PHP_VM_INTERFACE_CONSTANT_VISIBILITY: Access type for interface constant A::FOO must be public",
            ),
            Some(("A".to_owned(), "FOO".to_owned()))
        );
        assert_eq!(
            vm_compile_error_interface_method_missing(
                "E_PHP_VM_INTERFACE_METHOD_MISSING: class Derived must implement Contract::run",
            ),
            Some((
                "Derived".to_owned(),
                "Contract".to_owned(),
                "run".to_owned()
            ))
        );
    }

    #[test]
    fn runtime_error_writes_structured_diagnostic() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            [
                "run".to_string(),
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
