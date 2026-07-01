use crate::input::{PhpExecutionOutput, PhpExecutionStatus};
use crate::pipeline::Pipeline;
use php_diagnostics::{
    DiagnosticEnvelope, DiagnosticLayer, DiagnosticOutputFormat, DiagnosticPhase,
    DiagnosticSeverity, DiagnosticSuggestion,
};
use php_ir::{VerificationDiagnosticContext, lower::LoweringDiagnosticContext, module::IrUnit};
use php_runtime::api::{ExitStatus, RuntimeDiagnostic, RuntimeDiagnosticPayload};
use php_semantics::{Severity, diagnostics::DiagnosticId};
use php_source::{SourceText, TextRange};
use php_vm::api::VmResult;
use std::collections::BTreeMap;
use std::io::Write;

/// Creates a stable CLI/executor usage diagnostic envelope.
#[must_use]
pub fn usage_diagnostic(
    message: impl Into<String>,
    command: Option<&str>,
    argument: Option<&str>,
    accepted_values: Option<&str>,
    suggestion: impl Into<String>,
) -> DiagnosticEnvelope {
    let mut context = BTreeMap::new();
    if let Some(command) = command {
        context.insert("command".to_string(), command.to_string());
    }
    if let Some(argument) = argument {
        context.insert("argument".to_string(), argument.to_string());
    }
    if let Some(accepted_values) = accepted_values {
        context.insert("accepted_values".to_string(), accepted_values.to_string());
    }
    let mut envelope = DiagnosticEnvelope::new(
        "E_PHRUST_CLI_USAGE",
        DiagnosticLayer::cli(),
        DiagnosticPhase::new("parse"),
        DiagnosticSeverity::Error,
        message,
    )
    .with_context(context);
    envelope.suggestion = Some(DiagnosticSuggestion::new(suggestion));
    envelope.php_visible = false;
    envelope
}

/// Renders one shared diagnostic envelope in the selected output format.
pub fn render_diagnostic_envelope(
    envelope: &DiagnosticEnvelope,
    format: DiagnosticOutputFormat,
) -> Result<String, String> {
    match format {
        DiagnosticOutputFormat::Text => {
            let mut line = envelope.text_line();
            line.push('\n');
            Ok(line)
        }
        DiagnosticOutputFormat::Json => envelope.json_line().map_err(|error| error.to_string()),
    }
}

/// Writes one shared diagnostic envelope in the selected output format.
pub fn write_diagnostic_envelope<W: Write>(
    writer: &mut W,
    envelope: &DiagnosticEnvelope,
    format: DiagnosticOutputFormat,
) -> Result<(), String> {
    writer
        .write_all(render_diagnostic_envelope(envelope, format)?.as_bytes())
        .map_err(|error| error.to_string())
}

pub(crate) fn frontend_diagnostic_envelopes(pipeline: &Pipeline) -> Vec<DiagnosticEnvelope> {
    let mut diagnostics = Vec::new();
    for diagnostic in pipeline.frontend.parser_diagnostics() {
        diagnostics.push(diagnostic.to_diagnostic_envelope(
            Some(&pipeline.source),
            None,
            Some(&pipeline.path),
        ));
    }
    for diagnostic in pipeline.frontend.semantic_diagnostics() {
        if diagnostic.severity() == Severity::Error {
            diagnostics.push(diagnostic.to_diagnostic_envelope(
                Some(&pipeline.source),
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

pub(crate) fn render_frontend_diagnostics(pipeline: &Pipeline) -> Result<String, String> {
    let mut stderr = Vec::new();
    write_frontend_diagnostics(&mut stderr, pipeline)?;
    String::from_utf8(stderr).map_err(|error| error.to_string())
}

pub(crate) fn execution_output_from_vm(
    pipeline: &Pipeline,
    result: VmResult,
) -> PhpExecutionOutput {
    let status = PhpExecutionStatus::from(result.status.exit_status());
    let mut diagnostics = Vec::new();
    match result.status.exit_status() {
        ExitStatus::Success => {}
        ExitStatus::CompileError => {
            match write_vm_compile_fatal_line(&mut diagnostics, pipeline, &result.diagnostics) {
                Ok(true) => {}
                Ok(false) => {
                    let _ = write_runtime_diagnostics(
                        &mut diagnostics,
                        &pipeline.path,
                        &result.diagnostics,
                    );
                    let _ = writeln!(diagnostics, "{}: {}", pipeline.path, result.status);
                }
                Err(error) => {
                    let _ = writeln!(diagnostics, "{error}");
                }
            }
        }
        ExitStatus::RuntimeError | ExitStatus::Fatal | ExitStatus::Unsupported => {
            let _ =
                write_runtime_diagnostics(&mut diagnostics, &pipeline.path, &result.diagnostics);
            let _ = writeln!(diagnostics, "{}: {}", pipeline.path, result.status);
        }
    }
    let diagnostics_text = String::from_utf8(diagnostics).unwrap_or_default();
    PhpExecutionOutput {
        stdout: result.output.as_bytes().to_vec(),
        diagnostics_text,
        diagnostics: result
            .diagnostics
            .iter()
            .map(RuntimeDiagnostic::to_diagnostic_envelope)
            .collect(),
        status,
        runtime_diagnostics: result.diagnostics,
        http_response: result.http_response,
        upload_registry: result.upload_registry,
        session: result.session,
        trace: result.trace,
        counters: result.counters,
        tiering_stats: result.tiering_stats,
    }
}

pub(crate) fn write_frontend_diagnostics<W: Write>(
    stderr: &mut W,
    pipeline: &Pipeline,
) -> Result<(), String> {
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

pub(crate) fn write_php_fatal_line<W: Write>(
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

pub(crate) fn write_vm_compile_fatal_line<W: Write>(
    stderr: &mut W,
    pipeline: &Pipeline,
    diagnostics: &[RuntimeDiagnostic],
) -> Result<bool, String> {
    let Some((payload, span)) =
        diagnostics
            .iter()
            .find_map(|diagnostic| match diagnostic.payload()? {
                RuntimeDiagnosticPayload::VmCompile(payload) => {
                    Some((payload, diagnostic.source_span()))
                }
                RuntimeDiagnosticPayload::WordPressBringup(_) => None,
            })
    else {
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

pub(crate) fn line_number_for_span(source: &SourceText, span: TextRange) -> usize {
    source.line_col(span.start()).line
}

pub(crate) fn write_runtime_diagnostics<W: Write>(
    stderr: &mut W,
    path: &str,
    diagnostics: &[RuntimeDiagnostic],
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
