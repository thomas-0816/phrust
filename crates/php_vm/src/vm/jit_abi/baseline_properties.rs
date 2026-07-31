//! Baseline-only object-property semantic continuation.
//!
//! Prepared optimizing sites use authoritative numeric property slots.
//! Dynamic names, magic methods, unsupported dimensions, and cold object
//! shapes cross this module once.

use super::*;
use php_runtime::api::PhpString;
use php_runtime::api::Value;

fn invoke_native_property_magic(
    context: &mut NativeRequestColdState<'_>,
    class: &php_ir::module::ClassEntry,
    receiver: i64,
    property: &str,
    magic: &str,
    caller_function: u32,
) -> Result<Option<Value>, String> {
    let Some(method) = class
        .methods
        .iter()
        .find(|method| method.name.eq_ignore_ascii_case(magic))
    else {
        return Ok(None);
    };
    if method.function.raw() == caller_function {
        return Ok(None);
    }
    let name =
        context.encode_native_string_owner(PhpString::from_bytes(property.as_bytes().to_vec()))?;
    let value = invoke_native_method(context, method.function, &[receiver, name])?;
    context.decode_baseline_value(value).map(Some)
}

pub(super) fn execute_native_property_instruction(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
    caller_function: u32,
    trusted_continuation: Option<u32>,
) -> Option<Result<i64, String>> {
    use php_ir::InstructionKind;
    let (object, property, dynamic_property) = match &instruction.kind {
        InstructionKind::FetchDynamicProperty { .. }
        | InstructionKind::IssetDynamicProperty { .. }
        | InstructionKind::EmptyDynamicProperty { .. }
        | InstructionKind::IssetDynamicPropertyDim { .. }
        | InstructionKind::EmptyDynamicPropertyDim { .. }
        | InstructionKind::AssignDynamicProperty { .. }
        | InstructionKind::AssignDynamicPropertyDim { .. }
        | InstructionKind::UnsetDynamicPropertyDim { .. }
        | InstructionKind::UnsetDynamicProperty { .. } => {
            let [object, property, ..] = arguments else {
                return Some(Err("dynamic property operands are missing".to_owned()));
            };
            (*object, String::new(), Some(*property))
        }
        InstructionKind::IssetProperty {
            object: _,
            property,
            ..
        }
        | InstructionKind::EmptyProperty {
            object: _,
            property,
            ..
        }
        | InstructionKind::UnsetProperty {
            object: _,
            property,
            ..
        }
        | InstructionKind::UnsetPropertyDim {
            object: _,
            property,
            ..
        }
        | InstructionKind::AssignPropertyDim {
            object: _,
            property,
            ..
        }
        | InstructionKind::IssetPropertyDim {
            object: _,
            property,
            ..
        }
        | InstructionKind::EmptyPropertyDim {
            object: _,
            property,
            ..
        } => {
            let [object, ..] = arguments else {
                return Some(Err("property object operand is missing".to_owned()));
            };
            (*object, property.clone(), None)
        }
        _ => return None,
    };
    let property = if let Some(property) = dynamic_property {
        if let Some(property) = context.native_string_name_bytes(property) {
            String::from_utf8_lossy(&property).into_owned()
        } else {
            match context
                .decode_baseline_value(property)
                .and_then(native_string)
            {
                Ok(property) => String::from_utf8_lossy(&property).into_owned(),
                Err(error) => return Some(Err(error)),
            }
        }
    } else {
        property
    };
    let object_encoded = object;
    if matches!(
        instruction.kind,
        InstructionKind::FetchDynamicProperty { .. }
            | InstructionKind::AssignDynamicProperty { .. }
            | InstructionKind::UnsetDynamicProperty { .. }
    ) && let Some(native_object) = context.native_query_object(object_encoded)
    {
        let normalized_class = normalize_class_name(&native_object.class_name());
        let prepared_class = context.prepared_native_runtime_class(&normalized_class);
        let prepared_entry = prepared_class.as_ref().and_then(|class| {
            class
                .entry
                .properties
                .iter()
                .rev()
                .find(|entry| entry.name == property)
        });
        let class = native_active_class_handle(context, &normalized_class).or_else(|| {
            native_external_class_handle(context, &normalized_class).map(|(_, class)| class)
        });
        let declaration = native_instance_property_declaration(
            context,
            &normalized_class,
            &property,
            caller_function,
        );
        let accessible = prepared_entry
            .is_some_and(|entry| !entry.flags.is_private && !entry.flags.is_protected)
            || declaration.as_ref().is_some_and(|declaration| {
                native_instance_property_readable(context, declaration, caller_function)
            });
        let get_hook_free = prepared_entry.map_or_else(
            || {
                declaration
                    .as_ref()
                    .is_some_and(|declaration| declaration.entry.hooks.get.is_none())
            },
            |entry| entry.hooks.get_function_id.is_none(),
        );
        if prepared_entry.is_some() || declaration.is_some() {
            match &instruction.kind {
                InstructionKind::FetchDynamicProperty { .. } if accessible && get_hook_free => {
                    if let Some(slot) =
                        context.native_declared_property_slot(object_encoded, &property)
                    {
                        if slot.initialized != 0
                            && context.php_handle_is_uninitialized(slot.value)
                            && prepared_entry.map_or_else(
                                || {
                                    declaration.as_ref().is_some_and(|declaration| {
                                        declaration.entry.type_.is_some()
                                    })
                                },
                                |entry| entry.type_.is_some(),
                            )
                        {
                            return Some(Err(format!(
                                "E_PHP_THROW:Error:Typed property {}::${property} must not be accessed before initialization",
                                prepared_class.as_ref().map_or_else(
                                    || {
                                        declaration.as_ref().map_or_else(
                                            || native_object.display_name(),
                                            |declaration| declaration.owner.display_name.clone(),
                                        )
                                    },
                                    |class| class.display_name.clone(),
                                ),
                            )));
                        }
                        let value = if slot.initialized == 0 {
                            Ok(php_jit::jit_encode_constant(u32::MAX))
                        } else {
                            context.duplicate_dereferenced_native_value(slot.value)
                        };
                        let value = match value {
                            Ok(value) => value,
                            Err(error) => return Some(Err(error)),
                        };
                        if let Some(continuation) = trusted_continuation
                            && let Err(error) = context.publish_direct_object_slots(
                                object_encoded,
                                &property,
                                value,
                                i64::from(caller_function),
                                i64::from(continuation),
                                php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_PUBLISHED,
                            )
                        {
                            let _ = context.release(value);
                            return Some(Err(error));
                        }
                        return Some(Ok(value));
                    }
                }
                InstructionKind::AssignDynamicProperty { .. }
                    if accessible
                        && prepared_entry.map_or_else(
                            || {
                                declaration.as_ref().is_some_and(|declaration| {
                                    let entry = &declaration.entry;
                                    !entry.flags.is_readonly
                                        && entry.hooks.get.is_none()
                                        && entry.hooks.set.is_none()
                                })
                            },
                            |entry| {
                                !entry.flags.is_readonly
                                    && entry.hooks.get_function_id.is_none()
                                    && entry.hooks.set_function_id.is_none()
                            },
                        ) =>
                {
                    let Some(value) = arguments.get(2).copied() else {
                        return Some(Err(
                            "dynamic property assignment value is missing".to_owned()
                        ));
                    };
                    let exact_type = if let Some(declaration) = &declaration {
                        declaration.entry.type_.as_ref().is_none_or(|type_| {
                            context.native_encoded_exactly_matches_ir_type(value, type_)
                                == Some(true)
                        })
                    } else {
                        prepared_entry.is_some_and(|entry| entry.type_.is_none())
                    };
                    if exact_type {
                        match context.assign_plain_native_declared_property(
                            object_encoded,
                            value,
                            &property,
                            false,
                        ) {
                            Ok(Some(result)) => {
                                if let Some(continuation) = trusted_continuation
                                    && let Err(error) = context.publish_direct_object_slots(
                                        object_encoded,
                                        &property,
                                        value,
                                        i64::from(caller_function),
                                        i64::from(continuation),
                                        php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_WRITABLE,
                                    )
                                {
                                    return Some(Err(error));
                                }
                                return Some(Ok(result));
                            }
                            Ok(None) => {}
                            Err(error) => return Some(Err(error)),
                        }
                    }
                }
                _ => {}
            }
        } else {
            let dynamic = context.native_dynamic_property_slot(object_encoded, &property);
            match &instruction.kind {
                InstructionKind::FetchDynamicProperty { .. } => {
                    if let Some(Some(slot)) = dynamic {
                        return Some(context.duplicate_dereferenced_native_value(slot.value));
                    }
                }
                InstructionKind::AssignDynamicProperty { .. }
                    if dynamic.is_some_and(|slot| slot.is_some()) =>
                {
                    let Some(value) = arguments.get(2).copied() else {
                        return Some(Err(
                            "dynamic property assignment value is missing".to_owned()
                        ));
                    };
                    match context.assign_plain_native_dynamic_property(
                        object_encoded,
                        value,
                        &property,
                        false,
                    ) {
                        Ok(Some(result)) => return Some(Ok(result)),
                        Ok(None) => {}
                        Err(error) => return Some(Err(error)),
                    }
                }
                InstructionKind::UnsetDynamicProperty { .. } => {
                    let has_unset_magic = class.as_ref().is_some_and(|class| {
                        class.methods.iter().any(|method| {
                            method.name.eq_ignore_ascii_case("__unset")
                                && method.function.raw() != caller_function
                        })
                    });
                    if dynamic.as_ref().is_some_and(Option::is_some)
                        || (dynamic.is_some() && !has_unset_magic)
                    {
                        match context.unset_plain_native_dynamic_property(object_encoded, &property)
                        {
                            Ok(Some(())) => {
                                return Some(Ok(php_jit::jit_encode_constant(u32::MAX)));
                            }
                            Ok(None) => {}
                            Err(error) => return Some(Err(error)),
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let direct_query = match &instruction.kind {
        InstructionKind::IssetProperty { .. } | InstructionKind::IssetDynamicProperty { .. } => {
            Some((true, 0usize, 0usize))
        }
        InstructionKind::EmptyProperty { .. } | InstructionKind::EmptyDynamicProperty { .. } => {
            Some((false, 0usize, 0usize))
        }
        InstructionKind::IssetPropertyDim { dims, .. } => Some((true, 1usize, dims.len())),
        InstructionKind::EmptyPropertyDim { dims, .. } => Some((false, 1usize, dims.len())),
        InstructionKind::IssetDynamicPropertyDim { dims, .. } => Some((true, 2usize, dims.len())),
        InstructionKind::EmptyDynamicPropertyDim { dims, .. } => Some((false, 2usize, dims.len())),
        _ => None,
    };
    if let Some((isset, key_offset, key_count)) = direct_query
        && let Some(native_object) = context.native_query_object(object_encoded)
    {
        let normalized_class = normalize_class_name(&native_object.class_name());
        let prepared_class = context.prepared_native_runtime_class(&normalized_class);
        let prepared_entry = prepared_class.as_ref().and_then(|class| {
            class
                .entry
                .properties
                .iter()
                .rev()
                .find(|entry| entry.name == property)
        });
        let class = native_active_class_handle(context, &normalized_class);
        let declaration = native_instance_property_declaration(
            context,
            &normalized_class,
            &property,
            caller_function,
        );
        let accessible = prepared_entry
            .is_some_and(|entry| !entry.flags.is_private && !entry.flags.is_protected)
            || declaration.as_ref().is_some_and(|declaration| {
                native_instance_property_readable(context, declaration, caller_function)
            });
        let get_hook_free = prepared_entry.map_or_else(
            || {
                declaration
                    .as_ref()
                    .is_none_or(|declaration| declaration.entry.hooks.get.is_none())
            },
            |entry| entry.hooks.get_function_id.is_none(),
        );
        if accessible
            && get_hook_free
            && let Some(slot) = context.native_declared_property_slot(object_encoded, &property)
        {
            let has_isset_magic = prepared_class.as_ref().map_or_else(
                || {
                    class.as_ref().is_some_and(|class| {
                        class.methods.iter().any(|method| {
                            method.name.eq_ignore_ascii_case("__isset")
                                && method.function.raw() != caller_function
                        })
                    })
                },
                |class| {
                    class.entry.methods.iter().any(|method| {
                        method.name.eq_ignore_ascii_case("__isset")
                            && method.function_id != caller_function
                    })
                },
            );
            if slot.initialized != 0 || !has_isset_magic {
                let classified = if slot.initialized == 0 {
                    Some(!isset)
                } else if key_count == 0 {
                    if isset {
                        context.native_encoded_is_set(slot.value)
                    } else {
                        context
                            .native_encoded_truthy(slot.value)
                            .map(|truthy| !truthy)
                    }
                } else {
                    let keys = arguments
                        .get(key_offset..key_offset.saturating_add(key_count))
                        .unwrap_or_default();
                    if keys.len() != key_count {
                        None
                    } else {
                        match context.direct_dimension_path_encoded(slot.value, keys) {
                            Ok(Some(Some(value))) => {
                                if isset {
                                    context.native_encoded_is_set(value)
                                } else {
                                    context.native_encoded_truthy(value).map(|truthy| !truthy)
                                }
                            }
                            Ok(Some(None)) => Some(!isset),
                            Ok(None) | Err(_) => None,
                        }
                    }
                };
                if let Some(result) = classified {
                    if let Some(continuation) = trusted_continuation
                        && let Err(error) = context.publish_direct_object_slots(
                            object_encoded,
                            &property,
                            0,
                            i64::from(caller_function),
                            i64::from(continuation),
                            php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_PUBLISHED,
                        )
                    {
                        return Some(Err(error));
                    }
                    return Some(context.encode_baseline_value(Value::Bool(result)));
                }
            }
        }
        if prepared_entry.is_none()
            && declaration.is_none()
            && let Some(slot) = context.native_dynamic_property_slot(object_encoded, &property)
        {
            let has_isset_magic = prepared_class.as_ref().map_or_else(
                || {
                    class.as_ref().is_some_and(|class| {
                        class.methods.iter().any(|method| {
                            method.name.eq_ignore_ascii_case("__isset")
                                && method.function.raw() != caller_function
                        })
                    })
                },
                |class| {
                    class.entry.methods.iter().any(|method| {
                        method.name.eq_ignore_ascii_case("__isset")
                            && method.function_id != caller_function
                    })
                },
            );
            let classified = match slot {
                None if !has_isset_magic => Some(!isset),
                None => None,
                Some(slot) if key_count == 0 => {
                    if isset {
                        context.native_encoded_is_set(slot.value)
                    } else {
                        context
                            .native_encoded_truthy(slot.value)
                            .map(|truthy| !truthy)
                    }
                }
                Some(slot) => {
                    let keys = arguments
                        .get(key_offset..key_offset.saturating_add(key_count))
                        .unwrap_or_default();
                    if keys.len() != key_count {
                        None
                    } else {
                        match context.direct_dimension_path_encoded(slot.value, keys) {
                            Ok(Some(Some(value))) => {
                                if isset {
                                    context.native_encoded_is_set(value)
                                } else {
                                    context.native_encoded_truthy(value).map(|truthy| !truthy)
                                }
                            }
                            Ok(Some(None)) => Some(!isset),
                            Ok(None) | Err(_) => None,
                        }
                    }
                }
            };
            if let Some(result) = classified {
                return Some(context.encode_baseline_value(Value::Bool(result)));
            }
        }
    }
    let closure_operand = context
        .unit
        .functions
        .get(caller_function as usize)
        .and_then(|function| {
            let object_register = match &instruction.kind {
                InstructionKind::AssignDynamicProperty {
                    object: php_ir::Operand::Register(register),
                    ..
                }
                | InstructionKind::AssignDynamicPropertyDim {
                    object: php_ir::Operand::Register(register),
                    ..
                } => Some(*register),
                _ => None,
            }?;
            let local = function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .find_map(|candidate| match candidate.kind {
                    InstructionKind::LoadLocal { dst, local }
                    | InstructionKind::LoadLocalQuiet { dst, local }
                        if dst == object_register =>
                    {
                        Some(local)
                    }
                    _ => None,
                })?;
            function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(|candidate| match candidate.kind {
                    InstructionKind::StoreLocal {
                        local: target,
                        src: php_ir::Operand::Register(source),
                    } if target == local => function
                        .blocks
                        .iter()
                        .flat_map(|block| &block.instructions)
                        .any(|origin| {
                            matches!(origin.kind, InstructionKind::MakeClosure { dst, .. } if dst == source)
                        }),
                    _ => false,
                })
                .then_some(())
        })
        .is_some();
    let mut decoded_object = match context.decode_baseline_value(object) {
        Ok(value) => value,
        Err(error) => return Some(Err(error)),
    };
    for _ in 0..16 {
        let Value::Reference(reference) = decoded_object else {
            break;
        };
        decoded_object = reference.get();
    }
    if !matches!(decoded_object, Value::Object(_)) {
        let quiet_result = match instruction.kind {
            InstructionKind::IssetProperty { .. }
            | InstructionKind::IssetDynamicProperty { .. }
            | InstructionKind::IssetDynamicPropertyDim { .. }
            | InstructionKind::IssetPropertyDim { .. } => Some(false),
            InstructionKind::EmptyProperty { .. }
            | InstructionKind::EmptyDynamicProperty { .. }
            | InstructionKind::EmptyDynamicPropertyDim { .. }
            | InstructionKind::EmptyPropertyDim { .. } => Some(true),
            _ => None,
        };
        if let Some(value) = quiet_result {
            return Some(context.encode_baseline_value(Value::Bool(value)));
        }
    }
    let object = match decoded_object {
        Value::Object(object) => object,
        Value::Callable(_) => {
            return Some(Err(format!(
                "E_PHP_THROW:Error:Cannot create dynamic property Closure::${property}"
            )));
        }
        _ if closure_operand => {
            return Some(Err(format!(
                "E_PHP_THROW:Error:Cannot create dynamic property Closure::${property}"
            )));
        }
        value => {
            return Some(Err(format!(
                "Attempt to access property {property} on {}",
                native_value_type_name(&value)
            )));
        }
    };
    if let Err(error) = context.materialize_direct_object_alias(&object) {
        return Some(Err(error));
    }
    let normalized_class = normalize_class_name(&object.class_name());
    let class = native_active_class_handle(context, &normalized_class);
    let declaration = native_instance_property_declaration(
        context,
        &normalized_class,
        &property,
        caller_function,
    );
    let result = match &instruction.kind {
        InstructionKind::FetchDynamicProperty { .. } => {
            if object.get_property(&property).is_none()
                && native_calling_class(context, caller_function).is_some_and(|class| {
                    class.methods.iter().any(|method| {
                        method.function.raw() == caller_function
                            && method.name.eq_ignore_ascii_case("__get")
                    })
                })
            {
                return Some(Err(format!(
                    "Undefined property: {}::${property}",
                    object.display_name()
                )));
            }
            object.get_property(&property).unwrap_or(Value::Null)
        }
        InstructionKind::IssetProperty { .. } | InstructionKind::IssetDynamicProperty { .. } => {
            if object.get_property(&property).is_none()
                && let Some(class) = &class
                && let Some(value) = match invoke_native_property_magic(
                    context,
                    class,
                    object_encoded,
                    &property,
                    "__isset",
                    caller_function,
                ) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                }
            {
                Value::Bool(native_property_truthy(&value))
            } else {
                Value::Bool(
                    object
                        .get_property(&property)
                        .is_some_and(|value| native_property_is_set(&value)),
                )
            }
        }
        InstructionKind::EmptyProperty { .. } | InstructionKind::EmptyDynamicProperty { .. } => {
            if object.get_property(&property).is_none()
                && let Some(class) = &class
                && let Some(isset) = match invoke_native_property_magic(
                    context,
                    class,
                    object_encoded,
                    &property,
                    "__isset",
                    caller_function,
                ) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                }
            {
                if native_property_truthy(&isset) {
                    let value = match invoke_native_property_magic(
                        context,
                        class,
                        object_encoded,
                        &property,
                        "__get",
                        caller_function,
                    ) {
                        Ok(value) => value.unwrap_or(Value::Null),
                        Err(error) => return Some(Err(error)),
                    };
                    Value::Bool(!native_property_truthy(&value))
                } else {
                    Value::Bool(true)
                }
            } else {
                Value::Bool(
                    object
                        .get_property(&property)
                        .is_none_or(|value| !native_property_truthy(&value)),
                )
            }
        }
        InstructionKind::IssetPropertyDim { dims, .. }
        | InstructionKind::EmptyPropertyDim { dims, .. }
        | InstructionKind::IssetDynamicPropertyDim { dims, .. }
        | InstructionKind::EmptyDynamicPropertyDim { dims, .. } => {
            let key_offset = match instruction.kind {
                InstructionKind::IssetDynamicPropertyDim { .. }
                | InstructionKind::EmptyDynamicPropertyDim { .. } => 2,
                _ => 1,
            };
            let value = match native_dimension_path_value(
                context,
                object.get_property(&property),
                &arguments[key_offset..],
                dims.len(),
                instruction,
                NativeDimensionOperation::Fetch { quiet: true },
            ) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            if matches!(
                instruction.kind,
                InstructionKind::IssetPropertyDim { .. }
                    | InstructionKind::IssetDynamicPropertyDim { .. }
            ) {
                Value::Bool(value.is_some_and(|value| native_property_is_set(&value)))
            } else {
                Value::Bool(value.is_none_or(|value| !native_property_truthy(&value)))
            }
        }
        InstructionKind::AssignDynamicProperty { .. } => {
            let Some(value) = arguments.get(2).copied() else {
                return Some(Err(
                    "dynamic property assignment value is missing".to_owned()
                ));
            };
            let mut value = match context.decode_baseline_value(value) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            if let Some(declaration) = &declaration {
                let entry = &declaration.entry;
                if !native_instance_property_writable(context, declaration, caller_function) {
                    return Some(Err(format!(
                        "E_PHP_THROW:Error:Cannot access property {}::${property}",
                        declaration.owner.display_name
                    )));
                }
                if let Some(type_) = &entry.type_ {
                    value = native_coerce_call_argument(value, type_, context.unit.strict_types);
                    if !native_value_matches_ir_type_in_context(context, &value, type_) {
                        return Some(Err(format!(
                            "E_PHP_THROW:TypeError:Cannot assign {} to property {}::${} of type {}",
                            native_assignment_type_name(&value),
                            declaration.owner.display_name,
                            property,
                            native_ir_type_name(type_)
                        )));
                    }
                }
            } else if let Some(class) = &class {
                if let Some(method) = class
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("__set"))
                    .filter(|method| method.function.raw() != caller_function)
                {
                    let name = match context.encode_native_string_owner(PhpString::from_bytes(
                        property.as_bytes().to_vec(),
                    )) {
                        Ok(name) => name,
                        Err(error) => return Some(Err(error)),
                    };
                    if let Err(error) = invoke_native_method(
                        context,
                        method.function,
                        &[object_encoded, name, arguments[2]],
                    ) {
                        return Some(Err(error.into()));
                    }
                    return Some(context.encode_baseline_value(value));
                }
            }
            object.set_property(property.clone(), value.clone());
            value
        }
        InstructionKind::UnsetProperty { .. } | InstructionKind::UnsetDynamicProperty { .. } => {
            if let Some(declaration) = &declaration {
                if !native_instance_property_writable(context, declaration, caller_function) {
                    return Some(Err(format!(
                        "E_PHP_THROW:Error:Cannot access property {}::${property}",
                        declaration.owner.display_name
                    )));
                }
            } else if let Some(class) = &class {
                if let Some(method) = class
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("__unset"))
                    .filter(|method| method.function.raw() != caller_function)
                {
                    let name = match context.encode_native_string_owner(PhpString::from_bytes(
                        property.as_bytes().to_vec(),
                    )) {
                        Ok(name) => name,
                        Err(error) => return Some(Err(error)),
                    };
                    if let Err(error) =
                        invoke_native_method(context, method.function, &[object_encoded, name])
                    {
                        return Some(Err(error.into()));
                    }
                    return Some(context.encode_baseline_value(Value::Null));
                }
            }
            object.unset_property(&property);
            Value::Null
        }
        InstructionKind::UnsetPropertyDim { dims, .. }
        | InstructionKind::UnsetDynamicPropertyDim { dims, .. } => {
            let key_offset = usize::from(matches!(
                instruction.kind,
                InstructionKind::UnsetDynamicPropertyDim { .. }
            )) + 1;
            let keys = arguments
                .iter()
                .skip(key_offset)
                .take(dims.len())
                .map(|key| {
                    context
                        .decode_baseline_value(*key)
                        .ok()
                        .and_then(|key| php_runtime::api::ArrayKey::from_value(&key))
                })
                .collect::<Option<Vec<_>>>();
            let Some(keys) = keys else {
                let block = context
                    .unit
                    .functions
                    .get(caller_function as usize)
                    .and_then(|function| {
                        function.blocks.iter().find(|block| {
                            block
                                .instructions
                                .iter()
                                .any(|candidate| candidate == instruction)
                        })
                    })
                    .map(|block| {
                        format!(
                            "{:?}",
                            block
                                .instructions
                                .iter()
                                .map(|candidate| &candidate.kind)
                                .collect::<Vec<_>>()
                        )
                    });
                let decoded = arguments
                    .iter()
                    .map(|value| context.decode_baseline_value(*value))
                    .collect::<Vec<_>>();
                return Some(Err(format!(
                    "property dimension key is invalid: instruction={:?} arguments={arguments:?} decoded={:?} block={:?}",
                    instruction.kind, decoded, block,
                )));
            };
            let _ = object.try_modify_property_value(&property, |value| {
                unset_native_array_dims(value, &keys);
            });
            Value::Null
        }
        InstructionKind::AssignPropertyDim { dims, append, .. }
        | InstructionKind::AssignDynamicPropertyDim { dims, append, .. } => {
            let key_offset = usize::from(matches!(
                instruction.kind,
                InstructionKind::AssignDynamicPropertyDim { .. }
            )) + 1;
            let value_index = key_offset + dims.len();
            let Some(replacement) = arguments.get(value_index).copied() else {
                return Some(Err("property dimension value is missing".to_owned()));
            };
            let replacement = match context.decode_baseline_value(replacement) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let keys = arguments
                .iter()
                .skip(key_offset)
                .take(dims.len())
                .map(|key| {
                    context
                        .decode_baseline_value(*key)
                        .ok()
                        .and_then(|key| php_runtime::api::ArrayKey::from_value(&key))
                })
                .collect::<Option<Vec<_>>>();
            let Some(keys) = keys else {
                let block = context
                    .unit
                    .functions
                    .get(caller_function as usize)
                    .and_then(|function| {
                        function.blocks.iter().find(|block| {
                            block
                                .instructions
                                .iter()
                                .any(|candidate| candidate == instruction)
                        })
                    })
                    .map(|block| {
                        format!(
                            "{:?}",
                            block
                                .instructions
                                .iter()
                                .map(|candidate| &candidate.kind)
                                .collect::<Vec<_>>()
                        )
                    });
                let decoded = arguments
                    .iter()
                    .map(|value| context.decode_baseline_value(*value))
                    .collect::<Vec<_>>();
                return Some(Err(format!(
                    "property dimension key is invalid: instruction={:?} arguments={arguments:?} decoded={:?} block={:?}",
                    instruction.kind, decoded, block,
                )));
            };
            if let Some(declaration) = &declaration
                && declaration.entry.flags.is_readonly
            {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot indirectly modify readonly property {}::${property}",
                    declaration.owner.display_name
                )));
            }
            if let Some(Value::Object(target)) = object.get_property(&property)
                && let Some(target_class) = context.unit.classes.iter().find(|class| {
                    class.name == normalize_class_name(&target.class_name())
                        && class
                            .interfaces
                            .iter()
                            .any(|interface| interface.eq_ignore_ascii_case("ArrayAccess"))
                })
                && let Some(offset_set) = target_class
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("offsetSet"))
                    .map(|method| method.function)
            {
                let key = keys.first().cloned().map_or(Value::Null, |key| match key {
                    php_runtime::api::ArrayKey::Int(value) => Value::Int(value),
                    php_runtime::api::ArrayKey::String(value) => Value::String(value),
                });
                let receiver = match context.encode_native_object_owner(target) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                let key = match context.encode_baseline_value(key) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                let replacement_encoded = match context.encode_baseline_value(replacement.clone()) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                if let Err(error) =
                    invoke_native_method(context, offset_set, &[receiver, key, replacement_encoded])
                {
                    return Some(Err(error.into()));
                }
                return Some(context.encode_baseline_value(replacement));
            }
            let result = replacement.clone();
            let modified = object.try_modify_property_value(&property, |value| {
                assign_native_array_dims(value, &keys, replacement, *append);
            });
            if !matches!(modified, Ok(Some(()))) {
                let mut value = object.get_property(&property).unwrap_or(Value::Null);
                assign_native_array_dims(&mut value, &keys, result.clone(), *append);
                object.set_property(property.clone(), value);
            }
            result
        }
        _ => return None,
    };
    if let Some(continuation) = trusted_continuation {
        let accessible = declaration.as_ref().is_some_and(|declaration| {
            native_instance_property_readable(context, declaration, caller_function)
                && native_instance_property_writable(context, declaration, caller_function)
        });
        let state = match instruction.kind {
            InstructionKind::IssetProperty { .. }
            | InstructionKind::EmptyProperty { .. }
            | InstructionKind::IssetPropertyDim { .. }
            | InstructionKind::EmptyPropertyDim { .. }
                if accessible
                    && declaration
                        .as_ref()
                        .is_none_or(|declaration| declaration.entry.hooks.get.is_none()) =>
            {
                Some(php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_PUBLISHED)
            }
            InstructionKind::AssignPropertyDim { .. }
            | InstructionKind::UnsetPropertyDim { .. }
                if accessible
                    && declaration.as_ref().is_some_and(|declaration| {
                        let entry = &declaration.entry;
                        !entry.flags.is_readonly
                            && entry.hooks.get.is_none()
                            && entry.hooks.set.is_none()
                    })
                    && matches!(object.get_property(&property), Some(Value::Array(_))) =>
            {
                Some(php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_DIMENSION_WRITABLE)
            }
            InstructionKind::UnsetProperty { .. }
                if accessible
                    && declaration.as_ref().is_some_and(|declaration| {
                        let entry = &declaration.entry;
                        !entry.flags.is_readonly
                            && entry.hooks.get.is_none()
                            && entry.hooks.set.is_none()
                    }) =>
            {
                Some(php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_WRITABLE)
            }
            _ => None,
        };
        if let Some(state) = state
            && let Err(error) = context.publish_direct_object_slots(
                object_encoded,
                &property,
                0,
                i64::from(caller_function),
                i64::from(continuation),
                state,
            )
        {
            return Some(Err(error));
        }
    }
    Some(context.encode_baseline_value(result))
}
