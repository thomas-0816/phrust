use super::prelude::*;

/// Borrowed mirror of [`property_state_value`]: resolves the property the
/// same way, then runs `f` against a borrowed view of the stored value
/// instead of cloning it out of object storage. `None` (bail to the cloning
/// path) only when object storage is already mutably borrowed. `f` receives
/// `None` when the property is missing or visibility validation fails,
/// mirroring `property_state_value`'s `None` results.
pub(super) fn with_property_state_value<R>(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    object: &ObjectRef,
    property: &str,
    f: &mut dyn FnMut(Option<&Value>) -> R,
) -> Option<R> {
    let Some(class) = lookup_class_in_state(compiled, state, &object.class_name()) else {
        return object
            .try_with_property_lookup(property, property, &mut *f)
            .ok();
    };
    let scope = current_scope_class(compiled, stack);
    let resolved = match lookup_resolved_property_in_state(
        compiled,
        state,
        &class,
        property,
        scope.as_deref(),
    ) {
        Ok(Some(resolved)) => resolved,
        Ok(None) => {
            return object
                .try_with_property_lookup(property, property, &mut *f)
                .ok();
        }
        Err(_) => return Some(f(None)),
    };
    if validate_property_access_in_state(
        compiled,
        state,
        stack,
        &resolved.class,
        &resolved.property,
    )
    .is_err()
    {
        return Some(f(None));
    }
    let storage_name = property_storage_name(&resolved.class, &resolved.property);
    object
        .try_with_property_lookup(&storage_name, property, &mut *f)
        .ok()
}

pub(super) fn property_state_value(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    object: &ObjectRef,
    property: &str,
) -> Option<Value> {
    let Some(class) = lookup_class_in_state(compiled, state, &object.class_name()) else {
        return object.get_property(property);
    };
    let scope = current_scope_class(compiled, stack);
    let Some(resolved) =
        lookup_resolved_property_in_state(compiled, state, &class, property, scope.as_deref())
            .ok()?
    else {
        return object.get_property(property);
    };
    if validate_property_access_in_state(
        compiled,
        state,
        stack,
        &resolved.class,
        &resolved.property,
    )
    .is_err()
    {
        return None;
    }
    let storage_name = property_storage_name(&resolved.class, &resolved.property);
    object
        .get_property(&storage_name)
        .or_else(|| object.get_property(property))
}

pub(super) fn property_dimension_storage_name(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    object: &ObjectRef,
    property: &str,
    write: bool,
) -> Result<String, String> {
    let Some(class) = lookup_class_in_state(compiled, state, &object.class_name()) else {
        return Ok(property.to_owned());
    };
    let scope = current_scope_class(compiled, stack);
    let Some(resolved) =
        lookup_resolved_property_in_state(compiled, state, &class, property, scope.as_deref())?
    else {
        return Ok(property.to_owned());
    };
    validate_property_access_in_state(compiled, state, stack, &resolved.class, &resolved.property)?;
    if write {
        validate_property_set_access_in_state(
            compiled,
            state,
            stack,
            &resolved.class,
            &resolved.property,
        )?;
        validate_property_write(&resolved.class, &resolved.property, object, stack, compiled)?;
        if resolved.property.flags.is_static {
            return Err(format!(
                "E_PHP_VM_NON_STATIC_PROPERTY_ACCESS: property {}::${} is static",
                resolved.class.name, resolved.property.name
            ));
        }
        if !resolved.property.hooks.backed
            && (resolved.property.hooks.get.is_some() || resolved.property.hooks.set.is_some())
        {
            return Err(format!(
                "E_PHP_VM_VIRTUAL_PROPERTY_WRITE: property {}::${} has no backing storage",
                resolved.class.name, resolved.property.name
            ));
        }
    }
    Ok(property_storage_name(&resolved.class, &resolved.property))
}

pub(super) fn unset_property_dim(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    object: &ObjectRef,
    property: &str,
    dims: &[ArrayKey],
) -> Result<(), String> {
    let storage_name =
        property_dimension_storage_name(compiled, state, stack, object, property, true)?;
    let Some(mut current) = object
        .get_property(&storage_name)
        .or_else(|| object.get_property(property))
    else {
        return Ok(());
    };
    unset_dim_value(&mut current, dims);
    object.set_property(storage_name, current);
    Ok(())
}

pub(super) fn magic_property_call_is_active(
    state: &ExecutionState,
    object: &ObjectRef,
    method: &str,
    property: &str,
) -> bool {
    let guard = MagicPropertyCall {
        object_id: object.id(),
        method: normalize_method_name(method),
        property: property.to_owned(),
    };
    state
        .magic_property_stack
        .iter()
        .any(|active| active == &guard)
}

