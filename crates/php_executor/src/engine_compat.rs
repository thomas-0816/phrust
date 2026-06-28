use crate::diagnostics::{
    write_frontend_diagnostics, write_runtime_diagnostics, write_vm_compile_fatal_line,
};
use crate::pipeline::compile_source;
use crate::request::{include_loader_for, runtime_context_for};
use php_runtime::api::ExitStatus;
use php_vm::api::{Vm, VmOptions};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) const EXIT_SUCCESS: i32 = 0;
pub(crate) const EXIT_PHP_ERROR: i32 = 255;

/// Compatibility INI options for the legacy CLI-compatible execution entry point.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CliIniOptions {
    pub include_path: Option<Vec<PathBuf>>,
    pub display_errors: Option<bool>,
    pub error_reporting: Option<i64>,
    /// Raw `-d name=value` ini overrides forwarded to the runtime registry.
    pub overrides: Vec<(String, String)>,
}

/// Compatibility input for `execute_php`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineInput {
    pub source: String,
    pub source_path: String,
    pub real_path: Option<PathBuf>,
    pub script_name: String,
    pub script_args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub ini: CliIniOptions,
    pub stdin: Vec<u8>,
}

/// Executes PHP through the legacy CLI-compatible engine path.
pub fn execute_php<W, E>(input: EngineInput, stdout: &mut W, stderr: &mut E) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    let pipeline = compile_source(&input.source, &input.source_path)?;
    if !pipeline.ok() {
        write_frontend_diagnostics(stderr, &pipeline)?;
        return Ok(EXIT_PHP_ERROR);
    }
    let include_loader = include_loader_for(&input)?;
    let runtime_context = runtime_context_for(&input, include_loader.as_ref());
    let vm = Vm::with_options(VmOptions {
        include_loader,
        runtime_context,
        ..VmOptions::default()
    });
    let result = vm.execute(pipeline.lowering.unit.clone());
    stdout
        .write_all(result.output.as_bytes())
        .map_err(|error| error.to_string())?;
    match result.status.exit_status() {
        ExitStatus::Success => Ok(EXIT_SUCCESS),
        ExitStatus::CompileError => {
            if write_vm_compile_fatal_line(stderr, &pipeline, &result.diagnostics)? {
                return Ok(EXIT_PHP_ERROR);
            }
            write_runtime_diagnostics(stderr, &input.source_path, &result.diagnostics)?;
            writeln!(stderr, "{}: {}", input.source_path, result.status)
                .map_err(|error| error.to_string())?;
            Ok(EXIT_PHP_ERROR)
        }
        ExitStatus::RuntimeError | ExitStatus::Fatal | ExitStatus::Unsupported => {
            write_runtime_diagnostics(stderr, &input.source_path, &result.diagnostics)?;
            writeln!(stderr, "{}: {}", input.source_path, result.status)
                .map_err(|error| error.to_string())?;
            Ok(EXIT_PHP_ERROR)
        }
    }
}

pub fn read_script(path: &Path) -> Result<(String, PathBuf, String), String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let real_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let source_path = real_path.to_string_lossy().into_owned();
    Ok((source, real_path, source_path))
}
