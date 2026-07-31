//! Baseline-only static-property semantic continuation.
//!
//! Prepared optimizing sites access authoritative numeric static-property
//! slots directly. Dynamic names, autoload, visibility, typed references,
//! and nested cold shapes enter this module once.

use super::*;
use php_runtime::api::Value;

pub(super) fn execute_native_static_property(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
    caller_function: u32,
) -> Option<Result<i64, String>> {
    if let php_ir::InstructionKind::BindReferenceFromStaticPropertyDim {
        class_name,
        property,
        dims,
        ..
    } = &instruction.kind
    {
        let keys = match arguments
            .iter()
            .map(|argument| {
                context.decode_baseline_value(*argument).and_then(|value| {
                    php_runtime::api::ArrayKey::from_value(&value)
                        .ok_or_else(|| "Illegal offset type".to_owned())
                })
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(keys) if keys.len() == dims.len() => keys,
            Ok(_) => {
                return Some(Err(
                    "static property dimension operands are missing".to_owned()
                ));
            }
            Err(error) => return Some(Err(error)),
        };
        let calling_class = native_calling_class(context, caller_function);
        let resolved_class = match class_name.to_ascii_lowercase().as_str() {
            "self" => calling_class.map_or_else(|| class_name.clone(), |class| class.name.clone()),
            "parent" => calling_class
                .and_then(|class| class.parent.clone())
                .unwrap_or_else(|| class_name.clone()),
            "static" => context
                .called_classes
                .last()
                .map(|class| class.to_string())
                .or_else(|| calling_class.map(|class| class.name.clone()))
                .unwrap_or_else(|| class_name.clone()),
            _ => class_name.clone(),
        };
        let Some(declaration) =
            native_static_property_declaration(context, &resolved_class, property, caller_function)
        else {
            return Some(Err(format!(
                "E_PHP_THROW:Error:Access to undeclared static property {resolved_class}::${property}"
            )));
        };
        let key = (declaration.owner_name.clone(), property.clone());
        let mut current = match context.direct_static_property_value(&key) {
            Some(Ok(value)) => Some(value),
            Some(Err(error)) => return Some(Err(error)),
            None => match context
                .baseline_values
                .static_property_transfer
                .remove(&key)
            {
                Some(value) => Some(value),
                None => match native_static_property_initial_value(context, &declaration) {
                    Ok(value) => Some(value),
                    Err(error) => return Some(Err(error)),
                },
            },
        };
        if matches!(current, Some(Value::Uninitialized)) && declaration.type_.is_some() {
            let nullable_reference = declaration.type_.as_ref().is_some_and(|type_| {
                native_value_matches_ir_type_in_context(context, &Value::Null, type_)
            });
            if nullable_reference {
                current = Some(Value::Null);
            } else {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot access uninitialized non-nullable property {}::${property} by reference",
                    declaration.owner_display_name
                )));
            }
        }
        if keys.is_empty() {
            let reference = match current.unwrap_or(Value::Null) {
                Value::Reference(reference) => reference,
                value => php_runtime::api::ReferenceCell::new(value),
            };
            if let Err(error) =
                context.bind_typed_static_reference(&reference, &declaration, property)
            {
                return Some(Err(error));
            }
            let replacement = Value::Reference(reference.clone());
            match context.store_direct_static_property_value(&key, replacement.clone()) {
                Some(Ok(())) => {}
                Some(Err(error)) => return Some(Err(error)),
                None => {
                    if let Err(error) =
                        context.ensure_direct_static_property_encoded(&key, replacement)
                    {
                        return Some(Err(error));
                    }
                }
            }
            return Some(context.encode_native_reference_owner(reference));
        }

        // Binding one dimension must put the leaf ReferenceCell into the
        // authoritative array itself. Wrapping the whole static property in a
        // separate root reference before descending leaves later dimension
        // fetches observing the old array snapshot.
        let mut root = current.unwrap_or(Value::Null);
        let reference = match native_nested_array_reference(&mut root, &keys) {
            Ok(reference) => reference,
            Err(error) => return Some(Err(error)),
        };
        match context.store_direct_static_property_value(&key, root.clone()) {
            Some(Ok(())) => {}
            Some(Err(error)) => return Some(Err(error)),
            None => {
                if let Err(error) = context.ensure_direct_static_property_encoded(&key, root) {
                    return Some(Err(error));
                }
            }
        }
        return Some(context.encode_native_reference_owner(reference));
    }
    let (class_name, property, assigned, bind_reference) = match &instruction.kind {
        php_ir::InstructionKind::FetchStaticProperty {
            class_name,
            property,
            ..
        } => (class_name.clone(), property.clone(), None, false),
        php_ir::InstructionKind::AssignStaticProperty {
            class_name,
            property,
            ..
        } => {
            let Some(value) = arguments.first() else {
                return Some(Err("static property assignment value is missing".to_owned()));
            };
            (class_name.clone(), property.clone(), Some(*value), false)
        }
        php_ir::InstructionKind::AssignDynamicStaticProperty { property, .. } => {
            let [class_name, value] = arguments else {
                return Some(Err(
                    "dynamic static property assignment operands are missing".to_owned(),
                ));
            };
            let class_name = match context.decode_baseline_value(*class_name) {
                Ok(Value::Reference(reference)) => reference.get(),
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let class_name = match class_name {
                Value::String(class_name) => class_name.to_string_lossy(),
                Value::Object(object) => object.class_name(),
                value => {
                    return Some(Err(format!(
                        "class name must be a valid object or a string, {} given",
                        native_value_type_name(&value)
                    )));
                }
            };
            (class_name, property.clone(), Some(*value), false)
        }
        php_ir::InstructionKind::FetchDynamicStaticProperty { property, .. } => {
            let Some(class_name) = arguments.first() else {
                return Some(Err(
                    "dynamic static property class operand is missing".to_owned()
                ));
            };
            let class_name = match context.decode_baseline_value(*class_name) {
                Ok(Value::Reference(reference)) => reference.get(),
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let class_name = match class_name {
                Value::String(class_name) => class_name.to_string_lossy(),
                Value::Object(object) => object.class_name(),
                value => {
                    return Some(Err(format!(
                        "class name must be a valid object or a string, {} given",
                        native_value_type_name(&value)
                    )));
                }
            };
            (class_name, property.clone(), None, false)
        }
        php_ir::InstructionKind::BindReferenceStaticProperty {
            class_name,
            property,
            ..
        } => {
            let Some(value) = arguments.first() else {
                return Some(Err("static property reference source is missing".to_owned()));
            };
            (class_name.clone(), property.clone(), Some(*value), true)
        }
        php_ir::InstructionKind::IssetStaticProperty {
            class_name,
            property,
            ..
        }
        | php_ir::InstructionKind::EmptyStaticProperty {
            class_name,
            property,
            ..
        }
        | php_ir::InstructionKind::IssetStaticPropertyDim {
            class_name,
            property,
            ..
        }
        | php_ir::InstructionKind::EmptyStaticPropertyDim {
            class_name,
            property,
            ..
        }
        | php_ir::InstructionKind::UnsetStaticPropertyDim {
            class_name,
            property,
            ..
        } => (class_name.clone(), property.clone(), None, false),
        _ => return None,
    };
    let calling_class = native_calling_class(context, caller_function);
    let resolved_class = match class_name.to_ascii_lowercase().as_str() {
        "self" => calling_class.map_or_else(|| class_name.clone(), |class| class.name.clone()),
        "parent" => calling_class
            .and_then(|class| class.parent.clone())
            .unwrap_or_else(|| class_name.clone()),
        "static" => context
            .called_classes
            .last()
            .map(|class| class.to_string())
            .or_else(|| calling_class.map(|class| class.name.clone()))
            .unwrap_or_else(|| class_name.clone()),
        _ => class_name.clone(),
    };
    let normalized = normalize_class_name(&resolved_class);
    let requested_local_display_name = context
        .unit
        .classes
        .iter()
        .find(|class| class.name == normalized)
        .map(|class| class.display_name.clone());
    if requested_local_display_name.is_none()
        && !native_external_class_exists(context, &resolved_class)
        && context.autoload_in_progress.insert(normalized.clone())
    {
        let result = invoke_registered_autoload_callbacks_until(
            context,
            resolved_class.as_bytes(),
            instruction,
            |context| native_external_class_exists(context, &resolved_class),
        );
        context.autoload_in_progress.remove(&normalized);
        if let Err(error) = result {
            return Some(Err(error));
        }
    }
    let requested_display_name = requested_local_display_name
        .or_else(|| {
            native_external_class_ref(context, &resolved_class)
                .map(|(_, class)| class.display_name.clone())
        })
        .unwrap_or_else(|| resolved_class.clone());
    let Some(declaration) =
        native_static_property_declaration(context, &resolved_class, &property, caller_function)
    else {
        if matches!(
            instruction.kind,
            php_ir::InstructionKind::IssetStaticProperty { .. }
                | php_ir::InstructionKind::IssetStaticPropertyDim { .. }
        ) {
            return Some(context.encode_baseline_value(Value::Bool(false)));
        }
        if matches!(
            instruction.kind,
            php_ir::InstructionKind::EmptyStaticProperty { .. }
                | php_ir::InstructionKind::EmptyStaticPropertyDim { .. }
        ) {
            return Some(context.encode_baseline_value(Value::Bool(true)));
        }
        return Some(Err(format!(
            "E_PHP_THROW:Error:Access to undeclared static property {requested_display_name}::${property}"
        )));
    };
    let display_name = declaration.owner_display_name.clone();
    if (declaration.flags.is_private || declaration.flags.is_protected)
        && !declaration.caller_owns_scope
    {
        return Some(Err(format!(
            "E_PHP_THROW:Error:Cannot access {} property {}::${property}",
            if declaration.flags.is_private {
                "private"
            } else {
                "protected"
            },
            display_name
        )));
    }
    let key = (declaration.owner_name.clone(), property.clone());
    if assigned.is_none()
        && let Some(encoded) = context.direct_static_property_encoded(&key)
    {
        if matches!(
            instruction.kind,
            php_ir::InstructionKind::FetchStaticProperty { .. }
                | php_ir::InstructionKind::FetchDynamicStaticProperty { .. }
        ) && declaration.type_.is_some()
            && context.php_handle_is_uninitialized(encoded)
        {
            return Some(Err(uninitialized_static_property_fetch_error(
                &declaration,
                &property,
            )));
        }
        let direct = match &instruction.kind {
            php_ir::InstructionKind::FetchStaticProperty { .. }
            | php_ir::InstructionKind::FetchDynamicStaticProperty { .. } => {
                Some(context.duplicate_dereferenced_native_value(encoded))
            }
            php_ir::InstructionKind::IssetStaticProperty { .. } => context
                .native_encoded_is_set(encoded)
                .map(|value| context.encode_baseline_value(Value::Bool(value))),
            php_ir::InstructionKind::EmptyStaticProperty { .. } => context
                .native_encoded_truthy(encoded)
                .map(|value| context.encode_baseline_value(Value::Bool(!value))),
            php_ir::InstructionKind::IssetStaticPropertyDim { dims, .. }
            | php_ir::InstructionKind::EmptyStaticPropertyDim { dims, .. } => {
                let isset = matches!(
                    instruction.kind,
                    php_ir::InstructionKind::IssetStaticPropertyDim { .. }
                );
                if arguments.len() != dims.len() {
                    None
                } else {
                    match context.direct_dimension_path_encoded(encoded, arguments) {
                        Ok(Some(Some(value))) => {
                            let classified = if isset {
                                context.native_encoded_is_set(value)
                            } else {
                                context.native_encoded_truthy(value).map(|truthy| !truthy)
                            };
                            classified
                                .map(|value| context.encode_baseline_value(Value::Bool(value)))
                        }
                        Ok(Some(None)) => Some(context.encode_baseline_value(Value::Bool(!isset))),
                        Ok(None) | Err(_) => None,
                    }
                }
            }
            _ => None,
        };
        if let Some(result) = direct {
            return Some(result);
        }
    }
    let result = if bind_reference {
        let Some(source) = assigned else {
            return Some(Err("static property reference source is missing".to_owned()));
        };
        let value = match context.decode_baseline_value(source) {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        };
        let reference = match value {
            Value::Reference(reference) => reference,
            value => php_runtime::api::ReferenceCell::new(value),
        };
        if let Err(error) = context.bind_typed_static_reference(&reference, &declaration, &property)
        {
            return Some(Err(error));
        }
        let replacement = Value::Reference(reference.clone());
        let previous = match context.store_direct_static_property_value(&key, replacement.clone()) {
            Some(Ok(())) => None,
            Some(Err(error)) => return Some(Err(error)),
            None => {
                let previous = context
                    .baseline_values
                    .static_property_transfer
                    .remove(&key);
                if let Err(error) = context.ensure_direct_static_property_encoded(&key, replacement)
                {
                    return Some(Err(error));
                }
                previous
            }
        };
        if let Some(previous) = previous.map(dereference_native_assignment_value)
            && let Value::Object(previous) = previous
            && let Err(error) = context.run_object_destructor(previous)
        {
            return Some(Err(error));
        }
        Value::Reference(reference)
    } else if let Some(assigned) = assigned {
        let mut value = match context.decode_baseline_value(assigned) {
            Ok(value) => dereference_native_assignment_value(value),
            Err(error) => return Some(Err(error)),
        };
        if declaration.owner_unit.is_some() {
            // Closure function ids are unit-local. Preserve the assigning
            // unit when a closure crosses into a class owned by another unit.
            value = native_value_with_owner_unit(value, context.current_dynamic_unit);
        }
        if let Some(type_) = &declaration.type_ {
            value = native_coerce_call_argument(value, type_, context.unit.strict_types);
            if !native_value_matches_ir_type_in_context(context, &value, type_) {
                return Some(Err(format!(
                    "E_PHP_THROW:TypeError:Cannot assign {} to property {}::${} of type {}",
                    native_assignment_type_name(&value),
                    display_name,
                    property,
                    native_ir_type_name(type_)
                )));
            }
        }
        if context.direct_static_property_encoded(&key).is_none()
            && let Some(transferred) = context
                .baseline_values
                .static_property_transfer
                .remove(&key)
            && let Err(error) = context.ensure_direct_static_property_encoded(&key, transferred)
        {
            return Some(Err(error));
        }
        let direct_current = match context.direct_static_property_value(&key) {
            Some(Ok(value)) => Some(value),
            Some(Err(error)) => return Some(Err(error)),
            None => None,
        };
        let existing_reference = direct_current.as_ref().and_then(|current| {
            let Value::Reference(reference) = current else {
                return None;
            };
            Some(reference.clone())
        });
        let previous = if let Some(reference) = existing_reference {
            let previous = reference.get();
            reference.set(value.clone());
            Some(previous)
        } else if direct_current.is_some() {
            match context.store_direct_static_property_value(&key, value.clone()) {
                // Replacing an authoritative direct slot releases its prior
                // owner inside `store_direct_static_property_value`; that
                // release already performs the exact last-owner destructor
                // transition. Returning the decoded alias here and invoking
                // the destructor again re-entered user code twice for one
                // replacement and could corrupt an in-flight compilation.
                Some(Ok(())) => None,
                Some(Err(error)) => return Some(Err(error)),
                None => unreachable!("direct static value lost its published slot"),
            }
        } else {
            if let Err(error) = context.ensure_direct_static_property_encoded(&key, value.clone()) {
                return Some(Err(error));
            }
            None
        };
        context.mark_roots_dirty(RootMutationReason::EnumOrStaticObject);
        if let Some(Value::Object(previous)) = previous
            && !context.object_is_request_rooted(previous.id())
            && let Err(error) = context.run_object_destructor(previous)
        {
            return Some(Err(error));
        }
        value
    } else if let Some(value) = context.direct_static_property_value(&key) {
        match value {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        }
    } else if let Some(value) = context
        .baseline_values
        .static_property_transfer
        .remove(&key)
    {
        value
    } else {
        match native_static_property_initial_value(context, &declaration) {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        }
    };
    if assigned.is_none()
        && matches!(
            instruction.kind,
            php_ir::InstructionKind::FetchStaticProperty { .. }
                | php_ir::InstructionKind::FetchDynamicStaticProperty { .. }
        )
        && declaration.type_.is_some()
        && matches!(result, Value::Uninitialized)
    {
        return Some(Err(uninitialized_static_property_fetch_error(
            &declaration,
            &property,
        )));
    }
    if assigned.is_none()
        && !bind_reference
        && !matches!(
            instruction.kind,
            php_ir::InstructionKind::UnsetStaticPropertyDim { .. }
        )
    {
        let encoded = match context.ensure_direct_static_property_encoded(&key, result.clone()) {
            Ok(encoded) => encoded,
            Err(error) => return Some(Err(error)),
        };
        let direct = match &instruction.kind {
            php_ir::InstructionKind::FetchStaticProperty { .. }
            | php_ir::InstructionKind::FetchDynamicStaticProperty { .. } => {
                Some(context.duplicate_dereferenced_native_value(encoded))
            }
            php_ir::InstructionKind::IssetStaticProperty { .. } => context
                .native_encoded_is_set(encoded)
                .map(|value| context.encode_baseline_value(Value::Bool(value))),
            php_ir::InstructionKind::EmptyStaticProperty { .. } => context
                .native_encoded_truthy(encoded)
                .map(|value| context.encode_baseline_value(Value::Bool(!value))),
            php_ir::InstructionKind::IssetStaticPropertyDim { dims, .. }
            | php_ir::InstructionKind::EmptyStaticPropertyDim { dims, .. } => {
                let isset = matches!(
                    instruction.kind,
                    php_ir::InstructionKind::IssetStaticPropertyDim { .. }
                );
                if arguments.len() != dims.len() {
                    None
                } else {
                    match context.direct_dimension_path_encoded(encoded, arguments) {
                        Ok(Some(Some(value))) => {
                            let classified = if isset {
                                context.native_encoded_is_set(value)
                            } else {
                                context.native_encoded_truthy(value).map(|truthy| !truthy)
                            };
                            classified
                                .map(|value| context.encode_baseline_value(Value::Bool(value)))
                        }
                        Ok(Some(None)) => Some(context.encode_baseline_value(Value::Bool(!isset))),
                        Ok(None) | Err(_) => None,
                    }
                }
            }
            _ => None,
        };
        if let Some(result) = direct {
            return Some(result);
        }
    }
    if assigned.is_some()
        && let Some(encoded) = context.direct_static_property_encoded(&key)
    {
        // Assignment has already moved the authoritative owner into the
        // native static slot above. Returning by re-encoding the temporary
        // Rust value rebuilt the complete array/object graph a second time.
        // The expression result instead receives one owner from the slot that
        // now contains the PHP-visible value. Reference binding returns the
        // reference identity itself; ordinary assignment returns its value.
        return Some(if bind_reference {
            context.retain(encoded).map(|()| encoded)
        } else {
            context.duplicate_dereferenced_native_value(encoded)
        });
    }
    let result = match &instruction.kind {
        php_ir::InstructionKind::IssetStaticProperty { .. } => {
            Value::Bool(!matches!(result, Value::Null | Value::Uninitialized))
        }
        php_ir::InstructionKind::EmptyStaticProperty { .. } => {
            Value::Bool(!native_property_truthy(&result))
        }
        php_ir::InstructionKind::IssetStaticPropertyDim { dims, .. } => {
            let value = match native_dimension_path_value(
                context,
                Some(result),
                arguments,
                dims.len(),
                instruction,
                NativeDimensionOperation::Fetch { quiet: true },
            ) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            Value::Bool(
                value.is_some_and(|value| !matches!(value, Value::Null | Value::Uninitialized)),
            )
        }
        php_ir::InstructionKind::EmptyStaticPropertyDim { dims, .. } => {
            let value = match native_dimension_path_value(
                context,
                Some(result),
                arguments,
                dims.len(),
                instruction,
                NativeDimensionOperation::Fetch { quiet: true },
            ) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            Value::Bool(value.is_none_or(|value| !native_property_truthy(&value)))
        }
        php_ir::InstructionKind::UnsetStaticPropertyDim { dims, .. } => {
            let keys = arguments
                .iter()
                .take(dims.len())
                .map(|encoded| {
                    context
                        .decode_baseline_value(*encoded)
                        .ok()
                        .and_then(|value| php_runtime::api::ArrayKey::from_value(&value))
                })
                .collect::<Option<Vec<_>>>();
            if let Some(keys) = keys {
                match result {
                    Value::Reference(reference) => {
                        let mut value = reference.get();
                        unset_native_array_dims(&mut value, &keys);
                        reference.set(value);
                        context.mark_roots_dirty(RootMutationReason::EnumOrStaticObject);
                    }
                    mut value => {
                        unset_native_array_dims(&mut value, &keys);
                        match context.store_direct_static_property_value(&key, value.clone()) {
                            Some(Ok(())) => {}
                            Some(Err(error)) => return Some(Err(error)),
                            None => {
                                if let Err(error) =
                                    context.ensure_direct_static_property_encoded(&key, value)
                                {
                                    return Some(Err(error));
                                }
                            }
                        }
                    }
                }
            }
            Value::Null
        }
        php_ir::InstructionKind::FetchStaticProperty { .. }
        | php_ir::InstructionKind::FetchDynamicStaticProperty { .. }
        | php_ir::InstructionKind::AssignStaticProperty { .. }
        | php_ir::InstructionKind::AssignDynamicStaticProperty { .. } => {
            dereference_native_assignment_value(result)
        }
        php_ir::InstructionKind::BindReferenceStaticProperty { .. } => result,
        _ => result,
    };
    Some(context.encode_baseline_value(result))
}

fn native_static_property_initial_value(
    context: &mut NativeRequestColdState<'_>,
    declaration: &NativeStaticPropertyDeclaration,
) -> Result<Value, String> {
    let constant = declaration.default.and_then(|constant| {
        if declaration.owner_unit.is_none() {
            context.unit.constants.get(constant.index())
        } else {
            declaration.owner_unit.and_then(|unit| {
                context
                    .dynamic_units
                    .get(unit)
                    .and_then(|package| package.compiled.unit().constants.get(constant.index()))
            })
        }
    });
    match constant {
        Some(constant) => native_runtime_constant_value(context, constant),
        None if declaration.type_.is_some() || declaration.flags.is_typed => {
            Ok(Value::Uninitialized)
        }
        None => Ok(Value::Null),
    }
}

fn uninitialized_static_property_fetch_error(
    declaration: &NativeStaticPropertyDeclaration,
    property: &str,
) -> String {
    format!(
        "E_PHP_THROW:Error:Typed static property {}::${property} must not be accessed before initialization",
        declaration.owner_display_name
    )
}

fn native_nested_array_reference(
    value: &mut Value,
    keys: &[php_runtime::api::ArrayKey],
) -> Result<php_runtime::api::ReferenceCell, String> {
    if keys.is_empty() {
        return Ok(match value {
            Value::Reference(reference) => reference.clone(),
            value => {
                let reference = php_runtime::api::ReferenceCell::new(value.clone());
                *value = Value::Reference(reference.clone());
                reference
            }
        });
    }

    if let Value::Reference(reference) = value {
        let mut referenced = reference.get();
        let result = native_nested_array_reference(&mut referenced, keys)?;
        reference.set(referenced);
        return Ok(result);
    }

    if matches!(value, Value::Null | Value::Uninitialized) {
        *value = Value::Array(php_runtime::api::PhpArray::new());
    }
    let Value::Array(array) = value else {
        return Err(format!(
            "Cannot use a value of type {} as an array",
            native_value_type_name(value)
        ));
    };

    let key = keys[0].clone();
    let mut element = array.get(&key).cloned().unwrap_or(Value::Null);
    let reference = native_nested_array_reference(&mut element, &keys[1..])?;
    array.insert(key, element);
    Ok(reference)
}
