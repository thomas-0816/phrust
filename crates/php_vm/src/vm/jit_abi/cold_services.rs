//! Genuine cold request services shared by generated control boundaries.
//!
//! These helpers format diagnostics, project source metadata, and compare
//! already-published callback identities. They never resolve or invoke a PHP
//! body; registered handlers are issued through the generated callback entry.

use super::*;
use php_runtime::api::Value;

#[derive(Debug)]
pub(super) enum NativeCallControl {
    Rethrow,
    Throw {
        class: String,
        message: String,
    },
    ArgumentCount {
        function: String,
        passed: usize,
        required: usize,
        target_span: php_ir::IrSpan,
    },
    SuspendFiber,
    Exit(i64),
    PublishedRuntimeError,
    RuntimeError(String),
}

pub(super) type NativeCallResult = Result<i64, NativeCallControl>;

impl NativeCallControl {
    pub(super) fn throw(class: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Throw {
            class: class.into(),
            message: message.into(),
        }
    }

    pub(super) fn into_baseline_error(self) -> String {
        match self {
            Self::Rethrow => "E_PHP_RETHROW".to_owned(),
            Self::Throw { class, message } => format!("E_PHP_THROW:{class}:{message}"),
            Self::ArgumentCount {
                function,
                passed,
                required,
                ..
            } => format!(
                "E_PHP_THROW:ArgumentCountError:Too few arguments to function {function}(), {passed} passed and exactly {required} expected"
            ),
            Self::SuspendFiber => "E_PHP_SUSPEND_FIBER".to_owned(),
            Self::Exit(value) => format!("E_PHP_EXIT:{value}"),
            Self::PublishedRuntimeError => NATIVE_RUNTIME_ERROR_MARKER.to_owned(),
            Self::RuntimeError(message) => message,
        }
    }

    pub(super) fn from_baseline_error(message: String) -> Self {
        if message == "E_PHP_RETHROW" {
            return Self::Rethrow;
        }
        if let Some(payload) = message.strip_prefix("E_PHP_THROW:") {
            let (class, message) = payload.split_once(':').unwrap_or(("Error", payload));
            return Self::throw(class, message);
        }
        if message == "E_PHP_SUSPEND_FIBER" {
            return Self::SuspendFiber;
        }
        if let Some(value) = message.strip_prefix("E_PHP_EXIT:")
            && let Ok(value) = value.parse::<i64>()
        {
            return Self::Exit(value);
        }
        if message == NATIVE_RUNTIME_ERROR_MARKER {
            return Self::PublishedRuntimeError;
        }
        Self::RuntimeError(message)
    }
}

impl From<String> for NativeCallControl {
    fn from(message: String) -> Self {
        Self::from_baseline_error(message)
    }
}

impl From<&str> for NativeCallControl {
    fn from(message: &str) -> Self {
        Self::RuntimeError(message.to_owned())
    }
}

impl From<NativeCallControl> for String {
    fn from(control: NativeCallControl) -> Self {
        control.into_baseline_error()
    }
}

pub(super) fn native_string(value: Value) -> Result<Vec<u8>, String> {
    match value {
        Value::String(value) => Ok(value.as_bytes().to_vec()),
        Value::Int(value) => Ok(value.to_string().into_bytes()),
        Value::Float(value) => Ok(value.to_f64().to_string().into_bytes()),
        Value::Bool(true) => Ok(b"1".to_vec()),
        Value::Bool(false) | Value::Null => Ok(Vec::new()),
        Value::Reference(reference) => native_string(reference.get()),
        other => Err(format!(
            "cold string service expected scalar, got {other:?}"
        )),
    }
}

pub(super) fn stable_native_symbol_hash(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte.to_ascii_lowercase())).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

pub(super) fn native_catch_matches(
    context: &mut NativeRequestColdState<'_>,
    types: &[String],
    value: i64,
) -> bool {
    types
        .iter()
        .any(|type_name| context.direct_object_is_a(value, type_name))
}

pub(super) fn native_source_line(
    context: &NativeRequestColdState<'_>,
    source: &php_ir::Instruction,
) -> usize {
    native_source_line_for_span(context, source.span)
}

pub(super) fn native_source_line_for_span(
    context: &NativeRequestColdState<'_>,
    span: php_ir::IrSpan,
) -> usize {
    context
        .compiled
        .source_display_line(span, false)
        .and_then(|line| usize::try_from(line).ok())
        .unwrap_or(1)
}

pub(super) fn emit_native_php_diagnostic(
    context: &mut NativeRequestColdState<'_>,
    errno: i64,
    message: &str,
    source: &php_ir::Instruction,
    leading_newline: bool,
) -> Result<(), String> {
    emit_native_php_diagnostic_at_span(context, errno, message, source.span, leading_newline)
}

pub(super) fn emit_native_php_diagnostic_at_span(
    context: &mut NativeRequestColdState<'_>,
    errno: i64,
    message: &str,
    span: php_ir::IrSpan,
    leading_newline: bool,
) -> Result<(), String> {
    let path = context
        .unit
        .files
        .get(span.file.index())
        .map_or_else(|| "<unknown>".to_owned(), |file| file.path.clone());
    let line = native_source_line_for_span(context, span);
    context.record_last_error(errno, message, &path, line);
    if let Some(handler) = context
        .registered_callbacks
        .error_handlers
        .last()
        .filter(|handler| handler.levels == -1 || handler.levels & errno != 0)
        .copied()
    {
        context.retain(handler.callback)?;
        let arguments = [
            context.encode_native_int(errno)?,
            context.encode_direct_string_bytes(message.as_bytes())?,
            context.encode_direct_string_bytes(path.as_bytes())?,
            context.encode_native_int(line as i64)?,
        ];
        let invoke_result = context
            .enter_generated_callback_continuation(handler.callback, &arguments)
            .map_err(NativeCallControl::into_baseline_error)
            .and_then(|returned| context.release_if_live(returned));
        let mut release_error = context.release_if_live(handler.callback).err();
        for argument in arguments {
            if let Err(error) = context.release_if_live(argument) {
                release_error.get_or_insert(error);
            }
        }
        invoke_result?;
        return release_error.map_or(Ok(()), Err);
    }
    if context.error_reporting & errno == 0 || !context.display_errors {
        return Ok(());
    }
    let label = match errno {
        php_runtime::api::PHP_E_NOTICE | php_runtime::api::PHP_E_USER_NOTICE => "Notice",
        php_runtime::api::PHP_E_DEPRECATED | php_runtime::api::PHP_E_USER_DEPRECATED => {
            "Deprecated"
        }
        _ => "Warning",
    };
    let html = matches!(
        context.options.runtime_context.request_mode,
        php_runtime::api::RuntimeRequestMode::Http(_)
    );
    context.output.write_bytes(format_native_php_diagnostic(
        label,
        message,
        &path,
        line,
        leading_newline,
        html,
    ));
    Ok(())
}

pub(super) fn format_native_php_diagnostic(
    label: &str,
    message: &str,
    path: &str,
    line: usize,
    leading_newline: bool,
    html: bool,
) -> String {
    if html {
        let prefix = if leading_newline { "<br />\n" } else { "" };
        format!("{prefix}<b>{label}</b>:  {message} in <b>{path}</b> on line <b>{line}</b><br />\n")
    } else {
        let prefix = if leading_newline { "\n" } else { "" };
        format!("{prefix}{label}: {message} in {path} on line {line}\n")
    }
}
