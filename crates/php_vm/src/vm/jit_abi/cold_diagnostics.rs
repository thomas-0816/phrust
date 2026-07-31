use super::*;
use php_runtime::api::PhpString;
use php_runtime::api::Value;

pub(super) fn publish_native_call_diagnostic(
    context: &mut NativeRequestColdState<'_>,
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
    context: &mut NativeRequestColdState<'_>,
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
    context: &mut NativeRequestColdState<'_>,
    class: &str,
    message: &str,
) -> Result<i64, String> {
    encode_native_throwable_fields(context, class, message, None, None)
}

pub(super) fn encode_native_throwable_at(
    context: &mut NativeRequestColdState<'_>,
    class: &str,
    message: &str,
    span: php_ir::IrSpan,
) -> Result<i64, String> {
    encode_native_throwable_fields(context, class, message, Some(span), None)
}

pub(super) fn native_argument_count_message(
    context: &NativeRequestColdState<'_>,
    function: &str,
    passed: usize,
    required: usize,
    callsite: php_ir::IrSpan,
) -> String {
    let path = context
        .unit
        .files
        .get(callsite.file.index())
        .or_else(|| context.unit.files.first())
        .map_or("<unknown>", |file| file.path.as_str());
    let line = native_source_line_for_span(context, callsite);
    format!(
        "Too few arguments to function {function}(), {passed} passed in {path} on line {line} and exactly {required} expected"
    )
}

/// Immutable publication-time metadata for one optimizing `MakeException`
/// site. Generated code forwards only the opaque pointer and the native
/// message encoding to the exact allocator; class/source resolution never
/// runs in the exception-construction path.
#[repr(C)]
pub(crate) struct PreparedNativeThrowableSite {
    pub native_view: php_jit::JitNativePreparedExceptionView,
    pub runtime_class: php_runtime::api::ClassEntry,
    pub display_name: String,
    pub function_name: String,
    pub include_function_frame: bool,
    pub file: Box<[u8]>,
    pub line: i64,
}

pub(super) fn prepare_native_throwable_site(
    context: &NativeRequestColdState<'_>,
    class: &str,
    function_name: &str,
    include_function_frame: bool,
    span: php_ir::IrSpan,
) -> PreparedNativeThrowableSite {
    fn string_capacity(length: usize) -> usize {
        length
            .max(php_jit::JIT_NATIVE_DIRECT_STRING_MIN_CAPACITY as usize)
            .checked_next_power_of_two()
            .filter(|capacity| *capacity <= php_jit::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY)
            .unwrap_or(php_jit::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY)
    }

    let (runtime_class, display_name) = native_throwable_class(class);
    let file = context
        .unit
        .files
        .get(span.file.index())
        .or_else(|| context.unit.files.first())
        .map_or_else(
            || Box::<[u8]>::from(b"<unknown>".as_slice()),
            |file| Box::<[u8]>::from(file.path.as_bytes()),
        );
    let line = i64::try_from(native_source_line_for_span(context, span)).unwrap_or(i64::MAX);
    let fixed_string_bytes = string_capacity(file.len()).saturating_add(
        include_function_frame
            .then(|| {
                string_capacity("function".len())
                    .saturating_add(string_capacity(function_name.len()))
                    .saturating_add(string_capacity("args".len()))
            })
            .unwrap_or(0),
    );
    PreparedNativeThrowableSite {
        native_view: php_jit::JitNativePreparedExceptionView {
            fixed_string_bytes: u64::try_from(fixed_string_bytes).unwrap_or(u64::MAX),
            fixed_value_slots: if include_function_frame { 9 } else { 4 },
            fixed_array_entries: if include_function_frame { 3 } else { 1 },
            property_slots: 6,
            include_function_frame: u32::from(include_function_frame),
        },
        runtime_class,
        display_name,
        function_name: function_name.to_owned(),
        include_function_frame,
        file,
        line,
    }
}

