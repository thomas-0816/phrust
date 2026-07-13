use super::prelude::*;

pub(super) struct StaticPropertyDimProbe<'a> {
    pub(super) class_name: &'a str,
    pub(super) property: &'a str,
    pub(super) dims: &'a [ArrayKey],
    pub(super) is_empty: bool,
    pub(super) span: IrSpan,
    pub(super) call_site: Option<(u64, FunctionId, BlockId, InstrId)>,
}

pub(super) enum StaticPropertyIssetEmptyError {
    Runtime(String),
    Vm(Box<VmResult>),
}

impl From<String> for StaticPropertyIssetEmptyError {
    fn from(message: String) -> Self {
        Self::Runtime(message)
    }
}

pub(super) fn static_property_isset_empty_result(
    vm: &Vm,
    cursor: ExecutionCursor<'_>,
    class_name: &str,
    property: &str,
    is_empty: bool,
    span: IrSpan,
    call_site: Option<(u64, FunctionId, BlockId, InstrId)>,
) -> Result<bool, StaticPropertyIssetEmptyError> {
    let ExecutionCursor {
        compiled,
        output,
        stack,
        state,
    } = cursor;
    vm.autoload_static_class_if_missing(
        ExecutionCursor::new(compiled, output, stack, state),
        class_name,
        span,
        call_site,
    )
    .map_err(|result| StaticPropertyIssetEmptyError::Vm(Box::new(*result)))?;
    let class = resolve_static_class_name(compiled, state, stack, class_name)?;
    let scope = current_scope_class(compiled, stack);
    let Some(resolved) =
        lookup_resolved_property_in_state(compiled, state, &class, property, scope.as_deref())?
    else {
        return Ok(is_empty);
    };
    if !resolved.property.flags.is_static {
        return Err(StaticPropertyIssetEmptyError::Runtime(format!(
            "E_PHP_VM_NON_STATIC_PROPERTY_ACCESS: property {}::${} is not static",
            resolved.class.name, resolved.property.name
        )));
    }
    if validate_property_access_in_state(
        compiled,
        state,
        stack,
        &resolved.class,
        &resolved.property,
    )
    .is_err()
    {
        return Ok(is_empty);
    }
    let key = static_property_key(&resolved.class, &resolved.property);
    if !state.static_properties.contains_key(&key) {
        let default =
            static_property_default(compiled, state, stack, &resolved.class, &resolved.property)?;
        state.static_properties.insert(key.clone(), default);
    }
    let value = state.static_properties.get(&key);
    if is_empty {
        Ok(match value {
            Some(value) => php_empty(value)?,
            None => true,
        })
    } else {
        Ok(value.is_some_and(|value| !effective_is_uninitialized_or_null(value)))
    }
}

pub(super) fn static_property_dim_isset_empty_result(
    vm: &Vm,
    cursor: ExecutionCursor<'_>,
    probe: StaticPropertyDimProbe<'_>,
) -> Result<bool, StaticPropertyIssetEmptyError> {
    let ExecutionCursor {
        compiled,
        output,
        stack,
        state,
    } = cursor;
    let StaticPropertyDimProbe {
        class_name,
        property,
        dims,
        is_empty,
        span,
        call_site,
    } = probe;
    vm.autoload_static_class_if_missing(
        ExecutionCursor::new(compiled, output, stack, state),
        class_name,
        span,
        call_site,
    )
    .map_err(|result| StaticPropertyIssetEmptyError::Vm(Box::new(*result)))?;
    let class = resolve_static_class_name(compiled, state, stack, class_name)?;
    let scope = current_scope_class(compiled, stack);
    let Some(resolved) =
        lookup_resolved_property_in_state(compiled, state, &class, property, scope.as_deref())?
    else {
        return Ok(is_empty);
    };
    if !resolved.property.flags.is_static {
        return Err(StaticPropertyIssetEmptyError::Runtime(format!(
            "E_PHP_VM_NON_STATIC_PROPERTY_ACCESS: property {}::${} is not static",
            resolved.class.name, resolved.property.name
        )));
    }
    if validate_property_access_in_state(
        compiled,
        state,
        stack,
        &resolved.class,
        &resolved.property,
    )
    .is_err()
    {
        return Ok(is_empty);
    }
    let key = static_property_key(&resolved.class, &resolved.property);
    if !state.static_properties.contains_key(&key) {
        let default =
            static_property_default(compiled, state, stack, &resolved.class, &resolved.property)?;
        state.static_properties.insert(key.clone(), default);
    }
    // Borrowed probe first: cloning the static container for a predicate
    // shares its array handle and forces a copy-on-write separation on the
    // next write to the same static registry array.
    if let Some(stored) = state.static_properties.get(&key) {
        let borrowed = with_borrowed_dim_path(stored, dims, &mut |leaf| {
            if is_empty {
                php_empty_access_value(leaf.unwrap_or(&Value::Uninitialized))
            } else {
                Ok(!matches!(
                    leaf,
                    None | Some(Value::Null) | Some(Value::Uninitialized)
                ))
            }
        });
        if let Some(result) = borrowed {
            return result.map_err(StaticPropertyIssetEmptyError::Runtime);
        }
    }
    let value = state
        .static_properties
        .get(&key)
        .cloned()
        .unwrap_or(Value::Uninitialized);
    let value = fetch_dim_path_value(&value, dims)
        .ok()
        .flatten()
        .unwrap_or(Value::Uninitialized);
    if is_empty {
        Ok(php_empty_access_value(&value)?)
    } else {
        Ok(!matches!(value, Value::Uninitialized | Value::Null))
    }
}

pub(super) fn static_property_dim_unset_result(
    vm: &Vm,
    cursor: ExecutionCursor<'_>,
    class_name: &str,
    property: &str,
    dims: &[ArrayKey],
    span: IrSpan,
    call_site: Option<(u64, FunctionId, BlockId, InstrId)>,
) -> Result<(), StaticPropertyIssetEmptyError> {
    let ExecutionCursor {
        compiled,
        output,
        stack,
        state,
    } = cursor;
    vm.autoload_static_class_if_missing(
        ExecutionCursor::new(compiled, output, stack, state),
        class_name,
        span,
        call_site,
    )
    .map_err(|result| StaticPropertyIssetEmptyError::Vm(Box::new(*result)))?;
    let class = resolve_static_class_name(compiled, state, stack, class_name)?;
    let scope = current_scope_class(compiled, stack);
    let Some(resolved) =
        lookup_resolved_property_in_state(compiled, state, &class, property, scope.as_deref())?
    else {
        return Ok(());
    };
    if !resolved.property.flags.is_static {
        return Err(StaticPropertyIssetEmptyError::Runtime(format!(
            "E_PHP_VM_NON_STATIC_PROPERTY_ACCESS: property {}::${} is not static",
            resolved.class.name, resolved.property.name
        )));
    }
    validate_property_access_in_state(compiled, state, stack, &resolved.class, &resolved.property)?;
    validate_property_set_access_in_state(
        compiled,
        state,
        stack,
        &resolved.class,
        &resolved.property,
    )?;
    let key = static_property_key(&resolved.class, &resolved.property);
    let mut current = if let Some(value) = state.static_properties.get(&key) {
        value.clone()
    } else {
        static_property_default(compiled, state, stack, &resolved.class, &resolved.property)?
    };
    validate_static_property_write(
        compiled,
        stack,
        &resolved.class,
        &resolved.property,
        &current,
    )?;
    unset_dim_value(&mut current, dims);
    state.static_properties.insert(key, current);
    Ok(())
}
