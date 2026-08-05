//! Exact native builtin handlers over authoritative native values.
//!
//! Its explicit imports expose only fast-state capabilities and native
//! comparison/reference primitives. Publication rejects unsupported semantic
//! shapes before optimizer entry; a mismatch here is therefore an ABI
//! contract violation rather than a request to recompile.

use super::{
    NativeComparisonValue, NativeDirectStringPublishError, NativeJsonTraversal, NativeLastError,
    NativeRequestFastState, NativeScalarBytes, NativeSymbolQueryCapability,
    PreparedNativeCountThrowableSites, native_comparison_truthy, native_reference_state,
    php_constant_category, php_core_runtime_constant,
};
use php_ir::module::normalize_class_name;
use std::io::Write;
use std::sync::Arc;

fn exact_query_contract_violation() -> php_jit::JitNativeControlResult {
    php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::ABI_MISMATCH, 0, 0)
}

fn exact_query_runtime_error() -> php_jit::JitNativeControlResult {
    php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

include!("exact_call_dispatch/scalar_and_filter_families.rs");
include!("exact_call_dispatch/recursive_array_family.rs");
include!("exact_call_dispatch/array_multisort.rs");

fn native_object_vars_result(
    fast: &mut NativeRequestFastState,
    object: i64,
    mangled: bool,
) -> php_jit::JitNativeControlResult {
    let Some(entries) = super::exact_runtime_ops::native_object_vars_entries(fast, object, mangled)
    else {
        return exact_query_contract_violation();
    };
    match super::exact_runtime_ops::publish_native_array_cast_entries(fast, entries) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_get_object_vars_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    native_object_vars_result(fast, argument_0, false)
}

pub(crate) extern "C" fn jit_native_get_mangled_object_vars_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    native_object_vars_result(fast, argument_0, true)
}

fn exact_class_lineage(
    symbols: &NativeSymbolQueryCapability,
    name: &str,
) -> Option<Vec<crate::compiled_unit::CompiledClass>> {
    let mut lineage = Vec::new();
    let mut current = Some(normalize_class_name(name));
    let mut seen = std::collections::BTreeSet::new();
    while let Some(name) = current {
        if lineage.len() >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY || !seen.insert(name.clone())
        {
            return None;
        }
        let class = symbols.class_handle(&name)?;
        current = class.parent.as_deref().map(normalize_class_name);
        lineage.push(class);
    }
    Some(lineage)
}

fn exact_class_member_visible(
    symbols: &NativeSymbolQueryCapability,
    declaring_class: &str,
    is_private: bool,
    is_protected: bool,
    caller_class: Option<&str>,
) -> bool {
    if !is_private && !is_protected {
        return true;
    }
    let Some(caller_class) = caller_class else {
        return false;
    };
    if is_private {
        return normalize_class_name(caller_class) == normalize_class_name(declaring_class);
    }
    normalize_class_name(caller_class) == normalize_class_name(declaring_class)
        || symbols.class_is_a(caller_class, declaring_class) == Some(true)
}

struct NativeClassTarget {
    name: String,
    is_object: bool,
}

fn native_class_target(
    fast: &NativeRequestFastState,
    mut encoded: i64,
) -> Option<NativeClassTarget> {
    for _ in 0..16 {
        let Some((_, slot)) = fast.direct_slot(encoded) else {
            break;
        };
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            && native_reference_state(slot.reserved)
                != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
        {
            encoded = slot.payload as i64;
            continue;
        }
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT {
            return fast.direct_object(encoded).map(|object| NativeClassTarget {
                name: object.class_name(),
                is_object: true,
            });
        }
        break;
    }
    fast.native_string_view(encoded)
        .map(|name| NativeClassTarget {
            name: String::from_utf8_lossy(name).into_owned(),
            is_object: false,
        })
}

fn native_false_result() -> php_jit::JitNativeControlResult {
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(
        php_jit::JIT_VALUE_FALSE,
    ))
}

fn native_bool_result(value: bool) -> php_jit::JitNativeControlResult {
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(if value {
        php_jit::JIT_VALUE_TRUE
    } else {
        php_jit::JIT_VALUE_FALSE
    }))
}

pub(crate) extern "C" fn jit_native_get_class_methods_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    caller_function: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(target) = native_class_target(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(lineage) = exact_class_lineage(&fast.symbol_query, &target.name) else {
        return exact_query_contract_violation();
    };
    let caller_class = fast.symbol_query.caller_class(caller_function as u32);
    let mut seen = std::collections::BTreeSet::new();
    let mut methods = Vec::new();
    for class in lineage {
        for method in &class.methods {
            if exact_class_member_visible(
                &fast.symbol_query,
                &class.name,
                method.flags.is_private,
                method.flags.is_protected,
                caller_class.as_deref(),
            ) && seen.insert(method.name.to_ascii_lowercase())
            {
                methods.push(class.method_display_name(method));
            }
        }
    }
    publish_exact_string_list(fast, methods.len(), methods).map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

struct ExactClassVarProjection {
    class: crate::compiled_unit::CompiledClass,
    property: usize,
}

pub(crate) extern "C" fn jit_native_get_class_vars_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    caller_function: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(target) = native_class_target(fast, argument_0).filter(|target| !target.is_object)
    else {
        return exact_query_contract_violation();
    };
    let Some(lineage) = exact_class_lineage(&fast.symbol_query, &target.name) else {
        return exact_query_contract_violation();
    };
    let caller_class = fast.symbol_query.caller_class(caller_function as u32);
    let mut seen = std::collections::BTreeSet::new();
    let mut properties = Vec::new();
    // PHP exposes instance defaults before static defaults. Within either
    // partition the most-derived declaration wins and retains child-first
    // order.
    for is_static in [false, true] {
        for class in &lineage {
            for (property_index, property) in class.properties.iter().enumerate() {
                if property.flags.is_static == is_static
                    && exact_class_member_visible(
                        &fast.symbol_query,
                        &class.name,
                        property.flags.is_private,
                        property.flags.is_protected,
                        caller_class.as_deref(),
                    )
                    && seen.insert(property.name.clone())
                {
                    properties.push(ExactClassVarProjection {
                        class: class.clone(),
                        property: property_index,
                    });
                }
            }
        }
    }
    if properties.iter().any(|projection| {
        let property = &projection.class.properties[projection.property];
        property.default_class_constant.is_some()
            || property.default_named_constant.is_some()
            || property.default_expr.is_some()
            || property.default.is_some_and(|constant| {
                projection
                    .class
                    .constant(constant)
                    .is_none_or(|value| !exact_ir_constant_is_direct(value))
            })
    }) {
        return exact_query_contract_violation();
    }
    fast.publish_owned_direct_array_with(properties.len(), |fast, index| {
        let projection = properties
            .get(index)
            .ok_or("native class-var projection is truncated")?;
        let property = &projection.class.properties[projection.property];
        let key = fast.publish_direct_string_bytes(property.name.as_bytes())?;
        let value = if let Some(constant) = property.default {
            let Some(constant) = projection.class.constant(constant) else {
                let _ = fast.discard_owned_direct_value(key);
                return Err("native class-var default lost its owning constant table");
            };
            let Some(value) = publish_exact_ir_constant(fast, constant) else {
                let _ = fast.discard_owned_direct_value(key);
                return Err("native class-var default publication failed");
            };
            value
        } else {
            php_jit::jit_encode_constant(u32::MAX)
        };
        Ok(php_jit::JitNativeDirectArrayEntry { key, value })
    })
    .map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_is_callable_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let syntax_only = if argument_1 == missing {
        false
    } else {
        let Some(syntax_only) = exact_native_boolean_flag(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        syntax_only
    };
    let Some(callable) = fast.direct_callable_is_valid(argument_0, syntax_only) else {
        return exact_query_contract_violation();
    };
    if argument_2 != missing {
        let Some(name) = fast.direct_callable_name_bytes(argument_0) else {
            return exact_query_contract_violation();
        };
        let Ok(name) = fast.publish_direct_string_bytes(&name) else {
            return exact_query_contract_violation();
        };
        if !fast.replace_direct_reference(argument_2, name) {
            return exact_query_contract_violation();
        }
    }
    native_bool_result(callable)
}

pub(crate) extern "C" fn jit_native_set_error_handler_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    fast.exact_set_error_handler(argument_0, argument_1)
        .map_or_else(
            exact_query_contract_violation,
            php_jit::JitNativeControlResult::returning,
        )
}

pub(crate) extern "C" fn jit_native_restore_error_handler_abi(
    runtime: *mut NativeRequestFastState,
    _argument_0: i64,
    _argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    fast.exact_restore_error_handler().map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

fn exact_trigger_error_source(
    fast: &NativeRequestFastState,
    file: i64,
    start: i64,
) -> (String, usize) {
    // Safety: publication keeps the active compiled unit alive throughout the request.
    #[allow(unsafe_code)]
    let compiled = unsafe { fast.symbol_query.active_compiled.as_ref() };
    let path = compiled
        .and_then(|compiled| {
            usize::try_from(file)
                .ok()
                .and_then(|index| compiled.unit().files.get(index))
        })
        .map_or_else(|| "<unknown>".to_owned(), |entry| entry.path.clone());
    let offset = u32::try_from(start).unwrap_or(0);
    let span = php_ir::IrSpan::new(
        php_ir::FileId::new(u32::try_from(file).unwrap_or(u32::MAX)),
        offset,
        offset,
    );
    let line = compiled
        .and_then(|compiled| compiled.source_display_line(span, false))
        .and_then(|line| usize::try_from(line).ok())
        .unwrap_or(1);
    (path, line)
}

fn exact_trigger_error_action(
    fast: &mut NativeRequestFastState,
    callback: i64,
    level: i64,
    message: &str,
    path: &str,
    line: usize,
    file: i64,
    start: i64,
) -> Option<i64> {
    fast.retain_direct_encoded(callback).ok()?;
    let mut values = vec![callback];
    let mut submitted = false;
    let result = (|| {
        values.push(fast.publish_direct_int(level)?);
        values.push(fast.publish_direct_string_bytes(message.as_bytes())?);
        values.push(fast.publish_direct_string_bytes(path.as_bytes())?);
        values.push(fast.publish_direct_int(i64::try_from(line).unwrap_or(i64::MAX))?);
        values.push(fast.publish_direct_int(file)?);
        values.push(fast.publish_direct_int(start)?);
        let entries = values.iter().copied().enumerate().map(|(index, value)| {
            php_jit::JitNativeDirectArrayEntry {
                key: i64::try_from(index).unwrap_or(i64::MAX),
                value,
            }
        });
        submitted = true;
        fast.publish_owned_direct_array_from_iter(entries)
    })();
    match result {
        Ok(action) => Some(action),
        Err(_) if !submitted => {
            for value in values.into_iter().rev() {
                let _ = fast.discard_owned_direct_value(value);
            }
            None
        }
        Err(_) => None,
    }
}

pub(crate) extern "C" fn jit_native_trigger_error_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
    callback_result: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    if callback_result != missing {
        let Some(entries) = fast.native_direct_array_entries(argument_0) else {
            return exact_query_contract_violation();
        };
        if entries.len() != 7 {
            return exact_query_contract_violation();
        }
        let callback_result = fast.native_by_value_encoding(callback_result);
        if callback_result == Some(php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE)) {
            let Some(level) = exact_native_integer(fast, entries[1].value) else {
                return exact_query_contract_violation();
            };
            let Some(message) = fast.native_string_view(entries[2].value) else {
                return exact_query_contract_violation();
            };
            let message = String::from_utf8_lossy(message).into_owned();
            let (Some(file), Some(start)) = (
                exact_native_integer(fast, entries[5].value),
                exact_native_integer(fast, entries[6].value),
            ) else {
                return exact_query_contract_violation();
            };
            if super::exact_runtime_ops::emit_exact_native_diagnostic(
                fast, level, message, file, start,
            ) != 0
            {
                return exact_query_runtime_error();
            }
        }
        return exact_query_return_bool(true);
    }

    let Some(message) = exact_native_date_string(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let level = if argument_1 == missing {
        php_runtime::api::PHP_E_USER_NOTICE
    } else {
        let Some(level) = exact_native_integer(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        level
    };
    if !matches!(
        level,
        php_runtime::api::PHP_E_USER_ERROR
            | php_runtime::api::PHP_E_USER_WARNING
            | php_runtime::api::PHP_E_USER_NOTICE
            | php_runtime::api::PHP_E_USER_DEPRECATED
    ) {
        return exact_query_contract_violation();
    }
    let (path, line) = exact_trigger_error_source(fast, argument_2, argument_3);
    // Safety: the callback registry is separately boxed and request-stable.
    #[allow(unsafe_code)]
    let handler = unsafe { fast.callback_handlers.as_ref() }
        .and_then(|state| state.error_handlers.last())
        .filter(|handler| handler.levels == -1 || handler.levels & level != 0)
        .copied();
    let Some(handler) = handler else {
        return if super::exact_runtime_ops::emit_exact_native_diagnostic(
            fast, level, message, argument_2, argument_3,
        ) == 0
        {
            exact_query_return_bool(true)
        } else {
            exact_query_runtime_error()
        };
    };
    #[allow(unsafe_code)]
    if let Some(last_error) = unsafe { fast.last_error.as_mut() } {
        *last_error = Some(NativeLastError {
            error_type: level,
            message: message.clone(),
            file: path.clone(),
            line,
        });
    }
    let Some(action) = exact_trigger_error_action(
        fast,
        handler.callback,
        level,
        &message,
        &path,
        line,
        argument_2,
        argument_3,
    ) else {
        return exact_query_runtime_error();
    };
    php_jit::JitNativeControlResult::control(
        php_jit::JitCallStatus::INVOKE_USER_CALLBACK,
        0,
        action,
    )
}

pub(crate) extern "C" fn jit_native_set_exception_handler_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    _argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    fast.exact_set_exception_handler(argument_0).map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_restore_exception_handler_abi(
    runtime: *mut NativeRequestFastState,
    _argument_0: i64,
    _argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    fast.exact_restore_exception_handler().map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_get_exception_handler_abi(
    runtime: *mut NativeRequestFastState,
    _argument_0: i64,
    _argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    fast.exact_get_exception_handler().map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_spl_autoload_register_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    if argument_1 != missing && exact_native_boolean_flag(fast, argument_1).is_none() {
        return exact_query_contract_violation();
    }
    let prepend = if argument_2 == missing {
        false
    } else {
        let Some(prepend) = exact_native_boolean_flag(fast, argument_2) else {
            return exact_query_contract_violation();
        };
        prepend
    };
    fast.exact_register_autoload_callback(argument_0, prepend)
        .map_or_else(
            exact_query_contract_violation,
            php_jit::JitNativeControlResult::returning,
        )
}

pub(crate) extern "C" fn jit_native_spl_autoload_unregister_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    _argument_1: i64,
    _argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    fast.exact_unregister_autoload_callback(argument_0)
        .map_or_else(
            exact_query_contract_violation,
            php_jit::JitNativeControlResult::returning,
        )
}

pub(crate) extern "C" fn jit_native_spl_autoload_functions_abi(
    runtime: *mut NativeRequestFastState,
    _argument_0: i64,
    _argument_1: i64,
    _argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    fast.exact_autoload_functions().map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_register_shutdown_function_abi(
    runtime: *mut NativeRequestFastState,
    argument_count: i32,
    arguments: *const i64,
    function: u32,
    continuation: u32,
) -> php_jit::JitNativeControlResult {
    let Ok(argument_count) = usize::try_from(argument_count) else {
        return exact_query_contract_violation();
    };
    if argument_count == 0 || arguments.is_null() {
        return exact_query_contract_violation();
    }
    #[allow(unsafe_code)]
    let arguments = unsafe { std::slice::from_raw_parts(arguments, argument_count) };
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    fast.exact_register_shutdown_callback(arguments, function, continuation)
        .map_or_else(
            exact_query_contract_violation,
            php_jit::JitNativeControlResult::returning,
        )
}

pub(crate) extern "C" fn jit_native_get_parent_class_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(target) = native_class_target(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(class) = fast.symbol_query.class_handle(&target.name) else {
        return native_false_result();
    };
    let Some(parent) = class.parent.as_deref() else {
        return native_false_result();
    };
    let display = fast
        .symbol_query
        .class_handle(parent)
        .map_or_else(|| parent.to_owned(), |class| class.display_name.clone());
    match fast.publish_direct_string_bytes(display.as_bytes()) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => exact_query_contract_violation(),
    }
}

fn native_class_is_subclass_of(
    symbols: &NativeSymbolQueryCapability,
    class_name: &str,
    expected: &str,
) -> bool {
    let Some(class) = symbols.class_handle(class_name) else {
        return false;
    };
    class
        .parent
        .iter()
        .chain(class.interfaces.iter())
        .any(|candidate| symbols.class_is_a(candidate, expected) == Some(true))
}

fn native_class_is_a(
    symbols: &NativeSymbolQueryCapability,
    class_name: &str,
    expected: &str,
) -> bool {
    symbols.class_is_a(class_name, expected).unwrap_or(false)
}

pub(crate) extern "C" fn jit_native_is_subclass_of_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(target) = native_class_target(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(parent) = native_class_target(fast, argument_1).filter(|target| !target.is_object)
    else {
        return exact_query_contract_violation();
    };
    let allow_string =
        if argument_2 == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
            true
        } else {
            let Some(value) = fast
                .native_comparison_value(argument_2)
                .map(native_comparison_truthy)
            else {
                return exact_query_contract_violation();
            };
            value
        };
    if !target.is_object && !allow_string {
        return native_false_result();
    }
    native_bool_result(native_class_is_subclass_of(
        &fast.symbol_query,
        &target.name,
        &parent.name,
    ))
}

pub(crate) extern "C" fn jit_native_is_a_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(target) = native_class_target(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(expected) = native_class_target(fast, argument_1).filter(|target| !target.is_object)
    else {
        return exact_query_contract_violation();
    };
    let allow_string =
        if argument_2 == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
            false
        } else {
            let Some(value) = fast
                .native_comparison_value(argument_2)
                .map(native_comparison_truthy)
            else {
                return exact_query_contract_violation();
            };
            value
        };
    if !target.is_object && !allow_string {
        return native_false_result();
    }
    native_bool_result(native_class_is_a(
        &fast.symbol_query,
        &target.name,
        &expected.name,
    ))
}

fn exact_visit_interface_ancestry(
    symbols: &NativeSymbolQueryCapability,
    interface_name: &str,
    depth: usize,
    visit: &mut impl FnMut(&[u8]) -> Option<()>,
) -> Option<()> {
    if depth >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
        return None;
    }
    if let Some(interface) = symbols.class_handle(interface_name) {
        visit(interface.display_name.as_bytes())?;
        for parent in &interface.interfaces {
            exact_visit_interface_ancestry(symbols, parent, depth + 1, visit)?;
        }
        Some(())
    } else {
        visit(interface_name.as_bytes())
    }
}

fn exact_visit_class_interfaces(
    symbols: &NativeSymbolQueryCapability,
    class_name: &str,
    depth: usize,
    visit: &mut impl FnMut(&[u8]) -> Option<()>,
) -> Option<()> {
    if depth >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
        return None;
    }
    let class = symbols.class_handle(class_name)?;
    for interface in &class.interfaces {
        exact_visit_interface_ancestry(symbols, interface, depth + 1, visit)?;
    }
    if let Some(parent) = class.parent.as_deref() {
        exact_visit_class_interfaces(symbols, parent, depth + 1, visit)?;
    }
    Some(())
}

#[allow(unsafe_code)] // Immutable class metadata stays live while the disjoint native result arena mutates.
pub(crate) extern "C" fn jit_native_class_implements_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(target) = native_class_target(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let autoload =
        if argument_1 == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
            true
        } else {
            let Some(value) = fast
                .native_comparison_value(argument_1)
                .map(native_comparison_truthy)
            else {
                return exact_query_contract_violation();
            };
            value
        };
    if fast.symbol_query.class_handle(&target.name).is_none() {
        return if !target.is_object && autoload {
            exact_query_contract_violation()
        } else {
            native_false_result()
        };
    }
    let Some(mut writer) = fast
        .begin_owned_direct_array(4, php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY)
        .ok()
    else {
        return exact_query_contract_violation();
    };
    let fast_ptr = fast as *mut NativeRequestFastState;
    let symbols = &fast.symbol_query;
    let published = {
        let mut publish = |name: &[u8]| {
            let fast = unsafe { &mut *fast_ptr };
            for index in 0..writer.len() {
                let entry = writer.get(index)?;
                let (existing, length) = fast.stable_native_string_range(entry.key)?;
                let existing = unsafe { std::slice::from_raw_parts(existing, length) };
                if existing == name {
                    return Some(());
                }
            }
            let value = fast.publish_direct_string_bytes(name).ok()?;
            if fast.retain_direct_encoded(value).is_err() {
                let _ = fast.discard_owned_direct_value(value);
                return None;
            }
            if fast
                .push_owned_direct_array_entry(
                    &mut writer,
                    php_jit::JitNativeDirectArrayEntry { key: value, value },
                )
                .is_err()
            {
                let _ = fast.discard_owned_direct_value(value);
                let _ = fast.discard_owned_direct_value(value);
                return None;
            }
            Some(())
        };
        exact_visit_class_interfaces(symbols, &target.name, 0, &mut publish).is_some()
    };
    if !published {
        unsafe { &mut *fast_ptr }.abort_owned_direct_array(writer);
        return exact_query_contract_violation();
    }
    unsafe { &mut *fast_ptr }
        .finish_owned_direct_array(writer)
        .map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        )
}

pub(crate) extern "C" fn jit_native_extension_loaded_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &*runtime };
    let Some(name) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let name = String::from_utf8_lossy(name);
    native_bool_result(php_std::introspection::extension_loaded(
        php_std::ExtensionRegistry::standard_library(),
        name.as_ref(),
    ))
}

fn native_memory_usage_result(
    fast: &mut NativeRequestFastState,
    real_usage: i64,
) -> php_jit::JitNativeControlResult {
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    if real_usage != missing && exact_native_boolean_flag(fast, real_usage).is_none() {
        return exact_query_contract_violation();
    }
    let bytes = fast
        .native_output_buffer()
        .map_or(0_i64, |output| output.len().try_into().unwrap_or(i64::MAX));
    fast.publish_direct_int(bytes.max(0)).map_or_else(
        |_| exact_query_runtime_error(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_memory_get_usage_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    native_memory_usage_result(fast, argument_0)
}

pub(crate) extern "C" fn jit_native_memory_get_peak_usage_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // The current runtime memory contract exposes the same authoritative
    // request-owned byte count for current and peak observations.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    native_memory_usage_result(fast, argument_0)
}

fn native_gc_state_mut(
    fast: &mut NativeRequestFastState,
) -> Option<&mut php_runtime::api::GcRequestState> {
    // SAFETY: request construction publishes the sole request-owned GC state
    // for the full synchronous lifetime of this FastState.
    #[allow(unsafe_code)]
    unsafe {
        fast.gc_state.as_mut()
    }
}

pub(crate) extern "C" fn jit_native_gc_collect_cycles_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    fast.publish_direct_int(0).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_gc_disable_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(state) = native_gc_state_mut(fast) else {
        return exact_query_contract_violation();
    };
    state.set_enabled(false);
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX))
}

pub(crate) extern "C" fn jit_native_gc_enable_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(state) = native_gc_state_mut(fast) else {
        return exact_query_contract_violation();
    };
    state.set_enabled(true);
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX))
}

pub(crate) extern "C" fn jit_native_gc_enabled_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(state) = native_gc_state_mut(fast) else {
        return exact_query_contract_violation();
    };
    native_bool_result(state.enabled())
}

pub(crate) extern "C" fn jit_native_gc_mem_caches_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    fast.publish_direct_int(0).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

enum NativeGcStatusValue {
    Bool(bool),
    Int(i64),
    Float(f64),
}

const NATIVE_GC_STATUS: [(&[u8], NativeGcStatusValue); 12] = [
    (b"running", NativeGcStatusValue::Bool(false)),
    (b"protected", NativeGcStatusValue::Bool(false)),
    (b"full", NativeGcStatusValue::Bool(false)),
    (b"runs", NativeGcStatusValue::Int(0)),
    (b"collected", NativeGcStatusValue::Int(0)),
    (b"threshold", NativeGcStatusValue::Int(10_001)),
    (b"buffer_size", NativeGcStatusValue::Int(16_384)),
    (b"roots", NativeGcStatusValue::Int(0)),
    (b"application_time", NativeGcStatusValue::Float(0.0)),
    (b"collector_time", NativeGcStatusValue::Float(0.0)),
    (b"destructor_time", NativeGcStatusValue::Float(0.0)),
    (b"free_time", NativeGcStatusValue::Float(0.0)),
];

pub(crate) extern "C" fn jit_native_gc_status_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    fast.publish_owned_direct_array_with(NATIVE_GC_STATUS.len(), |fast, index| {
        let (name, value) = &NATIVE_GC_STATUS[index];
        let key = fast.publish_direct_string_bytes(name)?;
        let value = match value {
            NativeGcStatusValue::Bool(value) => php_jit::jit_encode_constant(if *value {
                php_jit::JIT_VALUE_TRUE
            } else {
                php_jit::JIT_VALUE_FALSE
            }),
            NativeGcStatusValue::Int(value) => match fast.publish_direct_int(*value) {
                Ok(value) => value,
                Err(error) => {
                    let _ = fast.discard_owned_direct_value(key);
                    return Err(error);
                }
            },
            NativeGcStatusValue::Float(value) => match fast.publish_direct_float(*value) {
                Ok(value) => value,
                Err(error) => {
                    let _ = fast.discard_owned_direct_value(key);
                    return Err(error);
                }
            },
        };
        Ok(php_jit::JitNativeDirectArrayEntry { key, value })
    })
    .map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_get_resource_id_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Ok(id) = i64::try_from(resource.id().get()) else {
        return exact_query_contract_violation();
    };
    fast.publish_direct_int(id).map_or_else(
        |_| exact_query_runtime_error(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_get_resource_type_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let resource_type = resource.resource_type();
    fast.publish_direct_string_bytes(resource_type.as_bytes())
        .map_or_else(
            |_| exact_query_runtime_error(),
            php_jit::JitNativeControlResult::returning,
        )
}

pub(crate) extern "C" fn jit_native_get_resources_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let requested_type = if argument_0 == missing
        || matches!(
            fast.native_comparison_value(argument_0),
            Some(NativeComparisonValue::Null)
        ) {
        None
    } else {
        let Some(value) = exact_native_scalar_string(fast, argument_0) else {
            return exact_query_contract_violation();
        };
        Some(value.into_lossy_owned())
    };
    // SAFETY: request construction publishes the sole ResourceTable owner for
    // this synchronous exact call.
    #[allow(unsafe_code)]
    let resources = unsafe { fast.resources.as_ref() }
        .map(php_runtime::api::ResourceTable::resources)
        .unwrap_or_default();
    if let Some(resource_type) = requested_type.as_deref() {
        let has_matching_resource = resources
            .iter()
            .any(|resource| resource.resource_type() == resource_type);
        let can_be_empty = matches!(resource_type, "stream" | "stream-context" | "Unknown");
        if !has_matching_resource && !can_be_empty {
            return exact_query_contract_violation();
        }
    }
    let selected = resources
        .into_iter()
        .filter(|resource| {
            requested_type
                .as_deref()
                .is_none_or(|resource_type| resource.resource_type() == resource_type)
        })
        .collect::<Vec<_>>();
    let mut selected = selected.into_iter();
    fast.publish_owned_direct_array_with(selected.len(), |fast, _| {
        let resource = selected
            .next()
            .ok_or("native resource inventory is truncated")?;
        let id = i64::try_from(resource.id().get())
            .map_err(|_| "native resource identity exceeds PHP integer range")?;
        let key = fast.publish_direct_int(id)?;
        let value = match fast.publish_direct_resource(resource) {
            Ok(value) => value,
            Err(error) => {
                let _ = fast.discard_owned_direct_value(key);
                return Err(error);
            }
        };
        Ok(php_jit::JitNativeDirectArrayEntry { key, value })
    })
    .map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_settype_abi(
    runtime: *mut NativeRequestFastState,
    reference: i64,
    type_name: i64,
) -> php_jit::JitNativeControlResult {
    let (current, type_name) = {
        #[allow(unsafe_code)]
        let fast = unsafe { &mut *runtime };
        let Some((_, slot)) = fast.direct_slot(reference) else {
            return exact_query_contract_violation();
        };
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            || slot.flags != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            || native_reference_state(slot.reserved)
                == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
            || slot.reserved & php_jit::JIT_NATIVE_REFERENCE_TYPED_PROPERTY_GUARD != 0
        {
            return exact_query_contract_violation();
        }
        let Some(type_name) = exact_native_scalar_string(fast, type_name) else {
            return exact_query_contract_violation();
        };
        (
            slot.payload as i64,
            type_name.into_lossy_owned().to_ascii_lowercase(),
        )
    };

    let cast = match type_name.as_str() {
        "bool" | "boolean" => {
            #[allow(unsafe_code)]
            let fast = unsafe { &mut *runtime };
            let Some(value) = fast
                .native_comparison_value(current)
                .map(native_comparison_truthy)
            else {
                return exact_query_contract_violation();
            };
            php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(if value {
                php_jit::JIT_VALUE_TRUE
            } else {
                php_jit::JIT_VALUE_FALSE
            }))
        }
        "int" | "integer" => super::exact_runtime_ops::jit_native_int_cast_abi(runtime, current),
        "float" | "double" => super::exact_runtime_ops::jit_native_float_cast_abi(runtime, current),
        "string" => super::exact_runtime_ops::jit_native_string_cast_abi(runtime, current),
        "array" => super::exact_runtime_ops::jit_native_array_cast_abi(runtime, current),
        "object" => super::exact_runtime_ops::jit_native_object_cast_abi(runtime, current),
        "null" => {
            php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX))
        }
        _ => return exact_query_contract_violation(),
    };
    if cast.status != php_jit::JitCallStatus::RETURN {
        return cast;
    }
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if !fast.replace_direct_reference(reference, cast.value) {
        return exact_query_contract_violation();
    }
    native_bool_result(true)
}

