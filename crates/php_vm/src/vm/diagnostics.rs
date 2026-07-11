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
        php_runtime::PhpReferenceClassification::from_diagnostic_id(id),
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
) -> php_runtime::PhpDiagnosticDisplayOptions {
    php_runtime::PhpDiagnosticDisplayOptions {
        display_errors: display_errors_enabled(state),
        error_reporting: current_error_reporting(state),
        ..php_runtime::PhpDiagnosticDisplayOptions::default()
    }
}

pub(super) fn emit_vm_diagnostic(
    output: &mut OutputBuffer,
    state: &ExecutionState,
    diagnostic: &RuntimeDiagnostic,
    channel: php_runtime::PhpDiagnosticChannel,
    level: i64,
) -> bool {
    emit_vm_diagnostic_with_options(output, state, diagnostic, channel, level, true)
}

pub(super) fn emit_vm_diagnostic_with_options(
    output: &mut OutputBuffer,
    state: &ExecutionState,
    diagnostic: &RuntimeDiagnostic,
    channel: php_runtime::PhpDiagnosticChannel,
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

pub(super) fn error_level_channel(level: i64) -> php_runtime::PhpDiagnosticChannel {
    match level {
        php_std::constants::E_USER_NOTICE => php_runtime::PhpDiagnosticChannel::Notice,
        php_std::constants::E_USER_DEPRECATED => php_runtime::PhpDiagnosticChannel::Deprecated,
        php_std::constants::E_USER_ERROR => php_runtime::PhpDiagnosticChannel::FatalError,
        _ => php_runtime::PhpDiagnosticChannel::Warning,
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