pub(super) fn ensure_property_reference_cell(
    compiled: &CompiledUnit,
    state: Option<&ExecutionState>,
    stack: &CallStack,
    object: &ObjectRef,
    property: &str,
) -> Result<ReferenceCell, String> {
    let storage_name = if let Some(state) = state {
        if let Some(class) = lookup_class_in_state(compiled, state, &object.class_name()) {
            let scope = current_scope_class(compiled, stack);
            if let Some(resolved) = lookup_resolved_property_in_state(
                compiled,
                state,
                &class,
                property,
                scope.as_deref(),
            )? {
                validate_property_access_in_state(
                    compiled,
                    state,
                    stack,
                    &resolved.class,
                    &resolved.property,
                )?;
                validate_property_set_access_in_state(
                    compiled,
                    state,
                    stack,
                    &resolved.class,
                    &resolved.property,
                )?;
                property_storage_name(&resolved.class, &resolved.property)
            } else {
                property.to_owned()
            }
        } else {
            property.to_owned()
        }
    } else if let Some(class) = compiled.lookup_class(&object.class_name()) {
        let scope = current_scope_class(compiled, stack);
        if let Some(resolved) =
            lookup_property_in_hierarchy(compiled, class, property, scope.as_deref())?
        {
            validate_property_access(compiled, stack, resolved.class, resolved.property)?;
            validate_property_set_access(compiled, stack, resolved.class, resolved.property)?;
            property_storage_name(resolved.class, resolved.property)
        } else {
            property.to_owned()
        }
    } else {
        property.to_owned()
    };
    if object.get_property(&storage_name).is_none()
        && let Some(value) = object.get_property(property)
    {
        object.set_property(storage_name.clone(), value);
    }
    Lvalue::object_property(object.clone(), storage_name, LvalueKind::ObjectProperty)
        .ensure_reference_cell()
        .map_err(|error| error.to_string())
}

pub(super) fn ensure_property_dim_reference_cell(
    compiled: &CompiledUnit,
    state: Option<&ExecutionState>,
    stack: &CallStack,
    object: &ObjectRef,
    property: &str,
    dims: &[ArrayKey],
) -> Result<ReferenceCell, String> {
    let property_cell = ensure_property_reference_cell(compiled, state, stack, object, property)?;
    let mut current = property_cell.get();
    if matches!(current, Value::Uninitialized | Value::Null) {
        current = Value::Array(PhpArray::new());
    }
    let cell = ensure_dim_reference_cell_value(&mut current, dims)?;
    property_cell.set(current);
    Ok(cell)
}

pub(super) fn validate_static_property_write(
    compiled: &CompiledUnit,
    stack: &CallStack,
    class: &php_ir::module::ClassEntry,
    property: &php_ir::module::ClassPropertyEntry,
    current: &Value,
) -> Result<(), String> {
    if !(property.flags.is_readonly || class.flags.is_readonly) {
        return Ok(());
    }
    let scope = current_scope_class(compiled, stack);
    if scope.as_deref() != Some(normalize_class_name(&class.name).as_str()) {
        return Err(format!(
            "E_PHP_VM_READONLY_STATIC_PROPERTY_WRITE: property {}::${} is readonly",
            class.name, property.name
        ));
    }
    if !matches!(current, Value::Uninitialized) {
        return Err(format!(
            "E_PHP_VM_READONLY_STATIC_PROPERTY_WRITE: property {}::${} is already initialized",
            class.name, property.name
        ));
    }
    Ok(())
}

pub(super) fn static_property_key(
    class: &php_ir::module::ClassEntry,
    property: &php_ir::module::ClassPropertyEntry,
) -> (String, String) {
    (normalize_class_name(&class.name), property.name.clone())
}

pub(super) fn write_static_property_lvalue(
    static_properties: &mut HashMap<(String, String), Value>,
    key: (String, String),
    default: Value,
    value: Value,
) -> Result<(), String> {
    let entry = static_properties.entry(key).or_insert(default);
    Lvalue::value(entry, LvalueKind::StaticProperty)
        .write_value(value)
        .map_err(|error| error.to_string())
}

pub(super) fn bind_static_property_lvalue(
    static_properties: &mut HashMap<(String, String), Value>,
    key: (String, String),
    default: Value,
    cell: ReferenceCell,
) -> Result<(), String> {
    let entry = static_properties.entry(key).or_insert(default);
    Lvalue::value(entry, LvalueKind::StaticProperty)
        .bind_reference_cell(cell)
        .map_err(|error| error.to_string())
}

