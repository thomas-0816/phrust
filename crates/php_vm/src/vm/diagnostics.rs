use super::*;

pub(super) fn runtime_diagnostic_for_message(
    message: &str,
    compiled: &CompiledUnit,
    stack: &CallStack,
) -> RuntimeDiagnostic {
    if message == "division by zero" {
        return division_by_zero_mvp(RuntimeSourceSpan::default(), stack_trace(compiled, stack));
    }
    let id = message
        .split_once(':')
        .and_then(|(id, _)| id.starts_with("E_").then_some(id))
        .unwrap_or("E_PHP_RUNTIME_ERROR");
    let diagnostic = RuntimeDiagnostic::new(
        id,
        RuntimeSeverity::FatalError,
        message.to_owned(),
        RuntimeSourceSpan::default(),
        stack_trace(compiled, stack),
        php_runtime::api::PhpReferenceClassification::from_diagnostic_id(id),
    );
    match runtime_bringup_payload_without_state(message, id) {
        Some(payload) => diagnostic.with_diagnostic_payload(payload),
        None => diagnostic,
    }
}

pub(super) fn runtime_diagnostic_for_message_with_source_span(
    message: &str,
    compiled: &CompiledUnit,
    stack: &CallStack,
    source_span: RuntimeSourceSpan,
) -> RuntimeDiagnostic {
    if message == "division by zero" {
        return division_by_zero_mvp(source_span, stack_trace(compiled, stack));
    }
    let id = message
        .split_once(':')
        .and_then(|(id, _)| id.starts_with("E_").then_some(id))
        .unwrap_or("E_PHP_RUNTIME_ERROR");
    let diagnostic = RuntimeDiagnostic::new(
        id,
        RuntimeSeverity::FatalError,
        message.to_owned(),
        source_span,
        stack_trace(compiled, stack),
        None,
    );
    match runtime_bringup_payload_without_state(message, id) {
        Some(payload) => diagnostic.with_diagnostic_payload(payload),
        None => diagnostic,
    }
}

pub(super) fn string_offset_negative_index(message: &str) -> Option<i64> {
    message
        .strip_prefix("E_PHP_VM_STRING_OFFSET_NEGATIVE: Illegal string offset ")
        .and_then(|index| index.parse().ok())
}

pub(super) fn emit_string_offset_negative_warning(
    compiled: &CompiledUnit,
    output: &mut OutputBuffer,
    stack: &CallStack,
    state: &mut ExecutionState,
    span: php_ir::IrSpan,
    index: i64,
) {
    let diagnostic = RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_ILLEGAL_STRING_OFFSET",
        RuntimeSeverity::Warning,
        format!("Illegal string offset {index}"),
        runtime_source_span(compiled, span),
        stack_trace(compiled, stack),
        Some(php_runtime::api::PhpReferenceClassification::Warning),
    );
    emit_vm_diagnostic(
        output,
        state,
        &diagnostic,
        php_runtime::api::PhpDiagnosticChannel::Warning,
        php_runtime::api::PHP_E_WARNING,
    );
    state.diagnostics.push(diagnostic);
}

/// Stamps a synthesized throwable with the file/line of the failing operation so
/// its `Fatal error: …` rendering and getFile/getLine report a real location.
pub(super) fn tag_throwable_location(
    throwable: &Value,
    compiled: &CompiledUnit,
    span: php_ir::IrSpan,
) {
    if let Value::Object(object) = throwable {
        if let Some(file) = compiled.unit().files.get(span.file.index()) {
            set_throwable_property(
                object,
                "file",
                Value::string(file.path.clone().into_bytes()),
            );
        }
        let line = source_span_display_line(compiled, span, false)
            .unwrap_or_else(|| i64::from(span.start));
        set_throwable_property(object, "line", Value::Int(line));
    }
}

pub(super) fn reapply_throwable_diagnostic_overrides(throwable: &Value, result: &VmResult) {
    let Value::Object(object) = throwable else {
        return;
    };
    for diagnostic in &result.diagnostics {
        if diagnostic.id() == "E_PHP_RUNTIME_TOKENIZER_PARSE"
            && let Some(RuntimeDiagnosticPayload::TokenizerParse(payload)) = diagnostic.payload()
        {
            set_throwable_property(object, "line", Value::Int(payload.line()));
        }
    }
}

pub(super) fn tag_throwable_location_from_diagnostic(
    object: &ObjectRef,
    diagnostic: &RuntimeDiagnostic,
) {
    let span = diagnostic.source_span();
    let Some(file) = span.file.as_deref().filter(|file| !file.is_empty()) else {
        return;
    };
    set_throwable_property(object, "file", Value::string(file.as_bytes().to_vec()));
    let line = std::fs::read(file)
        .ok()
        .map(|bytes| {
            let offset = (span.start as usize).min(bytes.len());
            1 + bytes[..offset]
                .iter()
                .filter(|&&byte| byte == b'\n')
                .count() as i64
        })
        .unwrap_or_else(|| i64::from(span.start));
    set_throwable_property(object, "line", Value::Int(line));
}

