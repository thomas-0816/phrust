//! Baseline-only class-constant resolution and publication.
//!
//! Optimizing sites consume immutable trusted constant slots. Autoload,
//! visibility, deferred defaults, and external-unit lookup enter this
//! continuation once and publish a native owner when cacheable.

use super::*;
use php_runtime::api::PhpString;
use php_runtime::api::Value;

fn cached_native_class_constant(
    context: &NativeRequestColdState<'_>,
    caller_function: u32,
    class: &str,
    constant: &str,
) -> Option<i64> {
    context
        .class_constant_cache
        .get(&(context.current_dynamic_unit, caller_function))
        .and_then(|classes| classes.get(class))
        .and_then(|constants| constants.get(constant))
        .copied()
}

fn encode_and_cache_native_class_constant(
    context: &mut NativeRequestColdState<'_>,
    caller_function: u32,
    class: &str,
    constant: &str,
    value: Value,
) -> Result<i64, String> {
    let encoded = context.encode_baseline_value(value)?;
    // The cache owns one request-lifetime reference; the original encoded
    // owner is returned to the current expression. Subsequent reads duplicate
    // the native handle instead of rebuilding a copied Rust `Value` graph.
    if let Err(error) = context.retain(encoded) {
        let _ = context.release(encoded);
        return Err(error);
    }
    let previous = context
        .class_constant_cache
        .entry((context.current_dynamic_unit, caller_function))
        .or_default()
        .entry(class.to_owned())
        .or_default()
        .insert(constant.to_owned(), encoded);
    if let Some(previous) = previous {
        context.release(previous)?;
    }
    Ok(encoded)
}

pub(super) fn baseline_class_constant_result_is_cacheable(
    context: &NativeRequestColdState<'_>,
    caller_function: u32,
    class_name: &str,
    constant: &str,
) -> bool {
    if class_name.eq_ignore_ascii_case("static") {
        return false;
    }
    let Some(mut resolved_class) = (match class_name.to_ascii_lowercase().as_str() {
        "self" => {
            native_effective_calling_class(context, caller_function).map(|class| class.name.clone())
        }
        "parent" => native_effective_calling_class(context, caller_function)
            .and_then(|class| class.parent.clone()),
        _ => Some(normalize_class_name(class_name)),
    }) else {
        return false;
    };
    if let Some(original) = context
        .class_aliases
        .get(&normalize_class_name(&resolved_class))
    {
        resolved_class = original.clone();
    }
    pdo_mysql_deprecated_constant(&normalize_class_name(&resolved_class), constant).is_none()
}

