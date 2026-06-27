use php_ir::{LoweringOptions, lower_frontend_result, verify_unit};
use php_runtime::{ErrorReporting, ExitStatus, FilesystemCapabilities, RuntimeContext};
use php_semantics::{FrontendResult, Severity, analyze_source, diagnostics::DiagnosticId};
use php_source::{SourceText, TextRange};
use php_vm::{IncludeLoader, Vm, VmOptions};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const EXIT_SUCCESS: i32 = 0;
const EXIT_PHP_ERROR: i32 = 255;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CliIniOptions {
    pub include_path: Option<Vec<PathBuf>>,
    pub display_errors: Option<bool>,
    pub error_reporting: Option<i64>,
    /// Raw `-d name=value` ini overrides forwarded to the runtime registry.
    pub overrides: Vec<(String, String)>,
}

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
    let result = vm.execute(pipeline.lowering.unit);
    stdout
        .write_all(result.output.as_bytes())
        .map_err(|error| error.to_string())?;
    match result.status.exit_status() {
        ExitStatus::Success => Ok(EXIT_SUCCESS),
        ExitStatus::CompileError
        | ExitStatus::RuntimeError
        | ExitStatus::Fatal
        | ExitStatus::Unsupported => {
            // An uncaught exception has already been rendered to stdout as a PHP
            // `Fatal error:`; emitting the internal diagnostic dump as well would
            // duplicate it and pollute PHPT output comparison.
            let rendered_uncaught = result
                .diagnostics
                .first()
                .is_some_and(|diagnostic| diagnostic.id() == "E_PHP_VM_UNCAUGHT_EXCEPTION");
            if !rendered_uncaught {
                write_runtime_diagnostics(stderr, &input.source_path, &result.diagnostics)?;
                writeln!(stderr, "{}: {}", input.source_path, result.status)
                    .map_err(|error| error.to_string())?;
            }
            Ok(EXIT_PHP_ERROR)
        }
    }
}

struct Pipeline {
    path: String,
    source: SourceText,
    frontend: FrontendResult,
    lowering: php_ir::LoweringResult,
}

impl Pipeline {
    fn ok(&self) -> bool {
        !self.frontend.has_errors()
            && self.lowering.diagnostics.is_empty()
            && self.lowering.verification.is_ok()
    }
}

fn compile_source(source: &str, source_path: &str) -> Result<Pipeline, String> {
    let frontend = analyze_source(source);
    let mut lowering = lower_frontend_result(
        &frontend,
        LoweringOptions {
            source_path: source_path.to_string(),
            source_text: Some(source.to_string()),
            ..LoweringOptions::default()
        },
    );
    if !frontend.has_errors() && lowering.verification.is_ok() {
        verify_unit(&lowering.unit).map_err(|errors| {
            format!(
                "{source_path}: IR verification failed: {} error(s)",
                errors.len()
            )
        })?;
        lowering.verification = verify_unit(&lowering.unit);
    }
    Ok(Pipeline {
        path: source_path.to_string(),
        source: SourceText::new(source),
        frontend,
        lowering,
    })
}

fn include_loader_for(input: &EngineInput) -> Result<Option<IncludeLoader>, String> {
    let mut roots = Vec::new();
    push_existing_root(&mut roots, &input.cwd);
    if let Some(real_path) = input.real_path.as_ref().and_then(|path| path.parent()) {
        push_existing_root(&mut roots, real_path);
    }
    if let Some(include_path) = &input.ini.include_path {
        for entry in include_path {
            if entry.is_absolute() {
                push_existing_root(&mut roots, entry);
            } else {
                push_existing_root(&mut roots, &input.cwd.join(entry));
                if let Some(real_path) = input.real_path.as_ref().and_then(|path| path.parent()) {
                    push_existing_root(&mut roots, &real_path.join(entry));
                }
            }
        }
    }
    if roots.is_empty() {
        return Ok(None);
    }
    IncludeLoader::new(roots).map(Some)
}

