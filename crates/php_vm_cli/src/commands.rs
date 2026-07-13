//! Native-only php-vm command implementation.

use php_diagnostics::DiagnosticOutputFormat;
use php_executor::{
    EngineProfileName, PhpCompileInput, PhpExecutionError, PhpExecutionOutput, PhpExecutionStatus,
    PhpExecutor, PhpExecutorOptions, PhpRequestExecutionInput, usage_diagnostic,
    write_diagnostic_envelope,
};
use php_optimizer::OptimizationLevel;
use php_runtime::api::RuntimeContext;
use php_vm::api::{InlineCacheMode, JitBlacklistMode, NativeOptimizationPolicy};
use serde_json::json;
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

const EXIT_SUCCESS: i32 = 0;
const EXIT_COMPILE_ERROR: i32 = 2;
const EXIT_RUNTIME_ERROR: i32 = 3;
const EXIT_UNSUPPORTED: i32 = 4;
const EXIT_USAGE: i32 = 5;
const EXIT_PHP_FATAL_ERROR: i32 = 255;

pub(crate) fn main_entry() {
    let mut stdin = io::stdin();
    let stdin_is_terminal = stdin.is_terminal();
    let code = run_with_stdin(
        env::args().skip(1),
        &mut stdin,
        stdin_is_terminal,
        &mut io::stdout(),
        &mut io::stderr(),
    );
    if code != EXIT_SUCCESS {
        std::process::exit(code);
    }
}

fn run_with_stdin<I, R, W, E>(
    args: I,
    stdin: &mut R,
    stdin_is_terminal: bool,
    stdout: &mut W,
    stderr: &mut E,
) -> i32
where
    I: IntoIterator<Item = String>,
    R: Read,
    W: Write,
    E: Write,
{
    match run_inner(
        args.into_iter().collect(),
        stdin,
        stdin_is_terminal,
        stdout,
        stderr,
    ) {
        Ok(code) => code,
        Err(error) => {
            let diagnostic =
                usage_diagnostic(error, Some("php-vm"), None, None, "run php-vm --help");
            let _ = write_diagnostic_envelope(stderr, &diagnostic, error_format_from_env());
            EXIT_USAGE
        }
    }
}

fn run_inner<R, W, E>(
    args: Vec<String>,
    stdin: &mut R,
    stdin_is_terminal: bool,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, String>
where
    R: Read,
    W: Write,
    E: Write,
{
    if args.is_empty()
        || args
            .iter()
            .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        print_usage(stdout)?;
        return Ok(EXIT_SUCCESS);
    }
    match args[0].as_str() {
        "run" => run_command(&args[1..], stdin, stdin_is_terminal, stdout, stderr),
        "compile" => compile_command(&args[1..], stdout, stderr),
        "dump-ir" => dump_ir_command(&args[1..], stdout, stderr),
        command => Err(format!("unknown php-vm command `{command}`")),
    }
}

#[derive(Debug)]
struct NativeRunOptions {
    path: String,
    script_args: Vec<String>,
    env: Vec<(String, String)>,
    profile: EngineProfileName,
    opt_level: Option<OptimizationLevel>,
    trace: bool,
    trace_runtime: bool,
    trace_includes: bool,
    counters_json: Option<PathBuf>,
    timings_json: Option<PathBuf>,
    inline_caches: Option<InlineCacheMode>,
    native_optimization: Option<NativeOptimizationPolicy>,
    jit_threshold: Option<u64>,
    jit_blacklist: Option<JitBlacklistMode>,
    jit_dump_clif: Option<PathBuf>,
    tiering_enabled: Option<bool>,
    jit_eager: bool,
    jit_max_compile_us: Option<u64>,
    jit_max_functions: Option<u64>,
}