pub(super) fn dynamic_property_deprecation_diagnostic(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: &php_ir::module::ClassEntry,
    object: &ObjectRef,
    property: &str,
    stack: &CallStack,
) -> Option<RuntimeDiagnostic> {
    (!class_allows_dynamic_properties(compiled, state, class)).then(|| {
        RuntimeDiagnostic::new(
            "E_PHP_VM_DYNAMIC_PROPERTY_DEPRECATED",
            RuntimeSeverity::Deprecation,
            format!(
                "E_PHP_VM_DYNAMIC_PROPERTY_DEPRECATED: creating dynamic property {}::${property}",
                object.class_name()
            ),
            RuntimeSourceSpan::default(),
            stack_trace(compiled, stack),
            None,
        )
    })
}

pub(super) fn current_instruction_diagnostic_span(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    instruction_span: IrSpan,
) -> RuntimeSourceSpan {
    if compiled
        .unit()
        .files
        .first()
        .is_some_and(|file| file.path.starts_with("eval://"))
        && let Some(span) = state.eval_diagnostic_spans.last()
    {
        return span.clone();
    }
    runtime_source_span(compiled, instruction_span)
}

pub(super) fn eval_diagnostic_source_span(
    compiled: &CompiledUnit,
    eval_span: IrSpan,
) -> RuntimeSourceSpan {
    if let Some((file, line)) = source_span_file_line(compiled, eval_span) {
        RuntimeSourceSpan {
            file: Some(format!("{file}({line}) : eval()'d code")),
            start: 1,
            end: 1,
        }
    } else {
        runtime_source_span(compiled, eval_span)
    }
}

pub(super) fn current_error_reporting(state: &ExecutionState) -> i64 {
    state
        .ini
        .get("error_reporting")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(-1)
}

pub(super) fn error_reporting_allows(state: &ExecutionState, level: i64) -> bool {
    error_reporting_allows_level(current_error_reporting(state), level)
}

pub(super) fn display_errors_enabled(state: &ExecutionState) -> bool {
    !matches!(
        state.ini.get("display_errors"),
        Some("") | Some("0") | Some("Off") | Some("off")
    )
}

pub(super) fn diagnostic_display_options(
    state: &ExecutionState,
) -> php_runtime::api::PhpDiagnosticDisplayOptions {
    php_runtime::api::PhpDiagnosticDisplayOptions {
        display_errors: display_errors_enabled(state),
        error_reporting: current_error_reporting(state),
        ..php_runtime::api::PhpDiagnosticDisplayOptions::default()
    }
}

pub(super) fn emit_vm_diagnostic(
    output: &mut OutputBuffer,
    state: &ExecutionState,
    diagnostic: &RuntimeDiagnostic,
    channel: php_runtime::api::PhpDiagnosticChannel,
    level: i64,
) -> bool {
    emit_vm_diagnostic_with_options(output, state, diagnostic, channel, level, true)
}

pub(super) fn emit_vm_diagnostic_with_options(
    output: &mut OutputBuffer,
    state: &ExecutionState,
    diagnostic: &RuntimeDiagnostic,
    channel: php_runtime::api::PhpDiagnosticChannel,
    level: i64,
    leading_newline: bool,
) -> bool {
    let mut options = diagnostic_display_options(state);
    options.leading_newline = leading_newline;
    emit_php_diagnostic(output, diagnostic, channel, level, options)
}

pub(super) fn runtime_source_span(
    compiled: &CompiledUnit,
    span: php_ir::IrSpan,
) -> RuntimeSourceSpan {
    RuntimeSourceSpan {
        file: compiled
            .unit()
            .files
            .get(span.file.index())
            .map(|file| file.path.clone()),
        start: span.start,
        end: span.end,
    }
}

pub(super) fn is_supported_user_error_level(level: i64) -> bool {
    matches!(
        level,
        php_std::constants::E_USER_WARNING
            | php_std::constants::E_USER_NOTICE
            | php_std::constants::E_USER_DEPRECATED
            | php_std::constants::E_USER_ERROR
    )
}

pub(super) fn error_level_channel(level: i64) -> php_runtime::api::PhpDiagnosticChannel {
    match level {
        php_std::constants::E_USER_NOTICE => php_runtime::api::PhpDiagnosticChannel::Notice,
        php_std::constants::E_USER_DEPRECATED => php_runtime::api::PhpDiagnosticChannel::Deprecated,
        php_std::constants::E_USER_ERROR => php_runtime::api::PhpDiagnosticChannel::FatalError,
        _ => php_runtime::api::PhpDiagnosticChannel::Warning,
    }
}

pub(super) fn error_level_diagnostic_id(level: i64) -> &'static str {
    match level {
        php_std::constants::E_USER_NOTICE => "E_PHP_VM_USER_NOTICE",
        php_std::constants::E_USER_DEPRECATED => "E_PHP_VM_USER_DEPRECATED",
        php_std::constants::E_USER_ERROR => "E_PHP_VM_USER_ERROR",
        _ => "E_PHP_VM_USER_WARNING",
    }
}

pub(super) fn error_level_severity(level: i64) -> RuntimeSeverity {
    match level {
        php_std::constants::E_USER_NOTICE => RuntimeSeverity::Notice,
        php_std::constants::E_USER_DEPRECATED => RuntimeSeverity::Deprecation,
        php_std::constants::E_USER_ERROR => RuntimeSeverity::FatalError,
        _ => RuntimeSeverity::Warning,
    }
}