pub(crate) extern "C" fn jit_native_error_get_last_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    #[allow(unsafe_code)]
    let last_error = unsafe { (&*fast.last_error).as_ref() };
    let Some(last_error) = last_error else {
        return php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX));
    };
    let mut index = 0_usize;
    fast.publish_owned_direct_array_with(4, |fast, _| {
        let field = index;
        index += 1;
        let name = match field {
            0 => b"type".as_slice(),
            1 => b"message".as_slice(),
            2 => b"file".as_slice(),
            3 => b"line".as_slice(),
            _ => return Err("native last-error projection exceeded its fixed field count"),
        };
        let key = fast.publish_direct_string_bytes(name)?;
        let value = match field {
            0 => fast.publish_direct_int(last_error.error_type),
            1 => fast.publish_direct_string_bytes(last_error.message.as_bytes()),
            2 => fast.publish_direct_string_bytes(last_error.file.as_bytes()),
            3 => fast.publish_direct_int(i64::try_from(last_error.line).unwrap_or(i64::MAX)),
            _ => unreachable!("fixed last-error field was validated above"),
        };
        let value = match value {
            Ok(value) => value,
            Err(error) => {
                let _ = fast.discard_owned_direct_value(key);
                return Err(error);
            }
        };
        Ok(php_jit::JitNativeDirectArrayEntry { key, value })
    })
    .map_or_else(
        |_| php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_error_clear_last_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    #[allow(unsafe_code)]
    unsafe {
        *fast.last_error = None;
    }
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX))
}

impl php_runtime::api::NativeStructuredValuePublisher for NativeRequestFastState {
    type Output = i64;

    fn publish_null(&mut self) -> Option<Self::Output> {
        Some(php_jit::jit_encode_constant(u32::MAX))
    }

    fn publish_bool(&mut self, value: bool) -> Option<Self::Output> {
        Some(php_jit::jit_encode_constant(if value {
            php_jit::JIT_VALUE_TRUE
        } else {
            php_jit::JIT_VALUE_FALSE
        }))
    }

    fn publish_int(&mut self, value: i64) -> Option<Self::Output> {
        self.publish_direct_int(value).ok()
    }

    fn publish_float(&mut self, value: f64) -> Option<Self::Output> {
        self.publish_direct_float(value).ok()
    }

    fn publish_string(&mut self, value: &[u8]) -> Option<Self::Output> {
        self.publish_direct_string_bytes(value).ok()
    }

    fn rollback(&mut self, value: Self::Output) {
        let _ = self.discard_owned_direct_value(value);
    }

    fn publish_array_stream<E>(
        &mut self,
        build: impl FnOnce(
            &mut Self,
            &mut dyn FnMut(&mut Self, Self::Output) -> Option<()>,
        ) -> Result<(), E>,
    ) -> Result<Option<Self::Output>, E> {
        let Some(mut writer) = self
            .begin_owned_direct_array(4, php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY)
            .ok()
        else {
            return Ok(None);
        };
        let built = {
            let mut push = |fast: &mut Self, value: Self::Output| {
                let Some(key) = i64::try_from(writer.len()).ok() else {
                    let _ = fast.discard_owned_direct_value(value);
                    return None;
                };
                let entry = php_jit::JitNativeDirectArrayEntry { key, value };
                if fast
                    .push_owned_direct_array_entry(&mut writer, entry)
                    .is_err()
                {
                    let _ = fast.discard_owned_direct_value(value);
                    return None;
                }
                Some(())
            };
            build(self, &mut push)
        };
        if let Err(error) = built {
            self.abort_owned_direct_array(writer);
            return Err(error);
        }
        Ok(self.finish_owned_direct_array(writer).ok())
    }

    fn publish_object_stream<E>(
        &mut self,
        build: impl FnOnce(
            &mut Self,
            &mut dyn FnMut(&mut Self, &[u8], Self::Output) -> Option<()>,
        ) -> Result<(), E>,
    ) -> Result<Option<Self::Output>, E> {
        let Some(mut writer) = self
            .begin_owned_direct_array(4, php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY)
            .ok()
        else {
            return Ok(None);
        };
        let built = {
            let mut push = |fast: &mut Self, key: &[u8], value: Self::Output| {
                for index in 0..writer.len() {
                    let Some(entry) = writer.get(index) else {
                        let _ = fast.discard_owned_direct_value(value);
                        return None;
                    };
                    let (existing, existing_length) =
                        match fast.stable_native_string_range(entry.key) {
                            Some(range) => range,
                            None => {
                                let _ = fast.discard_owned_direct_value(value);
                                return None;
                            }
                        };
                    // SAFETY: the unpublished key remains owned by `writer`
                    // for this synchronous comparison.
                    #[allow(unsafe_code)]
                    let existing = unsafe { std::slice::from_raw_parts(existing, existing_length) };
                    if existing == key {
                        let Some(previous) = writer.replace_owned(
                            index,
                            php_jit::JitNativeDirectArrayEntry {
                                key: entry.key,
                                value,
                            },
                        ) else {
                            let _ = fast.discard_owned_direct_value(value);
                            return None;
                        };
                        let _ = fast.discard_owned_direct_value(previous.value);
                        return Some(());
                    }
                }
                let key = match fast.publish_direct_string_bytes(key) {
                    Ok(key) => key,
                    Err(_) => {
                        let _ = fast.discard_owned_direct_value(value);
                        return None;
                    }
                };
                let entry = php_jit::JitNativeDirectArrayEntry { key, value };
                if fast
                    .push_owned_direct_array_entry(&mut writer, entry)
                    .is_err()
                {
                    let _ = fast.discard_owned_direct_value(value);
                    let _ = fast.discard_owned_direct_value(key);
                    return None;
                }
                Some(())
            };
            build(self, &mut push)
        };
        if let Err(error) = built {
            self.abort_owned_direct_array(writer);
            return Err(error);
        }
        Ok(self.finish_owned_direct_array(writer).ok())
    }

    fn publish_array_with(
        &mut self,
        length: usize,
        mut build: impl FnMut(&mut Self, usize) -> Option<Self::Output>,
    ) -> Option<Self::Output> {
        self.publish_owned_direct_array_with(length, |fast, index| {
            let value = build(fast, index).ok_or("native structured array value failed")?;
            Ok(php_jit::JitNativeDirectArrayEntry {
                key: i64::try_from(index).unwrap_or(i64::MAX),
                value,
            })
        })
        .ok()
    }
}

struct NativeJsonDecodePublisher<'a> {
    fast: &'a mut NativeRequestFastState,
    associative: bool,
}

impl php_runtime::api::NativeStructuredValuePublisher for NativeJsonDecodePublisher<'_> {
    type Output = i64;

    fn publish_null(&mut self) -> Option<Self::Output> {
        php_runtime::api::NativeStructuredValuePublisher::publish_null(self.fast)
    }

    fn publish_bool(&mut self, value: bool) -> Option<Self::Output> {
        php_runtime::api::NativeStructuredValuePublisher::publish_bool(self.fast, value)
    }

    fn publish_int(&mut self, value: i64) -> Option<Self::Output> {
        php_runtime::api::NativeStructuredValuePublisher::publish_int(self.fast, value)
    }

    fn publish_float(&mut self, value: f64) -> Option<Self::Output> {
        php_runtime::api::NativeStructuredValuePublisher::publish_float(self.fast, value)
    }

    fn publish_string(&mut self, value: &[u8]) -> Option<Self::Output> {
        php_runtime::api::NativeStructuredValuePublisher::publish_string(self.fast, value)
    }

    fn rollback(&mut self, value: Self::Output) {
        php_runtime::api::NativeStructuredValuePublisher::rollback(self.fast, value);
    }

    fn publish_array_stream<E>(
        &mut self,
        build: impl FnOnce(
            &mut Self,
            &mut dyn FnMut(&mut Self, Self::Output) -> Option<()>,
        ) -> Result<(), E>,
    ) -> Result<Option<Self::Output>, E> {
        let Some(mut writer) = self
            .fast
            .begin_owned_direct_array(4, php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY)
            .ok()
        else {
            return Ok(None);
        };
        let built = {
            let mut push = |publisher: &mut Self, value: Self::Output| {
                let Some(key) = i64::try_from(writer.len()).ok() else {
                    let _ = publisher.fast.discard_owned_direct_value(value);
                    return None;
                };
                if publisher
                    .fast
                    .push_owned_direct_array_entry(
                        &mut writer,
                        php_jit::JitNativeDirectArrayEntry { key, value },
                    )
                    .is_err()
                {
                    let _ = publisher.fast.discard_owned_direct_value(value);
                    return None;
                }
                Some(())
            };
            build(self, &mut push)
        };
        if let Err(error) = built {
            self.fast.abort_owned_direct_array(writer);
            return Err(error);
        }
        Ok(self.fast.finish_owned_direct_array(writer).ok())
    }

    fn publish_object_stream<E>(
        &mut self,
        build: impl FnOnce(
            &mut Self,
            &mut dyn FnMut(&mut Self, &[u8], Self::Output) -> Option<()>,
        ) -> Result<(), E>,
    ) -> Result<Option<Self::Output>, E> {
        if self.associative {
            let Some(mut writer) = self
                .fast
                .begin_owned_direct_array(4, php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY)
                .ok()
            else {
                return Ok(None);
            };
            let built = {
                let mut push = |publisher: &mut Self, key: &[u8], value: Self::Output| {
                    for index in 0..writer.len() {
                        let Some(entry) = writer.get(index) else {
                            let _ = publisher.fast.discard_owned_direct_value(value);
                            return None;
                        };
                        let Some(existing) = publisher.fast.native_string_view(entry.key) else {
                            let _ = publisher.fast.discard_owned_direct_value(value);
                            return None;
                        };
                        if existing == key {
                            let Some(previous) = writer.replace_owned(
                                index,
                                php_jit::JitNativeDirectArrayEntry {
                                    key: entry.key,
                                    value,
                                },
                            ) else {
                                let _ = publisher.fast.discard_owned_direct_value(value);
                                return None;
                            };
                            let _ = publisher.fast.discard_owned_direct_value(previous.value);
                            return Some(());
                        }
                    }
                    let key = match publisher.fast.publish_direct_string_bytes(key) {
                        Ok(key) => key,
                        Err(_) => {
                            let _ = publisher.fast.discard_owned_direct_value(value);
                            return None;
                        }
                    };
                    if publisher
                        .fast
                        .push_owned_direct_array_entry(
                            &mut writer,
                            php_jit::JitNativeDirectArrayEntry { key, value },
                        )
                        .is_err()
                    {
                        let _ = publisher.fast.discard_owned_direct_value(value);
                        let _ = publisher.fast.discard_owned_direct_value(key);
                        return None;
                    }
                    Some(())
                };
                build(self, &mut push)
            };
            if let Err(error) = built {
                self.fast.abort_owned_direct_array(writer);
                return Err(error);
            }
            return Ok(self.fast.finish_owned_direct_array(writer).ok());
        }

        // Allocate the outer stdClass before descending into member values so
        // object identities follow PHP's parent-before-child allocation order.
        let object = super::exact_runtime_ops::native_object_cast_stdclass();
        let layout_id = object.class_layout_epoch();
        let mut properties = Vec::<(Vec<u8>, i64)>::new();
        let built = {
            let mut push = |publisher: &mut Self, key: &[u8], value: Self::Output| {
                let name = String::from_utf8_lossy(key).into_owned();
                let slot = php_runtime::api::NativeDeclaredPropertySlot {
                    initialized: 1,
                    reserved: 0,
                    value,
                };
                let previous = match object.set_native_dynamic_property(layout_id, name, slot) {
                    Ok(previous) => previous.map(|previous| previous.value),
                    Err(_) => {
                        let _ = publisher.fast.discard_owned_direct_value(value);
                        return None;
                    }
                };
                if let Some((_, tracked)) = properties
                    .iter_mut()
                    .find(|(existing, _)| existing.as_slice() == key)
                {
                    *tracked = value;
                } else {
                    properties.push((key.to_vec(), value));
                }
                if let Some(previous) = previous {
                    let _ = publisher.fast.discard_owned_direct_value(previous);
                }
                Some(())
            };
            build(self, &mut push)
        };
        if let Err(error) = built {
            for (_, value) in properties.into_iter().rev() {
                let _ = self.fast.discard_owned_direct_value(value);
            }
            return Err(error);
        }

        match self.fast.publish_direct_object(object) {
            Ok(object) => Ok(Some(object)),
            Err(_) => {
                for (_, value) in properties.into_iter().rev() {
                    let _ = self.fast.discard_owned_direct_value(value);
                }
                Ok(None)
            }
        }
    }

    fn publish_array_with(
        &mut self,
        length: usize,
        mut build: impl FnMut(&mut Self, usize) -> Option<Self::Output>,
    ) -> Option<Self::Output> {
        let mut writer = self
            .fast
            .begin_owned_direct_array(length, length.max(1))
            .ok()?;
        for index in 0..length {
            let Some(value) = build(self, index) else {
                self.fast.abort_owned_direct_array(writer);
                return None;
            };
            let key = i64::try_from(index).ok()?;
            if self
                .fast
                .push_owned_direct_array_entry(
                    &mut writer,
                    php_jit::JitNativeDirectArrayEntry { key, value },
                )
                .is_err()
            {
                let _ = self.fast.discard_owned_direct_value(value);
                self.fast.abort_owned_direct_array(writer);
                return None;
            }
        }
        self.fast.finish_owned_direct_array(writer).ok()
    }
}

pub(super) fn decode_native_json_direct(
    fast: &mut NativeRequestFastState,
    input: i64,
    depth: i64,
    associative: bool,
    flags: i64,
) -> Option<Result<Option<i64>, php_runtime::api::BuiltinError>> {
    let state = fast.json_state;
    let (input, input_length) = fast.stable_native_string_range(input)?;
    // SAFETY: the encoded input remains owned throughout synchronous decode.
    #[allow(unsafe_code)]
    let input = unsafe { std::slice::from_raw_parts(input, input_length) };
    // SAFETY: the request publishes its JSON state for the full activation.
    #[allow(unsafe_code)]
    let state = unsafe { state.as_mut() }?;
    let mut publisher = NativeJsonDecodePublisher { fast, associative };
    Some(php_runtime::api::decode_native_json_into(
        state,
        input,
        depth,
        flags,
        &mut publisher,
    ))
}

impl php_runtime::api::NativePregCapturePublisher for NativeRequestFastState {
    fn publish_preg_capture_row<'a, E>(
        &mut self,
        length: usize,
        mut build: impl FnMut(&mut Self, usize) -> Result<(Option<&'a [u8]>, Self::Output), E>,
    ) -> Result<Option<Self::Output>, E> {
        let maximum_length = length.saturating_mul(2);
        let Some(mut writer) = self
            .begin_owned_direct_array(length.min(4), maximum_length)
            .ok()
        else {
            return Ok(None);
        };
        for index in 0..length {
            let (name, value) = match build(self, index) {
                Ok(built) => built,
                Err(error) => {
                    self.abort_owned_direct_array(writer);
                    return Err(error);
                }
            };
            if let Some(name) = name {
                let mut replaced = false;
                for named_index in 0..writer.len() {
                    let Some(entry) = writer.get(named_index) else {
                        let _ = self.discard_owned_direct_value(value);
                        self.abort_owned_direct_array(writer);
                        return Ok(None);
                    };
                    let Some((existing, existing_length)) =
                        self.stable_native_string_range(entry.key)
                    else {
                        continue;
                    };
                    // SAFETY: the unpublished key remains owned by `writer`
                    // throughout this synchronous comparison.
                    #[allow(unsafe_code)]
                    let existing = unsafe { std::slice::from_raw_parts(existing, existing_length) };
                    if existing != name {
                        continue;
                    }
                    if self.retain_direct_encoded(value).is_err() {
                        let _ = self.discard_owned_direct_value(value);
                        self.abort_owned_direct_array(writer);
                        return Ok(None);
                    }
                    let Some(previous) = writer.replace_owned(
                        named_index,
                        php_jit::JitNativeDirectArrayEntry {
                            key: entry.key,
                            value,
                        },
                    ) else {
                        let _ = self.discard_owned_direct_value(value);
                        let _ = self.discard_owned_direct_value(value);
                        self.abort_owned_direct_array(writer);
                        return Ok(None);
                    };
                    let _ = self.discard_owned_direct_value(previous.value);
                    replaced = true;
                    break;
                }
                if !replaced {
                    let name = match self.publish_direct_string_bytes(name) {
                        Ok(name) => name,
                        Err(_) => {
                            let _ = self.discard_owned_direct_value(value);
                            self.abort_owned_direct_array(writer);
                            return Ok(None);
                        }
                    };
                    if self.retain_direct_encoded(value).is_err() {
                        let _ = self.discard_owned_direct_value(name);
                        let _ = self.discard_owned_direct_value(value);
                        self.abort_owned_direct_array(writer);
                        return Ok(None);
                    }
                    if self
                        .push_owned_direct_array_entry(
                            &mut writer,
                            php_jit::JitNativeDirectArrayEntry { key: name, value },
                        )
                        .is_err()
                    {
                        let _ = self.discard_owned_direct_value(name);
                        let _ = self.discard_owned_direct_value(value);
                        let _ = self.discard_owned_direct_value(value);
                        self.abort_owned_direct_array(writer);
                        return Ok(None);
                    }
                }
            }
            if self
                .push_owned_direct_array_entry(
                    &mut writer,
                    php_jit::JitNativeDirectArrayEntry {
                        key: i64::try_from(index).unwrap_or(i64::MAX),
                        value,
                    },
                )
                .is_err()
            {
                let _ = self.discard_owned_direct_value(value);
                self.abort_owned_direct_array(writer);
                return Ok(None);
            }
        }
        Ok(self.finish_owned_direct_array(writer).ok())
    }

    fn publish_preg_capture_columns<E>(
        &mut self,
        groups: usize,
        build: impl FnOnce(
            &mut Self,
            &mut dyn FnMut(&mut Self, usize, Self::Output) -> Option<()>,
        ) -> Result<(), E>,
    ) -> Result<Option<Self::Output>, E> {
        let columns = match self.publish_owned_direct_array_with(groups, |fast, index| {
            let value = fast.publish_empty_owned_direct_array()?;
            Ok(php_jit::JitNativeDirectArrayEntry {
                key: i64::try_from(index).unwrap_or(i64::MAX),
                value,
            })
        }) {
            Ok(columns) => columns,
            Err(_) => return Ok(None),
        };
        let Some((column_entries, column_count)) = self.stable_native_array_range(columns) else {
            let _ = self.discard_owned_direct_value(columns);
            return Ok(None);
        };
        if column_count != groups {
            let _ = self.discard_owned_direct_value(columns);
            return Ok(None);
        }
        let mut push = |fast: &mut Self, group: usize, value: Self::Output| {
            if group >= column_count {
                let _ = fast.discard_owned_direct_value(value);
                return None;
            }
            // SAFETY: the published outer array owns this stable entry range
            // for the complete synchronous build. Only its uniquely owned
            // child arrays mutate, never the outer entry storage.
            #[allow(unsafe_code)]
            let column = unsafe { (*column_entries.add(group)).value };
            let appended = fast.mutate_owned_direct_array(column, |fast, writer| {
                let key = i64::try_from(writer.len()).map_err(|_| "native PCRE column overflow")?;
                fast.push_owned_direct_array_entry(
                    writer,
                    php_jit::JitNativeDirectArrayEntry { key, value },
                )
            });
            if appended.is_err() {
                let _ = fast.discard_owned_direct_value(value);
                return None;
            }
            Some(())
        };
        if let Err(error) = build(self, &mut push) {
            let _ = self.discard_owned_direct_value(columns);
            return Err(error);
        }
        Ok(Some(columns))
    }
}

fn publish_exact_string_list<I, B>(
    fast: &mut NativeRequestFastState,
    length: usize,
    values: I,
) -> Option<i64>
where
    I: IntoIterator<Item = B>,
    B: AsRef<[u8]>,
{
    let mut values = values.into_iter();
    fast.publish_owned_direct_array_with(length, |fast, index| {
        let bytes = values.next().ok_or("native string list truncated")?;
        let value = fast
            .publish_direct_string_bytes(bytes.as_ref())
            .map_err(|_| "native string list value publication failed")?;
        Ok(php_jit::JitNativeDirectArrayEntry {
            key: i64::try_from(index).unwrap_or(i64::MAX),
            value,
        })
    })
    .ok()
}

fn publish_exact_string_map<I, K, V>(
    fast: &mut NativeRequestFastState,
    length: usize,
    values: I,
) -> Option<i64>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<[u8]>,
    V: AsRef<[u8]>,
{
    let mut values = values.into_iter();
    fast.publish_owned_direct_array_with(length, |fast, _| {
        let (key_bytes, value_bytes) = values.next().ok_or("native string map truncated")?;
        let key = fast
            .publish_direct_string_bytes(key_bytes.as_ref())
            .map_err(|_| "native string map key publication failed")?;
        let value = match fast.publish_direct_string_bytes(value_bytes.as_ref()) {
            Ok(value) => value,
            Err(_) => {
                let _ = fast.discard_owned_direct_value(key);
                return Err("native string map value publication failed");
            }
        };
        Ok(php_jit::JitNativeDirectArrayEntry { key, value })
    })
    .ok()
}

fn publish_exact_named_owned_values<I, K>(
    fast: &mut NativeRequestFastState,
    values: I,
) -> Option<i64>
where
    I: IntoIterator<Item = (K, i64)>,
    I::IntoIter: ExactSizeIterator,
    K: AsRef<[u8]>,
{
    let mut values = values.into_iter();
    let length = values.len();
    let result = fast.publish_owned_direct_array_with(length, |fast, _| {
        let (key_bytes, value) = values
            .next()
            .ok_or("native named value list is truncated")?;
        let key = match fast.publish_direct_string_bytes(key_bytes.as_ref()) {
            Ok(key) => key,
            Err(_) => {
                let _ = fast.discard_owned_direct_value(value);
                return Err("native named value key publication failed");
            }
        };
        Ok(php_jit::JitNativeDirectArrayEntry { key, value })
    });
    if result.is_err() {
        for (_, value) in values {
            let _ = fast.discard_owned_direct_value(value);
        }
    }
    result.ok()
}

pub(crate) extern "C" fn jit_native_get_loaded_extensions_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let zend_only =
        if argument_0 == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
            false
        } else {
            let Some(value) = fast
                .native_comparison_value(argument_0)
                .map(native_comparison_truthy)
            else {
                return exact_query_contract_violation();
            };
            value
        };
    if zend_only {
        return publish_exact_string_list(fast, 0, std::iter::empty::<&[u8]>()).map_or_else(
            exact_query_contract_violation,
            php_jit::JitNativeControlResult::returning,
        );
    }
    let names = php_std::ExtensionRegistry::standard_library().enabled_extension_names();
    let length = names.len();
    match publish_exact_string_list(fast, length, names.iter().map(|name| name.as_bytes())) {
        Some(value) => php_jit::JitNativeControlResult::returning(value),
        None => exact_query_contract_violation(),
    }
}

#[derive(Clone, Copy)]
enum NativeConfigurationQuery {
    Current,
    Configured,
    IncludePath,
}

fn native_configuration_result(
    fast: &mut NativeRequestFastState,
    name: i64,
    query: NativeConfigurationQuery,
) -> php_jit::JitNativeControlResult {
    let registry = fast.configuration.ini_registry();
    let value = match query {
        NativeConfigurationQuery::IncludePath => registry.get("include_path"),
        NativeConfigurationQuery::Current | NativeConfigurationQuery::Configured => {
            let Some(name) = fast.native_string_view(name) else {
                return exact_query_contract_violation();
            };
            let name = String::from_utf8_lossy(name);
            match query {
                NativeConfigurationQuery::Current => registry.get(name.as_ref()),
                NativeConfigurationQuery::Configured => registry.cfg_var(name.as_ref()),
                NativeConfigurationQuery::IncludePath => unreachable!("matched above"),
            }
        }
    };
    let Some(value) = value else {
        return php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(
            php_jit::JIT_VALUE_FALSE,
        ));
    };
    let bytes = value.as_bytes();
    let bytes = (bytes.as_ptr(), bytes.len());
    // SAFETY: the request configuration owns immutable INI strings for the
    // complete synchronous exact call. Native string publication mutates only
    // the disjoint direct-value byte arena.
    #[allow(unsafe_code)]
    let bytes = unsafe { std::slice::from_raw_parts(bytes.0, bytes.1) };
    match fast.publish_direct_string_bytes(bytes) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_ini_get_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    native_configuration_result(fast, argument_0, NativeConfigurationQuery::Current)
}

pub(crate) extern "C" fn jit_native_ini_get_all_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let extension = if argument_0 == missing {
        None
    } else {
        match fast.native_printf_scalar(argument_0) {
            Some(php_runtime::api::NativePrintfScalar::Null) => None,
            Some(_) => {
                let Some(extension) = exact_native_scalar_string(fast, argument_0) else {
                    return exact_query_contract_violation();
                };
                Some(extension.into_lossy_owned())
            }
            None => return exact_query_contract_violation(),
        }
    };
    let details = if argument_1 == missing {
        true
    } else {
        if matches!(
            fast.native_printf_scalar(argument_1),
            Some(php_runtime::api::NativePrintfScalar::Null)
        ) {
            return exact_query_contract_violation();
        }
        let Some(details) = exact_native_boolean_flag(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        details
    };
    let entries = extension.as_deref().map_or_else(
        || fast.configuration.ini_registry().entries(),
        |extension| {
            fast.configuration
                .ini_registry()
                .entries_for_extension(extension)
        },
    );
    let length = entries.len();
    let mut entries = entries.into_iter();
    let result = fast.publish_owned_direct_array_with(length, |fast, _| {
        let entry = entries.next().ok_or("native INI inventory is truncated")?;
        let key = fast.publish_direct_string_bytes(entry.name.as_bytes())?;
        let value = if details {
            let global = match fast.publish_direct_string_bytes(entry.global_value.as_bytes()) {
                Ok(value) => value,
                Err(_) => {
                    let _ = fast.discard_owned_direct_value(key);
                    return Err("native INI global value publication failed");
                }
            };
            let local = match fast.publish_direct_string_bytes(entry.local_value.as_bytes()) {
                Ok(value) => value,
                Err(_) => {
                    let _ = fast.discard_owned_direct_value(global);
                    let _ = fast.discard_owned_direct_value(key);
                    return Err("native INI local value publication failed");
                }
            };
            let access = match fast.publish_direct_int(entry.access) {
                Ok(value) => value,
                Err(_) => {
                    let _ = fast.discard_owned_direct_value(local);
                    let _ = fast.discard_owned_direct_value(global);
                    let _ = fast.discard_owned_direct_value(key);
                    return Err("native INI access publication failed");
                }
            };
            let Some(value) = publish_exact_named_owned_values(
                fast,
                [
                    (b"global_value".as_slice(), global),
                    (b"local_value".as_slice(), local),
                    (b"access".as_slice(), access),
                ],
            ) else {
                let _ = fast.discard_owned_direct_value(key);
                return Err("native detailed INI entry publication failed");
            };
            value
        } else {
            match fast.publish_direct_string_bytes(entry.local_value.as_bytes()) {
                Ok(value) => value,
                Err(_) => {
                    let _ = fast.discard_owned_direct_value(key);
                    return Err("native INI value publication failed");
                }
            }
        };
        Ok(php_jit::JitNativeDirectArrayEntry { key, value })
    });
    result.map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_get_cfg_var_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    native_configuration_result(fast, argument_0, NativeConfigurationQuery::Configured)
}

pub(crate) extern "C" fn jit_native_get_include_path_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    native_configuration_result(fast, 0, NativeConfigurationQuery::IncludePath)
}

fn exact_native_scalar_string<'a>(
    fast: &'a NativeRequestFastState,
    encoded: i64,
) -> Option<NativeScalarBytes<'a>> {
    fast.native_scalar_bytes(encoded)
}

fn exact_native_configuration_set(
    fast: &mut NativeRequestFastState,
    name: String,
    value: String,
) -> php_jit::JitNativeControlResult {
    let previous = fast.configuration.ini_registry_mut().set(&name, &value);
    if previous.is_some() && name.eq_ignore_ascii_case("include_path") {
        let include_path = fast.configuration.include_path_mut();
        *include_path = Arc::new(std::env::split_paths(std::ffi::OsStr::new(&value)).collect());
    }
    if previous.is_some() && name.eq_ignore_ascii_case("display_errors") {
        let enabled = fast.configuration.ini_registry().get("display_errors") == Some("1");
        let display_errors = fast.configuration.display_errors_mut();
        *display_errors = enabled;
    }
    let Some(previous) = previous else {
        return exact_query_return_bool(false);
    };
    fast.publish_direct_string_bytes(previous.as_bytes())
        .map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        )
}

pub(crate) extern "C" fn jit_native_ini_set_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let (Some(name), Some(value)) = (
        exact_native_scalar_string(fast, argument_0),
        exact_native_scalar_string(fast, argument_1),
    ) else {
        return exact_query_contract_violation();
    };
    exact_native_configuration_set(fast, name.into_lossy_owned(), value.into_lossy_owned())
}

pub(crate) extern "C" fn jit_native_set_include_path_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(value) = exact_native_scalar_string(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    exact_native_configuration_set(fast, "include_path".to_owned(), value.into_lossy_owned())
}