fn parse_run_options(args: &[String]) -> Result<NativeRunOptions, String> {
    let mut options = NativeRunOptions {
        path: String::new(),
        script_args: Vec::new(),
        env: Vec::new(),
        profile: EngineProfileName::Default,
        opt_level: None,
        trace: false,
        trace_runtime: false,
        trace_includes: false,
        counters_json: None,
        timings_json: None,
        inline_caches: None,
        native_optimization: None,
        jit_threshold: None,
        jit_blacklist: None,
        jit_dump_clif: None,
        tiering_enabled: None,
        jit_eager: false,
        jit_max_compile_us: None,
        jit_max_functions: None,
    };
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--" {
            options.script_args.extend_from_slice(&args[index + 1..]);
            break;
        }
        if !arg.starts_with('-') {
            if options.path.is_empty() {
                options.path = arg.clone();
            } else {
                return Err("script arguments must follow --".to_owned());
            }
            index += 1;
            continue;
        }
        let (name, inline_value) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(name, value)| (name, Some(value)));
        let mut value = |label: &str| -> Result<String, String> {
            if let Some(value) = inline_value {
                return Ok(value.to_owned());
            }
            index += 1;
            args.get(index)
                .cloned()
                .ok_or_else(|| format!("run {label} requires a value"))
        };
        match name {
            "--trace" => options.trace = true,
            "--trace-runtime" => options.trace_runtime = true,
            "--trace-includes" => options.trace_includes = true,
            "--debug" => {}
            "--debug-log" | "--error-format" => {
                let _ = value(name)?;
            }
            "--engine-preset" => {
                options.profile =
                    EngineProfileName::parse(&value(name)?).map_err(|error| error.to_string())?;
            }
            "--opt-level" => options.opt_level = Some(parse_opt_level(&value(name)?)?),
            "--env" => options.env.push(parse_env(&value(name)?)?),
            "--counters-json" => options.counters_json = Some(PathBuf::from(value(name)?)),
            "--timings-json" => options.timings_json = Some(PathBuf::from(value(name)?)),
            "--inline-caches" => {
                options.inline_caches = Some(parse_toggle(&value(name)?).map(|enabled| {
                    if enabled {
                        InlineCacheMode::On
                    } else {
                        InlineCacheMode::Off
                    }
                })?);
            }
            "--native-optimization" => {
                options.native_optimization = Some(match value(name)?.as_str() {
                    "baseline" => NativeOptimizationPolicy::Baseline,
                    "optimizing" | "optimized" => NativeOptimizationPolicy::Optimizing,
                    other => return Err(format!("invalid native optimization `{other}`")),
                });
            }
            "--jit" => match value(name)?.as_str() {
                "cranelift" | "on" => {}
                "off" => return Err("Cranelift is mandatory; --jit=off was removed".to_owned()),
                other => return Err(format!("invalid --jit value `{other}`")),
            },
            "--jit-threshold" => options.jit_threshold = Some(parse_u64(name, &value(name)?)?),
            "--jit-blacklist" => {
                options.jit_blacklist = Some(if parse_toggle(&value(name)?)? {
                    JitBlacklistMode::On
                } else {
                    JitBlacklistMode::Off
                });
            }
            "--jit-dump-clif" => options.jit_dump_clif = Some(PathBuf::from(value(name)?)),
            "--tiering" => options.tiering_enabled = Some(parse_toggle(&value(name)?)?),
            "--jit-eager" => options.jit_eager = true,
            "--jit-max-compile-us" => {
                options.jit_max_compile_us = Some(parse_u64(name, &value(name)?)?)
            }
            "--jit-max-functions" => {
                options.jit_max_functions = Some(parse_u64(name, &value(name)?)?)
            }
            _ => return Err(format!("unsupported native run option `{arg}`")),
        }
        index += 1;
    }
    if options.path.is_empty() {
        return Err("php-vm run requires <path.php>".to_owned());
    }
    Ok(options)
}