pub(super) fn native_throwable_class(class: &str) -> (php_runtime::api::ClassEntry, String) {
    let normalized = normalize_class_name(class);
    let descriptor = php_std::ExtensionRegistry::standard_library().enabled_class(&normalized);
    let display_name = descriptor.map_or_else(
        || class.trim_start_matches('\\').to_owned(),
        |descriptor| descriptor.name().to_owned(),
    );
    let source = descriptor.and_then(php_std::ClassDescriptor::source_metadata);
    let parent = source
        .and_then(|metadata| metadata.parent)
        .map(ToOwned::to_owned)
        .or_else(|| match normalized.as_str() {
            "argumentcounterror" => Some("TypeError".to_owned()),
            "typeerror"
            | "valueerror"
            | "arithmeticerror"
            | "divisionbyzeroerror"
            | "compileerror"
            | "parseerror"
            | "fibererror"
            | "unhandledmatcherror" => Some("Error".to_owned()),
            "errorexception" => Some("Exception".to_owned()),
            _ if normalized.ends_with("exception") && normalized != "exception" => {
                Some("Exception".to_owned())
            }
            _ => None,
        });
    let interfaces = source
        .map(|metadata| {
            metadata
                .interfaces
                .iter()
                .map(|interface| (*interface).to_owned())
                .collect()
        })
        .unwrap_or_else(|| vec!["Throwable".to_owned()]);
    (
        php_runtime::api::ClassEntry {
            name: Arc::from(normalized),
            parent,
            interfaces,
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: php_runtime::api::ClassFlags::default(),
        },
        display_name,
    )
}

pub(super) fn initialize_native_throwable_parent(
    context: &mut NativeRequestColdState<'_>,
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
        let receiver = context.encode_native_object_owner(object)?;
        let default_message = arguments
            .first()
            .is_none()
            .then(|| context.encode_native_string_owner(PhpString::from_bytes(Vec::new())))
            .transpose()?;
        let message = arguments
            .first()
            .copied()
            .or(default_message)
            .expect("throwable message has an argument or native default");
        let code = arguments
            .get(1)
            .copied()
            .unwrap_or_else(|| context.encode_native_int(0).unwrap_or(0));
        let previous = arguments
            .get(2)
            .copied()
            .unwrap_or_else(|| php_jit::jit_encode_constant(u32::MAX));
        let assigned = (|| {
            for (property, value) in [("message", message), ("code", code), ("previous", previous)]
            {
                if context
                    .assign_plain_native_dynamic_property(receiver, value, property, true)?
                    .is_none()
                {
                    return Err(format!(
                        "{class}::__construct() could not publish native ${property}"
                    ));
                }
            }
            Ok(php_jit::jit_encode_constant(u32::MAX))
        })();
        if let Some(default_message) = default_message {
            let _ = context.release(default_message);
        }
        let released = context.release(receiver);
        assigned.and(released.map(|()| php_jit::jit_encode_constant(u32::MAX)))
    })())
}

fn encode_native_throwable_fields(
    context: &mut NativeRequestColdState<'_>,
    class: &str,
    message: &str,
    span: Option<php_ir::IrSpan>,
    code: Option<i64>,
) -> Result<i64, String> {
    let (runtime_class, display_name) = native_throwable_class(class);
    let exception =
        php_runtime::api::ObjectRef::new_with_display_name(&runtime_class, display_name);
    let file = span
        .and_then(|span| context.unit.files.get(span.file.index()))
        .or_else(|| context.unit.files.first())
        .map_or("<unknown>", |file| file.path.as_str());
    exception.set_property(
        "message",
        Value::String(PhpString::from_bytes(message.as_bytes().to_vec())),
    );
    exception.set_property(
        "file",
        Value::String(PhpString::from_bytes(file.as_bytes().to_vec())),
    );
    exception.set_property(
        "line",
        Value::Int(span.map_or(0, |span| {
            i64::try_from(native_source_line_for_span(context, span)).unwrap_or(i64::MAX)
        })),
    );
    exception.set_property("code", Value::Int(code.unwrap_or(0)));
    exception.set_property("previous", Value::Null);
    exception.set_property("trace", Value::Array(php_runtime::api::PhpArray::new()));
    context.encode_native_object_owner(exception)
}