pub(super) fn execute_baseline_class_constant(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    caller_function: u32,
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::FetchClassConstant {
        class_name,
        constant,
        ..
    } = &instruction.kind
    else {
        return None;
    };
    let resolved_class = match class_name.to_ascii_lowercase().as_str() {
        "self" => {
            native_effective_calling_class(context, caller_function).map(|class| class.name.clone())
        }
        "static" => context
            .called_classes
            .last()
            .map(|class| class.to_string())
            .or_else(|| {
                native_effective_calling_class(context, caller_function)
                    .map(|class| class.name.clone())
            }),
        "parent" => native_effective_calling_class(context, caller_function)
            .and_then(|class| class.parent.clone()),
        _ => Some(normalize_class_name(class_name)),
    };
    let Some(mut resolved_class) = resolved_class else {
        let message = if class_name.eq_ignore_ascii_case("self") {
            "Cannot use \"self\" in the global scope".to_owned()
        } else if class_name.eq_ignore_ascii_case("parent") {
            "Cannot use \"parent\" when no class scope is active".to_owned()
        } else {
            format!("Cannot resolve class {class_name}")
        };
        return Some(Err(format!("E_PHP_THROW:Error:{message}")));
    };
    if let Some(original) = context
        .class_aliases
        .get(&normalize_class_name(&resolved_class))
    {
        resolved_class = original.clone();
    }
    if constant.eq_ignore_ascii_case("class") {
        let display = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == normalize_class_name(&resolved_class))
            .map_or(resolved_class.as_str(), |class| class.display_name.as_str());
        return Some(
            context.encode_native_string_owner(PhpString::from_bytes(display.as_bytes().to_vec())),
        );
    }
    resolved_class = normalize_class_name(&resolved_class);
    if class_name.eq_ignore_ascii_case("ArrayObject")
        && constant.eq_ignore_ascii_case("ARRAY_AS_PROPS")
    {
        return Some(Ok(2));
    }
    if let Some((legacy, modern)) = pdo_mysql_deprecated_constant(&resolved_class, constant)
        && let Err(error) = emit_native_php_diagnostic(
            context,
            php_runtime::api::PHP_E_DEPRECATED,
            &format!(
                "Constant PDO::{legacy} is deprecated since 8.5, use Pdo\\Mysql::{modern} instead"
            ),
            instruction,
            true,
        )
    {
        return Some(Err(error));
    }
    if let Some(encoded) =
        cached_native_class_constant(context, caller_function, &resolved_class, constant)
    {
        return Some(
            context
                .duplicate_authoritative_native_value(encoded)
                .and_then(|native| {
                    native.map_or_else(|| context.duplicate_baseline_call_argument(encoded), Ok)
                }),
        );
    }
    if let Some(value) = native_internal_class_constant(&resolved_class, constant) {
        return Some(encode_and_cache_native_class_constant(
            context,
            caller_function,
            &resolved_class,
            constant,
            value,
        ));
    }
    let mut candidate = resolved_class.clone();
    while let Some(class) = native_active_class_handle(context, &candidate) {
        if let Some(entry) = class
            .constants
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(constant))
        {
            let caller = native_effective_calling_class(context, caller_function);
            if entry.flags.is_private && caller.is_none_or(|caller| caller.name != class.name) {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot access private constant {}::{}",
                    class.display_name, entry.name
                )));
            }
            if entry.flags.is_protected
                && caller
                    .is_none_or(|caller| !native_class_is_a(context, &caller.name, &class.name))
            {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot access protected constant {}::{}",
                    class.display_name, entry.name
                )));
            }
            if let Some(value) = entry
                .value
                .and_then(|value| context.unit.constants.get(value.index()))
            {
                return Some(
                    native_runtime_constant_value(context, value).and_then(|value| {
                        encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        )
                    }),
                );
            }
            if let Some(reference) = &entry.value_named_constant {
                for name in &reference.names {
                    if let Ok(value) = context.lookup_constant(name) {
                        return Some(encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        ));
                    }
                }
            }
            if let Some(reference) = &entry.value_class_constant {
                let value = php_ir::IrConstant::ClassConstant {
                    class_name: reference.class_name.clone(),
                    display_class_name: reference.display_class_name.clone(),
                    constant_name: reference.constant_name.clone(),
                };
                return Some(
                    native_runtime_constant_value(context, &value).and_then(|value| {
                        encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        )
                    }),
                );
            }
        }
        if let Some(case) = class
            .enum_cases
            .iter()
            .find(|case| case.name.eq_ignore_ascii_case(constant))
            .cloned()
        {
            return Some(encode_native_enum_case(context, &class, &case));
        }
        let Some(parent) = class.parent.clone() else {
            break;
        };
        candidate = normalize_class_name(&parent);
    }
    if context
        .unit
        .classes
        .iter()
        .all(|class| class.name != resolved_class)
        && !native_external_class_exists(context, &resolved_class)
    {
        let normalized = resolved_class.clone();
        let autoload_name = if matches!(
            class_name.to_ascii_lowercase().as_str(),
            "self" | "static" | "parent"
        ) {
            resolved_class.as_str()
        } else {
            class_name.as_str()
        };
        if context.autoload_in_progress.insert(normalized.clone()) {
            let result = invoke_registered_autoload_callbacks_until(
                context,
                autoload_name.as_bytes(),
                instruction,
                |context| native_external_class_exists(context, &resolved_class),
            );
            context.autoload_in_progress.remove(&normalized);
            if let Err(error) = result {
                return Some(Err(error));
            }
        }
    }
    // The late-static class may live in another unit while the requested
    // constant is declared by a parent in the current unit (or vice versa).
    // Walk the combined hierarchy instead of checking only the first external
    // class.
    let mut candidate = resolved_class.clone();
    loop {
        let (owner_unit, class) =
            if let Some(class) = native_active_class_handle(context, &candidate) {
                (None, class)
            } else if let Some((unit, class)) = native_external_class_handle(context, &candidate) {
                (Some(unit), class)
            } else {
                break;
            };
        if let Some(entry) = class
            .constants
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(constant))
        {
            let caller = native_effective_calling_class(context, caller_function);
            if entry.flags.is_private && caller.is_none_or(|caller| caller.name != class.name) {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot access private constant {}::{}",
                    class.display_name, entry.name
                )));
            }
            if entry.flags.is_protected
                && caller
                    .is_none_or(|caller| !native_class_is_a(context, &caller.name, &class.name))
            {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot access protected constant {}::{}",
                    class.display_name, entry.name
                )));
            }
            if let Some(value) = entry.value.and_then(|value| {
                owner_unit.map_or_else(
                    || context.unit.constants.get(value.index()),
                    |unit| {
                        context.dynamic_units.get(unit).and_then(|package| {
                            package.compiled.unit().constants.get(value.index())
                        })
                    },
                )
            }) {
                return Some(
                    native_runtime_constant_value(context, value).and_then(|value| {
                        encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        )
                    }),
                );
            }
            if let Some(reference) = &entry.value_named_constant {
                for name in &reference.names {
                    if let Ok(value) = context.lookup_constant(name) {
                        return Some(encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        ));
                    }
                }
            }
            if let Some(reference) = &entry.value_class_constant {
                let value = php_ir::IrConstant::ClassConstant {
                    class_name: reference.class_name.clone(),
                    display_class_name: reference.display_class_name.clone(),
                    constant_name: reference.constant_name.clone(),
                };
                return Some(
                    native_runtime_constant_value(context, &value).and_then(|value| {
                        encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        )
                    }),
                );
            }
        }
        let Some(parent) = class.parent.clone() else {
            break;
        };
        candidate = normalize_class_name(&parent);
    }
    Some(Err(format!(
        "Undefined constant {resolved_class}::{constant}"
    )))
}