pub(super) fn runtime_error_throwable(result: &VmResult) -> Option<Value> {
    for diagnostic in &result.diagnostics {
        if let Some(throwable) = runtime_diagnostic_throwable(diagnostic) {
            return Some(throwable);
        }
    }
    None
}

pub(super) fn runtime_diagnostic_throwable(diagnostic: &RuntimeDiagnostic) -> Option<Value> {
    let class_name = match diagnostic.id() {
        "E_PHP_RUNTIME_BUILTIN_ARITY" => "ArgumentCountError",
        "E_PHP_RUNTIME_BUILTIN_TYPE" => "TypeError",
        "E_PHP_RUNTIME_BUILTIN_VALUE" => "ValueError",
        "E_PHP_RUNTIME_TOKENIZER_PARSE" => "ParseError",
        "E_PHP_VM_EXCEPTION" => "Exception",
        "E_PHP_RUNTIME_JSON_EXCEPTION" => "JsonException",
        "E_PHP_RUNTIME_SODIUM_EXCEPTION" => "SodiumException",
        "E_PHP_VM_PDO_EXCEPTION" => "PDOException",
        "E_PHP_VM_SPL_RUNTIME_EXCEPTION" => "RuntimeException",
        "E_PHP_VM_SPL_BAD_METHOD_CALL" => "BadMethodCallException",
        "E_PHP_VM_SPL_INVALID_ARGUMENT" => "InvalidArgumentException",
        "E_PHP_VM_SPL_OUT_OF_BOUNDS" => "OutOfBoundsException",
        "E_PHP_VM_SPL_ERROR" => "Error",
        "E_PHP_VM_SPL_VALUE_ERROR" => "ValueError",
        "E_PHP_VM_SPL_TYPE_ERROR" => "TypeError",
        "E_PHP_STD_MISSING_ARGUMENT" | "E_PHP_STD_TOO_MANY_ARGUMENTS" => "ArgumentCountError",
        "E_PHP_STD_TYPE_ERROR" => "TypeError",
        "E_PHP_STD_VALUE_ERROR" => "ValueError",
        "E_PHP_VM_TOO_FEW_ARGS" | "E_PHP_VM_TOO_MANY_ARGS" => "ArgumentCountError",
        "E_PHP_VM_BY_REF_ARG_NOT_REFERENCEABLE" => "Error",
        "E_PHP_VM_ARRAY_ACCESS_BIND_REFERENCE" => "Error",
        "E_PHP_VM_FOREACH_BY_REF_ITERATOR" => "Error",
        "E_PHP_RUNTIME_SHMOP_READ_ONLY" => "Error",
        "E_PHP_RUNTIME_SYSVSHM_INVALID" => "Error",
        "E_PHP_VM_UNKNOWN_NAMED_ARG" | "E_PHP_VM_DUPLICATE_NAMED_ARG" => "Error",
        "E_PHP_VM_UNHANDLED_MATCH" => "UnhandledMatchError",
        "E_PHP_RUNTIME_OBJECT_TO_STRING_GAP" => "Error",
        "E_PHP_RUNTIME_NON_NUMERIC_STRING" => "TypeError",
        "E_PHP_RUNTIME_UNSUPPORTED_OPERAND_TYPES" => "TypeError",
        "E_PHP_VM_TOSTRING_RETURN_TYPE" => "TypeError",
        "E_PHP_VM_SLEEP_RETURN_TYPE" => "TypeError",
        "E_PHP_VM_STRING_OFFSET_TYPE" => "TypeError",
        "E_PHP_VM_PARAM_TYPE_MISMATCH" => "TypeError",
        "E_PHP_VM_DYNAMIC_CLASS_NAME_TYPE" => "TypeError",
        "E_PHP_VM_AUTOLOAD_INVALID_CALLBACK" => "TypeError",
        "E_PHP_VM_TOKENIZER_KIND_TYPE" | "E_PHP_VM_TOKENIZER_KIND_ELEMENT_TYPE" => "TypeError",
        "E_PHP_VM_ARRAY_KEY_CONVERSION" => "Error",
        "E_PHP_VM_PRIVATE_METHOD_ACCESS"
        | "E_PHP_VM_PROTECTED_METHOD_ACCESS"
        | "E_PHP_VM_ABSTRACT_METHOD_CALL"
        | "E_PHP_VM_CLOSURE_INSTANTIATION"
        | "E_PHP_VM_INTERFACE_INSTANTIATION"
        | "E_PHP_VM_PRIVATE_PROPERTY_ACCESS"
        | "E_PHP_VM_PROTECTED_PROPERTY_ACCESS"
        | "E_PHP_VM_PRIVATE_PROPERTY_SET_ACCESS"
        | "E_PHP_VM_PROTECTED_PROPERTY_SET_ACCESS"
        | "E_PHP_VM_READONLY_PROPERTY_WRITE"
        | "E_PHP_VM_UNINITIALIZED_PROPERTY"
        | "E_PHP_VM_UNINITIALIZED_STATIC_PROPERTY"
        | "E_PHP_VM_UNKNOWN_STATIC_PROPERTY"
        | "E_PHP_VM_NON_STATIC_METHOD_CALL"
        | "E_PHP_VM_INVALID_STATIC_SCOPE"
        | "E_PHP_VM_CURRENT_CLOSURE"
        | "E_PHP_VM_THIS_OUTSIDE_METHOD"
        | "E_PHP_VM_DYNAMIC_PROPERTY_ERROR"
        | "E_PHP_VM_PIPE_RHS_NOT_CALLABLE"
        | "E_PHP_VM_UNKNOWN_METHOD"
        | "E_PHP_VM_UNKNOWN_CLASS"
        | "E_PHP_VM_UNDEFINED_CONSTANT"
        | "E_PHP_VM_UNKNOWN_CLASS_CONSTANT"
        | "E_PHP_VM_PRIVATE_CLASS_CONSTANT_ACCESS"
        | "E_PHP_VM_PROTECTED_CLASS_CONSTANT_ACCESS"
        | "E_PHP_VM_UNSUPPORTED_PROPERTY_MODIFIER"
        | "E_PHP_VM_ABSTRACT_CLASS_INSTANTIATION"
        | "E_PHP_VM_METHOD_CALL_NON_OBJECT"
        | "E_PHP_VM_TOKENIZER_UNINITIALIZED_PROPERTY" => "Error",
        "E_PHP_VM_PROPERTY_TYPE_MISMATCH" => "TypeError",
        // The reference engine throws plain Error for first-class callable
        // acquisition failures, but Closure::fromCallable wraps the same
        // failures as TypeError with its own message prefix.
        "E_PHP_VM_FIRST_CLASS_CALLABLE_NOT_CALLABLE"
        | "E_PHP_VM_FIRST_CLASS_CALLABLE_UNDEFINED_FUNCTION"
        | "E_PHP_VM_FIRST_CLASS_CALLABLE_UNDEFINED_METHOD"
        | "E_PHP_VM_FIRST_CLASS_CALLABLE_NON_STATIC_METHOD"
        | "E_PHP_VM_FIRST_CLASS_CALLABLE_UNRESOLVED_DYNAMIC" => {
            if diagnostic
                .message()
                .contains("Failed to create closure from callable")
            {
                "TypeError"
            } else {
                "Error"
            }
        }
        "E_PHP_VM_REFLECTION_UNKNOWN_CLASS"
        | "E_PHP_VM_REFLECTION_UNKNOWN_METHOD"
        | "E_PHP_VM_REFLECTION_UNKNOWN_PROPERTY" => "ReflectionException",
        _ => return None,
    };
    let message = diagnostic
        .message()
        .split_once(": ")
        .filter(|(prefix, _)| prefix.starts_with("E_"))
        .map_or_else(|| diagnostic.message(), |(_, message)| message);
    let object =
        make_exception_object(class_name, &Value::string(message.as_bytes().to_vec())).ok()?;
    if diagnostic.id() == "E_PHP_RUNTIME_JSON_EXCEPTION"
        && let Some(RuntimeDiagnosticPayload::JsonBuiltin(payload)) = diagnostic.payload()
    {
        set_throwable_property(&object, "code", Value::Int(payload.error_code()));
    }
    tag_throwable_location_from_diagnostic(&object, diagnostic);
    if diagnostic.id() == "E_PHP_RUNTIME_TOKENIZER_PARSE"
        && let Some(RuntimeDiagnosticPayload::TokenizerParse(payload)) = diagnostic.payload()
    {
        set_throwable_property(&object, "line", Value::Int(payload.line()));
    }
    Some(Value::Object(object))
}