pub(crate) extern "C" fn jit_native_date_default_timezone_get_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let (timezone, timezone_length) = {
        let timezone = fast.configuration.default_timezone().as_bytes();
        (timezone.as_ptr(), timezone.len())
    };
    fast.try_publish_direct_string_with(timezone_length, |output| {
        // SAFETY: the request configuration is not mutated during this
        // synchronous native string publication.
        #[allow(unsafe_code)]
        let timezone = unsafe { std::slice::from_raw_parts(timezone, timezone_length) };
        output.copy_from_slice(timezone);
        Ok::<(), &'static str>(())
    })
    .map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_date_default_timezone_set_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(identifier) = exact_native_scalar_string(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let identifier = identifier.into_lossy_owned();
    let Some(identifier) = php_runtime::api::normalize_timezone_identifier(&identifier) else {
        return exact_query_return_bool(false);
    };
    let timezone = fast.configuration.default_timezone_mut();
    *timezone = identifier;
    exact_query_return_bool(true)
}

fn exact_session_null_argument(fast: &NativeRequestFastState, encoded: i64) -> Option<bool> {
    fast.native_printf_scalar(encoded)
        .map(|value| matches!(value, php_runtime::api::NativePrintfScalar::Null))
}

fn exact_session_argument_missing(encoded: i64) -> bool {
    encoded == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING)
}

fn exact_session_publish_string(
    fast: &mut NativeRequestFastState,
    value: String,
) -> php_jit::JitNativeControlResult {
    fast.publish_direct_string_bytes(value.as_bytes())
        .map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        )
}

fn exact_session_ini_bool(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "off" | "no"
        )
    })
}

pub(crate) extern "C" fn jit_native_session_status_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    fast.publish_direct_int(fast.session.control().status())
        .map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        )
}

pub(crate) extern "C" fn jit_native_session_cache_expire_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let previous = fast
        .configuration
        .ini_registry()
        .get("session.cache_expire")
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(180);
    if !exact_session_argument_missing(argument_0) {
        let Some(is_null) = exact_session_null_argument(fast, argument_0) else {
            return exact_query_contract_violation();
        };
        if !is_null {
            if fast.session.control().status() == php_runtime::api::PHP_SESSION_ACTIVE {
                return exact_query_contract_violation();
            }
            let Some(value) = exact_native_integer(fast, argument_0) else {
                return exact_query_contract_violation();
            };
            fast.session.control_mut().replace_cache_expire(value);
            let _ = fast
                .configuration
                .ini_registry_mut()
                .set("session.cache_expire", value.to_string());
        }
    }
    fast.publish_direct_int(previous).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_session_cache_limiter_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let previous = fast
        .configuration
        .ini_registry()
        .get("session.cache_limiter")
        .unwrap_or("nocache")
        .to_owned();
    if !exact_session_argument_missing(argument_0) {
        let Some(is_null) = exact_session_null_argument(fast, argument_0) else {
            return exact_query_contract_violation();
        };
        if !is_null {
            if fast.session.control().status() == php_runtime::api::PHP_SESSION_ACTIVE {
                return exact_query_contract_violation();
            }
            let Some(value) = exact_native_scalar_string(fast, argument_0) else {
                return exact_query_contract_violation();
            };
            let value = value.into_lossy_owned();
            fast.session
                .control_mut()
                .replace_cache_limiter(value.clone());
            let _ = fast
                .configuration
                .ini_registry_mut()
                .set("session.cache_limiter", value);
        }
    }
    exact_session_publish_string(fast, previous)
}

pub(crate) extern "C" fn jit_native_session_id_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let previous = fast.session.control().id().to_owned();
    if !exact_session_argument_missing(argument_0) {
        let Some(is_null) = exact_session_null_argument(fast, argument_0) else {
            return exact_query_contract_violation();
        };
        if !is_null {
            if fast.session.control().status() == php_runtime::api::PHP_SESSION_ACTIVE
                || !fast.session.control().id_replacement_is_value_free()
            {
                return exact_query_contract_violation();
            }
            let Some(value) = exact_native_scalar_string(fast, argument_0) else {
                return exact_query_contract_violation();
            };
            let value = value.into_lossy_owned();
            if fast
                .session
                .control_mut()
                .replace_id_value_free(value)
                .is_none()
            {
                return exact_query_contract_violation();
            }
        }
    }
    exact_session_publish_string(fast, previous)
}

pub(crate) extern "C" fn jit_native_session_module_name_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let previous = fast.session.control().module_name().to_owned();
    if !exact_session_argument_missing(argument_0) {
        let Some(is_null) = exact_session_null_argument(fast, argument_0) else {
            return exact_query_contract_violation();
        };
        if !is_null {
            if fast.session.control().status() == php_runtime::api::PHP_SESSION_ACTIVE {
                return exact_query_contract_violation();
            }
            let Some(value) = exact_native_scalar_string(fast, argument_0) else {
                return exact_query_contract_violation();
            };
            if value.as_bytes() != b"files" {
                return exact_query_contract_violation();
            }
            fast.session.control_mut().replace_module_name("files");
            let _ = fast
                .configuration
                .ini_registry_mut()
                .set("session.save_handler", "files");
        }
    }
    exact_session_publish_string(fast, previous)
}

pub(crate) extern "C" fn jit_native_session_name_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let configured = fast
        .configuration
        .ini_registry()
        .get("session.name")
        .unwrap_or("PHPSESSID")
        .to_owned();
    if fast.session.control().name() == "PHPSESSID" && configured != "PHPSESSID" {
        fast.session.control_mut().replace_name(configured);
    }
    let previous = fast.session.control().name().to_owned();
    if !exact_session_argument_missing(argument_0) {
        let Some(is_null) = exact_session_null_argument(fast, argument_0) else {
            return exact_query_contract_violation();
        };
        if !is_null {
            if fast.session.control().status() == php_runtime::api::PHP_SESSION_ACTIVE {
                return exact_query_contract_violation();
            }
            let Some(value) = exact_native_scalar_string(fast, argument_0) else {
                return exact_query_contract_violation();
            };
            let value = value.into_lossy_owned();
            if !php_runtime::api::native_session_name_is_valid(&value) {
                return exact_query_contract_violation();
            }
            fast.session.control_mut().replace_name(value.clone());
            let _ = fast
                .configuration
                .ini_registry_mut()
                .set("session.name", value);
        }
    }
    exact_session_publish_string(fast, previous)
}

pub(crate) extern "C" fn jit_native_session_save_path_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let previous = fast
        .configuration
        .ini_registry()
        .get("session.save_path")
        .map(str::to_owned)
        .unwrap_or_else(|| fast.session.control().save_path().to_owned());
    if !exact_session_argument_missing(argument_0) {
        let Some(is_null) = exact_session_null_argument(fast, argument_0) else {
            return exact_query_contract_violation();
        };
        if !is_null {
            if fast.session.control().status() == php_runtime::api::PHP_SESSION_ACTIVE {
                return exact_query_contract_violation();
            }
            let Some(value) = exact_native_scalar_string(fast, argument_0) else {
                return exact_query_contract_violation();
            };
            let value = value.into_lossy_owned();
            fast.session.control_mut().replace_save_path(value.clone());
            let _ = fast
                .configuration
                .ini_registry_mut()
                .set("session.save_path", value);
        }
    }
    exact_session_publish_string(fast, previous)
}

fn exact_session_cookie_value(
    fast: &mut NativeRequestFastState,
    name: &str,
    default: &str,
    kind: u8,
) -> Option<i64> {
    let value = fast
        .configuration
        .ini_registry()
        .get(name)
        .unwrap_or(default)
        .to_owned();
    match kind {
        0 => fast
            .publish_direct_int(value.trim().parse::<i64>().unwrap_or(0))
            .ok(),
        1 => fast.publish_direct_string_bytes(value.as_bytes()).ok(),
        2 => Some(php_jit::jit_encode_constant(
            if exact_session_ini_bool(Some(&value)) {
                php_jit::JIT_VALUE_TRUE
            } else {
                php_jit::JIT_VALUE_FALSE
            },
        )),
        _ => None,
    }
}

pub(crate) extern "C" fn jit_native_session_get_cookie_params_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let specs = [
        ("lifetime", "session.cookie_lifetime", "0", 0),
        ("path", "session.cookie_path", "/", 1),
        ("domain", "session.cookie_domain", "", 1),
        ("secure", "session.cookie_secure", "0", 2),
        ("partitioned", "session.cookie_partitioned", "0", 2),
        ("httponly", "session.cookie_httponly", "0", 2),
        ("samesite", "session.cookie_samesite", "", 1),
    ];
    let length = specs.len();
    let mut specs = specs.into_iter();
    let result = fast.publish_owned_direct_array_with(length, |fast, _| {
        let (key, ini, default, kind) = specs
            .next()
            .ok_or("native session cookie specification is truncated")?;
        let key = fast.publish_direct_string_bytes(key.as_bytes())?;
        let Some(value) = exact_session_cookie_value(fast, ini, default, kind) else {
            let _ = fast.discard_owned_direct_value(key);
            return Err("native session cookie value publication failed");
        };
        Ok(php_jit::JitNativeDirectArrayEntry { key, value })
    });
    result.map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_session_set_cookie_params_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
    argument_4: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if fast.session.control().status() == php_runtime::api::PHP_SESSION_ACTIVE {
        return exact_query_contract_violation();
    }
    let Some(lifetime) = exact_native_integer(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let mut updates = vec![("session.cookie_lifetime", lifetime.to_string())];
    for (index, name, boolean) in [
        (1, "session.cookie_path", false),
        (2, "session.cookie_domain", false),
        (3, "session.cookie_secure", true),
        (4, "session.cookie_httponly", true),
    ] {
        let encoded = [argument_0, argument_1, argument_2, argument_3, argument_4][index];
        if exact_session_argument_missing(encoded) {
            continue;
        }
        let Some(is_null) = exact_session_null_argument(fast, encoded) else {
            return exact_query_contract_violation();
        };
        if is_null {
            continue;
        }
        let value = if boolean {
            let Some(value) = exact_native_boolean_flag(fast, encoded) else {
                return exact_query_contract_violation();
            };
            if value { "1" } else { "0" }.to_owned()
        } else {
            let Some(value) = exact_native_scalar_string(fast, encoded) else {
                return exact_query_contract_violation();
            };
            value.into_lossy_owned()
        };
        updates.push((name, value));
    }
    for (name, value) in updates {
        let _ = fast.configuration.ini_registry_mut().set(name, value);
    }
    exact_query_return_bool(true)
}

pub(crate) extern "C" fn jit_native_session_gc_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if fast.session.control().status() != php_runtime::api::PHP_SESSION_ACTIVE {
        return exact_query_contract_violation();
    }
    php_jit::JitNativeControlResult::returning(0)
}

pub(crate) extern "C" fn jit_native_session_register_shutdown_abi(
    _runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX))
}

fn exact_session_sid_length(fast: &NativeRequestFastState) -> usize {
    fast.configuration
        .ini_registry()
        .get("session.sid_length")
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| (22..=256).contains(value))
        .unwrap_or(32)
}

fn exact_session_lifecycle_result(value: bool) -> php_jit::JitNativeControlResult {
    exact_query_return_bool(value)
}

pub(crate) extern "C" fn jit_native_session_abort_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if !fast.session.control().payload_operation_is_active() {
        return exact_session_lifecycle_result(false);
    }
    if !fast.restore_native_session_payload() {
        return exact_query_contract_violation();
    }
    exact_session_lifecycle_result(fast.session.control_mut().close_value_free())
}

pub(crate) extern "C" fn jit_native_session_commit_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if !fast.session.control().payload_operation_is_active() {
        return exact_session_lifecycle_result(false);
    }
    if !fast.commit_native_session_payload() {
        return exact_query_contract_violation();
    }
    exact_session_lifecycle_result(fast.session.control_mut().close_value_free())
}

pub(crate) extern "C" fn jit_native_session_destroy_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if !fast.session.control().payload_operation_is_active() {
        // PHP emits a warning for this state.
        return exact_query_contract_violation();
    }
    if !fast.clear_native_session_payload_and_commit() {
        return exact_query_contract_violation();
    }
    exact_session_lifecycle_result(fast.session.control_mut().destroy_value_free())
}

pub(crate) extern "C" fn jit_native_session_decode_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if fast.native_session_decode(argument_0).is_none() {
        // Type errors, inactive sessions, malformed input, and unsupported
        // object/reference records require PHP diagnostics or cold semantics.
        return exact_query_contract_violation();
    }
    exact_session_lifecycle_result(true)
}

pub(crate) extern "C" fn jit_native_session_encode_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(output_length) = fast.native_session_encode_output_length() else {
        return exact_query_contract_violation();
    };
    let Some(output_length) = output_length else {
        return exact_session_lifecycle_result(false);
    };
    fast.try_publish_direct_string_with(output_length, |output| {
        // SAFETY: direct string publication mutates only the disjoint byte
        // arena; the authoritative session graph remains stable for this
        // synchronous second serialization pass.
        #[allow(unsafe_code)]
        let fast = unsafe { &*runtime };
        fast.native_session_encode_into(output)
            .then_some(())
            .ok_or("native session serialization changed after its length pass")
    })
    .map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_session_create_id_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if fast.session.has_id_generator() {
        return exact_query_contract_violation();
    }
    let prefix = if exact_session_argument_missing(argument_0) {
        NativeScalarBytes::Empty
    } else {
        let Some(is_null) = exact_session_null_argument(fast, argument_0) else {
            return exact_query_contract_violation();
        };
        if is_null {
            NativeScalarBytes::Empty
        } else {
            let Some(prefix) = exact_native_scalar_string(fast, argument_0) else {
                return exact_query_contract_violation();
            };
            prefix
        }
    };
    if prefix.as_bytes().len() > 256
        || prefix.as_bytes().contains(&0)
        || !prefix
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b','))
    {
        return exact_query_contract_violation();
    }
    let prefix = prefix.into_lossy_owned();
    let id_length = exact_session_sid_length(fast);
    let id = fast
        .session
        .control_mut()
        .create_id_with_prefix(&prefix, id_length);
    exact_session_publish_string(fast, id)
}

pub(crate) extern "C" fn jit_native_session_regenerate_id_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if !exact_session_argument_missing(argument_0)
        && exact_native_boolean_flag(fast, argument_0).is_none()
    {
        return exact_query_contract_violation();
    }
    if !fast.session.control().payload_operation_is_active() || fast.session.has_id_generator() {
        return exact_query_contract_violation();
    }
    let id_length = exact_session_sid_length(fast);
    exact_session_lifecycle_result(
        fast.session
            .control_mut()
            .regenerate_id_value_free(id_length),
    )
}

pub(crate) extern "C" fn jit_native_session_reset_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if !fast.session.control().payload_operation_is_active() {
        return exact_session_lifecycle_result(false);
    }
    if !fast.restore_native_session_payload() {
        return exact_query_contract_violation();
    }
    exact_session_lifecycle_result(true)
}

pub(crate) extern "C" fn jit_native_session_set_save_handler_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    _argument_1: i64,
    argument_2: i64,
    argument_3: i64,
    argument_4: i64,
    argument_5: i64,
    argument_6: i64,
    argument_7: i64,
    argument_8: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if fast.session.control().payload_operation_is_active()
        || [
            argument_2, argument_3, argument_4, argument_5, argument_6, argument_7, argument_8,
        ]
        .into_iter()
        .any(|argument| !exact_session_argument_missing(argument))
        || fast.native_query_object(argument_0).is_none()
    {
        // Callback lists require callable validation and a deprecation; the
        // object form is representation-complete and has no retained payload
        // in the current PHP session implementation.
        return exact_query_contract_violation();
    }
    let _ = fast
        .configuration
        .ini_registry_mut()
        .set("session.save_handler", "user");
    fast.session.control_mut().replace_module_name("user");
    exact_session_lifecycle_result(true)
}

pub(crate) extern "C" fn jit_native_session_start_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if !exact_session_argument_missing(argument_0)
        || fast.session.control().payload_operation_is_active()
    {
        // Options and duplicate starts can emit diagnostics.
        return exact_query_contract_violation();
    }
    let ini = fast.configuration.ini_registry();
    if !matches!(ini.get("session.save_handler").unwrap_or("files"), "files")
        || !matches!(
            ini.get("session.serialize_handler").unwrap_or("php"),
            "php" | "php_binary" | "php_serialize"
        )
        || ini
            .get("session.save_path")
            .is_some_and(|path| !path.is_empty())
        || ini
            .get("open_basedir")
            .is_some_and(|paths| !paths.trim().is_empty())
    {
        return exact_query_contract_violation();
    }
    let strict_mode = exact_session_ini_bool(ini.get("session.use_strict_mode"));
    let generate = fast.session.control().id().is_empty() || strict_mode;
    if ((fast.session.control().needs_lazy_load() && fast.session.has_loader())
        || (generate && fast.session.has_id_generator()))
        && crate::vm::jit_abi::prepare_native_session_start_transport(
            fast.session.transport_context,
            generate && fast.session.has_id_generator(),
        )
        .is_err()
    {
        return exact_query_runtime_error();
    }
    let payload_ready = if generate {
        fast.clear_native_session_payload_and_commit()
    } else {
        fast.restore_native_session_payload()
    };
    if !payload_ready {
        return exact_query_contract_violation();
    }
    let id_length = exact_session_sid_length(fast);
    fast.session
        .control_mut()
        .start_value_free(id_length, strict_mode);
    exact_session_lifecycle_result(true)
}

pub(crate) extern "C" fn jit_native_session_unset_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if !fast.session.control().payload_operation_is_active() {
        return exact_session_lifecycle_result(false);
    }
    if !fast.clear_native_session_payload() {
        return exact_query_contract_violation();
    }
    exact_session_lifecycle_result(true)
}

pub(crate) extern "C" fn jit_native_session_write_close_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    jit_native_session_commit_abi(runtime)
}

pub(crate) extern "C" fn jit_native_header_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    file: i64,
    start: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(line) = exact_native_scalar_string(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let replace = if argument_1 != missing {
        let Some(replace) = exact_native_boolean_flag(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        replace
    } else {
        true
    };
    let response_code =
        if argument_2 == missing || argument_2 == php_jit::jit_encode_constant(u32::MAX) {
            None
        } else {
            let Some(code) = exact_native_integer(fast, argument_2) else {
                return exact_query_contract_violation();
            };
            if code == 0 {
                None
            } else {
                let Ok(code) = u16::try_from(code) else {
                    let message = format!("header(): invalid HTTP response code {code}");
                    if super::exact_runtime_ops::emit_exact_native_structured_warning(
                        fast,
                        "E_PHP_RUNTIME_INVALID_HEADER",
                        message,
                        file,
                        start,
                    ) != 0
                    {
                        return exact_query_runtime_error();
                    }
                    return php_jit::JitNativeControlResult::returning(
                        php_jit::jit_encode_constant(u32::MAX),
                    );
                };
                if !(100..=599).contains(&code) {
                    let message = format!("header(): invalid HTTP response code {code}");
                    if super::exact_runtime_ops::emit_exact_native_structured_warning(
                        fast,
                        "E_PHP_RUNTIME_INVALID_HEADER",
                        message,
                        file,
                        start,
                    ) != 0
                    {
                        return exact_query_runtime_error();
                    }
                    return php_jit::JitNativeControlResult::returning(
                        php_jit::jit_encode_constant(u32::MAX),
                    );
                }
                Some(code)
            }
        };
    let line = line.into_lossy_owned();
    if let Err(message) =
        fast.http_response
            .response_mut()
            .add_header_line(line.as_ref(), replace, response_code)
    {
        if super::exact_runtime_ops::emit_exact_native_structured_warning(
            fast,
            "E_PHP_RUNTIME_INVALID_HEADER",
            format!("header(): {message}"),
            file,
            start,
        ) != 0
        {
            return exact_query_runtime_error();
        }
    }
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX))
}

pub(crate) extern "C" fn jit_native_header_remove_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let name = if argument_0 == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING)
        || argument_0 == php_jit::jit_encode_constant(u32::MAX)
    {
        None
    } else {
        let Some(name) = exact_native_scalar_string(fast, argument_0) else {
            return exact_query_contract_violation();
        };
        Some(name.into_lossy_owned())
    };
    if fast
        .http_response
        .response_mut()
        .remove_header(name.as_deref())
        .is_err()
    {
        return exact_query_contract_violation();
    }
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX))
}

pub(crate) extern "C" fn jit_native_headers_list_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let lines = fast.http_response.response().headers_list();
    let length = lines.len();
    let mut lines = lines.into_iter();
    let result = fast.publish_owned_direct_array_with(length, |fast, index| {
        let line = lines
            .next()
            .ok_or("native response header list is truncated")?;
        let value = fast.publish_direct_string_bytes(line.as_bytes())?;
        Ok(php_jit::JitNativeDirectArrayEntry {
            key: i64::try_from(index).unwrap_or(i64::MAX),
            value,
        })
    });
    result.map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_headers_sent_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &*runtime };
    exact_query_return_bool(fast.http_response.response().headers_sent)
}

pub(crate) extern "C" fn jit_native_http_response_code_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let previous = i64::from(fast.http_response.response().status_code);
    if argument_0 == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING)
        || argument_0 == php_jit::jit_encode_constant(u32::MAX)
    {
        return php_jit::JitNativeControlResult::returning(previous);
    }
    let Some(code) = exact_native_integer(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let Ok(code) = u16::try_from(code) else {
        return exact_query_return_bool(false);
    };
    if !fast.http_response.response_mut().set_status_code(code) {
        return exact_query_return_bool(false);
    }
    php_jit::JitNativeControlResult::returning(previous)
}

fn exact_native_cookie_integer(fast: &NativeRequestFastState, encoded: i64) -> Option<i64> {
    match fast.native_printf_scalar(encoded)? {
        php_runtime::api::NativePrintfScalar::Null
        | php_runtime::api::NativePrintfScalar::Bool(false) => Some(0),
        php_runtime::api::NativePrintfScalar::Bool(true) => Some(1),
        php_runtime::api::NativePrintfScalar::Int(value) => Some(value),
        php_runtime::api::NativePrintfScalar::Float(value) if value.is_finite() => {
            Some(value as i64)
        }
        php_runtime::api::NativePrintfScalar::Float(_)
        | php_runtime::api::NativePrintfScalar::String(_) => None,
    }
}

fn exact_native_cookie_string(fast: &NativeRequestFastState, encoded: i64) -> Option<String> {
    exact_native_scalar_string(fast, encoded).map(NativeScalarBytes::into_lossy_owned)
}

fn exact_native_cookie_options_array(
    fast: &NativeRequestFastState,
    encoded: i64,
) -> Option<php_runtime::api::NativeCookieOptions> {
    let entries = fast.native_direct_array_entries(encoded)?;
    let mut options = php_runtime::api::NativeCookieOptions::default();
    for entry in entries {
        let Some(key) = fast.native_string_view(entry.key) else {
            continue;
        };
        if key.eq_ignore_ascii_case(b"expires") {
            options.expires = exact_native_cookie_integer(fast, entry.value)?;
        } else if key.eq_ignore_ascii_case(b"path") {
            options.path = exact_native_cookie_string(fast, entry.value)?;
        } else if key.eq_ignore_ascii_case(b"domain") {
            options.domain = exact_native_cookie_string(fast, entry.value)?;
        } else if key.eq_ignore_ascii_case(b"secure") {
            options.secure = exact_native_boolean_flag(fast, entry.value)?;
        } else if key.eq_ignore_ascii_case(b"httponly") {
            options.httponly = exact_native_boolean_flag(fast, entry.value)?;
        } else if key.eq_ignore_ascii_case(b"samesite") {
            options.samesite = exact_native_cookie_string(fast, entry.value)?;
        }
    }
    Some(options)
}

fn exact_native_cookie_options(
    fast: &NativeRequestFastState,
    arguments: &[i64; 7],
) -> Option<php_runtime::api::NativeCookieOptions> {
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    if arguments[2] == missing {
        return Some(php_runtime::api::NativeCookieOptions::default());
    }
    if let Some(options) = exact_native_cookie_options_array(fast, arguments[2]) {
        return Some(options);
    }
    let mut options = php_runtime::api::NativeCookieOptions {
        expires: exact_native_cookie_integer(fast, arguments[2])?,
        ..php_runtime::api::NativeCookieOptions::default()
    };
    if arguments[3] != missing {
        options.path = exact_native_cookie_string(fast, arguments[3])?;
    }
    if arguments[4] != missing {
        options.domain = exact_native_cookie_string(fast, arguments[4])?;
    }
    if arguments[5] != missing {
        options.secure = exact_native_boolean_flag(fast, arguments[5])?;
    }
    if arguments[6] != missing {
        options.httponly = exact_native_boolean_flag(fast, arguments[6])?;
    }
    Some(options)
}

fn exact_native_cookie<const RAW: bool>(
    runtime: *mut NativeRequestFastState,
    arguments: [i64; 7],
) -> php_jit::JitNativeControlResult {
    let fast = {
        // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
        #[allow(unsafe_code)]
        unsafe {
            &mut *runtime
        }
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let (Some(name), Some(value), Some(options)) = (
        exact_native_cookie_string(fast, arguments[0]),
        (arguments[1] != missing)
            .then(|| exact_native_cookie_string(fast, arguments[1]))
            .flatten()
            .or_else(|| (arguments[1] == missing).then(String::new)),
        exact_native_cookie_options(fast, &arguments),
    ) else {
        return exact_query_contract_violation();
    };
    let Some(header_value) =
        php_runtime::api::build_native_cookie_header_value(&name, &value, &options, RAW)
    else {
        return exact_query_contract_violation();
    };
    let line = format!("Set-Cookie: {header_value}");
    if fast
        .http_response
        .response_mut()
        .add_header_line(&line, false, None)
        .is_err()
    {
        return exact_query_contract_violation();
    }
    exact_query_return_bool(true)
}

pub(crate) extern "C" fn jit_native_setcookie_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
    argument_4: i64,
    argument_5: i64,
    argument_6: i64,
) -> php_jit::JitNativeControlResult {
    exact_native_cookie::<false>(
        runtime,
        [
            argument_0, argument_1, argument_2, argument_3, argument_4, argument_5, argument_6,
        ],
    )
}

pub(crate) extern "C" fn jit_native_setrawcookie_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
    argument_4: i64,
    argument_5: i64,
    argument_6: i64,
) -> php_jit::JitNativeControlResult {
    exact_native_cookie::<true>(
        runtime,
        [
            argument_0, argument_1, argument_2, argument_3, argument_4, argument_5, argument_6,
        ],
    )
}

fn exact_native_clock_elapsed() -> Option<std::time::Duration> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
}

pub(crate) extern "C" fn jit_native_time_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    fast.publish_direct_int(php_runtime::api::datetime::current_timestamp())
        .map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        )
}

pub(crate) extern "C" fn jit_native_uniqid_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let prefix = if argument_0 == missing {
        NativeScalarBytes::Borrowed(&[])
    } else {
        let Some(prefix) = exact_native_scalar_string(fast, argument_0) else {
            return exact_query_contract_violation();
        };
        prefix
    };
    let more_entropy = if argument_1 == missing {
        false
    } else {
        let Some(more_entropy) = exact_native_boolean_flag(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        more_entropy
    };
    let Some(value) = php_runtime::api::native_uniqid(prefix.as_bytes(), more_entropy) else {
        return exact_query_runtime_error();
    };
    fast.publish_direct_string_bytes(&value).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

fn exact_finfo_property(
    object: &php_runtime::api::ObjectRef,
    name: &str,
) -> Option<php_runtime::api::NativeDeclaredPropertySlot> {
    let layout = object.class_layout_epoch();
    object
        .native_dynamic_property_slot(layout, name)
        .flatten()
        .or_else(|| {
            let location = object.native_declared_property_slot_location(layout, name)?;
            // Safety: the object owns this stable slot for the synchronous borrow.
            #[allow(unsafe_code)]
            Some(unsafe { *location })
        })
        .filter(|slot| slot.initialized != 0)
}

fn exact_finfo_options(
    fast: &NativeRequestFastState,
    encoded: i64,
) -> Option<(php_runtime::api::ObjectRef, i64, Option<String>)> {
    let object = fast.direct_object(encoded)?.clone();
    if normalize_class_name(&object.class_name()) != "finfo" {
        return None;
    }
    let flags = exact_native_integer(
        fast,
        exact_finfo_property(&object, "__fileinfo_flags")?.value,
    )?;
    let magic = exact_finfo_property(&object, "__fileinfo_magic_file")?.value;
    let magic = match fast.native_printf_scalar(magic)? {
        php_runtime::api::NativePrintfScalar::Null => None,
        php_runtime::api::NativePrintfScalar::String(bytes) => {
            Some(String::from_utf8_lossy(bytes).into_owned())
        }
        _ => return None,
    };
    Some((object, flags, magic))
}

fn exact_finfo_context(fast: &NativeRequestFastState, encoded: i64) -> bool {
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    if encoded == missing
        || matches!(
            fast.native_printf_scalar(encoded),
            Some(php_runtime::api::NativePrintfScalar::Null)
        )
    {
        return true;
    }
    fast.native_resource_view(encoded)
        .cloned()
        .is_some_and(|resource| {
            fast.native_stream_context_resource_options(&resource)
                .is_some()
        })
}

fn exact_finfo_failure(
    fast: &mut NativeRequestFastState,
    function: &str,
    message: String,
    file: i64,
    start: i64,
) -> php_jit::JitNativeControlResult {
    let emitted = super::exact_runtime_ops::emit_exact_native_diagnostic(
        fast,
        2,
        format!("{function}(): {message}"),
        file,
        start,
    );
    if emitted == 0 {
        exact_query_runtime_error()
    } else {
        exact_query_return_bool(false)
    }
}

pub(crate) extern "C" fn jit_native_finfo_open_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    file: i64,
    start: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let flags = if argument_0 == missing {
        0
    } else {
        let Some(flags) = exact_native_weak_integer(fast, argument_0) else {
            return exact_query_contract_violation();
        };
        flags
    };
    let magic = if argument_1 == missing
        || matches!(
            fast.native_printf_scalar(argument_1),
            Some(php_runtime::api::NativePrintfScalar::Null)
        ) {
        None
    } else {
        let Some(magic) = exact_native_scalar_string(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        Some(magic.into_lossy_owned())
    };
    if let Err(message) = php_runtime::api::validate_fileinfo_options(flags, magic.as_deref()) {
        return exact_finfo_failure(fast, "finfo_open", message, file, start);
    }
    let prepared = fast.prepared_finfo_class;
    if prepared.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: request publication owns the immutable prepared class record.
    #[allow(unsafe_code)]
    let prepared = unsafe { &*prepared };
    let flags_value = match fast.publish_direct_int(flags) {
        Ok(value) => value,
        Err(_) => return exact_query_contract_violation(),
    };
    let magic_value = match magic.as_deref() {
        Some(magic) => match fast.publish_direct_string_bytes(magic.as_bytes()) {
            Ok(value) => value,
            Err(_) => {
                let _ = fast.discard_owned_direct_value(flags_value);
                return exact_query_contract_violation();
            }
        },
        None => php_jit::jit_encode_constant(u32::MAX),
    };
    let object = php_runtime::api::ObjectRef::from_layout_native_slots(
        &prepared.entry,
        prepared.display_name.clone(),
        prepared.default_native_slots.clone(),
    );
    let layout = prepared.layout_id;
    for (name, value) in [
        ("__fileinfo_flags", flags_value),
        ("__fileinfo_magic_file", magic_value),
    ] {
        if object
            .set_native_dynamic_property(
                layout,
                name.to_owned(),
                php_runtime::api::NativeDeclaredPropertySlot {
                    initialized: 1,
                    reserved: 0,
                    value,
                },
            )
            .is_err()
        {
            if magic.is_some() {
                let _ = fast.discard_owned_direct_value(magic_value);
            }
            let _ = fast.discard_owned_direct_value(flags_value);
            return exact_query_contract_violation();
        }
    }
    match fast.publish_direct_object(object) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            if magic.is_some() {
                let _ = fast.discard_owned_direct_value(magic_value);
            }
            let _ = fast.discard_owned_direct_value(flags_value);
            exact_query_contract_violation()
        }
    }
}