fn run_command<R, W, E>(
    args: &[String],
    stdin: &mut R,
    stdin_is_terminal: bool,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, String>
where
    R: Read,
    W: Write,
    E: Write,
{
    let options = parse_run_options(args)?;
    let total_started = Instant::now();
    let (source, real_path, source_path) = php_executor::read_script(Path::new(&options.path))?;
    let mut executor_options = PhpExecutorOptions::for_profile(options.profile);
    if let Some(level) = options.opt_level {
        executor_options.optimization_level = level;
    }
    let vm = &mut executor_options.vm_options;
    vm.trace = options.trace;
    vm.trace_runtime = options.trace_runtime;
    vm.trace_includes = options.trace_includes;
    vm.collect_counters = options.counters_json.is_some();
    if let Some(mode) = options.inline_caches {
        vm.inline_caches = mode;
    }
    if let Some(policy) = options.native_optimization {
        vm.native_optimization = policy;
    }
    if let Some(threshold) = options.jit_threshold {
        vm.jit_threshold = threshold;
        vm.tiering.function_entry_threshold = threshold;
    }
    if let Some(mode) = options.jit_blacklist {
        vm.jit_blacklist = mode;
    }
    vm.jit_dump_clif = options.jit_dump_clif;
    if let Some(enabled) = options.tiering_enabled {
        vm.tiering.enabled = enabled;
    }
    if options.jit_eager {
        vm.tiering.jit_eager = true;
        vm.jit_threshold = 1;
        vm.tiering.function_entry_threshold = 1;
    }
    if let Some(limit) = options.jit_max_compile_us {
        vm.tiering.jit_max_compile_us = limit;
    }
    if let Some(limit) = options.jit_max_functions {
        vm.tiering.jit_max_functions = limit;
    }

    let compile_started = Instant::now();
    let executor = PhpExecutor::with_options(executor_options);
    let compiled = match executor.compile_source(PhpCompileInput {
        source,
        source_path,
        optimization_level: options.opt_level,
    }) {
        Ok(compiled) => compiled,
        Err(PhpExecutionError::Compile(output)) => {
            write_output_diagnostics(stderr, &output)?;
            return Ok(EXIT_COMPILE_ERROR);
        }
        Err(PhpExecutionError::Engine(error)) => return Err(error),
    };
    let compile_ms = compile_started.elapsed().as_secs_f64() * 1_000.0;
    let cwd = std::env::current_dir().map_err(|error| format!("current directory: {error}"))?;
    let mut stdin_bytes = Vec::new();
    if !stdin_is_terminal {
        stdin
            .read_to_end(&mut stdin_bytes)
            .map_err(|error| format!("stdin: {error}"))?;
    }
    let runtime_context = RuntimeContext::controlled_cli(&options.path, options.script_args)
        .with_env(options.env)
        .with_cwd(cwd.clone())
        .with_stdin(stdin_bytes);
    let execute_started = Instant::now();
    let output = executor.execute_compiled(
        &compiled,
        PhpRequestExecutionInput {
            real_path: Some(real_path),
            cwd,
            include_roots: Vec::new(),
            runtime_context,
            collect_counters: options.counters_json.is_some(),
            collect_profile_spans: false,
            collect_layout_source_attribution: false,
        },
    );
    let execute_ms = execute_started.elapsed().as_secs_f64() * 1_000.0;
    stdout
        .write_all(&output.stdout)
        .map_err(|error| error.to_string())?;
    for event in &output.trace {
        writeln!(stderr, "{event}").map_err(|error| error.to_string())?;
    }
    write_output_diagnostics(stderr, &output)?;

    if let Some(path) = options.counters_json {
        let counters = output.counters.as_ref().cloned().unwrap_or_default();
        write_parented(&path, counters.to_json().as_bytes())?;
    }
    if let Some(path) = options.timings_json {
        let report = json!({
            "schema_version": 3,
            "command": "run",
            "phases_ms": {
                "compile_ms": compile_ms,
                "execute_ms": execute_ms,
                "total_ms": total_started.elapsed().as_secs_f64() * 1_000.0
            }
        });
        write_parented(
            &path,
            serde_json::to_string_pretty(&report).unwrap().as_bytes(),
        )?;
    }
    Ok(execution_exit_code(&output))
}

fn compile_command<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    let mut path = None;
    let mut json_output = false;
    let mut opt_level = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => json_output = true,
            "--opt-level" => {
                index += 1;
                opt_level = Some(parse_opt_level(
                    args.get(index)
                        .ok_or("compile --opt-level requires a value")?,
                )?);
            }
            arg if let Some(value) = arg.strip_prefix("--opt-level=") => {
                opt_level = Some(parse_opt_level(value)?);
            }
            arg if arg.starts_with('-') => {
                return Err(format!("unsupported compile option `{arg}`"));
            }
            arg => path = Some(arg.to_owned()),
        }
        index += 1;
    }
    let path = path.ok_or("php-vm compile requires <path.php>")?;
    let (source, _, source_path) = php_executor::read_script(Path::new(&path))?;
    match PhpExecutor::new().compile_source(PhpCompileInput {
        source,
        source_path: source_path.clone(),
        optimization_level: opt_level,
    }) {
        Ok(compiled) => {
            if json_output {
                writeln!(
                    stdout,
                    "{}",
                    json!({
                        "status": "ok",
                        "path": source_path,
                        "functions": compiled.ir_unit().functions.len(),
                        "classes": compiled.ir_unit().classes.len(),
                        "constants": compiled.ir_unit().constants.len(),
                    })
                )
                .map_err(|error| error.to_string())?;
            } else {
                writeln!(
                    stdout,
                    "ok path={} functions={} constants={}",
                    source_path,
                    compiled.ir_unit().functions.len(),
                    compiled.ir_unit().constants.len()
                )
                .map_err(|error| error.to_string())?;
            }
            Ok(EXIT_SUCCESS)
        }
        Err(PhpExecutionError::Compile(output)) => {
            write_output_diagnostics(stderr, &output)?;
            Ok(EXIT_COMPILE_ERROR)
        }
        Err(PhpExecutionError::Engine(error)) => Err(error),
    }
}