pub(super) fn ensure_static_property_dim_reference_cell(
    static_properties: &mut HashMap<(String, String), Value>,
    key: (String, String),
    default: Value,
    dims: &[ArrayKey],
) -> Result<ReferenceCell, String> {
    let entry = static_properties.entry(key).or_insert(default);
    if dims.is_empty() {
        return Lvalue::value(entry, LvalueKind::StaticProperty)
            .ensure_reference_cell()
            .map_err(|error| error.to_string());
    }
    if matches!(entry, Value::Uninitialized | Value::Null) {
        *entry = Value::Array(PhpArray::new());
    }
    ensure_dim_reference_cell_value(entry, dims)
}

pub(super) fn static_property_default(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    stack: &CallStack,
    class: &php_ir::module::ClassEntry,
    property: &php_ir::module::ClassPropertyEntry,
) -> Result<Value, String> {
    if let Some(default) = property.default {
        let owner = class_owner_in_state(compiled, state, &class.name);
        runtime_constant_value(compiled, state, stack, owner.unit(), default)
    } else if let Some(reference) = &property.default_class_constant {
        class_constant_reference_value(compiled, state, reference)
    } else if let Some(reference) = &property.default_named_constant {
        named_constant_reference_value(compiled, state, reference)
    } else if let Some(expr) = &property.default_expr {
        deferred_const_expr_value(compiled, state, expr)
    } else if property.flags.is_typed {
        Ok(Value::Uninitialized)
    } else {
        Ok(Value::Null)
    }
}

pub(super) fn runtime_constant_value(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    stack: &CallStack,
    unit: &IrUnit,
    constant: ConstId,
) -> Result<Value, String> {
    let Some(value) = unit.constants.get(constant.index()) else {
        return Err(format!("invalid constant const:{}", constant.raw()));
    };
    runtime_inline_constant_value(compiled, state, stack, value)
}

pub(super) fn runtime_inline_constant_value(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    stack: &CallStack,
    constant: &IrConstant,
) -> Result<Value, String> {
    match constant {
        IrConstant::Null => Ok(Value::Null),
        IrConstant::Bool(value) => Ok(Value::Bool(*value)),
        IrConstant::Int(value) => Ok(Value::Int(*value)),
        IrConstant::Float(value) => Ok(Value::float(*value)),
        IrConstant::String(value) => Ok(Value::String(PhpString::from_test_str(value))),
        IrConstant::StringBytes(value) => Ok(Value::String(PhpString::from_bytes(value.clone()))),
        IrConstant::NamedConstant(name) => lexical_constant_value(compiled, state, stack, name)?
            .ok_or_else(|| format!("E_PHP_VM_UNDEFINED_CONSTANT: constant {name} is not defined")),
        IrConstant::ClassConstant {
            class_name,
            constant_name,
        } => class_constant_value_by_name(
            compiled,
            state,
            stack,
            &format!("{class_name}::{constant_name}"),
        )?
        .ok_or_else(|| {
            format!(
                "E_PHP_VM_UNDEFINED_CLASS_CONSTANT: constant {class_name}::{constant_name} is not defined"
            )
        }),
        IrConstant::Array(entries) => {
            let mut array = PhpArray::new();
            for entry in entries {
                let value = runtime_inline_constant_value(compiled, state, stack, &entry.value)?;
                if let Some(key) = &entry.key {
                    let key_value = runtime_inline_constant_value(compiled, state, stack, key)?;
                    if let Some(key) = ArrayKey::from_value(&key_value) {
                        array.insert(key, value);
                    } else {
                        array.append(value);
                    }
                } else {
                    array.append(value);
                }
            }
            Ok(Value::Array(array))
        }
    }
}

pub(super) fn deferred_const_expr_value(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    expr: &DeferredConstExpr,
) -> Result<Value, String> {
    match expr {
        DeferredConstExpr::Literal(value) => Ok(inline_constant_value(value)),
        DeferredConstExpr::NamedConstant(reference) => {
            named_constant_reference_value(compiled, state, reference)
        }
        DeferredConstExpr::ClassConstant(reference) => {
            class_constant_reference_value(compiled, state, reference)
        }
        DeferredConstExpr::Array(entries) => {
            let mut array = PhpArray::new();
            for entry in entries {
                let value = deferred_const_expr_value(compiled, state, &entry.value)?;
                if let Some(key) = &entry.key {
                    let key_value = deferred_const_expr_value(compiled, state, key)?;
                    if let Some(key) = ArrayKey::from_value(&key_value) {
                        array.insert(key, value);
                    } else {
                        array.append(value);
                    }
                } else {
                    array.append(value);
                }
            }
            Ok(Value::Array(array))
        }
    }
}