pub(crate) extern "C" fn jit_native_finfo_close_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    exact_query_return_bool(exact_finfo_options(fast, argument_0).is_some())
}

fn exact_finfo_detect<const FILE: bool>(
    fast: &mut NativeRequestFastState,
    arguments: [i64; 4],
    file: i64,
    start: i64,
) -> php_jit::JitNativeControlResult {
    let Some((_, stored_flags, magic)) = exact_finfo_options(fast, arguments[0]) else {
        return exact_query_return_bool(false);
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let flags = if arguments[2] == missing {
        stored_flags
    } else {
        let Some(flags) = exact_native_weak_integer(fast, arguments[2]) else {
            return exact_query_contract_violation();
        };
        flags
    };
    if !exact_finfo_context(fast, arguments[3]) {
        return exact_query_contract_violation();
    }
    let result = if FILE {
        let Some(path) = fast.native_string_view(arguments[1]) else {
            return exact_query_contract_violation();
        };
        let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
            return exact_query_contract_violation();
        };
        php_runtime::api::native_fileinfo_detect_file(
            cwd,
            filesystem,
            path,
            flags,
            magic.as_deref(),
        )
    } else {
        let Some(bytes) = exact_native_scalar_string(fast, arguments[1]) else {
            return exact_query_contract_violation();
        };
        php_runtime::api::native_fileinfo_detect_buffer(bytes.as_bytes(), flags, magic.as_deref())
            .map(Some)
    };
    match result {
        Ok(Some(value)) => fast
            .publish_direct_string_bytes(value.as_bytes())
            .map_or_else(
                |_| exact_query_contract_violation(),
                php_jit::JitNativeControlResult::returning,
            ),
        Ok(None) => exact_query_return_bool(false),
        Err(message) => exact_finfo_failure(
            fast,
            if FILE { "finfo_file" } else { "finfo_buffer" },
            message,
            file,
            start,
        ),
    }
}

macro_rules! exact_finfo_detect_abi {
    ($name:ident, $file_mode:literal) => {
        pub(crate) extern "C" fn $name(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
            argument_2: i64,
            argument_3: i64,
            file: i64,
            start: i64,
        ) -> php_jit::JitNativeControlResult {
            // Safety: generated code passes the active request-owned fast state.
            #[allow(unsafe_code)]
            let fast = unsafe { &mut *runtime };
            exact_finfo_detect::<$file_mode>(
                fast,
                [argument_0, argument_1, argument_2, argument_3],
                file,
                start,
            )
        }
    };
}

exact_finfo_detect_abi!(jit_native_finfo_buffer_abi, false);
exact_finfo_detect_abi!(jit_native_finfo_file_abi, true);

pub(crate) extern "C" fn jit_native_finfo_set_flags_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some((object, _, _)) = exact_finfo_options(fast, argument_0) else {
        return exact_query_return_bool(false);
    };
    let Some(flags) = exact_native_weak_integer(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    let Ok(value) = fast.publish_direct_int(flags) else {
        return exact_query_contract_violation();
    };
    let slot = php_runtime::api::NativeDeclaredPropertySlot {
        initialized: 1,
        reserved: 0,
        value,
    };
    match object.set_native_dynamic_property(
        object.class_layout_epoch(),
        "__fileinfo_flags".to_owned(),
        slot,
    ) {
        Ok(previous) => {
            if let Some(previous) = previous.filter(|previous| previous.initialized != 0) {
                let _ = fast.discard_owned_direct_value(previous.value);
            }
            exact_query_return_bool(true)
        }
        Err(_) => {
            let _ = fast.discard_owned_direct_value(value);
            exact_query_contract_violation()
        }
    }
}

pub(crate) extern "C" fn jit_native_exif_imagetype_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    file: i64,
    start: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(path) = fast.native_string_view(argument_0).map(<[u8]>::to_vec) else {
        return exact_query_contract_violation();
    };
    if path.is_empty() || path.contains(&0) {
        return exact_query_contract_violation();
    }
    let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
        return exact_query_contract_violation();
    };
    let Some(bytes) = php_runtime::api::native_file_get_contents(cwd, filesystem, &path, 0, None)
    else {
        return exact_finfo_failure(
            fast,
            "exif_imagetype",
            "Failed to open stream: No such file or directory".to_owned(),
            file,
            start,
        );
    };
    let Some(image_type) = php_runtime::api::native_image_type(&bytes) else {
        return exact_query_return_bool(false);
    };
    fast.publish_direct_int(image_type).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

enum ExactNativeImageField {
    Int(i64),
    Bytes(Vec<u8>),
}

fn publish_exact_native_image_info(
    fast: &mut NativeRequestFastState,
    info: &php_runtime::api::ImageInfo,
) -> Result<i64, &'static str> {
    let mut fields = vec![
        (None, ExactNativeImageField::Int(info.width)),
        (None, ExactNativeImageField::Int(info.height)),
        (None, ExactNativeImageField::Int(info.image_type)),
        (
            None,
            ExactNativeImageField::Bytes(
                format!("width=\"{}\" height=\"{}\"", info.width, info.height).into_bytes(),
            ),
        ),
    ];
    if let Some(bits) = info.bits {
        fields.push((Some(b"bits".as_slice()), ExactNativeImageField::Int(bits)));
    }
    if let Some(channels) = info.channels {
        fields.push((
            Some(b"channels".as_slice()),
            ExactNativeImageField::Int(channels),
        ));
    }
    fields.extend([
        (
            Some(b"mime".as_slice()),
            ExactNativeImageField::Bytes(info.mime.as_bytes().to_vec()),
        ),
        (
            Some(b"width_unit".as_slice()),
            ExactNativeImageField::Bytes(b"px".to_vec()),
        ),
        (
            Some(b"height_unit".as_slice()),
            ExactNativeImageField::Bytes(b"px".to_vec()),
        ),
    ]);
    fast.publish_owned_direct_array_with(fields.len(), |fast, index| {
        let (key_bytes, value) = &fields[index];
        let key = if let Some(key) = key_bytes {
            fast.publish_direct_string_bytes(key)
                .map_err(|_| "native image-info key publication failed")?
        } else {
            i64::try_from(index).map_err(|_| "native image-info key overflow")?
        };
        let value = match value {
            ExactNativeImageField::Int(value) => fast.publish_direct_int(*value),
            ExactNativeImageField::Bytes(value) => fast.publish_direct_string_bytes(value),
        };
        let value = match value {
            Ok(value) => value,
            Err(_) => {
                if key_bytes.is_some() {
                    let _ = fast.discard_owned_direct_value(key);
                }
                return Err("native image-info value publication failed");
            }
        };
        Ok(php_jit::JitNativeDirectArrayEntry { key, value })
    })
}

fn publish_exact_native_image_app_info(
    fast: &mut NativeRequestFastState,
    entries: &[(String, Vec<u8>)],
) -> Result<i64, &'static str> {
    fast.publish_owned_direct_array_with(entries.len(), |fast, index| {
        let (key, value) = &entries[index];
        let key = fast
            .publish_direct_string_bytes(key.as_bytes())
            .map_err(|_| "native image APP key publication failed")?;
        let value = match fast.publish_direct_string_bytes(value) {
            Ok(value) => value,
            Err(_) => {
                let _ = fast.discard_owned_direct_value(key);
                return Err("native image APP value publication failed");
            }
        };
        Ok(php_jit::JitNativeDirectArrayEntry { key, value })
    })
}

pub(crate) extern "C" fn jit_native_getimagesize_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    file: i64,
    start: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let Some(path) = fast.native_string_view(argument_0).map(<[u8]>::to_vec) else {
        return exact_query_contract_violation();
    };
    if path.is_empty() || path.contains(&0) {
        return exact_query_contract_violation();
    }
    let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
        return exact_query_contract_violation();
    };
    let bytes = php_runtime::api::native_file_get_contents(cwd, filesystem, &path, 0, None);
    let info = bytes
        .as_deref()
        .and_then(php_runtime::api::native_image_size);
    if argument_1 != missing {
        let app_entries = match (&bytes, &info) {
            (Some(bytes), Some(_)) => php_runtime::api::native_image_app_info(bytes),
            _ => Vec::new(),
        };
        let Ok(app_info) = publish_exact_native_image_app_info(fast, &app_entries) else {
            return exact_query_contract_violation();
        };
        if !fast.replace_direct_reference(argument_1, app_info) {
            return exact_query_contract_violation();
        }
    }
    let Some(bytes) = bytes else {
        return exact_finfo_failure(
            fast,
            "getimagesize",
            "Failed to open stream: No such file or directory".to_owned(),
            file,
            start,
        );
    };
    let Some(info) = info else {
        let _ = bytes;
        return exact_query_return_bool(false);
    };
    publish_exact_native_image_info(fast, &info).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_image_type_to_mime_type_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(image_type) = exact_native_weak_integer(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let mime = php_runtime::api::native_image_type_to_mime_type(image_type);
    fast.publish_direct_string_bytes(mime.as_bytes())
        .map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        )
}

pub(crate) extern "C" fn jit_native_microtime_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let as_float =
        if argument_0 == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
            false
        } else {
            let Some(as_float) = exact_native_boolean_flag(fast, argument_0) else {
                return exact_query_contract_violation();
            };
            as_float
        };
    let Some(elapsed) = exact_native_clock_elapsed() else {
        return exact_query_contract_violation();
    };
    let seconds = elapsed.as_secs();
    let micros = elapsed.subsec_micros();
    if as_float {
        return fast
            .publish_direct_float(seconds as f64 + f64::from(micros) / 1_000_000.0)
            .map_or_else(
                |_| exact_query_contract_violation(),
                php_jit::JitNativeControlResult::returning,
            );
    }
    let value = format!("0.{micros:06} {seconds}");
    fast.publish_direct_string_bytes(value.as_bytes())
        .map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        )
}

pub(crate) extern "C" fn jit_native_hrtime_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let as_number =
        if argument_0 == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
            false
        } else {
            let Some(as_number) = exact_native_boolean_flag(fast, argument_0) else {
                return exact_query_contract_violation();
            };
            as_number
        };
    let Some(elapsed) = exact_native_clock_elapsed() else {
        return exact_query_contract_violation();
    };
    let Ok(seconds) = i64::try_from(elapsed.as_secs()) else {
        return exact_query_contract_violation();
    };
    let nanos = i64::from(elapsed.subsec_nanos());
    if as_number {
        let Some(total) = seconds
            .checked_mul(1_000_000_000)
            .and_then(|value| value.checked_add(nanos))
        else {
            return exact_query_contract_violation();
        };
        return fast.publish_direct_int(total).map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        );
    }
    let Ok(seconds) = fast.publish_direct_int(seconds) else {
        return exact_query_contract_violation();
    };
    let Ok(nanos) = fast.publish_direct_int(nanos) else {
        let _ = fast.discard_owned_direct_value(seconds);
        return exact_query_contract_violation();
    };
    fast.publish_owned_direct_array_from_iter(
        [
            php_jit::JitNativeDirectArrayEntry {
                key: 0,
                value: seconds,
            },
            php_jit::JitNativeDirectArrayEntry {
                key: 1,
                value: nanos,
            },
        ]
        .into_iter(),
    )
    .map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_usleep_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(NativeComparisonValue::Int(micros)) = fast.native_comparison_value(argument_0) else {
        return exact_query_contract_violation();
    };
    if micros < 0 {
        return exact_query_contract_violation();
    }
    std::thread::sleep(std::time::Duration::from_micros(micros as u64));
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX))
}

pub(crate) extern "C" fn jit_native_set_time_limit_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(NativeComparisonValue::Int(seconds)) = fast.native_comparison_value(argument_0) else {
        return exact_query_contract_violation();
    };
    let Ok(seconds) = u64::try_from(seconds) else {
        return exact_query_contract_violation();
    };
    if !fast.execution_deadline.reset_seconds(seconds) {
        return exact_query_contract_violation();
    }
    exact_query_return_bool(true)
}

fn exact_native_date_integer(fast: &NativeRequestFastState, encoded: i64) -> Option<i64> {
    match fast.native_printf_scalar(encoded)? {
        php_runtime::api::NativePrintfScalar::Null
        | php_runtime::api::NativePrintfScalar::Bool(false) => Some(0),
        php_runtime::api::NativePrintfScalar::Bool(true) => Some(1),
        php_runtime::api::NativePrintfScalar::Int(value) => Some(value),
        php_runtime::api::NativePrintfScalar::Float(value) => {
            Some(php_runtime::api::php_float_to_int(value))
        }
        php_runtime::api::NativePrintfScalar::String(bytes) => Some(
            php_runtime::api::native_bytes_to_number(bytes).map_or(0, |value| match value {
                php_runtime::api::NumericValue::Int(value) => value,
                php_runtime::api::NumericValue::Float(value) => {
                    php_runtime::api::php_float_to_int(value)
                }
            }),
        ),
    }
}

fn exact_native_date_string(fast: &NativeRequestFastState, encoded: i64) -> Option<String> {
    exact_native_scalar_string(fast, encoded).map(NativeScalarBytes::into_lossy_owned)
}

fn exact_native_publish_date_string(
    fast: &mut NativeRequestFastState,
    value: String,
) -> php_jit::JitNativeControlResult {
    fast.publish_direct_string_bytes(value.as_bytes())
        .map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        )
}

fn exact_native_format_date<const UTC: bool>(
    fast: &mut NativeRequestFastState,
    format: i64,
    timestamp: i64,
) -> php_jit::JitNativeControlResult {
    let Some(format) = exact_native_date_string(fast, format) else {
        return exact_query_contract_violation();
    };
    let timestamp =
        if timestamp != php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
            let Some(timestamp) = exact_native_date_integer(fast, timestamp) else {
                return exact_query_contract_violation();
            };
            timestamp
        } else {
            php_runtime::api::datetime::current_timestamp()
        };
    let timezone = if UTC {
        "GMT".to_owned()
    } else {
        fast.configuration.default_timezone().to_owned()
    };
    exact_native_publish_date_string(
        fast,
        php_runtime::api::datetime::format_timestamp(timestamp, &timezone, &format),
    )
}

pub(crate) extern "C" fn jit_native_checkdate_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let (Some(month), Some(day), Some(year)) = (
        exact_native_date_integer(fast, argument_0),
        exact_native_date_integer(fast, argument_1),
        exact_native_date_integer(fast, argument_2),
    ) else {
        return exact_query_contract_violation();
    };
    exact_query_return_bool(php_runtime::api::datetime::is_valid_gregorian_date(
        month, day, year,
    ))
}

pub(crate) extern "C" fn jit_native_date_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    exact_native_format_date::<false>(fast, argument_0, argument_1)
}

pub(crate) extern "C" fn jit_native_gmdate_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    exact_native_format_date::<true>(fast, argument_0, argument_1)
}

pub(crate) extern "C" fn jit_native_strtotime_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(text) = exact_native_date_string(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let base = if argument_1 != php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        let Some(base) = exact_native_date_integer(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        base
    } else {
        php_runtime::api::datetime::current_timestamp()
    };
    let timezone = fast.configuration.default_timezone().to_owned();
    match php_runtime::api::datetime::parse_datetime_text_in_timezone(&text, base, &timezone) {
        Some(timestamp) => fast.publish_direct_int(timestamp).map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        ),
        None => exact_query_return_bool(false),
    }
}

fn exact_native_date_create_error(
    fast: &mut NativeRequestFastState,
    prepared_error: u64,
    value_error: bool,
    message: &[u8],
) -> php_jit::JitNativeControlResult {
    exact_json_decode_error(fast, prepared_error, value_error, message)
}

fn exact_native_date_create_timezone(
    fast: &NativeRequestFastState,
    encoded: i64,
) -> Result<String, bool> {
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    if encoded == missing
        || matches!(
            fast.native_printf_scalar(encoded),
            Some(php_runtime::api::NativePrintfScalar::Null)
        )
    {
        return Ok(fast.configuration.default_timezone().to_owned());
    }
    let object = fast.direct_object(encoded).ok_or(false)?;
    if normalize_class_name(&object.class_name()) != "datetimezone" {
        return Err(false);
    }
    let layout = object.class_layout_epoch();
    let slot = object
        .native_dynamic_property_slot(layout, "timezone")
        .flatten()
        .or_else(|| {
            let location = object.native_declared_property_slot_location(layout, "timezone")?;
            // Safety: native declared cells remain stable for the lifetime of
            // this synchronous object borrow and the slot is copied by value.
            #[allow(unsafe_code)]
            Some(unsafe { *location })
        })
        .filter(|slot| slot.initialized != 0)
        .ok_or(true)?;
    let timezone = fast.native_string_view(slot.value).ok_or(true)?;
    Ok(String::from_utf8_lossy(timezone).into_owned())
}

fn exact_native_publish_datetime(
    fast: &mut NativeRequestFastState,
    timestamp: i64,
    timezone: &str,
) -> php_jit::JitNativeControlResult {
    let prepared = fast.prepared_datetime_class;
    if prepared.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: cold request publication owns the boxed class record for the
    // complete lifetime of this synchronous exact call.
    #[allow(unsafe_code)]
    let prepared = unsafe { &*prepared };
    let timestamp = match fast.publish_direct_int(timestamp) {
        Ok(timestamp) => timestamp,
        Err(_) => return exact_query_contract_violation(),
    };
    let timezone = match fast.publish_direct_string_bytes(timezone.as_bytes()) {
        Ok(timezone) => timezone,
        Err(_) => {
            let _ = fast.discard_owned_direct_value(timestamp);
            return exact_query_contract_violation();
        }
    };
    let object = php_runtime::api::ObjectRef::from_layout_native_slots(
        &prepared.entry,
        prepared.display_name.clone(),
        prepared.default_native_slots.clone(),
    );
    debug_assert_eq!(object.class_layout_epoch(), prepared.layout_id);
    let layout = prepared.layout_id;
    for (name, value) in [("__timestamp", timestamp), ("timezone", timezone)] {
        let slot = php_runtime::api::NativeDeclaredPropertySlot {
            initialized: 1,
            reserved: 0,
            value,
        };
        if object
            .set_native_dynamic_property(layout, name.to_owned(), slot)
            .is_err()
        {
            let _ = fast.discard_owned_direct_value(timezone);
            let _ = fast.discard_owned_direct_value(timestamp);
            return exact_query_contract_violation();
        }
    }
    match fast.publish_direct_object(object) {
        Ok(encoded) => php_jit::JitNativeControlResult::returning(encoded),
        Err(_) => {
            let _ = fast.discard_owned_direct_value(timezone);
            let _ = fast.discard_owned_direct_value(timestamp);
            exact_query_contract_violation()
        }
    }
}

pub(crate) extern "C" fn jit_native_date_create_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    prepared_error: u64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let text = if argument_0 == missing
        || matches!(
            fast.native_printf_scalar(argument_0),
            Some(php_runtime::api::NativePrintfScalar::Null)
        ) {
        "now".to_owned()
    } else {
        let Some(text) = exact_native_date_string(fast, argument_0) else {
            return exact_native_date_create_error(
                fast,
                prepared_error,
                false,
                b"date_create(): Argument #1 ($datetime) must be of type string, invalid value given",
            );
        };
        text
    };
    let timezone = match exact_native_date_create_timezone(fast, argument_1) {
        Ok(timezone) => timezone,
        Err(false) => {
            return exact_native_date_create_error(
                fast,
                prepared_error,
                false,
                b"date_create(): Argument #2 ($timezone) must be of type ?DateTimeZone",
            );
        }
        Err(true) => {
            return exact_native_date_create_error(
                fast,
                prepared_error,
                true,
                b"date_create(): DateTimeZone object has no timezone name",
            );
        }
    };
    let Some(timestamp) = php_runtime::api::datetime::parse_datetime_text_in_timezone(
        &text,
        php_runtime::api::datetime::current_timestamp(),
        &timezone,
    ) else {
        return exact_query_return_bool(false);
    };
    exact_native_publish_datetime(fast, timestamp, &timezone)
}

pub(crate) extern "C" fn jit_native_timezone_open_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(timezone) = exact_native_date_string(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(timezone) = php_runtime::api::datetime::normalize_timezone_identifier(&timezone)
    else {
        return exact_query_return_bool(false);
    };
    let prepared = fast.prepared_datetimezone_class;
    if prepared.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: cold request publication owns the boxed class record for the
    // complete lifetime of this synchronous exact call.
    #[allow(unsafe_code)]
    let prepared = unsafe { &*prepared };
    let timezone = match fast.publish_direct_string_bytes(timezone.as_bytes()) {
        Ok(timezone) => timezone,
        Err(_) => return exact_query_contract_violation(),
    };
    let object = php_runtime::api::ObjectRef::from_layout_native_slots(
        &prepared.entry,
        prepared.display_name.clone(),
        prepared.default_native_slots.clone(),
    );
    debug_assert_eq!(object.class_layout_epoch(), prepared.layout_id);
    let slot = php_runtime::api::NativeDeclaredPropertySlot {
        initialized: 1,
        reserved: 0,
        value: timezone,
    };
    if object
        .set_native_dynamic_property(prepared.layout_id, "timezone".to_owned(), slot)
        .is_err()
    {
        let _ = fast.discard_owned_direct_value(timezone);
        return exact_query_contract_violation();
    }
    match fast.publish_direct_object(object) {
        Ok(encoded) => php_jit::JitNativeControlResult::returning(encoded),
        Err(_) => {
            let _ = fast.discard_owned_direct_value(timezone);
            exact_query_contract_violation()
        }
    }
}

fn exact_native_mktime<const UTC: bool>(
    fast: &mut NativeRequestFastState,
    arguments: [i64; 6],
) -> php_jit::JitNativeControlResult {
    let timezone = if UTC {
        "GMT".to_owned()
    } else {
        fast.configuration.default_timezone().to_owned()
    };
    let defaults = php_runtime::api::datetime::format_timestamp(
        php_runtime::api::datetime::current_timestamp(),
        &timezone,
        "Y-n-j-G-i-s",
    )
    .split('-')
    .map(str::parse::<i64>)
    .collect::<Result<Vec<_>, _>>();
    let Ok(defaults) = defaults else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let component = |index: usize, default: i64| -> Option<i64> {
        if arguments[index] == missing {
            return Some(default);
        }
        if matches!(
            fast.native_printf_scalar(arguments[index]),
            Some(php_runtime::api::NativePrintfScalar::Null)
        ) {
            return Some(default);
        }
        exact_native_date_integer(fast, arguments[index])
    };
    let (Some(hour), Some(minute), Some(second), Some(month), Some(day), Some(year)) = (
        component(0, defaults[3]),
        component(1, defaults[4]),
        component(2, defaults[5]),
        component(3, defaults[1]),
        component(4, defaults[2]),
        component(5, defaults[0]),
    ) else {
        return exact_query_contract_violation();
    };
    match php_runtime::api::datetime::timestamp_from_components(
        year, month, day, hour, minute, second, &timezone,
    ) {
        Some(timestamp) => fast.publish_direct_int(timestamp).map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        ),
        None => exact_query_return_bool(false),
    }
}

pub(crate) extern "C" fn jit_native_mktime_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
    argument_4: i64,
    argument_5: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    exact_native_mktime::<false>(
        fast,
        [
            argument_0, argument_1, argument_2, argument_3, argument_4, argument_5,
        ],
    )
}

pub(crate) extern "C" fn jit_native_gmmktime_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
    argument_4: i64,
    argument_5: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    exact_native_mktime::<true>(
        fast,
        [
            argument_0, argument_1, argument_2, argument_3, argument_4, argument_5,
        ],
    )
}

pub(crate) extern "C" fn jit_native_timezone_identifiers_list_abi(
    runtime: *mut NativeRequestFastState,
    _argument_0: i64,
    _argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    match publish_exact_string_list(
        fast,
        php_runtime::api::datetime::TIMEZONE_IDENTIFIERS.len(),
        php_runtime::api::datetime::TIMEZONE_IDENTIFIERS
            .iter()
            .map(|identifier| identifier.as_bytes()),
    ) {
        Some(value) => php_jit::JitNativeControlResult::returning(value),
        None => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_sys_get_temp_dir_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    let path = std::env::temp_dir();
    let bytes = path.as_os_str().as_encoded_bytes();
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    fast.publish_direct_string_bytes(bytes).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_getcwd_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(cwd) = fast.native_current_directory() else {
        return exact_query_contract_violation();
    };
    let bytes = cwd.as_os_str().as_encoded_bytes();
    let bytes = (bytes.as_ptr(), bytes.len());
    // SAFETY: the request's current-directory path is stable for the active
    // synchronous call and result publication mutates disjoint arenas.
    #[allow(unsafe_code)]
    let bytes = unsafe { std::slice::from_raw_parts(bytes.0, bytes.1) };
    fast.publish_direct_string_bytes(bytes).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_chdir_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some((path, path_length)) = fast.stable_native_string_range(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some((cwd, filesystem)) = fast.native_chdir_capability() else {
        return exact_query_contract_violation();
    };
    // SAFETY: the encoded argument owner remains live for this synchronous
    // call, and replacing the disjoint cwd slot cannot relocate its bytes.
    #[allow(unsafe_code)]
    let path = unsafe { std::slice::from_raw_parts(path, path_length) };
    let Some(Some(target)) = php_runtime::api::native_chdir_target(cwd.as_path(), filesystem, path)
    else {
        // Publication must exclude warning-producing targets before mutation.
        // Reaching this state violates that immutable call contract.
        return exact_query_contract_violation();
    };
    *cwd = target;
    exact_query_return_bool(true)
}

pub(crate) extern "C" fn jit_native_umask_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let next = if argument_0 == missing {
        None
    } else {
        let Some(value) = exact_native_integer(fast, argument_0) else {
            return exact_query_contract_violation();
        };
        Some(value)
    };
    let Some(previous) = fast.native_filesystem_state().map(|state| state.umask()) else {
        return exact_query_contract_violation();
    };
    let Ok(encoded_previous) = fast.publish_direct_int(previous) else {
        return exact_query_contract_violation();
    };
    if let Some(next) = next {
        let Some(state) = fast.native_filesystem_state() else {
            return exact_query_contract_violation();
        };
        state.set_umask(next);
    }
    php_jit::JitNativeControlResult::returning(encoded_previous)
}

pub(crate) extern "C" fn jit_native_clearstatcache_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    let _runtime = runtime;
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    if argument_0 != missing || argument_1 != missing {
        // Optional argument coercion and validation remain one cold boundary;
        // the common zero-argument operation has no cache work in this
        // runtime and returns `null` directly.
        return exact_query_contract_violation();
    }
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX))
}

