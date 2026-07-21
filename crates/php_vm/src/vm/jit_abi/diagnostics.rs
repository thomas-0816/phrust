use super::*;

pub(super) fn publish_native_call_diagnostic(
    context: &mut NativeExecutionContext<'_>,
    message: String,
) {
    // A typed helper can publish the precise failure before the native caller
    // observes the generic non-zero status. Preserve that root cause instead
    // of replacing it with an outer "callee returned a runtime error".
    if context.diagnostic.is_some() {
        return;
    }
    if message.starts_with("E_PHP_VM_UNRESOLVED_CALLABLE:")
        || message.starts_with("E_PHP_VM_UNKNOWN_CLASS:")
    {
        let id = if message.starts_with("E_PHP_VM_UNKNOWN_CLASS:") {
            "E_PHP_VM_UNKNOWN_CLASS"
        } else {
            "E_PHP_VM_UNRESOLVED_CALLABLE"
        };
        context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
            id,
            php_runtime::api::RuntimeSeverity::RecoverableError,
            message,
            php_runtime::api::RuntimeSourceSpan::default(),
            Vec::new(),
            None,
        ));
        return;
    }
    let path = context
        .unit
        .files
        .first()
        .map_or("<unknown>", |file| file.path.as_str());
    context.output.write_slices(&[
        b"\nFatal error: Uncaught Error: ",
        message.as_bytes(),
        b"\n  thrown in ",
        path.as_bytes(),
        b"\n",
    ]);
    context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
        "E_NATIVE_CALL",
        php_runtime::api::RuntimeSeverity::FatalError,
        message,
        php_runtime::api::RuntimeSourceSpan::default(),
        Vec::new(),
        None,
    ));
}

pub(super) fn record_native_helper_failure(
    context: &mut NativeExecutionContext<'_>,
    message: String,
) {
    context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
        "E_PHP_NATIVE_HELPER",
        php_runtime::api::RuntimeSeverity::FatalError,
        message,
        php_runtime::api::RuntimeSourceSpan::default(),
        Vec::new(),
        None,
    ));
}

pub(super) fn native_value_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null | Value::Uninitialized => "null",
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) | Value::Fiber(_) | Value::Generator(_) | Value::Callable(_) => "object",
        Value::Resource(_) => "resource",
        Value::Reference(reference) => native_value_type_name(&reference.get()),
    }
}

pub(super) fn native_php_float_label(value: f64) -> String {
    if value.is_nan() {
        return "NAN".to_owned();
    }
    if value == f64::INFINITY {
        return "INF".to_owned();
    }
    if value == f64::NEG_INFINITY {
        return "-INF".to_owned();
    }
    if value != 0.0 && (value.abs() >= 1.0e14 || value.abs() < 1.0e-4) {
        let scientific = format!("{value:.1E}");
        if let Some((mantissa, exponent)) = scientific.split_once('E')
            && !exponent.starts_with(['+', '-'])
        {
            return format!("{mantissa}E+{exponent}");
        }
        return scientific;
    }
    value.to_string()
}

pub(super) fn native_implicit_float_to_int_message(value: &Value) -> Option<String> {
    match value {
        Value::Reference(reference) => native_implicit_float_to_int_message(&reference.get()),
        Value::Float(value) => {
            let value = value.to_f64();
            (value.is_finite() && value.fract() != 0.0).then(|| {
                format!(
                    "Implicit conversion from float {} to int loses precision",
                    native_php_float_label(value)
                )
            })
        }
        Value::String(string) => {
            let classified = php_runtime::experimental::numeric_string::classify_php_string(string);
            let float = match classified.value {
                Some(php_runtime::experimental::numeric_string::NumericStringValue::Float(
                    value,
                )) => value,
                _ => return None,
            };
            (float.is_finite() && float.fract() != 0.0).then(|| {
                format!(
                    "Implicit conversion from float-string \"{}\" to int loses precision",
                    string.to_string_lossy()
                )
            })
        }
        _ => None,
    }
}

pub(super) fn native_assignment_type_name(value: &Value) -> String {
    match value {
        Value::Reference(reference) => native_assignment_type_name(&reference.get()),
        Value::Object(object) => object.display_name(),
        _ => native_value_type_name(value).to_owned(),
    }
}

pub(super) fn encode_native_throwable(
    context: &mut NativeExecutionContext<'_>,
    class: &str,
    message: &str,
) -> Result<i64, String> {
    encode_native_throwable_fields(context, class, message, None)
}

pub(super) fn encode_native_throwable_at(
    context: &mut NativeExecutionContext<'_>,
    class: &str,
    message: &str,
    span: php_ir::IrSpan,
) -> Result<i64, String> {
    encode_native_throwable_fields(context, class, message, Some(span))
}

