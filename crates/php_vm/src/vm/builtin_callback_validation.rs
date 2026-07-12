//! Shared callback validation and diagnostics for internal builtins.

use super::builtin_adapter::builtin_source_span;
use super::prelude::*;

pub(super) fn array_callback_type_error(
    output: &OutputBuffer,
    compiled: &CompiledUnit,
    stack: &CallStack,
    function: &str,
    actual: &str,
) -> VmResult {
    let message =
        format!("{function}(): Argument #1 ($array) must be of type array, {actual} given");
    let diagnostic = RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_BUILTIN_TYPE",
        RuntimeSeverity::FatalError,
        message.clone(),
        builtin_source_span(compiled, None),
        stack_trace(compiled, stack),
        Some(php_runtime::api::PhpReferenceClassification::TypeError),
    );
    VmResult::runtime_error_with_diagnostic(output.clone(), message, diagnostic)
}

pub(super) fn validate_array_callback_arg(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    function: &str,
    position: usize,
    param_name: &str,
    nullable: bool,
    callback: &Value,
) -> Result<(), ArrayCallbackError> {
    if nullable && matches!(effective_value(callback), Value::Null) {
        return Ok(());
    }
    if value_is_callable(compiled, state, callback, false) {
        return Ok(());
    }
    let nullable_suffix = if nullable { " or null" } else { "" };
    Err(ArrayCallbackError::BuiltinTypeMessage(format!(
        "{function}(): Argument #{position} (${param_name}) must be a valid callback{nullable_suffix}, {}",
        invalid_array_callback_reason(compiled, state, callback)
    )))
}

fn invalid_array_callback_reason(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    callback: &Value,
) -> String {
    match effective_value(callback) {
        Value::String(name) => format!(
            "function \"{}\" not found or invalid function name",
            name.to_string_lossy()
        ),
        Value::Array(array) => invalid_array_callback_array_reason(compiled, state, &array),
        _ => "no array or string given".to_owned(),
    }
}

fn invalid_array_callback_array_reason(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    array: &PhpArray,
) -> String {
    let Some(target) = array.get(&ArrayKey::Int(0)) else {
        return "array callback must have exactly two members".to_owned();
    };
    let Some(method) = array.get(&ArrayKey::Int(1)) else {
        return "array callback must have exactly two members".to_owned();
    };
    let method = match effective_value(method) {
        Value::String(method) => method,
        _ => return "second array member is not a valid method".to_owned(),
    };
    match effective_value(target) {
        Value::Object(object) => invalid_array_callback_method_reason(
            compiled,
            state,
            &object.class_name(),
            &method.to_string_lossy(),
            false,
        ),
        Value::String(class) => invalid_array_callback_method_reason(
            compiled,
            state,
            &class.to_string_lossy(),
            &method.to_string_lossy(),
            true,
        ),
        _ => "first array member is not a valid class name or object".to_owned(),
    }
}

fn invalid_array_callback_method_reason(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    method: &str,
    static_target: bool,
) -> String {
    if !class_like_exists_direct(compiled, state, class_name, AutoloadClassLookupKind::Class) {
        return format!("class \"{class_name}\" not found");
    }
    match lookup_method_in_state(compiled, state, class_name, method) {
        Ok(Some(method_entry)) if static_target && !method_entry.flags.is_static => {
            format!("non-static method {class_name}::{method}() cannot be called statically")
        }
        Ok(Some(_)) => format!("class {class_name} does not have a method \"{method}\""),
        _ => format!("class {class_name} does not have a method \"{method}\""),
    }
}