pub(crate) extern "C" fn jit_native_getenv_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(environment) = fast.request_query.environment() else {
        return exact_query_contract_violation();
    };
    let environment = environment as *const [(String, String)];
    // SAFETY: the request environment is immutable and stable for the active
    // request; native result publication mutates only direct value arenas.
    #[allow(unsafe_code)]
    let environment = unsafe { &*environment };
    let name = if argument_0 == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        None
    } else {
        match fast.native_printf_scalar(argument_0) {
            Some(php_runtime::api::NativePrintfScalar::Null) => None,
            Some(php_runtime::api::NativePrintfScalar::String(name)) => Some(name),
            _ => return exact_query_contract_violation(),
        }
    };
    if let Some(name) = name {
        let Some((_, value)) = environment
            .iter()
            .find(|(candidate, _)| candidate.as_bytes() == name)
        else {
            return native_false_result();
        };
        return fast
            .publish_direct_string_bytes(value.as_bytes())
            .map_or_else(
                |_| exact_query_contract_violation(),
                php_jit::JitNativeControlResult::returning,
            );
    }
    publish_exact_string_map(
        fast,
        environment.len(),
        environment
            .iter()
            .map(|(name, value)| (name.as_bytes(), value.as_bytes())),
    )
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_php_sapi_name_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(sapi_name) = fast.request_query.sapi_name() else {
        return exact_query_contract_violation();
    };
    let bytes = sapi_name.as_bytes();
    let bytes = (bytes.as_ptr(), bytes.len());
    // SAFETY: the request SAPI name is immutable and stable for this call.
    #[allow(unsafe_code)]
    let bytes = unsafe { std::slice::from_raw_parts(bytes.0, bytes.1) };
    fast.publish_direct_string_bytes(bytes).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_php_uname_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let mode = if argument_0 == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        b'a'
    } else {
        match fast.native_printf_scalar(argument_0) {
            Some(php_runtime::api::NativePrintfScalar::String(mode)) => {
                mode.first().copied().unwrap_or(b'a').to_ascii_lowercase()
            }
            _ => return exact_query_contract_violation(),
        }
    };
    let version = php_source::reference_php_version();
    let value = match mode {
        b's' => b"Phrust".to_vec(),
        b'n' => b"localhost".to_vec(),
        b'r' => version.as_bytes().to_vec(),
        b'v' => b"Stdlib".to_vec(),
        b'm' => b"generic".to_vec(),
        _ => format!("Phrust localhost {version} Stdlib generic").into_bytes(),
    };
    fast.publish_direct_string_bytes(&value).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_get_current_user_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    fast.publish_direct_string_bytes(b"phrust").map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_get_included_files_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(included_files) = fast.request_query.included_files() else {
        return exact_query_contract_violation();
    };
    let included_files = included_files as *const std::collections::BTreeSet<std::path::PathBuf>;
    // SAFETY: request publication owns this included-file set for the complete
    // activation; native result publication does not mutate it.
    #[allow(unsafe_code)]
    let included_files = unsafe { &*included_files };
    publish_exact_string_list(
        fast,
        included_files.len(),
        included_files
            .iter()
            .map(|path| path.as_os_str().as_encoded_bytes()),
    )
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_get_defined_functions_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(compiled) = fast.symbol_query.active_compiled() else {
        return exact_query_contract_violation();
    };
    let compiled = compiled as *const crate::compiled_unit::CompiledUnit;
    // SAFETY: the active compiled unit is request-stable and native result
    // publication mutates only the fast state's value/string arenas.
    #[allow(unsafe_code)]
    let compiled = unsafe { &*compiled };
    let registry = php_extensions::BuiltinRegistry::new();
    let internal_entries = registry.entries();
    let Some(internal) = publish_exact_string_list(
        fast,
        internal_entries.len(),
        internal_entries.iter().map(|entry| entry.name().as_bytes()),
    ) else {
        return exact_query_contract_violation();
    };
    let user_entries = &compiled.unit().function_table;
    let Some(user) = publish_exact_string_list(
        fast,
        user_entries.len(),
        user_entries.iter().map(|entry| entry.name.as_bytes()),
    ) else {
        let _ = fast.discard_owned_direct_value(internal);
        return exact_query_contract_violation();
    };
    publish_exact_named_owned_values(
        fast,
        [
            (b"internal".as_slice(), internal),
            (b"user".as_slice(), user),
        ],
    )
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_get_declared_classes_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(compiled) = fast.symbol_query.active_compiled() else {
        return exact_query_contract_violation();
    };
    let compiled = compiled as *const crate::compiled_unit::CompiledUnit;
    // SAFETY: the active compiled unit is request-stable for this call.
    #[allow(unsafe_code)]
    let compiled = unsafe { &*compiled };
    let classes = &compiled.unit().classes;
    let length = classes
        .iter()
        .filter(|class| !class.flags.is_interface && !class.flags.is_trait)
        .count();
    publish_exact_string_list(
        fast,
        length,
        classes
            .iter()
            .filter(|class| !class.flags.is_interface && !class.flags.is_trait)
            .map(|class| class.display_name.as_bytes()),
    )
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_get_declared_interfaces_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(compiled) = fast.symbol_query.active_compiled() else {
        return exact_query_contract_violation();
    };
    let compiled = compiled as *const crate::compiled_unit::CompiledUnit;
    // SAFETY: the active compiled unit is request-stable for this call.
    #[allow(unsafe_code)]
    let compiled = unsafe { &*compiled };
    let classes = &compiled.unit().classes;
    let length = classes
        .iter()
        .filter(|class| class.flags.is_interface)
        .count();
    publish_exact_string_list(
        fast,
        length,
        classes
            .iter()
            .filter(|class| class.flags.is_interface)
            .map(|class| class.display_name.as_bytes()),
    )
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_get_declared_traits_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(compiled) = fast.symbol_query.active_compiled() else {
        return exact_query_contract_violation();
    };
    let compiled = compiled as *const crate::compiled_unit::CompiledUnit;
    // SAFETY: the active compiled unit is request-stable for this call.
    #[allow(unsafe_code)]
    let compiled = unsafe { &*compiled };
    let classes = &compiled.unit().classes;
    let length = classes.iter().filter(|class| class.flags.is_trait).count();
    publish_exact_string_list(
        fast,
        length,
        classes
            .iter()
            .filter(|class| class.flags.is_trait)
            .map(|class| class.display_name.as_bytes()),
    )
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

#[derive(Clone, Copy)]
enum ExactConstantInventorySource {
    Standard(php_std::ConstantValue),
    Ir(*const php_ir::IrConstant),
    BorrowedEncoded(i64),
}

#[derive(Clone, Copy)]
struct ExactConstantInventoryEntry {
    name: *const u8,
    name_length: usize,
    source: ExactConstantInventorySource,
}

impl ExactConstantInventoryEntry {
    fn new(name: &str, source: ExactConstantInventorySource) -> Self {
        Self {
            name: name.as_ptr(),
            name_length: name.len(),
            source,
        }
    }
}

fn publish_exact_standard_constant(
    fast: &mut NativeRequestFastState,
    value: php_std::ConstantValue,
) -> Option<i64> {
    match value {
        php_std::ConstantValue::Null => Some(php_jit::jit_encode_constant(u32::MAX)),
        php_std::ConstantValue::Bool(value) => Some(php_jit::jit_encode_constant(if value {
            php_jit::JIT_VALUE_TRUE
        } else {
            php_jit::JIT_VALUE_FALSE
        })),
        php_std::ConstantValue::Int(value) => fast.publish_direct_int(value).ok(),
        php_std::ConstantValue::Float(value) => fast.publish_direct_float(value.to_f64()).ok(),
        php_std::ConstantValue::String(value) => {
            fast.publish_direct_string_bytes(value.as_bytes()).ok()
        }
        php_std::ConstantValue::Array(values) => {
            let length = values.len();
            let mut values = values.iter().copied().enumerate();
            fast.publish_owned_direct_array_with(length, |fast, _| {
                let (index, value) = values
                    .next()
                    .ok_or("native standard constant array is truncated")?;
                let key = match i64::try_from(index)
                    .ok()
                    .and_then(|index| fast.publish_direct_int(index).ok())
                {
                    Some(key) => key,
                    None => return Err("native standard constant key publication failed"),
                };
                let value = match publish_exact_standard_constant(fast, value) {
                    Some(value) => value,
                    None => {
                        let _ = fast.discard_owned_direct_value(key);
                        return Err("native standard constant value publication failed");
                    }
                };
                Ok(php_jit::JitNativeDirectArrayEntry { key, value })
            })
            .ok()
        }
    }
}

fn publish_exact_ir_constant(
    fast: &mut NativeRequestFastState,
    value: &php_ir::IrConstant,
) -> Option<i64> {
    match value {
        php_ir::IrConstant::Null => Some(php_jit::jit_encode_constant(u32::MAX)),
        php_ir::IrConstant::Bool(value) => Some(php_jit::jit_encode_constant(if *value {
            php_jit::JIT_VALUE_TRUE
        } else {
            php_jit::JIT_VALUE_FALSE
        })),
        php_ir::IrConstant::Int(value) => fast.publish_direct_int(*value).ok(),
        php_ir::IrConstant::Float(value) => fast.publish_direct_float(*value).ok(),
        php_ir::IrConstant::String(value) => {
            fast.publish_direct_string_bytes(value.as_bytes()).ok()
        }
        php_ir::IrConstant::StringBytes(value) => fast.publish_direct_string_bytes(value).ok(),
        php_ir::IrConstant::Array(values) => {
            let length = values.len();
            let mut values = values.iter();
            let mut next = 0_i64;
            fast.publish_owned_direct_array_with(length, |fast, _| {
                let entry = values
                    .next()
                    .ok_or("native IR constant array is truncated")?;
                let key = match &entry.key {
                    Some(php_ir::IrConstant::Int(value)) => fast.publish_direct_int(*value).ok(),
                    Some(php_ir::IrConstant::String(value)) => {
                        fast.publish_direct_string_bytes(value.as_bytes()).ok()
                    }
                    Some(php_ir::IrConstant::StringBytes(value)) => {
                        fast.publish_direct_string_bytes(value).ok()
                    }
                    Some(_) => None,
                    None => {
                        let key = fast.publish_direct_int(next).ok();
                        next = next.saturating_add(1);
                        key
                    }
                };
                let Some(key) = key else {
                    return Err("native IR constant key publication failed");
                };
                let Some(value) = publish_exact_ir_constant(fast, &entry.value) else {
                    let _ = fast.discard_owned_direct_value(key);
                    return Err("native IR constant value publication failed");
                };
                Ok(php_jit::JitNativeDirectArrayEntry { key, value })
            })
            .ok()
        }
        php_ir::IrConstant::NamedConstant(_) | php_ir::IrConstant::ClassConstant { .. } => None,
    }
}

fn publish_exact_named_constant(
    fast: &mut NativeRequestFastState,
    name: &str,
    depth: usize,
) -> Option<i64> {
    if depth > 32 {
        return None;
    }
    if let Some(value) = fast
        .symbol_query
        .native_constants()
        .and_then(|constants| constants.get(name))
        .copied()
    {
        fast.retain_direct_encoded(value).ok()?;
        return Some(value);
    }
    if let Some(value) = fast
        .symbol_query
        .active_compiled()
        .and_then(|compiled| {
            compiled
                .unit()
                .constant_table
                .iter()
                .find(|constant| constant.name == name)
                .and_then(|constant| compiled.unit().constants.get(constant.value.index()))
        })
        .map(std::ptr::from_ref)
    {
        // SAFETY: active compiled-unit storage is publication-stable for the
        // complete synchronous exact call.
        #[allow(unsafe_code)]
        return publish_exact_resolved_ir_constant(fast, unsafe { &*value }, depth + 1);
    }
    php_std::ExtensionRegistry::standard_library()
        .enabled_constant(name)
        .and_then(php_std::ConstantDescriptor::value)
        .and_then(|value| publish_exact_standard_constant(fast, value))
}

fn publish_exact_resolved_ir_constant(
    fast: &mut NativeRequestFastState,
    value: &php_ir::IrConstant,
    depth: usize,
) -> Option<i64> {
    if depth > 32 {
        return None;
    }
    match value {
        php_ir::IrConstant::NamedConstant(name) => {
            publish_exact_named_constant(fast, name, depth + 1)
        }
        php_ir::IrConstant::ClassConstant { .. } => None,
        php_ir::IrConstant::Array(values) => {
            let length = values.len();
            let mut values = values.iter();
            let mut next = 0_i64;
            fast.publish_owned_direct_array_with(length, |fast, _| {
                let entry = values
                    .next()
                    .ok_or("native resolved IR constant array is truncated")?;
                let key = match &entry.key {
                    Some(php_ir::IrConstant::Int(value)) => fast.publish_direct_int(*value).ok(),
                    Some(php_ir::IrConstant::String(value)) => {
                        fast.publish_direct_string_bytes(value.as_bytes()).ok()
                    }
                    Some(php_ir::IrConstant::StringBytes(value)) => {
                        fast.publish_direct_string_bytes(value).ok()
                    }
                    Some(php_ir::IrConstant::NamedConstant(name)) => {
                        publish_exact_named_constant(fast, name, depth + 1)
                    }
                    Some(php_ir::IrConstant::ClassConstant { .. })
                    | Some(php_ir::IrConstant::Array(_))
                    | Some(php_ir::IrConstant::Null)
                    | Some(php_ir::IrConstant::Bool(_))
                    | Some(php_ir::IrConstant::Float(_)) => None,
                    None => {
                        let key = fast.publish_direct_int(next).ok();
                        next = next.saturating_add(1);
                        key
                    }
                };
                let Some(key) = key else {
                    return Err("native resolved IR constant key publication failed");
                };
                let Some(value) = publish_exact_resolved_ir_constant(fast, &entry.value, depth + 1)
                else {
                    let _ = fast.discard_owned_direct_value(key);
                    return Err("native resolved IR constant value publication failed");
                };
                Ok(php_jit::JitNativeDirectArrayEntry { key, value })
            })
            .ok()
        }
        value => publish_exact_ir_constant(fast, value),
    }
}

fn exact_ir_constant_is_direct(value: &php_ir::IrConstant) -> bool {
    match value {
        php_ir::IrConstant::NamedConstant(_) | php_ir::IrConstant::ClassConstant { .. } => false,
        php_ir::IrConstant::Array(entries) => entries.iter().all(|entry| {
            entry.key.as_ref().is_none_or(|key| {
                matches!(
                    key,
                    php_ir::IrConstant::Int(_)
                        | php_ir::IrConstant::String(_)
                        | php_ir::IrConstant::StringBytes(_)
                )
            }) && exact_ir_constant_is_direct(&entry.value)
        }),
        _ => true,
    }
}

fn exact_standard_constants()
-> impl Iterator<Item = (&'static php_std::ConstantDescriptor, php_std::ConstantValue)> {
    let registry = php_std::ExtensionRegistry::standard_library();
    registry
        .extensions()
        .filter(move |extension| registry.is_extension_enabled(extension.name()))
        .flat_map(php_std::ExtensionDescriptor::constants)
        .filter_map(|constant| constant.value().map(|value| (constant, value)))
}

fn exact_standard_constant_named(name: &str) -> bool {
    exact_standard_constants().any(|(constant, _)| constant.name() == name)
}

fn exact_standard_category_constant_named(category: &str, name: &str) -> bool {
    exact_standard_constants().any(|(constant, _)| {
        php_constant_category(constant.extension()) == category && constant.name() == name
    })
}

fn exact_user_constant_source(
    fast: &NativeRequestFastState,
    name: &str,
) -> Option<ExactConstantInventorySource> {
    if let Some(value) = fast
        .symbol_query
        .native_constants()
        .and_then(|constants| constants.get(name))
        .copied()
    {
        return Some(ExactConstantInventorySource::BorrowedEncoded(value));
    }
    let compiled = fast.symbol_query.active_compiled()?;
    compiled
        .unit()
        .constant_table
        .iter()
        .rev()
        .filter(|constant| constant.name == name)
        .find_map(|constant| {
            let value = compiled.unit().constants.get(constant.value.index())?;
            exact_ir_constant_is_direct(value)
                .then(|| ExactConstantInventorySource::Ir(std::ptr::from_ref(value)))
        })
}

#[derive(Clone, Copy)]
enum ExactUserConstantFilter {
    NonStandard,
    CoreExtra,
    NonCore,
}

fn exact_user_constant_filter_matches(filter: ExactUserConstantFilter, name: &str) -> bool {
    match filter {
        ExactUserConstantFilter::NonStandard => !exact_standard_constant_named(name),
        ExactUserConstantFilter::CoreExtra => {
            php_core_runtime_constant(name) && !exact_standard_category_constant_named("Core", name)
        }
        ExactUserConstantFilter::NonCore => !php_core_runtime_constant(name),
    }
}

fn exact_user_constants(
    fast: &NativeRequestFastState,
    filter: ExactUserConstantFilter,
) -> impl Iterator<Item = ExactConstantInventoryEntry> + '_ {
    let dynamic_constants = fast.symbol_query.native_constants();
    let compiled = fast.symbol_query.active_compiled();
    let dynamic = dynamic_constants
        .into_iter()
        .flat_map(|constants| constants.iter())
        .filter(move |(name, _)| exact_user_constant_filter_matches(filter, name))
        .map(|(name, value)| {
            ExactConstantInventoryEntry::new(
                name,
                ExactConstantInventorySource::BorrowedEncoded(*value),
            )
        });
    let ir = compiled
        .into_iter()
        .flat_map(|compiled| compiled.unit().constant_table.iter().enumerate())
        .filter_map(move |(index, constant)| {
            if dynamic_constants.is_some_and(|constants| constants.contains_key(&constant.name)) {
                return None;
            }
            let compiled = compiled?;
            let value = compiled.unit().constants.get(constant.value.index())?;
            if !exact_ir_constant_is_direct(value)
                || compiled.unit().constant_table[..index]
                    .iter()
                    .any(|previous| {
                        previous.name == constant.name
                            && compiled
                                .unit()
                                .constants
                                .get(previous.value.index())
                                .is_some_and(exact_ir_constant_is_direct)
                    })
                || !exact_user_constant_filter_matches(filter, &constant.name)
            {
                return None;
            }
            let source = exact_user_constant_source(fast, &constant.name)?;
            Some(ExactConstantInventoryEntry::new(&constant.name, source))
        });
    dynamic.chain(ir)
}

fn exact_nth_standard_category(target: usize) -> Option<&'static str> {
    let registry = php_std::ExtensionRegistry::standard_library();
    let mut found = 0_usize;
    for (index, extension) in registry.extensions().enumerate() {
        if !registry.is_extension_enabled(extension.name())
            || !extension
                .constants()
                .iter()
                .any(|constant| constant.value().is_some())
        {
            continue;
        }
        let category = php_constant_category(extension.name());
        if category == "Core"
            || registry.extensions().take(index).any(|previous| {
                registry.is_extension_enabled(previous.name())
                    && previous
                        .constants()
                        .iter()
                        .any(|constant| constant.value().is_some())
                    && php_constant_category(previous.name()) == category
            })
        {
            continue;
        }
        if found == target {
            return Some(category);
        }
        found += 1;
    }
    None
}

fn exact_standard_category_entries(
    category: &str,
) -> impl Iterator<Item = ExactConstantInventoryEntry> + '_ {
    exact_standard_constants()
        .filter(move |(constant, _)| php_constant_category(constant.extension()) == category)
        .map(|(constant, value)| {
            ExactConstantInventoryEntry::new(
                constant.name(),
                ExactConstantInventorySource::Standard(value),
            )
        })
}

fn publish_exact_constant_inventory_entry(
    fast: &mut NativeRequestFastState,
    entry: ExactConstantInventoryEntry,
) -> Result<php_jit::JitNativeDirectArrayEntry, &'static str> {
    // SAFETY: entry names belong to the immutable standard registry, the
    // active compiled unit, or the request-stable dynamic-constant map.
    #[allow(unsafe_code)]
    let name = unsafe { std::slice::from_raw_parts(entry.name, entry.name_length) };
    let key = fast
        .publish_direct_string_bytes(name)
        .map_err(|_| "native constant inventory key publication failed")?;
    let value = match entry.source {
        ExactConstantInventorySource::Standard(value) => {
            publish_exact_standard_constant(fast, value)
        }
        ExactConstantInventorySource::Ir(value) => {
            // SAFETY: IR constants live in the publication-stable active unit.
            #[allow(unsafe_code)]
            publish_exact_ir_constant(fast, unsafe { &*value })
        }
        ExactConstantInventorySource::BorrowedEncoded(value) => {
            fast.retain_direct_encoded(value).ok().map(|()| value)
        }
    };
    let Some(value) = value else {
        let _ = fast.discard_owned_direct_value(key);
        return Err("native constant inventory value publication failed");
    };
    Ok(php_jit::JitNativeDirectArrayEntry { key, value })
}

#[derive(Clone, Copy)]
enum ExactConstantCategory {
    Core,
    Standard(&'static str),
    User,
}

fn publish_exact_constant_category(
    fast: &mut NativeRequestFastState,
    state: *const NativeRequestFastState,
    category: ExactConstantCategory,
) -> Result<i64, &'static str> {
    let standard_category = match category {
        ExactConstantCategory::Core => Some("Core"),
        ExactConstantCategory::Standard(category) => Some(category),
        ExactConstantCategory::User => None,
    };
    let standard_length = standard_category
        .map(|category| exact_standard_category_entries(category).count())
        .unwrap_or(0);
    let user_filter = match category {
        ExactConstantCategory::Core => Some(ExactUserConstantFilter::CoreExtra),
        ExactConstantCategory::Standard(_) => None,
        ExactConstantCategory::User => Some(ExactUserConstantFilter::NonCore),
    };
    let user_length = if let Some(filter) = user_filter {
        // SAFETY: state is the same live request fast state; the immutable
        // symbol capability fields are stable while disjoint result arenas mutate.
        #[allow(unsafe_code)]
        let state = unsafe { &*state };
        exact_user_constants(state, filter).count()
    } else {
        0
    };
    let length = standard_length
        .checked_add(user_length)
        .ok_or("native constant category length overflow")?;
    fast.publish_owned_direct_array_with(length, |fast, index| {
        let entry = if index < standard_length {
            let mut entry = exact_standard_category_entries(
                standard_category.ok_or("native constant category disappeared")?,
            )
            .nth(index)
            .ok_or("native standard constant category is truncated")?;
            if matches!(category, ExactConstantCategory::Core) {
                // SAFETY: see the stable-state contract above.
                #[allow(unsafe_code)]
                let state = unsafe { &*state };
                // SAFETY: the name pointer belongs to the static registry.
                #[allow(unsafe_code)]
                let name = unsafe {
                    std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                        entry.name,
                        entry.name_length,
                    ))
                };
                if php_core_runtime_constant(name)
                    && let Some(source) = exact_user_constant_source(state, name)
                {
                    entry.source = source;
                }
            }
            entry
        } else {
            let filter = user_filter.ok_or("native constant user category disappeared")?;
            // SAFETY: see the stable-state contract above.
            #[allow(unsafe_code)]
            exact_user_constants(unsafe { &*state }, filter)
                .nth(index - standard_length)
                .ok_or("native user constant category is truncated")?
        };
        publish_exact_constant_inventory_entry(fast, entry)
    })
}

fn publish_exact_uncategorized_constants(
    fast: &mut NativeRequestFastState,
    state: *const NativeRequestFastState,
) -> Result<i64, &'static str> {
    let standard_length = exact_standard_constants().count();
    // SAFETY: state addresses the same live request and only immutable symbol
    // capability fields are read while result arenas mutate.
    #[allow(unsafe_code)]
    let user_length =
        exact_user_constants(unsafe { &*state }, ExactUserConstantFilter::NonStandard).count();
    let length = standard_length
        .checked_add(user_length)
        .ok_or("native constant inventory length overflow")?;
    fast.publish_owned_direct_array_with(length, |fast, index| {
        let entry = if index < standard_length {
            let (constant, value) = exact_standard_constants()
                .nth(index)
                .ok_or("native standard constant inventory is truncated")?;
            // SAFETY: see the stable-state contract above.
            #[allow(unsafe_code)]
            let source = exact_user_constant_source(unsafe { &*state }, constant.name())
                .unwrap_or(ExactConstantInventorySource::Standard(value));
            ExactConstantInventoryEntry::new(constant.name(), source)
        } else {
            // SAFETY: see the stable-state contract above.
            #[allow(unsafe_code)]
            exact_user_constants(unsafe { &*state }, ExactUserConstantFilter::NonStandard)
                .nth(index - standard_length)
                .ok_or("native user constant inventory is truncated")?
        };
        publish_exact_constant_inventory_entry(fast, entry)
    })
}

pub(crate) extern "C" fn jit_native_get_defined_constants_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let categorized =
        if argument_0 == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
            false
        } else {
            let Some(value) = exact_native_boolean_flag(fast, argument_0) else {
                return exact_query_contract_violation();
            };
            value
        };
    if fast.symbol_query.active_compiled().is_none() {
        return exact_query_contract_violation();
    }
    let state = std::ptr::from_ref(fast);
    let result = if categorized {
        // SAFETY: state is the same live fast state and only stable symbol
        // capability fields are read while native result arenas mutate.
        #[allow(unsafe_code)]
        let core_length = exact_standard_category_entries("Core").count()
            + exact_user_constants(unsafe { &*state }, ExactUserConstantFilter::CoreExtra).count();
        let mut standard_category_count = 0_usize;
        while exact_nth_standard_category(standard_category_count).is_some() {
            standard_category_count += 1;
        }
        // SAFETY: see the stable-state contract above.
        #[allow(unsafe_code)]
        let user_length =
            exact_user_constants(unsafe { &*state }, ExactUserConstantFilter::NonCore).count();
        let category_count = usize::from(core_length != 0)
            .checked_add(standard_category_count)
            .and_then(|count| count.checked_add(usize::from(user_length != 0)));
        let Some(category_count) = category_count else {
            return exact_query_contract_violation();
        };
        fast.publish_owned_direct_array_with(category_count, |fast, index| {
            let (name, category) = if core_length != 0 && index == 0 {
                ("Core", ExactConstantCategory::Core)
            } else {
                let offset = index - usize::from(core_length != 0);
                if offset < standard_category_count {
                    let name = exact_nth_standard_category(offset)
                        .ok_or("native constant category inventory is truncated")?;
                    (name, ExactConstantCategory::Standard(name))
                } else if user_length != 0 && offset == standard_category_count {
                    ("user", ExactConstantCategory::User)
                } else {
                    return Err("native constant category inventory is truncated");
                }
            };
            let value = publish_exact_constant_category(fast, state, category)?;
            let key = match fast.publish_direct_string_bytes(name.as_bytes()) {
                Ok(key) => key,
                Err(error) => {
                    let _ = fast.discard_owned_direct_value(value);
                    return Err(error);
                }
            };
            Ok(php_jit::JitNativeDirectArrayEntry { key, value })
        })
    } else {
        publish_exact_uncategorized_constants(fast, state)
    };
    result.ok().map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

fn exact_compact_dereference(fast: &NativeRequestFastState, mut encoded: i64) -> Option<i64> {
    for _ in 0..16 {
        let Some((_, slot)) = fast.direct_slot(encoded) else {
            return Some(encoded);
        };
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            || slot.flags != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            || native_reference_state(slot.reserved)
                == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
        {
            return Some(encoded);
        }
        encoded = slot.payload as i64;
    }
    None
}

#[allow(unsafe_code)] // The visitor borrows stable request arenas while publishing into disjoint arenas.
fn exact_compact_visit_names(
    fast: *mut NativeRequestFastState,
    encoded: i64,
    depth: usize,
    visit: &mut impl FnMut(*mut NativeRequestFastState, &[u8]) -> Option<()>,
) -> Option<()> {
    if depth > 64 {
        return None;
    }
    let encoded = exact_compact_dereference(unsafe { &*fast }, encoded)?;
    if let Some((name, length)) = unsafe { &*fast }.stable_native_string_range(encoded) {
        let name = unsafe { std::slice::from_raw_parts(name, length) };
        visit(fast, name)?;
        return Some(());
    }
    let (entries, length) = unsafe { &*fast }.stable_native_array_range(encoded)?;
    for index in 0..length {
        let entry = unsafe { *entries.add(index) };
        exact_compact_visit_names(fast, entry.value, depth + 1, visit)?;
    }
    Some(())
}

#[allow(unsafe_code)] // Stable compiled metadata/input arenas stay live while the disjoint result arena mutates.
pub(crate) extern "C" fn jit_native_compact_abi(
    runtime: *mut NativeRequestFastState,
    function_metadata: i64,
    local_values: i64,
    compact_arguments: i64,
    compact_argument_count: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    // `-1` is the published frame-projection mode for get_defined_vars().
    // It reuses this one numeric ABI instead of adding a wrapper ABI around
    // the same local snapshot.
    let defined_vars = compact_argument_count == -1;
    let compact_argument_count = if defined_vars {
        0
    } else {
        let Ok(count) = usize::try_from(compact_argument_count) else {
            return exact_query_contract_violation();
        };
        count
    };
    // Safety: generated code loads this immutable pointer from the active
    // function contract published with the selected unit runtime view. The
    // owning CompiledUnit outlives every synchronous native invocation.
    #[allow(unsafe_code)]
    let function_metadata = unsafe {
        &*(function_metadata as usize
            as *const crate::compiled_unit::PreparedNativeFunctionMetadata)
    };
    let local_count = function_metadata.local_names.len();
    let local_names = function_metadata.local_names.as_ptr();
    let local_values = if local_count == 0 {
        &[]
    } else {
        let values = local_values as usize as *const i64;
        if values.is_null() {
            return exact_query_contract_violation();
        }
        // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
        #[allow(unsafe_code)]
        unsafe {
            std::slice::from_raw_parts(values, local_count)
        }
    };
    let compact_arguments = if compact_argument_count == 0 {
        &[]
    } else {
        let arguments = compact_arguments as usize as *const i64;
        if arguments.is_null() {
            return exact_query_contract_violation();
        }
        // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
        #[allow(unsafe_code)]
        unsafe {
            std::slice::from_raw_parts(arguments, compact_argument_count)
        }
    };
    let Some(mut writer) = fast
        .begin_owned_direct_array(4, php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY)
        .ok()
    else {
        return exact_query_contract_violation();
    };
    let fast = fast as *mut NativeRequestFastState;
    let published = 'publishing: {
        let mut publish_name = |fast: *mut NativeRequestFastState, name: &[u8]| {
            let fast = unsafe { &mut *fast };
            let index = (0..local_count).find(|index| {
                // SAFETY: compiled function metadata is immutable and request-stable.
                unsafe { (&*local_names.add(*index)).as_bytes() == name }
            })?;
            let value = *local_values.get(index)?;
            let value = if defined_vars {
                // get_defined_vars() exposes the symbol table and therefore
                // preserves shared ReferenceCell identity between returned
                // entries. compact() deliberately copies the dereferenced value.
                value
            } else {
                exact_compact_dereference(fast, value)?
            };
            if php_jit::jit_decode_constant(value) == Some(php_jit::JIT_VALUE_UNINITIALIZED) {
                return Some(false);
            }
            for index in 0..writer.len() {
                let entry = writer.get(index)?;
                let (existing, length) = fast.stable_native_string_range(entry.key)?;
                // SAFETY: unpublished keys are owned by the stable writer range.
                let existing = unsafe { std::slice::from_raw_parts(existing, length) };
                if existing == name {
                    return Some(true);
                }
            }
            let key = fast.publish_direct_string_bytes(name).ok()?;
            if fast.retain_direct_encoded(value).is_err() {
                let _ = fast.discard_owned_direct_value(key);
                return None;
            }
            if fast
                .push_owned_direct_array_entry(
                    &mut writer,
                    php_jit::JitNativeDirectArrayEntry { key, value },
                )
                .is_err()
            {
                let _ = fast.discard_owned_direct_value(value);
                let _ = fast.discard_owned_direct_value(key);
                return None;
            }
            Some(true)
        };
        if defined_vars {
            for index in 0..local_count {
                let name = unsafe { &*local_names.add(index) };
                if php_ir::is_compiler_generated_local_name(name) {
                    continue;
                }
                if publish_name(fast, name.as_bytes()).is_none() {
                    break 'publishing false;
                }
            }
        } else {
            let mut publish_compact_name = |fast, name: &[u8]| {
                publish_name(fast, name).and_then(|published| published.then_some(()))
            };
            for argument in compact_arguments {
                if exact_compact_visit_names(fast, *argument, 0, &mut publish_compact_name)
                    .is_none()
                {
                    break 'publishing false;
                }
            }
        }
        true
    };
    if !published {
        unsafe { &mut *fast }.abort_owned_direct_array(writer);
        return exact_query_contract_violation();
    }
    unsafe { &mut *fast }
        .finish_owned_direct_array(writer)
        .map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        )
}