pub(super) fn native_throwable_with_frame(
    throwable: Value,
    function: &str,
    arguments: Vec<Value>,
) -> Value {
    let trace_key = php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"trace".to_vec()));
    let mut trace = match &throwable {
        Value::Array(exception) => match exception.get(&trace_key) {
            Some(Value::Array(trace)) => trace.clone(),
            _ => php_runtime::api::PhpArray::new(),
        },
        Value::Object(exception) => match exception.get_property("trace") {
            Some(Value::Array(trace)) => trace,
            _ => php_runtime::api::PhpArray::new(),
        },
        _ => php_runtime::api::PhpArray::new(),
    };
    trace.append(native_trace_frame(function, arguments));
    match &throwable {
        Value::Array(exception) => {
            let mut exception = exception.clone();
            exception.insert(trace_key, Value::Array(trace));
            return Value::Array(exception);
        }
        Value::Object(exception) => exception.set_property("trace", Value::Array(trace)),
        _ => {}
    }
    throwable
}

fn native_trace_frame(function: &str, arguments: Vec<Value>) -> Value {
    let mut frame = php_runtime::api::PhpArray::new();
    frame.insert(
        php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"function".to_vec())),
        Value::String(PhpString::from_bytes(function.as_bytes().to_vec())),
    );
    frame.insert(
        php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"args".to_vec())),
        Value::Array(php_runtime::api::PhpArray::from_packed(arguments)),
    );
    Value::Array(frame)
}

/// Appends a call frame to a throwable whose object properties are already
/// authoritative in the native slot plane. Exception propagation is a cold
/// boundary, so frame values may materialize here, but the updated trace must
/// be written back through the native property owner rather than through the
/// now-empty compatibility `ObjectRef` maps.
pub(super) fn append_native_throwable_frame(
    context: &mut NativeRequestColdState<'_>,
    throwable: i64,
    function: &str,
    arguments: &[i64],
) -> Result<(), String> {
    let trace = context
        .native_object_property_value(throwable, "trace")
        .ok_or_else(|| "native throwable has no authoritative trace property".to_owned())?;
    let Value::Array(mut trace) = context.decode_baseline_value(trace)? else {
        return Err("native throwable trace property is not an array".to_owned());
    };
    let arguments = arguments
        .iter()
        .map(|argument| context.decode_baseline_value(*argument))
        .collect::<Result<Vec<_>, _>>()?;
    trace.append(native_trace_frame(function, arguments));
    let trace = context.encode_baseline_value(Value::Array(trace))?;
    if context.replace_native_object_property_owned(throwable, "trace", trace)? {
        Ok(())
    } else {
        Err("native throwable trace property rejected an exact native replacement".to_owned())
    }
}

pub(super) fn native_throwable_with_internal_frame(
    context: &NativeRequestColdState<'_>,
    throwable: Value,
    source: &php_ir::Instruction,
) -> Value {
    let trace_key = php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"trace".to_vec()));
    let Some(Value::Array(mut trace)) = (match &throwable {
        Value::Array(exception) => exception.get(&trace_key).cloned(),
        Value::Object(exception) => exception.get_property("trace"),
        _ => None,
    }) else {
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
    match &throwable {
        Value::Array(exception) => {
            let mut exception = exception.clone();
            exception.insert(trace_key, Value::Array(trace));
            return Value::Array(exception);
        }
        Value::Object(exception) => exception.set_property("trace", Value::Array(trace)),
        _ => {}
    }
    throwable
}

pub(super) fn native_throwable_with_call_source(
    context: &NativeRequestColdState<'_>,
    throwable: Value,
    source_span: php_ir::IrSpan,
) -> Value {
    let trace_key = php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"trace".to_vec()));
    let Some(Value::Array(mut trace)) = (match &throwable {
        Value::Array(exception) => exception.get(&trace_key).cloned(),
        Value::Object(exception) => exception.get_property("trace"),
        _ => None,
    }) else {
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
    match &throwable {
        Value::Array(exception) => {
            let mut exception = exception.clone();
            exception.insert(trace_key, Value::Array(trace));
            return Value::Array(exception);
        }
        Value::Object(exception) => exception.set_property("trace", Value::Array(trace)),
        _ => {}
    }
    throwable
}