fn dump_ir_command<W, E>(args: &[String], stdout: &mut W, stderr: &mut E) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    let with_source = args.iter().any(|arg| arg == "--with-source");
    let path = args
        .iter()
        .find(|arg| !arg.starts_with('-'))
        .ok_or("php-vm dump-ir requires <path.php>")?;
    let (source, _, source_path) = php_executor::read_script(Path::new(path))?;
    let compiled = match PhpExecutor::new().compile_source(PhpCompileInput {
        source: source.clone(),
        source_path: source_path.clone(),
        optimization_level: None,
    }) {
        Ok(compiled) => compiled,
        Err(PhpExecutionError::Compile(output)) => {
            write_output_diagnostics(stderr, &output)?;
            return Ok(EXIT_COMPILE_ERROR);
        }
        Err(PhpExecutionError::Engine(error)) => return Err(error),
    };
    if with_source {
        writeln!(stdout, "source path={source_path}").map_err(|error| error.to_string())?;
        for (index, line) in source.lines().enumerate() {
            writeln!(stdout, "source {:04}: {}", index + 1, line)
                .map_err(|error| error.to_string())?;
        }
        writeln!(stdout, "--- ir ---").map_err(|error| error.to_string())?;
    }
    write!(stdout, "{}", compiled.ir_unit().to_snapshot_text())
        .map_err(|error| error.to_string())?;
    Ok(EXIT_SUCCESS)
}

fn write_output_diagnostics<E: Write>(
    stderr: &mut E,
    output: &PhpExecutionOutput,
) -> Result<(), String> {
    if !output.diagnostics_text.is_empty() {
        write!(stderr, "{}", output.diagnostics_text).map_err(|error| error.to_string())?;
        if !output.diagnostics_text.ends_with('\n') {
            writeln!(stderr).map_err(|error| error.to_string())?;
        }
    }
    for diagnostic in &output.runtime_diagnostics {
        writeln!(stderr, "{}", diagnostic.to_json()).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn execution_exit_code(output: &PhpExecutionOutput) -> i32 {
    match output.status {
        PhpExecutionStatus::Success => EXIT_SUCCESS,
        PhpExecutionStatus::CompileError => EXIT_COMPILE_ERROR,
        PhpExecutionStatus::RuntimeError => EXIT_RUNTIME_ERROR,
        PhpExecutionStatus::Unsupported => EXIT_UNSUPPORTED,
        PhpExecutionStatus::Fatal => EXIT_PHP_FATAL_ERROR,
    }
}

fn parse_opt_level(value: &str) -> Result<OptimizationLevel, String> {
    match value {
        "0" | "o0" | "O0" => Ok(OptimizationLevel::O0),
        "1" | "o1" | "O1" => Ok(OptimizationLevel::O1),
        "2" | "o2" | "O2" => Ok(OptimizationLevel::O2),
        _ => Err(format!("invalid optimization level `{value}`")),
    }
}

fn parse_toggle(value: &str) -> Result<bool, String> {
    match value {
        "1" | "on" | "true" | "yes" => Ok(true),
        "0" | "off" | "false" | "no" => Ok(false),
        _ => Err(format!("expected on or off, got `{value}`")),
    }
}

fn parse_env(value: &str) -> Result<(String, String), String> {
    let (name, value) = value
        .split_once('=')
        .ok_or_else(|| "--env requires KEY=VALUE".to_owned())?;
    if name.is_empty() {
        return Err("--env requires a non-empty key".to_owned());
    }
    Ok((name.to_owned(), value.to_owned()))
}

fn parse_u64(name: &str, value: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|error| format!("invalid {name} value `{value}`: {error}"))
}

fn error_format_from_env() -> DiagnosticOutputFormat {
    match env::var("PHRUST_ERROR_FORMAT").as_deref() {
        Ok("json" | "jsonl") => DiagnosticOutputFormat::Json,
        _ => DiagnosticOutputFormat::Text,
    }
}

fn write_parented(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

fn print_usage<W: Write>(stdout: &mut W) -> Result<(), String> {
    writeln!(
        stdout,
        "Usage:\n  php-vm run [native options] <file> [-- args...]\n  php-vm compile <file> [--json] [--opt-level 0|1|2]\n  php-vm dump-ir <file> [--with-source]\n\nNative options:\n  --engine-preset baseline|default|fast\n  --opt-level 0|1|2\n  --native-optimization baseline|optimizing\n  --inline-caches off|on\n  --jit=cranelift\n  --jit-threshold N\n  --jit-blacklist off|on\n  --jit-dump-clif PATH\n  --tiering off|on\n  --jit-eager\n  --jit-max-compile-us N\n  --jit-max-functions N\n  --counters-json PATH\n  --timings-json PATH\n  --trace --trace-runtime --trace-includes\n  --env KEY=VALUE"
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_executor_switches_are_rejected() {
        let error = parse_run_options(&[
            "--exec-format=bytecode".to_owned(),
            "fixture.php".to_owned(),
        ])
        .expect_err("removed option");
        assert!(error.contains("unsupported native run option"));
    }
}