pub(super) fn stack_trace(compiled: &CompiledUnit, stack: &CallStack) -> Vec<RuntimeStackFrame> {
    stack
        .frames()
        .iter()
        .rev()
        .map(|frame| {
            let name = compiled
                .unit()
                .functions
                .get(frame.function.index())
                .map(|function| function.name.as_str())
                .unwrap_or("<missing>");
            RuntimeStackFrame::new(name)
        })
        .collect()
}

pub(super) fn debug_backtrace_array(
    compiled: &CompiledUnit,
    stack: &CallStack,
    options: i64,
    limit: usize,
) -> PhpArray {
    let ignore_args = options & 2 != 0;
    let mut trace = PhpArray::new();
    for frame in debug_backtrace_frames(compiled, stack, limit) {
        let mut entry = PhpArray::new();
        let (file, line) = frame_source_location(compiled, frame);
        if !file.is_empty() {
            entry.insert(string_key("file"), Value::string(file));
        }
        if line > 0 {
            entry.insert(string_key("line"), Value::Int(line));
        }
        entry.insert(
            string_key("function"),
            Value::string(frame_function_display_name(compiled, frame)),
        );
        if !ignore_args {
            entry.insert(
                string_key("args"),
                Value::Array(frame_trace_args_array(compiled, frame)),
            );
        }
        trace.append(Value::Array(entry));
    }
    trace
}