pub(crate) extern "C" fn jit_native_func_num_args_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    fast.native_func_num_args().map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_func_get_arg_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(index) = fast
        .native_printf_scalar(argument_0)
        .and_then(|value| match value {
            php_runtime::api::NativePrintfScalar::Int(index) => usize::try_from(index).ok(),
            _ => None,
        })
    else {
        return exact_query_contract_violation();
    };
    match fast.native_func_get_arg(index) {
        Ok(Some(value)) => php_jit::JitNativeControlResult::returning(value),
        Ok(None) | Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_func_get_args_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    fast.native_func_get_args().map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_base64_decode_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(input) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let strict = if argument_1 != php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING)
    {
        let Some(strict) = exact_native_boolean_flag(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        strict
    } else {
        false
    };
    let Some(output_length) = php_runtime::api::native_base64_decode_output_length(input, strict)
    else {
        return exact_query_return_bool(false);
    };
    fast.publish_direct_string_transform(
        argument_0,
        |_| Some(output_length),
        |input, output| php_runtime::api::native_base64_decode_into(input, strict, output),
    )
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

fn exact_native_parsed_base_result(
    fast: &mut NativeRequestFastState,
    parsed: php_runtime::api::NativeParsedBaseNumber,
) -> php_jit::JitNativeControlResult {
    let encoded = match parsed {
        php_runtime::api::NativeParsedBaseNumber::Int(value) => fast.publish_direct_int(value),
        php_runtime::api::NativeParsedBaseNumber::Float(value) => fast.publish_direct_float(value),
    };
    encoded.map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

macro_rules! exact_native_parse_base_abi {
    ($abi:ident, $base:literal) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
        ) -> php_jit::JitNativeControlResult {
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            #[allow(unsafe_code)]
            let fast = unsafe { &mut *runtime };
            let Some(input) = fast.native_string_view(argument_0) else {
                return exact_query_contract_violation();
            };
            let Some(parsed) = php_runtime::api::native_parse_base_digits(input, $base) else {
                return exact_query_contract_violation();
            };
            exact_native_parsed_base_result(fast, parsed)
        }
    };
}

exact_native_parse_base_abi!(jit_native_bindec_abi, 2);
exact_native_parse_base_abi!(jit_native_hexdec_abi, 16);
exact_native_parse_base_abi!(jit_native_octdec_abi, 8);

macro_rules! exact_native_decimal_to_base_abi {
    ($abi:ident, $base:literal) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
        ) -> php_jit::JitNativeControlResult {
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            #[allow(unsafe_code)]
            let fast = unsafe { &mut *runtime };
            let Some(value) = exact_native_integer(fast, argument_0) else {
                return exact_query_contract_violation();
            };
            let Some(conversion) = php_runtime::api::native_decimal_base_conversion(value, $base)
            else {
                return exact_query_contract_violation();
            };
            fast.try_publish_direct_string_with(conversion.output_length(), |output| {
                conversion
                    .write_into(output)
                    .then_some(())
                    .ok_or("native decimal base conversion length mismatch")
            })
            .map_or_else(
                |_| exact_query_contract_violation(),
                php_jit::JitNativeControlResult::returning,
            )
        }
    };
}

exact_native_decimal_to_base_abi!(jit_native_decbin_abi, 2);
exact_native_decimal_to_base_abi!(jit_native_dechex_abi, 16);
exact_native_decimal_to_base_abi!(jit_native_decoct_abi, 8);

pub(crate) extern "C" fn jit_native_base_convert_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let (Some(from_base), Some(to_base)) = (
        exact_native_integer(fast, argument_1).and_then(|base| u32::try_from(base).ok()),
        exact_native_integer(fast, argument_2).and_then(|base| u32::try_from(base).ok()),
    ) else {
        return exact_query_contract_violation();
    };
    let Some(input) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(conversion) = php_runtime::api::native_base_conversion(input, from_base, to_base)
    else {
        return exact_query_contract_violation();
    };
    fast.try_publish_direct_string_with(conversion.output_length(), |output| {
        conversion
            .write_into(output)
            .then_some(())
            .ok_or("native base conversion length mismatch")
    })
    .map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_ip2long_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(address) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(value) = php_runtime::api::native_ip2long(address) else {
        return exact_query_return_bool(false);
    };
    fast.publish_direct_int(value).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_long2ip_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(address) = exact_native_integer(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let output = php_runtime::api::native_long2ip(address);
    fast.publish_direct_string_bytes(output.as_bytes())
        .map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        )
}

macro_rules! exact_native_network_string_abi {
    ($abi:ident, $operation:path) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
        ) -> php_jit::JitNativeControlResult {
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            #[allow(unsafe_code)]
            let fast = unsafe { &mut *runtime };
            let Some(input) = fast.native_string_view(argument_0) else {
                return exact_query_contract_violation();
            };
            let Some(output) = $operation(input) else {
                return exact_query_return_bool(false);
            };
            fast.publish_direct_string_bytes(output.as_bytes())
                .map_or_else(
                    |_| exact_query_contract_violation(),
                    php_jit::JitNativeControlResult::returning,
                )
        }
    };
}

exact_native_network_string_abi!(jit_native_inet_pton_abi, php_runtime::api::native_inet_pton);
exact_native_network_string_abi!(jit_native_inet_ntop_abi, php_runtime::api::native_inet_ntop);

fn exact_native_max_length(fast: &NativeRequestFastState, encoded: i64) -> Option<Option<usize>> {
    if encoded == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        return Some(None);
    }
    let length = exact_native_integer(fast, encoded)?;
    Some((length > 0).then_some(length as usize))
}

macro_rules! exact_native_compression_encode_abi {
    ($abi:ident, $encoding:expr) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
            _argument_2: i64,
        ) -> php_jit::JitNativeControlResult {
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            #[allow(unsafe_code)]
            let fast = unsafe { &mut *runtime };
            let level = if argument_1
                != php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING)
            {
                let Some(level) = exact_native_integer(fast, argument_1) else {
                    return exact_query_contract_violation();
                };
                level
            } else {
                -1
            };
            let Some((input, input_length)) = fast.stable_native_string_range(argument_0) else {
                return exact_query_contract_violation();
            };
            let Some(output_capacity) =
                php_runtime::api::native_zlib_encode_output_capacity(input_length, $encoding)
            else {
                return exact_query_return_bool(false);
            };
            match fast.try_publish_direct_string_with_capacity(output_capacity, |output| {
                // SAFETY: the input owner remains live for this synchronous
                // exact call and native arena reservations never relocate it.
                #[allow(unsafe_code)]
                let input = unsafe { std::slice::from_raw_parts(input, input_length) };
                php_runtime::api::native_zlib_encode_into(input, $encoding, level, output).ok_or(())
            }) {
                Ok(encoded) => php_jit::JitNativeControlResult::returning(encoded),
                Err(NativeDirectStringPublishError::Fill(())) => exact_query_return_bool(false),
                Err(NativeDirectStringPublishError::Arena(_)) => exact_query_contract_violation(),
            }
        }
    };
}

exact_native_compression_encode_abi!(
    jit_native_gzencode_abi,
    php_runtime::api::ZLIB_ENCODING_GZIP
);
exact_native_compression_encode_abi!(
    jit_native_gzcompress_abi,
    php_runtime::api::ZLIB_ENCODING_DEFLATE
);
exact_native_compression_encode_abi!(
    jit_native_gzdeflate_abi,
    php_runtime::api::ZLIB_ENCODING_RAW
);

macro_rules! exact_native_compression_decode_abi {
    ($abi:ident, $encoding:expr) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
        ) -> php_jit::JitNativeControlResult {
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            #[allow(unsafe_code)]
            let fast = unsafe { &mut *runtime };
            let Some(max_length) = exact_native_max_length(fast, argument_1) else {
                return exact_query_contract_violation();
            };
            let Some((input, input_length)) = fast.stable_native_string_range(argument_0) else {
                return exact_query_contract_violation();
            };
            // SAFETY: validation borrows the stable source only until the
            // immutable decode plan has been produced.
            #[allow(unsafe_code)]
            let input_bytes = unsafe { std::slice::from_raw_parts(input, input_length) };
            let Some(plan) =
                php_runtime::api::native_zlib_decode(input_bytes, $encoding, max_length)
            else {
                return exact_query_contract_violation();
            };
            fast.try_publish_direct_string_with(plan.output_length(), |output| {
                // SAFETY: direct string publication cannot relocate the
                // source range during this synchronous second decode pass.
                #[allow(unsafe_code)]
                let input = unsafe { std::slice::from_raw_parts(input, input_length) };
                plan.write_into(input, output)
                    .then_some(())
                    .ok_or("native zlib decode changed after validation")
            })
            .map_or_else(
                |_| exact_query_contract_violation(),
                php_jit::JitNativeControlResult::returning,
            )
        }
    };
}

exact_native_compression_decode_abi!(
    jit_native_gzdecode_abi,
    php_runtime::api::ZLIB_ENCODING_GZIP
);
exact_native_compression_decode_abi!(
    jit_native_gzuncompress_abi,
    php_runtime::api::ZLIB_ENCODING_DEFLATE
);
exact_native_compression_decode_abi!(
    jit_native_gzinflate_abi,
    php_runtime::api::ZLIB_ENCODING_RAW
);

pub(crate) extern "C" fn jit_native_zlib_decode_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(max_length) = exact_native_max_length(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    let Some((input, input_length)) = fast.stable_native_string_range(argument_0) else {
        return exact_query_contract_violation();
    };
    // SAFETY: validation borrows the stable source only until the immutable
    // decode plan has been produced.
    #[allow(unsafe_code)]
    let input_bytes = unsafe { std::slice::from_raw_parts(input, input_length) };
    let Some(plan) = php_runtime::api::native_zlib_decode_auto(input_bytes, max_length) else {
        return exact_query_contract_violation();
    };
    fast.try_publish_direct_string_with(plan.output_length(), |output| {
        // SAFETY: direct string publication cannot relocate the source range
        // during this synchronous second decode pass.
        #[allow(unsafe_code)]
        let input = unsafe { std::slice::from_raw_parts(input, input_length) };
        plan.write_into(input, output)
            .then_some(())
            .ok_or("native zlib auto-decode changed after validation")
    })
    .map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_zlib_encode_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let (Some(encoding), Some(level)) = (
        exact_native_integer(fast, argument_1),
        if argument_2 != php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
            exact_native_integer(fast, argument_2)
        } else {
            Some(-1)
        },
    ) else {
        return exact_query_contract_violation();
    };
    let Some((input, input_length)) = fast.stable_native_string_range(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(output_capacity) =
        php_runtime::api::native_zlib_encode_output_capacity(input_length, encoding)
    else {
        return exact_query_return_bool(false);
    };
    match fast.try_publish_direct_string_with_capacity(output_capacity, |output| {
        // SAFETY: the input owner remains live for this synchronous exact
        // call and native arena reservations never relocate it.
        #[allow(unsafe_code)]
        let input = unsafe { std::slice::from_raw_parts(input, input_length) };
        php_runtime::api::native_zlib_encode_into(input, encoding, level, output).ok_or(())
    }) {
        Ok(encoded) => php_jit::JitNativeControlResult::returning(encoded),
        Err(NativeDirectStringPublishError::Fill(())) => exact_query_return_bool(false),
        Err(NativeDirectStringPublishError::Arena(_)) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_basename_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: exact handlers synchronously borrow the active request's
    // published FastState and never retain a borrowed view.
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some((path, path_length)) = fast.stable_native_string_range(argument_0) else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let suffix = if argument_1 == missing {
        None
    } else {
        let Some((suffix, suffix_length)) = fast.stable_native_string_range(argument_1) else {
            return exact_query_contract_violation();
        };
        // SAFETY: exact-call string owners remain stable for this synchronous
        // byte-range calculation.
        #[allow(unsafe_code)]
        Some(unsafe { std::slice::from_raw_parts(suffix, suffix_length) })
    };
    // SAFETY: the source owner remains live while the immutable path plan is
    // calculated.
    #[allow(unsafe_code)]
    let path_bytes = unsafe { std::slice::from_raw_parts(path, path_length) };
    let output = php_runtime::api::native_basename(path_bytes, suffix);
    fast.try_publish_direct_string_with(output.output_length(), |destination| {
        // SAFETY: native arena publication never relocates the stable source
        // owner during this synchronous copy.
        #[allow(unsafe_code)]
        let path = unsafe { std::slice::from_raw_parts(path, path_length) };
        output
            .write_into(path, destination)
            .then_some(())
            .ok_or("native basename plan no longer matches its source")
    })
    .map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_dirname_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some((path, path_length)) = fast.stable_native_string_range(argument_0) else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let levels = if argument_1 != missing {
        let Some(php_runtime::api::NativePrintfScalar::Int(levels)) =
            fast.native_printf_scalar(argument_1)
        else {
            return exact_query_contract_violation();
        };
        levels
    } else {
        1
    };
    // SAFETY: the source owner remains live while the immutable path plan is
    // calculated.
    #[allow(unsafe_code)]
    let path_bytes = unsafe { std::slice::from_raw_parts(path, path_length) };
    let output = php_runtime::api::native_dirname(path_bytes, levels);
    fast.try_publish_direct_string_with(output.output_length(), |destination| {
        // SAFETY: native arena publication never relocates the stable source
        // owner during this synchronous copy.
        #[allow(unsafe_code)]
        let path = unsafe { std::slice::from_raw_parts(path, path_length) };
        output
            .write_into(path, destination)
            .then_some(())
            .ok_or("native dirname plan no longer matches its source")
    })
    .map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_pathinfo_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some((path, path_length)) = fast.stable_native_string_range(argument_0) else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let flags = if argument_1 == missing {
        None
    } else {
        let Some(flags) = exact_native_integer(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        Some(flags)
    };
    // SAFETY: the encoded path remains owned for this synchronous parse while
    // the structured publisher writes only to disjoint request arenas.
    #[allow(unsafe_code)]
    let path = unsafe { std::slice::from_raw_parts(path, path_length) };
    php_runtime::api::native_pathinfo_into(path, flags, fast).map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

fn publish_exact_stat_record(
    fast: &mut NativeRequestFastState,
    record: php_runtime::api::NativeStatRecord,
) -> Option<i64> {
    let fields = [
        (ExactStatKey::Int(2), ExactStatField::Int(record.mode)),
        (ExactStatKey::Int(7), ExactStatField::Int(record.size)),
        (ExactStatKey::Int(9), ExactStatField::Int(record.mtime)),
        (
            ExactStatKey::String(b"mode"),
            ExactStatField::Int(record.mode),
        ),
        (
            ExactStatKey::String(b"size"),
            ExactStatField::Int(record.size),
        ),
        (
            ExactStatKey::String(b"mtime"),
            ExactStatField::Int(record.mtime),
        ),
        (
            ExactStatKey::String(b"type"),
            ExactStatField::String(record.file_type),
        ),
    ];
    let length = fields.len();
    let mut fields = fields.into_iter();
    fast.publish_owned_direct_array_with(length, |fast, _| {
        let (key, value) = fields.next().ok_or("native stat record is truncated")?;
        let key = match key {
            ExactStatKey::Int(value) => fast.publish_direct_int(value),
            ExactStatKey::String(value) => fast.publish_direct_string_bytes(value),
        }?;
        let value = match value {
            ExactStatField::Int(value) => fast.publish_direct_int(value).ok(),
            ExactStatField::String(value) => fast.publish_direct_string_bytes(value).ok(),
        };
        let Some(value) = value else {
            let _ = fast.discard_owned_direct_value(key);
            return Err("native stat value publication failed");
        };
        Ok(php_jit::JitNativeDirectArrayEntry { key, value })
    })
    .ok()
}

enum ExactStatKey {
    Int(i64),
    String(&'static [u8]),
}

enum ExactStatField {
    Int(i64),
    String(&'static [u8]),
}

macro_rules! exact_native_stat_abi {
    ($abi:ident, $follow_links:expr) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
        ) -> php_jit::JitNativeControlResult {
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            #[allow(unsafe_code)]
            let fast = unsafe { &mut *runtime };
            let Some(path) = fast.native_string_view(argument_0) else {
                return exact_query_contract_violation();
            };
            let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
                return exact_query_contract_violation();
            };
            match php_runtime::api::native_stat(cwd, filesystem, path, $follow_links) {
                Some(Some(record)) => publish_exact_stat_record(fast, record).map_or_else(
                    exact_query_contract_violation,
                    php_jit::JitNativeControlResult::returning,
                ),
                Some(None) => exact_query_return_bool(false),
                None => exact_query_contract_violation(),
            }
        }
    };
}

exact_native_stat_abi!(jit_native_stat_abi, true);
exact_native_stat_abi!(jit_native_lstat_abi, false);

pub(crate) extern "C" fn jit_native_file_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let flags = if argument_1 == missing {
        0
    } else {
        let Some(flags) = exact_native_integer(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        flags
    };
    if argument_2 != missing
        && !matches!(
            fast.native_printf_scalar(argument_2),
            Some(php_runtime::api::NativePrintfScalar::Null)
        )
    {
        return exact_query_contract_violation();
    }
    let Some(lines) = fast.native_file_lines_direct(argument_0, flags) else {
        return exact_query_contract_violation();
    };
    php_jit::JitNativeControlResult::returning(lines)
}

pub(crate) extern "C" fn jit_native_glob_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    if argument_1 != missing && exact_native_integer(fast, argument_1) != Some(0) {
        return exact_query_contract_violation();
    }
    match fast.native_glob_direct(argument_0) {
        Some(php_runtime::api::NativeGlobPublished::Matches(paths)) => {
            php_jit::JitNativeControlResult::returning(paths)
        }
        Some(php_runtime::api::NativeGlobPublished::False) => exact_query_return_bool(false),
        None => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_opendir_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(path) = fast.native_string_view(argument_0).map(<[u8]>::to_vec) else {
        return exact_query_contract_violation();
    };
    let resource = {
        let Some((resources, cwd, filesystem)) = fast.native_directory_capability() else {
            return exact_query_contract_violation();
        };
        match php_runtime::api::native_directory_entries(cwd, filesystem, &path) {
            Some(Some((resolved, entries))) => {
                let uri = resolved.to_string_lossy().into_owned();
                resources.register_directory(resolved, entries, uri)
            }
            Some(None) => return exact_query_return_bool(false),
            None => return exact_query_contract_violation(),
        }
    };
    match fast.publish_direct_resource(resource.clone()) {
        Ok(encoded) => php_jit::JitNativeControlResult::returning(encoded),
        Err(_) => {
            resource.close();
            exact_query_contract_violation()
        }
    }
}

pub(crate) extern "C" fn jit_native_readdir_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0).cloned() else {
        return exact_query_contract_violation();
    };
    let Some(checkpoint) = resource.native_directory_cursor_checkpoint() else {
        return exact_query_contract_violation();
    };
    match resource.read_dir_entry() {
        Ok(Some(entry)) => match fast.publish_direct_string_bytes(entry.as_bytes()) {
            Ok(encoded) => php_jit::JitNativeControlResult::returning(encoded),
            Err(_) if resource.restore_native_directory_cursor(checkpoint) => {
                exact_query_contract_violation()
            }
            Err(_) => {
                debug_assert!(false, "native directory cursor rollback failed");
                exact_query_return_bool(false)
            }
        },
        Ok(None) => exact_query_return_bool(false),
        Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_rewinddir_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0).cloned() else {
        return exact_query_contract_violation();
    };
    match resource.rewind_dir() {
        Ok(()) => {
            php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX))
        }
        Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_closedir_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0).cloned() else {
        return exact_query_contract_violation();
    };
    if !resource.is_user_closable() {
        return exact_query_contract_violation();
    }
    resource.close();
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX))
}

pub(crate) extern "C" fn jit_native_scandir_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let order = if argument_1 == missing {
        0
    } else {
        let Some(order) = exact_native_integer(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        order
    };
    if !matches!(order, 0 | 1) {
        return exact_query_contract_violation();
    }
    match fast.native_scandir_direct(argument_0, order == 1) {
        Some(php_runtime::api::NativeGlobPublished::Matches(entries)) => {
            php_jit::JitNativeControlResult::returning(entries)
        }
        Some(php_runtime::api::NativeGlobPublished::False) => exact_query_return_bool(false),
        None => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_stream_get_wrappers_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    publish_exact_string_list(fast, 2, [b"file".as_slice(), b"php".as_slice()]).map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_stream_get_meta_data_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0).cloned() else {
        return exact_query_contract_violation();
    };
    let metadata = resource.metadata();
    let flags = resource.flags();
    let fields = [
        (
            b"wrapper_type".as_slice(),
            Some(metadata.wrapper_type.as_bytes()),
            None,
        ),
        (
            b"stream_type".as_slice(),
            Some(metadata.stream_type.as_bytes()),
            None,
        ),
        (b"mode".as_slice(), Some(metadata.mode.as_bytes()), None),
        (b"uri".as_slice(), Some(metadata.uri.as_bytes()), None),
        (b"seekable".as_slice(), None, Some(flags.seekable)),
        (
            b"eof".as_slice(),
            None,
            Some(resource.eof().unwrap_or(true)),
        ),
        (b"timed_out".as_slice(), None, Some(false)),
        (b"blocked".as_slice(), None, Some(true)),
    ];
    fast.publish_owned_direct_array_with(fields.len(), |fast, index| {
        let (key, string, boolean) = fields[index];
        let key = fast
            .publish_direct_string_bytes(key)
            .map_err(|_| "native stream metadata key publication failed")?;
        let value = if let Some(string) = string {
            fast.publish_direct_string_bytes(string).ok()
        } else {
            boolean.map(|value| {
                php_jit::jit_encode_constant(if value {
                    php_jit::JIT_VALUE_TRUE
                } else {
                    php_jit::JIT_VALUE_FALSE
                })
            })
        };
        let Some(value) = value else {
            let _ = fast.discard_owned_direct_value(key);
            return Err("native stream metadata value publication failed");
        };
        Ok(php_jit::JitNativeDirectArrayEntry { key, value })
    })
    .map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_stream_is_local_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if let Some(resource) = fast.native_resource_view(argument_0) {
        return native_bool_result(matches!(
            resource.metadata().wrapper_type.as_str(),
            "plainfile" | "PHP"
        ));
    }
    let Some((path, path_length)) = fast.stable_native_string_range(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
        return exact_query_contract_violation();
    };
    #[allow(unsafe_code)]
    let path = unsafe { std::slice::from_raw_parts(path, path_length) };
    native_bool_result(php_runtime::api::native_stream_is_local(
        cwd, filesystem, path,
    ))
}

pub(crate) extern "C" fn jit_native_stream_resolve_include_path_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some((path, path_length)) = fast.stable_native_string_range(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
        return exact_query_contract_violation();
    };
    let include_path = Arc::clone(fast.configuration.include_path());
    #[allow(unsafe_code)]
    let path = unsafe { std::slice::from_raw_parts(path, path_length) };
    match php_runtime::api::native_stream_resolve_include_path(
        cwd,
        filesystem,
        include_path.as_slice(),
        path,
    ) {
        Some(Some(resolved)) => fast.publish_direct_string_bytes(&resolved).map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        ),
        Some(None) => exact_query_return_bool(false),
        None => exact_query_contract_violation(),
    }
}

fn publish_native_stream_context(
    fast: &mut NativeRequestFastState,
    options: i64,
) -> Result<i64, &'static str> {
    let resource = fast
        .native_stream_context_resources()
        .ok_or("native stream context resource table is unavailable")?
        .register_native_stream_context();
    let encoded = match fast.publish_direct_resource(resource.clone()) {
        Ok(encoded) => encoded,
        Err(error) => {
            let _ = fast.discard_owned_direct_value(options);
            resource.close();
            return Err(error);
        }
    };
    if let Err(error) = fast.insert_native_stream_context_resource_owned(&resource, options) {
        let _ = fast.discard_owned_direct_value(encoded);
        let _ = fast.discard_owned_direct_value(options);
        resource.close();
        return Err(error);
    }
    Ok(encoded)
}

pub(crate) extern "C" fn jit_native_stream_context_create_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let options = if argument_0 == missing {
        fast.publish_owned_direct_array_with(0, |_, _| {
            unreachable!("zero-length native array builder")
        })
    } else {
        fast.duplicate_native_stream_context_array(argument_0)
    };
    let Ok(options) = options else {
        return exact_query_contract_violation();
    };
    publish_native_stream_context(fast, options).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_stream_context_get_default_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let requested = (argument_0 != missing).then_some(argument_0);
    let default_owner = if let Some(requested) = requested {
        match fast.duplicate_native_stream_context_array(requested) {
            Ok(options) => options,
            Err(_) => return exact_query_contract_violation(),
        }
    } else {
        let Some(default) = fast.native_stream_context_default_options() else {
            return exact_query_contract_violation();
        };
        match fast.duplicate_native_stream_context_array(default) {
            Ok(options) => options,
            Err(_) => return exact_query_contract_violation(),
        }
    };
    let resource_owner = match fast.duplicate_native_stream_context_array(default_owner) {
        Ok(options) => options,
        Err(_) => {
            let _ = fast.discard_owned_direct_value(default_owner);
            return exact_query_contract_violation();
        }
    };
    let resource = match publish_native_stream_context(fast, resource_owner) {
        Ok(resource) => resource,
        Err(_) => {
            let _ = fast.discard_owned_direct_value(default_owner);
            return exact_query_contract_violation();
        }
    };
    if requested.is_some()
        && fast
            .replace_native_stream_context_default_owned(default_owner)
            .is_err()
    {
        return exact_query_return_bool(false);
    }
    if requested.is_none() {
        let _ = fast.discard_owned_direct_value(default_owner);
    }
    php_jit::JitNativeControlResult::returning(resource)
}

pub(crate) extern "C" fn jit_native_stream_context_get_options_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0).cloned() else {
        return exact_query_contract_violation();
    };
    let Some(options) = fast.native_stream_context_resource_options(&resource) else {
        return exact_query_contract_violation();
    };
    fast.duplicate_native_stream_context_array(options)
        .map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        )
}

pub(crate) extern "C" fn jit_native_stream_context_set_default_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    jit_native_stream_context_get_default_abi(runtime, argument_0)
}

fn merge_native_stream_context_resource(
    fast: &mut NativeRequestFastState,
    resource: &php_runtime::api::ResourceRef,
    additions: i64,
) -> Result<(), &'static str> {
    let current = fast
        .native_stream_context_resource_options(resource)
        .ok_or("native stream context resource options are unavailable")?;
    let merged = fast.merge_native_stream_context_options(current, additions)?;
    fast.insert_native_stream_context_resource_owned(resource, merged)
}

pub(crate) extern "C" fn jit_native_stream_context_set_options_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0).cloned() else {
        return exact_query_contract_violation();
    };
    if fast.native_stream_context_array(argument_1).is_none() {
        return exact_query_contract_violation();
    }
    match merge_native_stream_context_resource(fast, &resource, argument_1) {
        Ok(()) => exact_query_return_bool(true),
        Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_stream_context_set_option_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0).cloned() else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    if argument_2 == missing && argument_3 == missing {
        if fast.native_stream_context_array(argument_1).is_none() {
            return exact_query_contract_violation();
        }
        return match merge_native_stream_context_resource(fast, &resource, argument_1) {
            Ok(()) => exact_query_return_bool(true),
            Err(_) => exact_query_contract_violation(),
        };
    }
    let Some((wrapper, wrapper_length)) = fast.stable_native_string_range(argument_1) else {
        return exact_query_contract_violation();
    };
    let Some((option, option_length)) = fast.stable_native_string_range(argument_2) else {
        return exact_query_contract_violation();
    };
    let Ok(value) = fast.duplicate_native_stream_context_value(argument_3) else {
        return exact_query_contract_violation();
    };
    let Some(current) = fast.native_stream_context_resource_options(&resource) else {
        let _ = fast.discard_owned_direct_value(value);
        return exact_query_contract_violation();
    };
    #[allow(unsafe_code)]
    let wrapper = unsafe { std::slice::from_raw_parts(wrapper, wrapper_length) };
    #[allow(unsafe_code)]
    let option = unsafe { std::slice::from_raw_parts(option, option_length) };
    let updated = fast.native_stream_context_set_named_option(current, wrapper, option, value);
    let _ = fast.discard_owned_direct_value(value);
    let Ok(updated) = updated else {
        return exact_query_contract_violation();
    };
    match fast.insert_native_stream_context_resource_owned(&resource, updated) {
        Ok(()) => exact_query_return_bool(true),
        Err(_) => exact_query_return_bool(false),
    }
}