fn push_existing_root(roots: &mut Vec<PathBuf>, path: &Path) {
    if path.exists() {
        roots.push(path.to_path_buf());
    }
}

fn runtime_context_for(
    input: &EngineInput,
    include_loader: Option<&IncludeLoader>,
) -> RuntimeContext {
    let include_path = input
        .ini
        .include_path
        .clone()
        .unwrap_or_else(|| vec![PathBuf::from(".")]);
    let mut context =
        RuntimeContext::controlled_cli(input.script_name.clone(), input.script_args.clone())
            .with_cwd(input.cwd.clone())
            .with_include_path(include_path)
            .with_env(input.env.clone())
            .with_ini_overrides(input.ini.overrides.clone())
            .with_stdin(input.stdin.clone());
    if let Some(mask) = input.ini.error_reporting {
        context.ini.error_reporting = ErrorReporting { mask };
    }
    if let Some(display_errors) = input.ini.display_errors {
        context.ini.display_errors = display_errors;
    }
    let mut capabilities = FilesystemCapabilities::none().with_stdio(true);
    if let Some(loader) = include_loader {
        capabilities = capabilities.with_allowed_roots(loader.allowed_roots().to_vec());
    }
    context.with_filesystem_capabilities(capabilities)
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
                if diagnostic.id() == DiagnosticId::InvalidTypeCallableContext {
                    write_php_fatal_line(
                        stderr,
                        &pipeline.path,
                        &pipeline.source,
                        span,
                        diagnostic.message(),
                    )?;
                    continue;
                }
                if semantic_diagnostic_uses_php_fatal_line(diagnostic.id()) {
                    write_php_fatal_line(
                        stderr,
                        &pipeline.path,
                        &pipeline.source,
                        span,
                        diagnostic.message(),
                    )?;
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

fn write_php_fatal_line<W: Write>(
    stderr: &mut W,
    path: &str,
    source: &SourceText,
    span: TextRange,
    message: &str,
) -> Result<(), String> {
    let line = line_number_for_span(source, span);
    writeln!(stderr, "Fatal error: {message} in {path} on line {line}")
        .map_err(|error| error.to_string())
}

fn write_php_parse_error_line<W: Write>(
    stderr: &mut W,
    path: &str,
    source: &SourceText,
    span: TextRange,
    message: &str,
) -> Result<(), String> {
    let line = line_number_for_span(source, span);
    writeln!(stderr, "Parse error: {message} in {path} on line {line}")
        .map_err(|error| error.to_string())
}

fn line_number_for_span(source: &SourceText, span: TextRange) -> usize {
    source.line_col(span.start()).line
}

fn write_runtime_diagnostics<W: Write>(
    stderr: &mut W,
    path: &str,
    diagnostics: &[php_runtime::RuntimeDiagnostic],
) -> Result<(), String> {
    for diagnostic in diagnostics {
        writeln!(
            stderr,
            "{path}: runtime-diagnostic: {}",
            diagnostic.to_json()
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
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
    source: &SourceText,
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
    )
}

pub fn read_script(path: &Path) -> Result<(String, PathBuf, String), String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let real_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let source_path = real_path.to_string_lossy().into_owned();
    Ok((source, real_path, source_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_number_for_span_uses_one_based_source_lines() {
        let source = SourceText::new("<?php\nfunction f(callable&Traversable $x) {}\n");
        assert_eq!(line_number_for_span(&source, TextRange::new(6, 14)), 2);
    }

    #[test]
    fn php_fatal_line_matches_php_compile_error_shape() {
        let source = SourceText::new("<?php\nfunction f(callable&Traversable $x) {}\n");
        let mut stderr = Vec::new();

        write_php_fatal_line(
            &mut stderr,
            "fixture.php",
            &source,
            TextRange::new(6, 14),
            "Type callable cannot be part of an intersection type",
        )
        .expect("fatal line should render");

        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Type callable cannot be part of an intersection type in fixture.php on line 2\n"
        );
    }
}