pub(super) fn capture_debug_print_backtrace_string(
    compiled: &CompiledUnit,
    stack: &CallStack,
    options: i64,
    limit: usize,
) -> String {
    let ignore_args = options & 2 != 0;
    debug_backtrace_frames(compiled, stack, limit)
        .into_iter()
        .enumerate()
        .map(|(index, frame)| {
            let (file, line) = frame_source_location(compiled, frame);
            let args = if ignore_args {
                String::new()
            } else {
                format_frame_trace_args(compiled, frame)
            };
            format!(
                "#{index} {file}({line}): {}({args})",
                frame_function_display_name(compiled, frame)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn debug_backtrace_frames<'a>(
    compiled: &CompiledUnit,
    stack: &'a CallStack,
    limit: usize,
) -> Vec<&'a Frame> {
    let frames = stack.frames().iter().rev().filter(|frame| {
        compiled
            .unit()
            .functions
            .get(frame.function.index())
            .is_some_and(|function| !function.flags.is_top_level)
    });
    if limit == 0 {
        frames.collect()
    } else {
        frames.take(limit).collect()
    }
}

pub(super) fn string_key(name: &str) -> ArrayKey {
    ArrayKey::String(PhpString::from_test_str(name))
}

pub(super) fn defined_symbol_names_array(names: &BTreeSet<String>) -> PhpArray {
    PhpArray::from_packed(names.iter().cloned().map(Value::string).collect())
}

fn frame_source_location(compiled: &CompiledUnit, frame: &Frame) -> (String, i64) {
    let function = compiled.unit().functions.get(frame.function.index());
    let display_span = frame
        .call_span
        .or_else(|| function.map(|function| function.span));
    let file = display_span
        .and_then(|span| compiled.unit().files.get(span.file.index()))
        .map(|file| file.path.clone())
        .unwrap_or_default();
    let line = display_span
        .and_then(|span| source_span_display_line(compiled, span, false))
        .or_else(|| display_span.map(|span| i64::from(span.start)))
        .unwrap_or(0);
    (file, line)
}

fn frame_function_display_name(compiled: &CompiledUnit, frame: &Frame) -> String {
    let Some(function) = compiled.unit().functions.get(frame.function.index()) else {
        return "{closure}".to_owned();
    };
    if function.flags.is_closure {
        let span = runtime_source_span(compiled, function.span);
        return format!(
            "{{closure:{}:{}}}",
            span.file.unwrap_or_default(),
            source_span_display_line(compiled, function.span, false)
                .unwrap_or_else(|| i64::from(function.span.start))
        );
    }
    if function.flags.is_method && function.name.contains("::") {
        let has_this = function
            .locals
            .iter()
            .position(|local| local == "this")
            .and_then(|index| frame.locals.get(LocalId::new(index as u32)))
            .is_some_and(|value| matches!(value, Value::Object(_)));
        if has_this {
            return function.name.replacen("::", "->", 1);
        }
    }
    function.name.clone()
}

/// Materializes a frame's backtrace arguments (see [`TraceArguments`]).
///
/// The lazy variant reconstructs from the live parameter locals — matching the
/// reference engine, where traces show the *current* slot values, parameter
/// mutations included — with the `arguments` vector supplying positional
/// extras beyond the declared parameters. A variadic tail expands its
/// collected array (named-argument labels preserved as string keys). Sensitive
/// parameters redact exactly as the eager builder did. When the frame's
/// function is not resolvable in `compiled` (foreign-unit frame), the raw
/// `arguments` vector is the fallback, mirroring the previous empty-snapshot
/// behavior.
fn materialized_frame_trace_arguments(
    compiled: &CompiledUnit,
    frame: &Frame,
) -> Vec<FrameTraceArgument> {
    let arg_count = match &frame.trace_arguments {
        TraceArguments::Materialized(entries) => return entries.clone(),
        TraceArguments::Lazy { arg_count } => *arg_count as usize,
    };
    let raw_fallback = |values: &[Value]| -> Vec<FrameTraceArgument> {
        values
            .iter()
            .map(|value| FrameTraceArgument {
                name: None,
                value: value.clone(),
            })
            .collect()
    };
    let Some(function) = compiled.unit().functions.get(frame.function.index()) else {
        return raw_fallback(&frame.arguments);
    };
    // A parameter local can be unbound when the frame failed argument
    // binding (a bind-time TypeError's own trace still shows the raw
    // argument, as the reference engine does) — fall back to the preserved
    // arguments vector by position.
    let live_local = |local: LocalId, index: usize| -> Value {
        match frame.locals.get_slot(local) {
            Some(Slot::Value(value)) if !value.is_uninitialized() => value.clone(),
            Some(Slot::Reference(cell)) => cell.get(),
            _ => frame.arguments.get(index).cloned().unwrap_or(Value::Null),
        }
    };
    let mut out = Vec::with_capacity(arg_count);
    for (index, param) in function.params.iter().enumerate() {
        if param.variadic {
            let sensitive = param_is_sensitive(param);
            if let Value::Array(array) = live_local(param.local, index) {
                for (key, value) in array.iter() {
                    let name = match key {
                        ArrayKey::String(name) => Some(name.to_string_lossy()),
                        ArrayKey::Int(_) => None,
                    };
                    out.push(FrameTraceArgument {
                        name,
                        value: trace_value_for_param(value, sensitive),
                    });
                }
            }
            return out;
        }
        if index >= arg_count {
            return out;
        }
        let value = live_local(param.local, index);
        out.push(FrameTraceArgument {
            name: None,
            value: trace_value_for_param(&value, param_is_sensitive(param)),
        });
    }
    // Positional extras beyond the declared parameters come from the
    // preserved arguments vector (they have no bound local).
    for value in frame
        .arguments
        .iter()
        .take(arg_count)
        .skip(function.params.len())
    {
        out.push(FrameTraceArgument {
            name: None,
            value: value.clone(),
        });
    }
    out
}

fn frame_trace_args_array(compiled: &CompiledUnit, frame: &Frame) -> PhpArray {
    let entries = materialized_frame_trace_arguments(compiled, frame);
    if entries.is_empty() {
        return PhpArray::from_packed(frame.arguments.clone());
    }
    let mut array = PhpArray::new();
    for arg in entries {
        // By-ref parameters store the live cell; PHP-visible trace args
        // carry the current value, not the reference wrapper.
        let value = match arg.value {
            Value::Reference(cell) => cell.get(),
            value => value,
        };
        if let Some(name) = arg.name {
            array.insert(ArrayKey::String(PhpString::from(name.as_str())), value);
        } else {
            array.append(value);
        }
    }
    array
}

fn format_frame_trace_args(compiled: &CompiledUnit, frame: &Frame) -> String {
    let entries = materialized_frame_trace_arguments(compiled, frame);
    if entries.is_empty() {
        return frame
            .arguments
            .iter()
            .map(format_trace_arg)
            .collect::<Vec<_>>()
            .join(", ");
    }
    entries
        .iter()
        .map(|arg| {
            let value = format_trace_arg(&arg.value);
            if let Some(name) = &arg.name {
                format!("{name}: {value}")
            } else {
                value
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Renders one argument as it appears in a PHP stack trace.
pub(super) fn format_trace_arg(value: &Value) -> String {
    match value {
        Value::Reference(cell) => format_trace_arg(&cell.get()),
        Value::Null | Value::Uninitialized => "NULL".to_owned(),
        Value::Bool(true) => "true".to_owned(),
        Value::Bool(false) => "false".to_owned(),
        Value::Int(value) => value.to_string(),
        Value::Float(value) => {
            let value = value.to_f64();
            if value.fract() == 0.0 && value.is_finite() {
                format!("{value:.1}")
            } else {
                format!("{value}")
            }
        }
        Value::String(value) => {
            let text = value.to_string_lossy();
            if let Some(preview) = trace_string_preview(&text, 15) {
                format!("'{preview}...'")
            } else {
                format!("'{text}'")
            }
        }
        Value::Array(_) => "Array".to_owned(),
        Value::Object(object) => format!("Object({})", object.display_name()),
        Value::Fiber(_) => "Object(Fiber)".to_owned(),
        Value::Generator(_) => "Object(Generator)".to_owned(),
        Value::Resource(_) => "Resource".to_owned(),
        Value::Callable(_) => "Object(Closure)".to_owned(),
    }
}

fn trace_string_preview(text: &str, max_chars: usize) -> Option<&str> {
    text.char_indices()
        .nth(max_chars)
        .map(|(index, _)| &text[..index])
}

/// Renders a frame's `Class->method(args)` / `func(args)` call descriptor.
fn format_trace_call(compiled: &CompiledUnit, frame: &Frame) -> String {
    if compiled
        .unit()
        .functions
        .get(frame.function.index())
        .is_none()
    {
        return "{closure}()".to_owned();
    }
    let args = format_frame_trace_args(compiled, frame);
    let name = frame_function_display_name(compiled, frame);
    format!("{name}({args})")
}

/// Captures the current call stack as PHP's `Stack trace:` body, newest frame
/// first, ending with `#N {main}`.
pub(super) fn capture_backtrace_string(compiled: &CompiledUnit, stack: &CallStack) -> String {
    capture_backtrace_string_from_index(compiled, stack, 0)
}

pub(super) fn capture_backtrace_string_with_failed_call(
    compiled: &CompiledUnit,
    stack: &CallStack,
    function: &IrFunction,
    call_span: php_ir::IrSpan,
) -> String {
    let file = compiled
        .unit()
        .files
        .get(call_span.file.index())
        .map(|file| file.path.clone())
        .unwrap_or_default();
    let line = source_span_display_line(compiled, call_span, false)
        .unwrap_or_else(|| i64::from(call_span.start));
    let name = if function.flags.is_method && function.name.contains("::") {
        function.name.replacen("::", "->", 1)
    } else {
        function.name.clone()
    };
    let mut lines = vec![format!("#0 {file}({line}): {name}()")];
    let rest = capture_backtrace_string_from_index(compiled, stack, 1);
    if !rest.is_empty() {
        lines.push(rest);
    }
    lines.join("\n")
}

pub(super) fn capture_backtrace_string_with_builtin_failed_call(
    compiled: &CompiledUnit,
    stack: &CallStack,
    function: &str,
    values: &[Value],
    call_span: php_ir::IrSpan,
) -> String {
    let file = compiled
        .unit()
        .files
        .get(call_span.file.index())
        .map(|file| file.path.clone())
        .unwrap_or_default();
    let line = source_span_display_line(compiled, call_span, false)
        .unwrap_or_else(|| i64::from(call_span.start));
    let args = values
        .iter()
        .map(format_trace_arg)
        .collect::<Vec<_>>()
        .join(", ");
    let mut lines = vec![format!("#0 {file}({line}): {function}({args})")];
    let rest = capture_backtrace_string_from_index(compiled, stack, 1);
    if !rest.is_empty() {
        lines.push(rest);
    }
    lines.join("\n")
}

fn builtin_failed_call_trace_array(
    compiled: &CompiledUnit,
    function: &str,
    values: &[Value],
    call_span: php_ir::IrSpan,
) -> PhpArray {
    let mut trace = PhpArray::new();
    let mut frame = PhpArray::new();
    if let Some(file) = compiled.unit().files.get(call_span.file.index()) {
        frame.insert(
            string_key("file"),
            Value::string(file.path.clone().into_bytes()),
        );
    }
    let line = source_span_display_line(compiled, call_span, false)
        .unwrap_or_else(|| i64::from(call_span.start));
    frame.insert(string_key("line"), Value::Int(line));
    frame.insert(
        string_key("function"),
        Value::string(function.as_bytes().to_vec()),
    );
    frame.insert(
        string_key("args"),
        Value::Array(PhpArray::from_packed(values.to_vec())),
    );
    trace.append(Value::Array(frame));
    trace
}

pub(super) fn attach_builtin_failed_call_trace(
    throwable: &Value,
    compiled: &CompiledUnit,
    stack: &CallStack,
    function: &str,
    values: &[Value],
    call_span: php_ir::IrSpan,
) -> String {
    let trace_string = capture_backtrace_string_with_builtin_failed_call(
        compiled, stack, function, values, call_span,
    );
    if let Value::Object(object) = throwable {
        set_throwable_property(
            object,
            "trace",
            Value::Array(builtin_failed_call_trace_array(
                compiled, function, values, call_span,
            )),
        );
        set_throwable_property(
            object,
            "trace_string",
            Value::string(trace_string.clone().into_bytes()),
        );
    }
    trace_string
}

pub(super) fn attach_tokenizer_static_error_handler_throw_trace(
    compiled: &CompiledUnit,
    stack: &CallStack,
    state: &mut ExecutionState,
    result: &VmResult,
    context: &TokenizerStaticCallTraceContext,
    level: i64,
    diagnostic: &RuntimeDiagnostic,
) {
    let handler_trace = state
        .pending_trace
        .take()
        .unwrap_or_else(|| capture_backtrace_string(compiled, stack));
    let trace = capture_backtrace_string_with_internal_error_handler_static_call(
        compiled,
        stack,
        context,
        &handler_trace,
        level,
        diagnostic,
    );
    if let Some(Value::Object(object)) = state
        .pending_throw
        .as_ref()
        .cloned()
        .or_else(|| runtime_error_throwable(result))
    {
        set_throwable_property(
            &object,
            "trace_string",
            Value::string(trace.clone().into_bytes()),
        );
    }
    state.pending_trace = Some(trace);
}

fn capture_backtrace_string_with_internal_error_handler_static_call(
    compiled: &CompiledUnit,
    stack: &CallStack,
    context: &TokenizerStaticCallTraceContext,
    handler_trace: &str,
    level: i64,
    diagnostic: &RuntimeDiagnostic,
) -> String {
    let file = compiled
        .unit()
        .files
        .get(context.call_span.file.index())
        .map(|file| file.path.clone())
        .unwrap_or_default();
    let line = source_span_display_line(compiled, context.call_span, false)
        .unwrap_or_else(|| i64::from(context.call_span.start));
    let args = context
        .values
        .iter()
        .map(format_trace_arg)
        .collect::<Vec<_>>()
        .join(", ");
    let mut handler_lines = handler_trace.lines();
    let first = handler_lines
        .next()
        .and_then(trace_line_call_descriptor)
        .map(|call| {
            let name = trace_call_name(call);
            let span = diagnostic.source_span();
            let handler_args = [
                Value::Int(level),
                Value::string(diagnostic.message()),
                Value::string(span.file.clone().unwrap_or_default()),
                Value::Int(span.start as i64),
            ]
            .iter()
            .map(format_trace_arg)
            .collect::<Vec<_>>()
            .join(", ");
            format!("{name}({handler_args})")
        })
        .unwrap_or_else(|| "{closure}()".to_owned());
    let mut lines = vec![
        format!("#0 [internal function]: {first}"),
        format!("#1 {file}({line}): {}({args})", context.call),
    ];
    let mut index = 2;
    let mut appended_rest = false;
    for line in handler_lines {
        lines.push(format!("#{index} {}", trace_line_without_index(line)));
        index += 1;
        appended_rest = true;
    }
    if !appended_rest {
        let rest = capture_backtrace_string_from_index(compiled, stack, 1);
        if !rest.is_empty() {
            for line in rest.lines() {
                lines.push(format!("#{index} {}", trace_line_without_index(line)));
                index += 1;
            }
        }
    }
    lines.join("\n")
}

fn trace_line_call_descriptor(line: &str) -> Option<&str> {
    line.split_once(": ").map(|(_, call)| call)
}

fn trace_call_name(call: &str) -> &str {
    call.split_once('(').map_or(call, |(name, _)| name)
}

fn trace_line_without_index(line: &str) -> &str {
    let Some(rest) = line.strip_prefix('#') else {
        return line;
    };
    let Some((index, rest)) = rest.split_once(' ') else {
        return line;
    };
    if index.chars().all(|char| char.is_ascii_digit()) {
        rest
    } else {
        line
    }
}

pub(super) fn capture_backtrace_string_with_internal_iterator_builtin_call(
    compiled: &CompiledUnit,
    stack: &CallStack,
    function: &str,
    values: &[Value],
    call_span: php_ir::IrSpan,
) -> String {
    let file = compiled
        .unit()
        .files
        .get(call_span.file.index())
        .map(|file| file.path.clone())
        .unwrap_or_default();
    let line = source_span_display_line(compiled, call_span, false)
        .unwrap_or_else(|| i64::from(call_span.start));
    let args = values
        .iter()
        .map(format_trace_arg)
        .collect::<Vec<_>>()
        .join(", ");
    let mut lines = vec![
        format!(
            "#0 [internal function]: {}",
            format_internal_iterator_trace_call(compiled, values.first())
        ),
        format!("#1 {file}({line}): {function}({args})"),
    ];
    let rest = capture_backtrace_string_from_index(compiled, stack, 2);
    if !rest.is_empty() {
        lines.push(rest);
    }
    lines.join("\n")
}

pub(super) fn append_throwable_internal_iterator_trace_arg_frame(
    throwable: &Value,
    compiled: &CompiledUnit,
    function: &str,
    values: &[Value],
    call_span: php_ir::IrSpan,
) {
    let Value::Object(object) = throwable else {
        return;
    };
    let mut trace = match get_throwable_property(object, "trace") {
        Some(Value::Array(array)) => array,
        _ => PhpArray::new(),
    };
    let mut frame = PhpArray::new();
    if let Some(file) = compiled.unit().files.get(call_span.file.index()) {
        frame.insert(
            string_key("file"),
            Value::string(file.path.clone().into_bytes()),
        );
    }
    let line = source_span_display_line(compiled, call_span, false)
        .unwrap_or_else(|| i64::from(call_span.start));
    frame.insert(string_key("line"), Value::Int(line));
    frame.insert(
        string_key("function"),
        Value::string(function.as_bytes().to_vec()),
    );
    frame.insert(
        string_key("args"),
        Value::Array(PhpArray::from_packed(values.to_vec())),
    );
    trace.append(Value::Array(frame));
    set_throwable_property(object, "trace", Value::Array(trace));
}

fn format_internal_iterator_trace_call(compiled: &CompiledUnit, value: Option<&Value>) -> String {
    let Some(Value::Generator(generator)) = value.map(effective_value) else {
        return "{closure}()".to_owned();
    };
    let Some(function) = compiled
        .unit()
        .functions
        .get(FunctionId::new(generator.function()).index())
    else {
        return "{closure}()".to_owned();
    };
    if function.flags.is_closure {
        let span = runtime_source_span(compiled, function.span);
        return format!(
            "{{closure:{}:{}}}()",
            span.file.unwrap_or_default(),
            source_span_display_line(compiled, function.span, false)
                .unwrap_or_else(|| i64::from(function.span.start))
        );
    }
    format!("{}()", function.name)
}

pub(super) fn capture_backtrace_string_with_internal_spl_method_call(
    compiled: &CompiledUnit,
    stack: &CallStack,
    internal_call: &str,
    builtin_call: &str,
    call_span: php_ir::IrSpan,
) -> String {
    let file = compiled
        .unit()
        .files
        .get(call_span.file.index())
        .map(|file| file.path.clone())
        .unwrap_or_default();
    let line = source_span_display_line(compiled, call_span, false)
        .unwrap_or_else(|| i64::from(call_span.start));
    let mut lines = vec![
        format!("#0 [internal function]: {internal_call}()"),
        format!("#1 {file}({line}): {builtin_call}()"),
    ];
    let rest = capture_backtrace_string_from_index(compiled, stack, 2);
    if !rest.is_empty() {
        lines.push(rest);
    }
    lines.join("\n")
}

pub(super) fn capture_backtrace_string_with_constant_expression(
    compiled: &CompiledUnit,
    stack: &CallStack,
    call_span: php_ir::IrSpan,
) -> String {
    let file = compiled
        .unit()
        .files
        .get(call_span.file.index())
        .map(|file| file.path.clone())
        .unwrap_or_default();
    let line = source_span_display_line(compiled, call_span, false)
        .unwrap_or_else(|| i64::from(call_span.start));
    let mut lines = vec![format!("#0 {file}({line}): [constant expression]()")];
    let rest = capture_backtrace_string_from_index(compiled, stack, 1);
    if !rest.is_empty() {
        lines.push(rest);
    }
    lines.join("\n")
}

pub(super) fn capture_backtrace_string_from_index(
    compiled: &CompiledUnit,
    stack: &CallStack,
    start_index: usize,
) -> String {
    let mut lines = Vec::new();
    let mut index = start_index;
    for frame in stack.frames().iter().rev() {
        let function = compiled.unit().functions.get(frame.function.index());
        if function.is_some_and(|function| function.flags.is_top_level) {
            lines.push(format!("#{index} {{main}}"));
            index += 1;
            continue;
        }
        let display_span = frame
            .call_span
            .or_else(|| function.map(|function| function.span));
        let file = display_span
            .and_then(|span| compiled.unit().files.get(span.file.index()))
            .map(|file| file.path.clone())
            .unwrap_or_default();
        let line = display_span
            .and_then(|span| source_span_display_line(compiled, span, false))
            .or_else(|| display_span.map(|span| i64::from(span.start)))
            .unwrap_or(0);
        lines.push(format!(
            "#{index} {file}({line}): {}",
            format_trace_call(compiled, frame)
        ));
        index += 1;
    }
    if lines.is_empty() {
        lines.push("#0 {main}".to_owned());
    }
    lines.join("\n")
}