fn attach_native_stream_filter(
    fast: &mut NativeRequestFastState,
    stream: i64,
    filter_name: i64,
    mode: i64,
    prepend: bool,
) -> php_jit::JitNativeControlResult {
    let Some(stream) = fast.native_resource_view(stream).cloned() else {
        return exact_query_contract_violation();
    };
    let Some(filter_name) = fast.native_string_view(filter_name) else {
        return exact_query_contract_violation();
    };
    let filter_name = String::from_utf8_lossy(filter_name).into_owned();
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let mode = if mode == missing {
        0
    } else {
        let Some(mode) = exact_native_integer(fast, mode) else {
            return exact_query_contract_violation();
        };
        mode
    };
    let Some(mode) = php_runtime::api::StreamFilterMode::from_php(mode) else {
        return exact_query_contract_violation();
    };
    let filter = match fast
        .native_stream_context_resources()
        .and_then(|resources| {
            resources
                .register_stream_filter(&stream, filter_name, mode, prepend)
                .ok()
        }) {
        Some(Some(filter)) => filter,
        Some(None) | None => return exact_query_contract_violation(),
    };
    match fast.publish_direct_resource(filter.clone()) {
        Ok(encoded) => php_jit::JitNativeControlResult::returning(encoded),
        Err(_) => {
            filter.remove_stream_filter_resource();
            exact_query_contract_violation()
        }
    }
}

pub(crate) extern "C" fn jit_native_stream_filter_append_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    _argument_3: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    attach_native_stream_filter(fast, argument_0, argument_1, argument_2, false)
}

pub(crate) extern "C" fn jit_native_stream_filter_prepend_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    _argument_3: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    attach_native_stream_filter(fast, argument_0, argument_1, argument_2, true)
}

pub(crate) extern "C" fn jit_native_stream_filter_remove_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(filter) = fast.native_resource_view(argument_0).cloned() else {
        return exact_query_contract_violation();
    };
    exact_query_return_bool(filter.remove_stream_filter_resource())
}

pub(crate) extern "C" fn jit_native_stream_isatty_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if fast.native_resource_view(argument_0).is_none() {
        return exact_query_contract_violation();
    }
    exact_query_return_bool(false)
}

pub(crate) extern "C" fn jit_native_stream_set_timeout_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if fast.native_resource_view(argument_0).is_none() {
        return exact_query_contract_violation();
    }
    let Some(seconds) = exact_native_integer(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let microseconds = if argument_2 == missing {
        0
    } else {
        let Some(microseconds) = exact_native_integer(fast, argument_2) else {
            return exact_query_contract_violation();
        };
        microseconds
    };
    if seconds < 0 || microseconds < 0 {
        return exact_query_return_bool(false);
    }
    exact_query_return_bool(false)
}

pub(crate) extern "C" fn jit_native_realpath_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(path) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
        return exact_query_contract_violation();
    };
    let Some(bytes) = php_runtime::api::native_realpath(cwd, filesystem, path) else {
        return exact_query_return_bool(false);
    };
    fast.publish_direct_string_bytes(&bytes).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

macro_rules! exact_native_boolean_path_abi {
    ($abi:ident, $native:path) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
        ) -> php_jit::JitNativeControlResult {
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            #[allow(unsafe_code)]
            let fast = unsafe { &mut *runtime };
            let Some(path) = fast.native_string_view(argument_0) else {
                return exact_query_contract_violation();
            };
            let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
                return exact_query_contract_violation();
            };
            match $native(cwd, filesystem, path) {
                Some(value) => exact_query_return_bool(value),
                None => exact_query_contract_violation(),
            }
        }
    };
}

exact_native_boolean_path_abi!(
    jit_native_file_exists_abi,
    php_runtime::api::native_file_exists
);
exact_native_boolean_path_abi!(jit_native_is_file_abi, php_runtime::api::native_is_file);
exact_native_boolean_path_abi!(jit_native_is_dir_abi, php_runtime::api::native_is_dir);
exact_native_boolean_path_abi!(
    jit_native_is_readable_abi,
    php_runtime::api::native_is_readable
);
exact_native_boolean_path_abi!(
    jit_native_is_writable_abi,
    php_runtime::api::native_is_writable
);
exact_native_boolean_path_abi!(jit_native_is_link_abi, php_runtime::api::native_is_link);

macro_rules! exact_native_integer_path_abi {
    ($abi:ident, $native:path) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
        ) -> php_jit::JitNativeControlResult {
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            #[allow(unsafe_code)]
            let fast = unsafe { &mut *runtime };
            let Some(path) = fast.native_string_view(argument_0) else {
                return exact_query_contract_violation();
            };
            let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
                return exact_query_contract_violation();
            };
            match $native(cwd, filesystem, path) {
                Some(Some(value)) => fast.publish_direct_int(value).map_or_else(
                    |_| exact_query_contract_violation(),
                    php_jit::JitNativeControlResult::returning,
                ),
                Some(None) => exact_query_return_bool(false),
                None => exact_query_contract_violation(),
            }
        }
    };
}

exact_native_integer_path_abi!(jit_native_filesize_abi, php_runtime::api::native_filesize);
exact_native_integer_path_abi!(jit_native_filemtime_abi, php_runtime::api::native_filemtime);
exact_native_integer_path_abi!(jit_native_fileperms_abi, php_runtime::api::native_fileperms);
exact_native_integer_path_abi!(jit_native_fileowner_abi, php_runtime::api::native_fileowner);
exact_native_integer_path_abi!(jit_native_filegroup_abi, php_runtime::api::native_filegroup);

pub(crate) extern "C" fn jit_native_filetype_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(path) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
        return exact_query_contract_violation();
    };
    match php_runtime::api::native_filetype(cwd, filesystem, path) {
        Some(Some(bytes)) => fast.publish_direct_string_bytes(bytes).map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        ),
        Some(None) => exact_query_return_bool(false),
        None => exact_query_contract_violation(),
    }
}

macro_rules! exact_native_disk_space_abi {
    ($abi:ident) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
        ) -> php_jit::JitNativeControlResult {
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            #[allow(unsafe_code)]
            let fast = unsafe { &mut *runtime };
            let Some(path) = fast.native_string_view(argument_0) else {
                return exact_query_contract_violation();
            };
            let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
                return exact_query_contract_violation();
            };
            match php_runtime::api::native_disk_space(cwd, filesystem, path) {
                Some(Some(value)) => fast.publish_direct_float(value).map_or_else(
                    |_| exact_query_contract_violation(),
                    php_jit::JitNativeControlResult::returning,
                ),
                Some(None) => exact_query_return_bool(false),
                None => exact_query_contract_violation(),
            }
        }
    };
}

exact_native_disk_space_abi!(jit_native_disk_free_space_abi);
exact_native_disk_space_abi!(jit_native_disk_total_space_abi);

pub(crate) extern "C" fn jit_native_file_get_contents_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
    argument_4: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(path) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    if argument_1 != missing
        && !matches!(
            fast.native_printf_scalar(argument_1),
            Some(php_runtime::api::NativePrintfScalar::Bool(_))
        )
    {
        return exact_query_contract_violation();
    }
    if argument_2 != missing
        && !matches!(
            fast.native_printf_scalar(argument_2),
            Some(php_runtime::api::NativePrintfScalar::Null)
        )
    {
        // Non-null stream contexts can change wrapper/read semantics.
        return exact_query_contract_violation();
    }
    let offset = if argument_3 != missing {
        let Some(php_runtime::api::NativePrintfScalar::Int(offset)) =
            fast.native_printf_scalar(argument_3)
        else {
            return exact_query_contract_violation();
        };
        offset
    } else {
        0
    };
    let length = if argument_4 != missing {
        match fast.native_printf_scalar(argument_4) {
            Some(php_runtime::api::NativePrintfScalar::Null) => None,
            Some(php_runtime::api::NativePrintfScalar::Int(length)) if length >= 0 => Some(length),
            _ => return exact_query_contract_violation(),
        }
    } else {
        None
    };
    let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
        return exact_query_contract_violation();
    };
    let Some(bytes) =
        php_runtime::api::native_file_get_contents(cwd, filesystem, path, offset, length)
    else {
        return exact_query_contract_violation();
    };
    fast.publish_direct_string_bytes(&bytes).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_file_put_contents_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let flags = if argument_2 == missing {
        0
    } else {
        let Some(php_runtime::api::NativePrintfScalar::Int(flags)) =
            fast.native_printf_scalar(argument_2)
        else {
            return exact_query_contract_violation();
        };
        flags
    };
    if argument_3 != missing
        && !matches!(
            fast.native_printf_scalar(argument_3),
            Some(php_runtime::api::NativePrintfScalar::Null)
        )
    {
        // Publication admits only the null/default stream-context capability.
        return exact_query_contract_violation();
    }
    let Some(path) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(bytes) = fast.native_string_view(argument_1) else {
        return exact_query_contract_violation();
    };
    let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
        return exact_query_contract_violation();
    };
    match php_runtime::api::native_file_put_contents(cwd, filesystem, path, bytes, flags) {
        Some(Some(written)) => fast.publish_direct_int(written).map_or_else(
            |_| exact_query_return_bool(false),
            php_jit::JitNativeControlResult::returning,
        ),
        Some(None) => exact_query_return_bool(false),
        None => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_rename_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(from) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(to) = fast.native_string_view(argument_1) else {
        return exact_query_contract_violation();
    };
    let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
        return exact_query_contract_violation();
    };
    match php_runtime::api::native_rename(cwd, filesystem, from, to) {
        Some(result) => exact_query_return_bool(result),
        None => exact_query_contract_violation(),
    }
}

macro_rules! exact_native_unary_path_mutation_abi {
    ($abi:ident, $native:path) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
        ) -> php_jit::JitNativeControlResult {
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            #[allow(unsafe_code)]
            let fast = unsafe { &mut *runtime };
            let Some(path) = fast.native_string_view(argument_0) else {
                return exact_query_contract_violation();
            };
            let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
                return exact_query_contract_violation();
            };
            match $native(cwd, filesystem, path) {
                Some(result) => exact_query_return_bool(result),
                None => exact_query_contract_violation(),
            }
        }
    };
}

exact_native_unary_path_mutation_abi!(jit_native_unlink_abi, php_runtime::api::native_unlink);
exact_native_unary_path_mutation_abi!(jit_native_rmdir_abi, php_runtime::api::native_rmdir);
exact_native_unary_path_mutation_abi!(jit_native_touch_abi, php_runtime::api::native_touch);

pub(crate) extern "C" fn jit_native_mkdir_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let mode = if argument_1 == missing {
        0o777
    } else {
        let Some(mode) = exact_native_weak_integer(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        mode
    };
    let recursive = if argument_2 == missing {
        false
    } else {
        let Some(recursive) = exact_native_boolean_flag(fast, argument_2) else {
            return exact_query_contract_violation();
        };
        recursive
    };
    if argument_3 != missing {
        let null_context = matches!(
            fast.native_printf_scalar(argument_3),
            Some(php_runtime::api::NativePrintfScalar::Null)
        );
        let stream_context =
            fast.native_resource_view(argument_3)
                .cloned()
                .is_some_and(|resource| {
                    fast.native_stream_context_resource_options(&resource)
                        .is_some()
                });
        if !null_context && !stream_context {
            return exact_query_contract_violation();
        }
    }
    let Some(umask) = fast.native_filesystem_state().map(|state| state.umask()) else {
        return exact_query_contract_violation();
    };
    let Some(path) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
        return exact_query_contract_violation();
    };
    match php_runtime::api::native_mkdir(cwd, filesystem, path, mode, recursive, umask) {
        Some(result) => exact_query_return_bool(result),
        None => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_chmod_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(path) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(mode) = exact_native_integer(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
        return exact_query_contract_violation();
    };
    match php_runtime::api::native_chmod(cwd, filesystem, path, mode) {
        Some(result) => exact_query_return_bool(result),
        None => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_symlink_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(target) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(link) = fast.native_string_view(argument_1) else {
        return exact_query_contract_violation();
    };
    let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
        return exact_query_contract_violation();
    };
    match php_runtime::api::native_symlink(cwd, filesystem, target, link) {
        Some(result) => exact_query_return_bool(result),
        None => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_readfile_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(path) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
        return exact_query_contract_violation();
    };
    let Some(bytes) = php_runtime::api::native_file_get_contents(cwd, filesystem, path, 0, None)
    else {
        // Read failures need the source-aware PHP warning, and no output has
        // been written yet.
        return exact_query_contract_violation();
    };
    let length = i64::try_from(bytes.len()).unwrap_or(i64::MAX);
    match fast.write_output_slice(&bytes) {
        Ok(()) => php_jit::JitNativeControlResult::returning(length),
        Err(_) => exact_query_return_bool(false),
    }
}

pub(crate) extern "C" fn jit_native_is_uploaded_file_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(path) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let path = String::from_utf8_lossy(path);
    let Some(registry) = fast.native_upload_registry() else {
        return exact_query_contract_violation();
    };
    exact_query_return_bool(registry.is_active_upload(&path))
}

pub(crate) extern "C" fn jit_native_move_uploaded_file_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    file: i64,
    start: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(from) = fast.native_string_view(argument_0).map(<[u8]>::to_vec) else {
        return exact_query_contract_violation();
    };
    let Some(to) = fast.native_string_view(argument_1).map(<[u8]>::to_vec) else {
        return exact_query_contract_violation();
    };
    let Some((cwd, filesystem, registry)) = fast.native_upload_move_capability() else {
        return exact_query_contract_violation();
    };
    let Some(result) =
        php_runtime::api::native_move_uploaded_file(cwd, filesystem, registry, &from, &to)
    else {
        return exact_query_contract_violation();
    };
    use php_runtime::api::NativeMoveUploadedFileResult as Result;
    let (diagnostic_id, message) = match result {
        Result::NotActiveUpload => return exact_query_return_bool(false),
        Result::Moved => return exact_query_return_bool(true),
        Result::DestinationDenied => (
            "E_PHP_UPLOAD_DESTINATION_DENIED",
            "move_uploaded_file(): destination is outside allowed filesystem roots",
        ),
        Result::SamePath => (
            "E_PHP_UPLOAD_SAME_PATH",
            "move_uploaded_file(): source and destination must differ",
        ),
        Result::MoveFailed => (
            "E_PHP_UPLOAD_MOVE_FAILED",
            "move_uploaded_file(): failed to move uploaded file",
        ),
    };
    if super::exact_runtime_ops::emit_exact_native_structured_warning(
        fast,
        diagnostic_id,
        message.to_owned(),
        file,
        start,
    ) != 0
    {
        return exact_query_runtime_error();
    }
    exact_query_return_bool(false)
}

pub(crate) extern "C" fn jit_native_tempnam_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(directory) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(prefix) = fast.native_string_view(argument_1) else {
        return exact_query_contract_violation();
    };
    let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
        return exact_query_contract_violation();
    };
    let Some(path) = php_runtime::api::native_tempnam(cwd, filesystem, directory, prefix) else {
        return exact_query_contract_violation();
    };
    let Some(path) = path else {
        return exact_query_return_bool(false);
    };
    let path_bytes = path.to_string_lossy();
    match fast.publish_direct_string_bytes(path_bytes.as_bytes()) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            let _ = std::fs::remove_file(path);
            exact_query_contract_violation()
        }
    }
}

pub(crate) extern "C" fn jit_native_tmpfile_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some((resources, cwd, filesystem, stdin)) = fast.native_stream_open_capability() else {
        return exact_query_contract_violation();
    };
    let Some(resource) = php_runtime::api::native_tmpfile(resources, cwd, filesystem, stdin) else {
        return exact_query_return_bool(false);
    };
    match fast.publish_direct_resource(resource.clone()) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            resource.close();
            exact_query_contract_violation()
        }
    }
}

pub(crate) extern "C" fn jit_native_fopen_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(path) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(mode) = fast.native_string_view(argument_1) else {
        return exact_query_contract_violation();
    };
    let path = String::from_utf8_lossy(path).into_owned();
    let mode = String::from_utf8_lossy(mode).into_owned();
    let Some((resources, cwd, filesystem, stdin)) = fast.native_stream_open_capability() else {
        return exact_query_contract_violation();
    };
    match php_runtime::api::StreamWrapperRegistry::new()
        .open(resources, &path, &mode, cwd, filesystem, stdin)
    {
        Ok(resource) => match fast.publish_direct_resource(resource.clone()) {
            Ok(encoded) => php_jit::JitNativeControlResult::returning(encoded),
            Err(_) => {
                resource.close();
                exact_query_contract_violation()
            }
        },
        Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_fwrite_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0).cloned() else {
        return exact_query_contract_violation();
    };
    let Some(data) = fast.native_string_view(argument_1) else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let length = if argument_2 != missing {
        let Some(php_runtime::api::NativePrintfScalar::Int(length)) =
            fast.native_printf_scalar(argument_2)
        else {
            return exact_query_contract_violation();
        };
        usize::try_from(length.max(0)).unwrap_or(usize::MAX)
    } else {
        data.len()
    };
    let data = &data[..data.len().min(length)];
    let metadata = resource.metadata();
    let uri = metadata.uri;
    match resource.write_bytes(data) {
        Ok(written) => {
            let output = match uri.as_str() {
                "php://stdout" | "php://output" => fast.write_output_slice(&data[..written]),
                "php://stderr" => {
                    use std::io::Write as _;
                    std::io::stderr()
                        .lock()
                        .write_all(&data[..written])
                        .map_err(|_| "native stderr write failed")
                }
                _ => Ok(()),
            };
            match output {
                Ok(()) => php_jit::JitNativeControlResult::returning(
                    i64::try_from(written).unwrap_or(i64::MAX),
                ),
                Err(_) => exact_query_return_bool(false),
            }
        }
        Err(_) => exact_query_return_bool(false),
    }
}

pub(crate) extern "C" fn jit_native_fclose_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0).cloned() else {
        return exact_query_contract_violation();
    };
    if !resource.is_user_closable() {
        return exact_query_contract_violation();
    }
    exact_query_return_bool(resource.close())
}

fn exact_stream_integer(fast: &NativeRequestFastState, encoded: i64) -> Option<i64> {
    match fast.native_printf_scalar(encoded) {
        Some(php_runtime::api::NativePrintfScalar::Int(value)) => Some(value),
        _ => None,
    }
}

fn exact_stream_publish_read(
    fast: &mut NativeRequestFastState,
    resource: &php_runtime::api::ResourceRef,
    checkpoint: (usize, bool),
    bytes: &[u8],
) -> php_jit::JitNativeControlResult {
    match fast.publish_direct_string_bytes(bytes) {
        Ok(encoded) => php_jit::JitNativeControlResult::returning(encoded),
        Err(_) if resource.restore_native_read_cursor(checkpoint) => {
            exact_query_contract_violation()
        }
        Err(_) => {
            debug_assert!(false, "native stream cursor rollback failed");
            exact_query_return_bool(false)
        }
    }
}

pub(crate) extern "C" fn jit_native_fread_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0).cloned() else {
        return exact_query_contract_violation();
    };
    let Some(length) = exact_stream_integer(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    if length <= 0 {
        return exact_query_contract_violation();
    }
    let Some(checkpoint) = resource.native_read_cursor_checkpoint() else {
        return exact_query_contract_violation();
    };
    let length = usize::try_from(length).unwrap_or(usize::MAX);
    match resource.read_bytes(length) {
        Ok(bytes) => exact_stream_publish_read(fast, &resource, checkpoint, &bytes),
        Err(_) => exact_query_return_bool(false),
    }
}

pub(crate) extern "C" fn jit_native_fgets_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0).cloned() else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let length = if argument_1 != missing {
        let Some(length) = exact_stream_integer(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        if length <= 0 {
            return exact_query_contract_violation();
        }
        Some(usize::try_from(length - 1).unwrap_or(usize::MAX))
    } else {
        None
    };
    let Some(checkpoint) = resource.native_read_cursor_checkpoint() else {
        return exact_query_contract_violation();
    };
    let bytes = match length {
        Some(length) => resource.read_line_bounded(length),
        None => resource.read_line(),
    };
    let bytes = match bytes {
        Ok(bytes) => bytes,
        Err(_) => return exact_query_return_bool(false),
    };
    if bytes.is_empty() {
        exact_query_return_bool(false)
    } else {
        exact_stream_publish_read(fast, &resource, checkpoint, &bytes)
    }
}

pub(crate) extern "C" fn jit_native_fgetc_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0).cloned() else {
        return exact_query_contract_violation();
    };
    let Some(checkpoint) = resource.native_read_cursor_checkpoint() else {
        return exact_query_contract_violation();
    };
    match resource.read_bytes(1) {
        Ok(bytes) if bytes.is_empty() => exact_query_return_bool(false),
        Ok(bytes) => exact_stream_publish_read(fast, &resource, checkpoint, &bytes),
        Err(_) => exact_query_return_bool(false),
    }
}

pub(crate) extern "C" fn jit_native_feof_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0) else {
        return exact_query_contract_violation();
    };
    exact_query_return_bool(resource.eof().unwrap_or(true))
}

pub(crate) extern "C" fn jit_native_fflush_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0) else {
        return exact_query_contract_violation();
    };
    exact_query_return_bool(resource.flush().is_ok())
}

pub(crate) extern "C" fn jit_native_fseek_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(offset) = exact_stream_integer(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let whence = if argument_2 != missing {
        let Some(whence) = exact_stream_integer(fast, argument_2) else {
            return exact_query_contract_violation();
        };
        whence
    } else {
        0
    };
    let whence = match whence {
        0 => php_runtime::api::StreamSeekWhence::Set,
        1 => php_runtime::api::StreamSeekWhence::Current,
        2 => php_runtime::api::StreamSeekWhence::End,
        _ => return php_jit::JitNativeControlResult::returning(-1),
    };
    php_jit::JitNativeControlResult::returning(if resource.seek_from(offset, whence).is_ok() {
        0
    } else {
        -1
    })
}

pub(crate) extern "C" fn jit_native_ftell_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0) else {
        return exact_query_contract_violation();
    };
    if !resource.flags().seekable {
        return exact_query_return_bool(false);
    }
    match resource.tell() {
        Ok(offset) => php_jit::JitNativeControlResult::returning(offset as i64),
        Err(_) => exact_query_return_bool(false),
    }
}

pub(crate) extern "C" fn jit_native_ftruncate_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(size) = exact_stream_integer(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    if size < 0 {
        return exact_query_contract_violation();
    }
    exact_query_return_bool(resource.truncate(size as usize).is_ok())
}

pub(crate) extern "C" fn jit_native_rewind_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0) else {
        return exact_query_contract_violation();
    };
    exact_query_return_bool(resource.rewind().is_ok())
}

pub(crate) extern "C" fn jit_native_stream_get_contents_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(resource) = fast.native_resource_view(argument_0).cloned() else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let length = if argument_1 != missing {
        let Some(length) = exact_stream_integer(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        if length < -1 {
            return exact_query_contract_violation();
        }
        Some(length)
    } else {
        None
    };
    let offset = if argument_2 != missing {
        let Some(offset) = exact_stream_integer(fast, argument_2) else {
            return exact_query_contract_violation();
        };
        Some(offset)
    } else {
        None
    };
    let Some(checkpoint) = resource.native_read_cursor_checkpoint() else {
        return exact_query_contract_violation();
    };
    if let Some(offset) = offset
        && offset >= 0
        && resource.seek(offset as usize).is_err()
    {
        return exact_query_return_bool(false);
    }
    let bytes = match length {
        Some(length) if length >= 0 => resource.read_bytes(length as usize),
        Some(_) | None => resource.read_to_end(),
    };
    match bytes {
        Ok(bytes) => exact_stream_publish_read(fast, &resource, checkpoint, &bytes),
        Err(_) => exact_query_return_bool(false),
    }
}

pub(crate) extern "C" fn jit_native_stream_copy_to_stream_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(source) = fast.native_resource_view(argument_0).cloned() else {
        return exact_query_contract_violation();
    };
    let Some(destination) = fast.native_resource_view(argument_1).cloned() else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let length = if argument_2 != missing {
        let Some(length) = exact_stream_integer(fast, argument_2) else {
            return exact_query_contract_violation();
        };
        Some(length)
    } else {
        None
    };
    let offset = if argument_3 != missing {
        let Some(offset) = exact_stream_integer(fast, argument_3) else {
            return exact_query_contract_violation();
        };
        Some(offset)
    } else {
        None
    };
    if let Some(offset) = offset
        && offset >= 0
        && source.seek(offset as usize).is_err()
    {
        return exact_query_return_bool(false);
    }
    let bytes = match length {
        Some(length) if length >= 0 => source.read_bytes(length as usize),
        Some(_) | None => source.read_to_end(),
    };
    let Ok(bytes) = bytes else {
        return exact_query_return_bool(false);
    };
    match destination.write_bytes(&bytes) {
        Ok(written) => php_jit::JitNativeControlResult::returning(written as i64),
        Err(_) => exact_query_return_bool(false),
    }
}

pub(crate) extern "C" fn jit_native_ob_start_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(output) = fast.native_output_buffer() else {
        return exact_query_contract_violation();
    };
    output.start_buffer();
    exact_query_return_bool(true)
}

pub(crate) extern "C" fn jit_native_ob_get_clean_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let encoded = match fast.publish_current_output_buffer() {
        Ok(Some(encoded)) => encoded,
        Ok(None) => return exact_query_return_bool(false),
        Err(_) => return exact_query_contract_violation(),
    };
    let Some(output) = fast.native_output_buffer() else {
        return exact_query_contract_violation();
    };
    debug_assert!(output.pop_buffer_clean().is_some());
    php_jit::JitNativeControlResult::returning(encoded)
}

pub(crate) extern "C" fn jit_native_ob_get_contents_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    match fast.publish_current_output_buffer() {
        Ok(Some(encoded)) => php_jit::JitNativeControlResult::returning(encoded),
        Ok(None) => exact_query_return_bool(false),
        Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_ob_get_flush_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let encoded = match fast.publish_current_output_buffer() {
        Ok(Some(encoded)) => encoded,
        Ok(None) => {
            // Publication excludes the empty-stack notice case before entry.
            // No output-stack effect has happened yet.
            return exact_query_contract_violation();
        }
        Err(_) => return exact_query_contract_violation(),
    };
    let Some(output) = fast.native_output_buffer() else {
        return exact_query_contract_violation();
    };
    debug_assert!(output.pop_buffer_flush().is_some());
    php_jit::JitNativeControlResult::returning(encoded)
}

pub(crate) extern "C" fn jit_native_ob_get_length_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(length) = fast
        .native_output_buffer()
        .and_then(|output| output.current_buffer_len())
    else {
        return exact_query_return_bool(false);
    };
    php_jit::JitNativeControlResult::returning(i64::try_from(length).unwrap_or(i64::MAX))
}

pub(crate) extern "C" fn jit_native_ob_get_level_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(output) = fast.native_output_buffer() else {
        return exact_query_contract_violation();
    };
    php_jit::JitNativeControlResult::returning(
        i64::try_from(output.buffer_level()).unwrap_or(i64::MAX),
    )
}

pub(crate) extern "C" fn jit_native_ob_end_flush_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(output) = fast.native_output_buffer() else {
        return exact_query_contract_violation();
    };
    if output.buffer_level() == 0 {
        return exact_query_contract_violation();
    }
    debug_assert!(output.pop_buffer_flush().is_some());
    exact_query_return_bool(true)
}

pub(crate) extern "C" fn jit_native_ob_end_clean_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(output) = fast.native_output_buffer() else {
        return exact_query_contract_violation();
    };
    if output.buffer_level() == 0 {
        return exact_query_contract_violation();
    }
    debug_assert!(output.pop_buffer_clean().is_some());
    exact_query_return_bool(true)
}

pub(crate) extern "C" fn jit_native_phpinfo_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    if argument_0 != missing && exact_native_weak_integer(fast, argument_0).is_none() {
        return exact_query_contract_violation();
    }
    if fast
        .write_output_slice(&php_runtime::api::native_phpinfo_output())
        .is_err()
    {
        return exact_query_contract_violation();
    }
    exact_query_return_bool(true)
}

pub(crate) extern "C" fn jit_native_var_dump_abi(
    runtime: *mut NativeRequestFastState,
    argument_count: u32,
    arguments: *const i64,
) -> php_jit::JitNativeControlResult {
    let Ok(argument_count) = usize::try_from(argument_count) else {
        return exact_query_contract_violation();
    };
    if argument_count == 0 || arguments.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: generated code owns this synchronous argument slice.
    #[allow(unsafe_code)]
    let arguments = unsafe { std::slice::from_raw_parts(arguments, argument_count) };
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let mut bytes = Vec::new();
    let mut traversal = NativeJsonTraversal::new();
    for argument in arguments {
        if fast
            .write_native_var_dump(*argument, 0, &mut bytes, &mut traversal)
            .is_none()
        {
            return exact_query_contract_violation();
        }
    }
    if fast.write_output_slice(&bytes).is_err() {
        return exact_query_contract_violation();
    }
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX))
}

pub(crate) extern "C" fn jit_native_print_abi(
    runtime: *mut NativeRequestFastState,
    argument: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(bytes) = exact_native_scalar_string(fast, argument) else {
        return exact_query_contract_violation();
    };
    if fast.write_output_slice(bytes.as_bytes()).is_err() {
        return exact_query_contract_violation();
    }
    php_jit::JitNativeControlResult::returning(1)
}