pub(super) fn initialize_native_throwable_parent(
    context: &mut NativeExecutionContext<'_>,
    class: &str,
    method: &str,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    if !method.eq_ignore_ascii_case("__construct")
        || !matches!(
            normalize_class_name(class).as_str(),
            "exception"
                | "errorexception"
                | "error"
                | "typeerror"
                | "valueerror"
                | "argumentcounterror"
                | "fibererror"
        )
    {
        return None;
    }
    Some((|| {
        let object = context
            .call_frames
            .last()
            .and_then(|frame| frame.object.clone())
            .ok_or_else(|| format!("{class}::__construct() has no active object receiver"))?;
        let message = arguments
            .first()
            .map(|value| context.decode(*value))
            .transpose()?
            .map(native_string)
            .transpose()?
            .map_or_else(
                || Value::String(PhpString::from_bytes(Vec::new())),
                |message| Value::String(PhpString::from_bytes(message)),
            );
        let code = arguments
            .get(1)
            .map(|value| context.decode(*value))
            .transpose()?
            .map_or(Value::Int(0), |value| match value {
                Value::Reference(reference) => reference.get(),
                value => value,
            });
        let previous = arguments
            .get(2)
            .map(|value| context.decode(*value))
            .transpose()?
            .map_or(Value::Null, |value| match value {
                Value::Reference(reference) => reference.get(),
                value => value,
            });
        object.set_property("message", message);
        object.set_property("code", code);
        object.set_property("previous", previous);
        context.encode(Value::Null)
    })())
}

fn encode_native_throwable_fields(
    context: &mut NativeExecutionContext<'_>,
    class: &str,
    message: &str,
    span: Option<php_ir::IrSpan>,
) -> Result<i64, String> {
    let mut exception = php_runtime::api::PhpArray::new();
    for (name, value) in [
        ("class", class),
        ("message", message),
        (
            "file",
            context
                .unit
                .files
                .first()
                .map_or("<unknown>", |file| file.path.as_str()),
        ),
    ] {
        exception.insert(
            php_runtime::api::ArrayKey::String(PhpString::from_bytes(name.as_bytes().to_vec())),
            Value::String(PhpString::from_bytes(value.as_bytes().to_vec())),
        );
    }
    if let Some(span) = span {
        exception.insert(
            php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"line".to_vec())),
            Value::Int(
                i64::try_from(native_source_line_for_span(context, span)).unwrap_or(i64::MAX),
            ),
        );
    }
    context.encode(Value::Array(exception))
}

pub(super) fn native_throwable_with_frame(
    mut throwable: Value,
    function: &str,
    arguments: Vec<Value>,
) -> Value {
    let Value::Array(exception) = &mut throwable else {
        return throwable;
    };
    let trace_key = php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"trace".to_vec()));
    let mut trace = match exception.get(&trace_key) {
        Some(Value::Array(trace)) => trace.clone(),
        _ => php_runtime::api::PhpArray::new(),
    };
    let mut frame = php_runtime::api::PhpArray::new();
    frame.insert(
        php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"function".to_vec())),
        Value::String(PhpString::from_bytes(function.as_bytes().to_vec())),
    );
    frame.insert(
        php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"args".to_vec())),
        Value::Array(php_runtime::api::PhpArray::from_packed(arguments)),
    );
    trace.append(Value::Array(frame));
    exception.insert(trace_key, Value::Array(trace));
    throwable
}

pub(super) fn native_throwable_with_internal_frame(
    context: &NativeExecutionContext<'_>,
    mut throwable: Value,
    source: &php_ir::Instruction,
) -> Value {
    let Value::Array(exception) = &mut throwable else {
        return throwable;
    };
    let trace_key = php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"trace".to_vec()));
    let Some(Value::Array(mut trace)) = exception.get(&trace_key).cloned() else {
        return throwable;
    };
    let Some(index) = trace.len().checked_sub(1) else {
        return throwable;
    };
    let frame_key = php_runtime::api::ArrayKey::Int(i64::try_from(index).unwrap_or(i64::MAX));
    let Some(Value::Array(mut frame)) = trace.get(&frame_key).cloned() else {
        return throwable;
    };
    let function_key =
        php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"function".to_vec()));
    if matches!(frame.get(&function_key), Some(Value::String(name)) if name.as_bytes().starts_with(b"closure@"))
    {
        let path = context
            .unit
            .files
            .get(source.span.file.index())
            .map_or("<unknown>", |file| file.path.as_str());
        let display = format!("{{closure:{path}:{}}}", native_source_line(context, source));
        frame.insert(
            function_key,
            Value::String(PhpString::from_bytes(display.into_bytes())),
        );
    }
    frame.insert(
        php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"internal".to_vec())),
        Value::Bool(true),
    );
    trace.insert(frame_key, Value::Array(frame));
    exception.insert(trace_key, Value::Array(trace));
    throwable
}

pub(super) fn native_throwable_with_call_source(
    context: &NativeExecutionContext<'_>,
    mut throwable: Value,
    source_span: php_ir::IrSpan,
) -> Value {
    let Value::Array(exception) = &mut throwable else {
        return throwable;
    };
    let trace_key = php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"trace".to_vec()));
    let Some(Value::Array(mut trace)) = exception.get(&trace_key).cloned() else {
        return throwable;
    };
    let Some(index) = trace.len().checked_sub(1) else {
        return throwable;
    };
    let frame_key = php_runtime::api::ArrayKey::Int(i64::try_from(index).unwrap_or(i64::MAX));
    let Some(Value::Array(mut frame)) = trace.get(&frame_key).cloned() else {
        return throwable;
    };
    let path = context
        .unit
        .files
        .get(source_span.file.index())
        .map_or("<unknown>", |file| file.path.as_str());
    frame.insert(
        php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"file".to_vec())),
        Value::String(PhpString::from_bytes(path.as_bytes().to_vec())),
    );
    frame.insert(
        php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"line".to_vec())),
        Value::Int(
            i64::try_from(native_source_line_for_span(context, source_span)).unwrap_or(i64::MAX),
        ),
    );
    trace.insert(frame_key, Value::Array(frame));
    exception.insert(trace_key, Value::Array(trace));
    throwable
}