pub(crate) extern "C" fn jit_native_print_r_abi(
    runtime: *mut NativeRequestFastState,
    argument: i64,
    return_output: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let return_output = if exact_session_argument_missing(return_output) {
        false
    } else {
        let Some(value) = fast.native_comparison_value(return_output) else {
            return exact_query_contract_violation();
        };
        native_comparison_truthy(value)
    };
    let mut bytes = Vec::new();
    let mut traversal = NativeJsonTraversal::new();
    if fast
        .write_native_print_r(argument, 0, &mut bytes, &mut traversal)
        .is_none()
    {
        return exact_query_contract_violation();
    }
    if return_output {
        return fast.publish_direct_string_bytes(&bytes).map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        );
    }
    if fast.write_output_slice(&bytes).is_err() {
        return exact_query_contract_violation();
    }
    exact_query_return_bool(true)
}

pub(crate) extern "C" fn jit_native_var_export_abi(
    runtime: *mut NativeRequestFastState,
    argument: i64,
    return_output: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let return_output = if exact_session_argument_missing(return_output) {
        false
    } else {
        let Some(value) = fast.native_comparison_value(return_output) else {
            return exact_query_contract_violation();
        };
        native_comparison_truthy(value)
    };
    let mut bytes = Vec::new();
    let mut traversal = NativeJsonTraversal::new();
    if fast
        .write_native_var_export(argument, 0, &mut bytes, &mut traversal)
        .is_none()
    {
        return exact_query_contract_violation();
    }
    if return_output {
        return fast.publish_direct_string_bytes(&bytes).map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        );
    }
    if fast.write_output_slice(&bytes).is_err() {
        return exact_query_contract_violation();
    }
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX))
}

pub(crate) extern "C" fn jit_native_mysqli_set_charset_abi(
    runtime: *mut NativeRequestFastState,
    connection: i64,
    charset: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(connection_id) = fast.native_mysqli_connection_id(connection) else {
        return exact_query_contract_violation();
    };
    let Some(charset) = fast.native_string_view(charset) else {
        return exact_query_contract_violation();
    };
    let charset = String::from_utf8_lossy(charset);
    if fast.mysql_state.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: request publication stores the stable `Rc<RefCell<_>>` target;
    // generated calls are synchronous on the owning request thread.
    #[allow(unsafe_code)]
    let state = unsafe { &*fast.mysql_state };
    match state.borrow_mut().set_charset(connection_id, &charset) {
        Ok(()) => exact_query_return_bool(true),
        Err(_) => exact_query_return_bool(false),
    }
}

pub(crate) extern "C" fn jit_native_mysqli_select_db_abi(
    runtime: *mut NativeRequestFastState,
    connection: i64,
    database: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(connection_id) = fast.native_mysqli_connection_id(connection) else {
        return exact_query_contract_violation();
    };
    let Some(database) = fast.native_string_view(database) else {
        return exact_query_contract_violation();
    };
    let database = String::from_utf8_lossy(database).into_owned();
    if fast.mysql_state.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: request publication stores the stable `Rc<RefCell<_>>` target.
    #[allow(unsafe_code)]
    let state = unsafe { &*fast.mysql_state };
    match state.borrow_mut().select_db(connection_id, &database) {
        Ok(()) => exact_query_return_bool(true),
        Err(_) => exact_query_return_bool(false),
    }
}

pub(crate) extern "C" fn jit_native_mysqli_real_escape_string_abi(
    runtime: *mut NativeRequestFastState,
    connection: i64,
    value: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if fast.native_mysqli_connection_id(connection).is_none() {
        return exact_query_contract_violation();
    }
    let Some(value) = fast.native_string_view(value) else {
        return exact_query_contract_violation();
    };
    let escaped = php_runtime::api::native_mysql_escape_string(value);
    fast.publish_direct_string_bytes(&escaped).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_mysqli_error_abi(
    runtime: *mut NativeRequestFastState,
    connection: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(connection_id) = fast.native_mysqli_connection_id(connection) else {
        return exact_query_contract_violation();
    };
    if fast.mysql_state.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: request publication stores the stable `Rc<RefCell<_>>` target.
    #[allow(unsafe_code)]
    let state = unsafe { &*fast.mysql_state };
    let error = state.borrow().error(connection_id);
    fast.publish_direct_string_bytes(error.as_bytes())
        .map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        )
}

macro_rules! exact_mysqli_integer_status {
    ($name:ident, $accessor:ident) => {
        pub(crate) extern "C" fn $name(
            runtime: *mut NativeRequestFastState,
            connection: i64,
        ) -> php_jit::JitNativeControlResult {
            // Safety: generated code passes the active request-owned fast state.
            #[allow(unsafe_code)]
            let fast = unsafe { &mut *runtime };
            let Some(connection_id) = fast.native_mysqli_connection_id(connection) else {
                return exact_query_contract_violation();
            };
            if fast.mysql_state.is_null() {
                return exact_query_contract_violation();
            }
            // Safety: request publication stores the stable `Rc<RefCell<_>>` target.
            #[allow(unsafe_code)]
            let state = unsafe { &*fast.mysql_state };
            let value = state.borrow().$accessor(connection_id);
            fast.publish_direct_int(value).map_or_else(
                |_| exact_query_contract_violation(),
                php_jit::JitNativeControlResult::returning,
            )
        }
    };
}

exact_mysqli_integer_status!(jit_native_mysqli_errno_abi, errno);
exact_mysqli_integer_status!(jit_native_mysqli_affected_rows_abi, affected_rows);
exact_mysqli_integer_status!(jit_native_mysqli_insert_id_abi, last_insert_id);
exact_mysqli_integer_status!(jit_native_mysqli_field_count_abi, field_count);

pub(crate) extern "C" fn jit_native_mysqli_more_results_abi(
    runtime: *mut NativeRequestFastState,
    connection: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(connection_id) = fast.native_mysqli_connection_id(connection) else {
        return exact_query_contract_violation();
    };
    if fast.mysql_state.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: request publication stores the stable `Rc<RefCell<_>>` target.
    #[allow(unsafe_code)]
    let state = unsafe { &*fast.mysql_state };
    exact_query_return_bool(state.borrow().more_results(connection_id))
}

pub(crate) extern "C" fn jit_native_mysqli_next_result_abi(
    runtime: *mut NativeRequestFastState,
    connection: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(connection_id) = fast.native_mysqli_connection_id(connection) else {
        return exact_query_contract_violation();
    };
    if fast.mysql_state.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: request publication stores the stable `Rc<RefCell<_>>` target.
    #[allow(unsafe_code)]
    let state = unsafe { &*fast.mysql_state };
    exact_query_return_bool(state.borrow_mut().next_result(connection_id))
}

pub(crate) extern "C" fn jit_native_mysqli_report_abi(
    runtime: *mut NativeRequestFastState,
    flags: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(php_runtime::api::NativePrintfScalar::Int(flags)) = fast.native_printf_scalar(flags)
    else {
        return exact_query_contract_violation();
    };
    if fast.mysql_state.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: request publication stores the stable `Rc<RefCell<_>>` target.
    #[allow(unsafe_code)]
    let state = unsafe { &*fast.mysql_state };
    state.borrow_mut().set_report_flags(flags);
    exact_query_return_bool(true)
}

fn publish_exact_mysqli_object(fast: &mut NativeRequestFastState) -> Result<i64, &'static str> {
    let prepared = fast.prepared_mysqli_class;
    if prepared.is_null() {
        return Err("mysqli class plan unavailable");
    }
    // Safety: request publication owns the immutable prepared class record.
    #[allow(unsafe_code)]
    let prepared = unsafe { &*prepared };
    let mut owned = Vec::with_capacity(8);
    macro_rules! publish_owned {
        ($result:expr) => {{
            let value = $result?;
            owned.push(value);
            value
        }};
    }
    let properties = [
        ("connect_errno", publish_owned!(fast.publish_direct_int(0))),
        (
            "connect_error",
            publish_owned!(fast.publish_direct_string_bytes(b"")),
        ),
        ("errno", publish_owned!(fast.publish_direct_int(0))),
        (
            "error",
            publish_owned!(fast.publish_direct_string_bytes(b"")),
        ),
        ("affected_rows", publish_owned!(fast.publish_direct_int(0))),
        ("insert_id", publish_owned!(fast.publish_direct_int(0))),
        (
            "client_info",
            publish_owned!(
                fast.publish_direct_string_bytes(php_runtime::api::MYSQLND_CLIENT_INFO.as_bytes(),)
            ),
        ),
        (
            "client_version",
            publish_owned!(fast.publish_direct_int(php_runtime::api::MYSQLND_CLIENT_VERSION)),
        ),
    ];
    let object = php_runtime::api::ObjectRef::from_layout_native_slots(
        &prepared.entry,
        prepared.display_name.clone(),
        prepared.default_native_slots.clone(),
    );
    for (name, value) in properties {
        if object
            .set_native_dynamic_property(
                prepared.layout_id,
                name.to_owned(),
                php_runtime::api::NativeDeclaredPropertySlot {
                    initialized: 1,
                    reserved: 0,
                    value,
                },
            )
            .is_err()
        {
            for value in owned {
                let _ = fast.discard_owned_direct_value(value);
            }
            return Err("mysqli property publication failed");
        }
    }
    match fast.publish_direct_object(object) {
        Ok(value) => Ok(value),
        Err(_) => {
            for value in owned {
                let _ = fast.discard_owned_direct_value(value);
            }
            Err("mysqli object publication failed")
        }
    }
}

pub(crate) extern "C" fn jit_native_mysqli_init_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    publish_exact_mysqli_object(fast).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_mysqli_options_abi(
    runtime: *mut NativeRequestFastState,
    object: i64,
    option: i64,
    _value: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if fast.native_mysqli_object(object).is_none()
        || !matches!(
            fast.native_printf_scalar(option),
            Some(php_runtime::api::NativePrintfScalar::Int(_))
        )
    {
        return exact_query_contract_violation();
    }
    // The current native client consumes connect arguments directly; mysqli
    // option values are accepted for PHP compatibility but do not alter that
    // connection plan, matching the existing runtime implementation.
    exact_query_return_bool(true)
}

pub(crate) extern "C" fn jit_native_mysqli_real_connect_abi(
    runtime: *mut NativeRequestFastState,
    object: i64,
    host: i64,
    user: i64,
    password: i64,
    database: i64,
    port: i64,
    socket: i64,
    flags: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if fast.native_mysqli_object(object).is_none() || fast.mysql_state.is_null() {
        return exact_query_contract_violation();
    }
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let string_argument = |fast: &NativeRequestFastState, value: i64, default: &[u8]| {
        if value == missing {
            Some(default.to_vec())
        } else {
            fast.native_string_view(value).map(<[u8]>::to_vec)
        }
    };
    let Some(host) = string_argument(fast, host, b"localhost") else {
        return exact_query_contract_violation();
    };
    let Some(user) = string_argument(fast, user, b"") else {
        return exact_query_contract_violation();
    };
    let Some(password) = string_argument(fast, password, b"") else {
        return exact_query_contract_violation();
    };
    let optional_string = |fast: &NativeRequestFastState, value: i64| {
        if value == missing
            || matches!(
                fast.native_printf_scalar(value),
                Some(php_runtime::api::NativePrintfScalar::Null)
            )
        {
            Some(None)
        } else {
            fast.native_string_view(value)
                .map(|value| Some(value.to_vec()))
        }
    };
    let Some(database) = optional_string(fast, database) else {
        return exact_query_contract_violation();
    };
    let Some(socket) = optional_string(fast, socket) else {
        return exact_query_contract_violation();
    };
    let port = if port == missing
        || matches!(
            fast.native_printf_scalar(port),
            Some(php_runtime::api::NativePrintfScalar::Null)
        ) {
        None
    } else {
        let Some(php_runtime::api::NativePrintfScalar::Int(port)) = fast.native_printf_scalar(port)
        else {
            return exact_query_contract_violation();
        };
        u16::try_from(port).ok()
    };
    if flags != missing
        && !matches!(
            fast.native_printf_scalar(flags),
            Some(php_runtime::api::NativePrintfScalar::Int(_))
        )
    {
        return exact_query_contract_violation();
    }
    let host = String::from_utf8_lossy(&host);
    let user = String::from_utf8_lossy(&user);
    let password = String::from_utf8_lossy(&password);
    let database = database
        .as_deref()
        .map(|value| String::from_utf8_lossy(value));
    let socket = socket
        .as_deref()
        .map(|value| String::from_utf8_lossy(value));
    let options = php_runtime::api::MysqlConnectOptions::from_parts_with_socket(
        &host,
        &user,
        &password,
        database.as_deref(),
        port,
        socket.as_deref(),
    );
    // Safety: request publication stores the stable `Rc<RefCell<_>>` target.
    #[allow(unsafe_code)]
    let state = unsafe { &*fast.mysql_state };
    let connection = match options {
        Ok(options) => state.borrow_mut().connect(&options),
        Err(error) => {
            state
                .borrow_mut()
                .record_connect_error(error.mysql_errno(), error.message.clone());
            Err(error)
        }
    };
    let Ok(connection_id) = connection else {
        return exact_query_return_bool(false);
    };
    let connection_id = match fast.publish_direct_int(connection_id) {
        Ok(value) => value,
        Err(_) => return exact_query_contract_violation(),
    };
    if fast
        .store_native_mysqli_property_owned(object, "__mysqli_connection", connection_id)
        .is_none()
    {
        let _ = fast.discard_owned_direct_value(connection_id);
        return exact_query_contract_violation();
    }
    exact_query_return_bool(true)
}

pub(crate) extern "C" fn jit_native_error_log_abi(
    runtime: *mut NativeRequestFastState,
    message: i64,
    message_type: i64,
    destination: i64,
    additional_headers: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let Some(message) = fast.native_string_view(message) else {
        return exact_query_contract_violation();
    };
    let message = message.to_vec();
    let message_type = if message_type == missing {
        0
    } else {
        let Some(php_runtime::api::NativePrintfScalar::Int(message_type)) =
            fast.native_printf_scalar(message_type)
        else {
            return exact_query_contract_violation();
        };
        message_type
    };
    let destination = if destination == missing {
        None
    } else if matches!(
        fast.native_printf_scalar(destination),
        Some(php_runtime::api::NativePrintfScalar::Null)
    ) {
        None
    } else {
        let Some(destination) = fast.native_string_view(destination) else {
            return exact_query_contract_violation();
        };
        Some(destination.to_vec())
    };
    if additional_headers != missing
        && !matches!(
            fast.native_printf_scalar(additional_headers),
            Some(php_runtime::api::NativePrintfScalar::Null)
        )
        && fast.native_string_view(additional_headers).is_none()
    {
        return exact_query_contract_violation();
    }
    let success = match message_type {
        0 | 4 => {
            let stderr = std::io::stderr();
            let mut sink = stderr.lock();
            sink.write_all(&message)
                .and_then(|()| sink.write_all(b"\n"))
                .is_ok()
        }
        3 => {
            let Some(destination) = destination else {
                return exact_query_return_bool(false);
            };
            let Some((cwd, filesystem)) = fast.native_filesystem_capability() else {
                return exact_query_contract_violation();
            };
            matches!(
                php_runtime::api::native_file_put_contents(
                    cwd,
                    filesystem,
                    &destination,
                    &message,
                    8,
                ),
                Some(Some(_))
            )
        }
        // Email delivery is intentionally unavailable without a published mail
        // transport capability; PHP exposes that as a `false` result.
        1 => false,
        _ => false,
    };
    exact_query_return_bool(success)
}

pub(crate) extern "C" fn jit_native_sleep_abi(
    runtime: *mut NativeRequestFastState,
    seconds: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(php_runtime::api::NativePrintfScalar::Int(seconds)) =
        fast.native_printf_scalar(seconds)
    else {
        return exact_query_contract_violation();
    };
    let Ok(seconds) = u64::try_from(seconds) else {
        return exact_query_runtime_error();
    };
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    fast.publish_direct_int(0).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_mysqli_query_abi(
    runtime: *mut NativeRequestFastState,
    connection: i64,
    sql: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(connection_id) = fast.native_mysqli_connection_id(connection) else {
        return exact_query_contract_violation();
    };
    let Some(sql) = fast.native_string_view(sql) else {
        return exact_query_contract_violation();
    };
    let sql = String::from_utf8_lossy(sql).into_owned();
    if fast.mysql_state.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: request publication stores the stable `Rc<RefCell<_>>` target.
    #[allow(unsafe_code)]
    let state = unsafe { &*fast.mysql_state };
    let query = {
        let mut state = state.borrow_mut();
        match state.query(connection_id, &sql) {
            Ok(Some(result_id)) => Ok(Some((result_id, state.num_rows(result_id)))),
            Ok(None) => Ok(None),
            Err(error) => Err(error),
        }
    };
    let (result_id, row_count) = match query {
        Ok(Some(result)) => result,
        Ok(None) => return exact_query_return_bool(true),
        Err(_) => return exact_query_return_bool(false),
    };
    let prepared = fast.prepared_mysqli_result_class;
    if prepared.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: request publication owns the immutable prepared class record.
    #[allow(unsafe_code)]
    let prepared = unsafe { &*prepared };
    let result_id_value = match fast.publish_direct_int(result_id) {
        Ok(value) => value,
        Err(_) => return exact_query_contract_violation(),
    };
    let row_count_value = match fast.publish_direct_int(row_count) {
        Ok(value) => value,
        Err(_) => {
            let _ = fast.discard_owned_direct_value(result_id_value);
            return exact_query_contract_violation();
        }
    };
    let object = php_runtime::api::ObjectRef::from_layout_native_slots(
        &prepared.entry,
        prepared.display_name.clone(),
        prepared.default_native_slots.clone(),
    );
    for (name, value) in [
        ("__mysqli_result", result_id_value),
        ("num_rows", row_count_value),
    ] {
        if object
            .set_native_dynamic_property(
                prepared.layout_id,
                name.to_owned(),
                php_runtime::api::NativeDeclaredPropertySlot {
                    initialized: 1,
                    reserved: 0,
                    value,
                },
            )
            .is_err()
        {
            let _ = fast.discard_owned_direct_value(row_count_value);
            let _ = fast.discard_owned_direct_value(result_id_value);
            return exact_query_contract_violation();
        }
    }
    match fast.publish_direct_object(object) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            let _ = fast.discard_owned_direct_value(row_count_value);
            let _ = fast.discard_owned_direct_value(result_id_value);
            exact_query_contract_violation()
        }
    }
}

#[derive(Clone)]
enum ExactMysqliRowKey {
    Int(i64),
    String(Vec<u8>),
}

fn publish_exact_mysqli_cell(
    fast: &mut NativeRequestFastState,
    cell: &php_runtime::api::MysqlCell,
) -> Result<i64, &'static str> {
    match cell {
        php_runtime::api::MysqlCell::Null => Ok(php_jit::jit_encode_constant(u32::MAX)),
        php_runtime::api::MysqlCell::Int(value) => match i64::try_from(*value) {
            Ok(value) => fast.publish_direct_int(value),
            Err(_) => fast.publish_direct_string_bytes(value.to_string().as_bytes()),
        },
        php_runtime::api::MysqlCell::Float(value)
        | php_runtime::api::MysqlCell::DateTime(value)
        | php_runtime::api::MysqlCell::Time(value) => {
            fast.publish_direct_string_bytes(value.as_bytes())
        }
        php_runtime::api::MysqlCell::Bytes(value) => fast.publish_direct_string_bytes(value),
    }
}

pub(crate) extern "C" fn jit_native_mysqli_fetch_array_abi(
    runtime: *mut NativeRequestFastState,
    result: i64,
    mode: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(result_id) = fast.native_mysqli_result_id(result) else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let mode = if mode == missing {
        php_runtime::api::MYSQLI_BOTH
    } else {
        let Some(mode) = exact_native_weak_integer(fast, mode) else {
            return exact_query_contract_violation();
        };
        mode
    };
    if fast.mysql_state.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: request publication stores the stable `Rc<RefCell<_>>` target.
    #[allow(unsafe_code)]
    let state = unsafe { &*fast.mysql_state };
    let row = state.borrow_mut().fetch_native_row(result_id);
    let Some((columns, row)) = row else {
        return exact_query_return_bool(false);
    };
    let mut entries = Vec::<(ExactMysqliRowKey, php_runtime::api::MysqlCell)>::new();
    if mode & php_runtime::api::MYSQLI_NUM != 0 {
        entries.extend(
            row.values
                .iter()
                .cloned()
                .enumerate()
                .map(|(index, value)| {
                    (
                        ExactMysqliRowKey::Int(i64::try_from(index).unwrap_or(i64::MAX)),
                        value,
                    )
                }),
        );
    }
    if mode & php_runtime::api::MYSQLI_ASSOC != 0 {
        for (name, value) in columns.iter().zip(&row.values) {
            let key = name.as_bytes();
            if let Some((_, existing)) = entries.iter_mut().find(|(existing, _)| {
                matches!(existing, ExactMysqliRowKey::String(bytes) if bytes == key)
            }) {
                *existing = value.clone();
            } else {
                entries.push((ExactMysqliRowKey::String(key.to_vec()), value.clone()));
            }
        }
    }
    match fast.publish_owned_direct_array_with(entries.len(), |fast, index| {
        let (key, cell) = &entries[index];
        let key = match key {
            ExactMysqliRowKey::Int(key) => *key,
            ExactMysqliRowKey::String(key) => fast.publish_direct_string_bytes(key)?,
        };
        let value = match publish_exact_mysqli_cell(fast, cell) {
            Ok(value) => value,
            Err(error) => {
                if matches!(&entries[index].0, ExactMysqliRowKey::String(_)) {
                    let _ = fast.discard_owned_direct_value(key);
                }
                return Err(error);
            }
        };
        Ok(php_jit::JitNativeDirectArrayEntry { key, value })
    }) {
        Ok(array) => php_jit::JitNativeControlResult::returning(array),
        Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_mysqli_fetch_object_abi(
    runtime: *mut NativeRequestFastState,
    result: i64,
    class_name: i64,
    _constructor_args: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(result_id) = fast.native_mysqli_result_id(result) else {
        return exact_query_contract_violation();
    };
    if fast.mysql_state.is_null() {
        return exact_query_contract_violation();
    }
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let class_name = if class_name == missing {
        "stdClass".to_owned()
    } else {
        let Some(class_name) = fast.native_string_view(class_name) else {
            return exact_query_contract_violation();
        };
        String::from_utf8_lossy(class_name).into_owned()
    };
    // Safety: request publication stores the stable `Rc<RefCell<_>>` target.
    #[allow(unsafe_code)]
    let state = unsafe { &*fast.mysql_state };
    let row = state.borrow_mut().fetch_native_row(result_id);
    let Some((columns, row)) = row else {
        return exact_query_return_bool(false);
    };
    let object = if class_name.eq_ignore_ascii_case("stdClass") {
        super::exact_runtime_ops::native_object_cast_stdclass()
    } else {
        let entry = php_runtime::api::ClassEntry {
            name: std::sync::Arc::from(class_name.to_ascii_lowercase()),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: php_runtime::api::ClassFlags::default(),
        };
        php_runtime::api::ObjectRef::from_layout_native_slots(&entry, class_name, Box::new([]))
    };
    let layout_id = object.class_layout_epoch();
    let mut owned = Vec::<(String, i64)>::with_capacity(columns.len());
    for (name, cell) in columns.into_iter().zip(&row.values) {
        let value = match publish_exact_mysqli_cell(fast, cell) {
            Ok(value) => value,
            Err(_) => {
                for (_, value) in owned {
                    let _ = fast.discard_owned_direct_value(value);
                }
                return exact_query_contract_violation();
            }
        };
        let previous = match object.set_native_dynamic_property(
            layout_id,
            name.clone(),
            php_runtime::api::NativeDeclaredPropertySlot {
                initialized: 1,
                reserved: 0,
                value,
            },
        ) {
            Ok(previous) => previous,
            Err(_) => {
                let _ = fast.discard_owned_direct_value(value);
                for (_, value) in owned {
                    let _ = fast.discard_owned_direct_value(value);
                }
                return exact_query_contract_violation();
            }
        };
        if let Some(previous) = previous {
            if let Some((_, tracked)) = owned.iter_mut().find(|(existing, _)| *existing == name) {
                *tracked = value;
            }
            let _ = fast.discard_owned_direct_value(previous.value);
        } else {
            owned.push((name, value));
        }
    }
    match fast.publish_direct_object(object) {
        Ok(object) => php_jit::JitNativeControlResult::returning(object),
        Err(_) => {
            for (_, value) in owned {
                let _ = fast.discard_owned_direct_value(value);
            }
            exact_query_contract_violation()
        }
    }
}

pub(crate) extern "C" fn jit_native_mysqli_character_set_name_abi(
    runtime: *mut NativeRequestFastState,
    connection: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    if fast.native_mysqli_object(connection).is_none() {
        return exact_query_contract_violation();
    }
    match fast.publish_direct_string_bytes(b"utf8mb4") {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => exact_query_contract_violation(),
    }
}

macro_rules! exact_mysqli_result_count {
    ($name:ident, $method:ident) => {
        pub(crate) extern "C" fn $name(
            runtime: *mut NativeRequestFastState,
            result: i64,
        ) -> php_jit::JitNativeControlResult {
            // Safety: generated code passes the active request-owned fast state.
            #[allow(unsafe_code)]
            let fast = unsafe { &mut *runtime };
            let Some(result_id) = fast.native_mysqli_result_id(result) else {
                return exact_query_contract_violation();
            };
            if fast.mysql_state.is_null() {
                return exact_query_contract_violation();
            }
            // Safety: request publication stores the stable `Rc<RefCell<_>>` target.
            #[allow(unsafe_code)]
            let state = unsafe { &*fast.mysql_state };
            let value = state.borrow().$method(result_id);
            match fast.publish_direct_int(value) {
                Ok(value) => php_jit::JitNativeControlResult::returning(value),
                Err(_) => exact_query_contract_violation(),
            }
        }
    };
}

exact_mysqli_result_count!(jit_native_mysqli_num_fields_abi, num_fields);
exact_mysqli_result_count!(jit_native_mysqli_num_rows_abi, num_rows);

pub(crate) extern "C" fn jit_native_mysqli_fetch_field_abi(
    runtime: *mut NativeRequestFastState,
    result: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(result_id) = fast.native_mysqli_result_id(result) else {
        return exact_query_contract_violation();
    };
    if fast.mysql_state.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: request publication stores the stable `Rc<RefCell<_>>` target.
    #[allow(unsafe_code)]
    let state = unsafe { &*fast.mysql_state };
    let name = state.borrow_mut().fetch_native_field_name(result_id);
    let Some(name) = name else {
        return exact_query_return_bool(false);
    };
    let value = match fast.publish_direct_string_bytes(name.as_bytes()) {
        Ok(value) => value,
        Err(_) => return exact_query_contract_violation(),
    };
    let object = super::exact_runtime_ops::native_object_cast_stdclass();
    let layout_id = object.class_layout_epoch();
    if object
        .set_native_dynamic_property(
            layout_id,
            "name".to_owned(),
            php_runtime::api::NativeDeclaredPropertySlot {
                initialized: 1,
                reserved: 0,
                value,
            },
        )
        .is_err()
    {
        let _ = fast.discard_owned_direct_value(value);
        return exact_query_contract_violation();
    }
    match fast.publish_direct_object(object) {
        Ok(object) => php_jit::JitNativeControlResult::returning(object),
        Err(_) => {
            let _ = fast.discard_owned_direct_value(value);
            exact_query_contract_violation()
        }
    }
}

pub(crate) extern "C" fn jit_native_mysqli_connect_errno_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let value = if fast.mysql_state.is_null() {
        2002
    } else {
        // Safety: request publication stores the stable `Rc<RefCell<_>>` target.
        #[allow(unsafe_code)]
        let state = unsafe { &*fast.mysql_state };
        state.borrow().connect_errno()
    };
    match fast.publish_direct_int(value) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_mysqli_connect_error_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let value = if fast.mysql_state.is_null() {
        "MySQL extension state is unavailable".to_owned()
    } else {
        // Safety: request publication stores the stable `Rc<RefCell<_>>` target.
        #[allow(unsafe_code)]
        let state = unsafe { &*fast.mysql_state };
        state.borrow().connect_error()
    };
    match fast.publish_direct_string_bytes(value.as_bytes()) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_mysqli_close_abi(
    runtime: *mut NativeRequestFastState,
    connection: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(connection_id) = fast.native_mysqli_connection_id(connection) else {
        return exact_query_return_bool(false);
    };
    if fast.mysql_state.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: request publication stores the stable `Rc<RefCell<_>>` target.
    #[allow(unsafe_code)]
    let state = unsafe { &*fast.mysql_state };
    let closed = state.borrow_mut().close(connection_id);
    if closed
        && fast
            .native_mysqli_invalidate_connection(connection)
            .is_none()
    {
        return exact_query_contract_violation();
    }
    exact_query_return_bool(closed)
}

pub(crate) extern "C" fn jit_native_mysqli_get_server_info_abi(
    runtime: *mut NativeRequestFastState,
    connection: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(connection_id) = fast.native_mysqli_connection_id(connection) else {
        return exact_query_contract_violation();
    };
    if fast.mysql_state.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: request publication stores the stable `Rc<RefCell<_>>` target.
    #[allow(unsafe_code)]
    let state = unsafe { &*fast.mysql_state };
    let value = state.borrow().server_info(connection_id);
    match fast.publish_direct_string_bytes(value.as_bytes()) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_mysqli_free_result_abi(
    runtime: *mut NativeRequestFastState,
    result: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(result_id) = fast.native_mysqli_result_id(result) else {
        return exact_query_contract_violation();
    };
    if fast.mysql_state.is_null() {
        return exact_query_contract_violation();
    }
    // Safety: request publication stores the stable `Rc<RefCell<_>>` target.
    #[allow(unsafe_code)]
    let state = unsafe { &*fast.mysql_state };
    let freed = state.borrow_mut().free_result(result_id);
    if freed && fast.native_mysqli_invalidate_result(result).is_none() {
        return exact_query_contract_violation();
    }
    exact_query_return_bool(freed)
}
