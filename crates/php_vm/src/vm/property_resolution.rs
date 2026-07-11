use super::prelude::*;

pub(super) fn lookup_property_in_hierarchy<'a>(
    compiled: &'a CompiledUnit,
    class: &'a php_ir::module::ClassEntry,
    property: &str,
    caller_scope: Option<&str>,
) -> Result<Option<ResolvedProperty<'a>>, String> {
    if let Some(resolved) =
        lookup_private_property_in_caller_scope(compiled, class, property, caller_scope)?
    {
        return Ok(Some(resolved));
    }
    lookup_property_in_hierarchy_inner(compiled, class, property, caller_scope, &mut Vec::new())
}

pub(super) fn lookup_private_property_in_caller_scope<'a>(
    compiled: &'a CompiledUnit,
    class: &'a php_ir::module::ClassEntry,
    property: &str,
    caller_scope: Option<&str>,
) -> Result<Option<ResolvedProperty<'a>>, String> {
    let Some(scope) = caller_scope else {
        return Ok(None);
    };
    if !class_is_or_extends(compiled, &class.name, scope)? {
        return Ok(None);
    }
    let Some(scope_class) = compiled.lookup_class(scope) else {
        return Ok(None);
    };
    let Some(scope_property) = scope_class
        .properties
        .iter()
        .find(|entry| entry.flags.is_private && entry.name == property)
    else {
        return Ok(None);
    };
    Ok(Some(ResolvedProperty {
        class: scope_class,
        property: scope_property,
    }))
}

pub(super) fn lookup_property_in_hierarchy_inner<'a>(
    compiled: &'a CompiledUnit,
    class: &'a php_ir::module::ClassEntry,
    property: &str,
    caller_scope: Option<&str>,
    seen: &mut Vec<String>,
) -> Result<Option<ResolvedProperty<'a>>, String> {
    let class_name = normalize_class_name(&class.name);
    if seen.iter().any(|name| name == &class_name) {
        return Err(format!(
            "E_PHP_VM_CLASS_INHERITANCE_CYCLE: class {} participates in an inheritance cycle",
            class.name
        ));
    }
    seen.push(class_name.clone());
    if let Some(entry) = class.properties.iter().find(|entry| entry.name == property) {
        if entry.flags.is_private
            && caller_scope.is_some_and(|scope| normalize_class_name(scope) != class_name)
            && class.parent.is_some()
            && let Some(parent) = parent_class(compiled, class)?
            && let Some(parent_property) = lookup_property_in_hierarchy_inner(
                compiled,
                parent,
                entry.name.as_str(),
                caller_scope,
                seen,
            )?
        {
            seen.pop();
            return Ok(Some(parent_property));
        }
        seen.pop();
        return Ok(Some(ResolvedProperty {
            class,
            property: entry,
        }));
    }
    if let Some(parent) = parent_class(compiled, class)? {
        let resolved =
            lookup_property_in_hierarchy_inner(compiled, parent, property, caller_scope, seen)?;
        seen.pop();
        return Ok(resolved);
    }
    seen.pop();
    Ok(None)
}

pub(super) fn lookup_constant_in_hierarchy<'a>(
    compiled: &'a CompiledUnit,
    class: &'a php_ir::module::ClassEntry,
    constant: &str,
    caller_scope: Option<&str>,
) -> Result<Option<ResolvedConstant<'a>>, String> {
    lookup_constant_in_hierarchy_inner(compiled, class, constant, caller_scope, &mut Vec::new())
}

pub(super) fn lookup_constant_in_hierarchy_inner<'a>(
    compiled: &'a CompiledUnit,
    class: &'a php_ir::module::ClassEntry,
    constant: &str,
    caller_scope: Option<&str>,
    seen: &mut Vec<String>,
) -> Result<Option<ResolvedConstant<'a>>, String> {
    let class_name = normalize_class_name(&class.name);
    if seen.iter().any(|name| name == &class_name) {
        return Err(format!(
            "E_PHP_VM_CLASS_INHERITANCE_CYCLE: class {} participates in an inheritance cycle",
            class.name
        ));
    }
    seen.push(class_name.clone());
    if let Some(entry) = class.constants.iter().find(|entry| entry.name == constant) {
        if entry.flags.is_private
            && caller_scope.is_some_and(|scope| normalize_class_name(scope) != class_name)
            && class.parent.is_some()
            && let Some(parent) = parent_class(compiled, class)?
            && let Some(parent_constant) = lookup_constant_in_hierarchy_inner(
                compiled,
                parent,
                entry.name.as_str(),
                caller_scope,
                seen,
            )?
        {
            seen.pop();
            return Ok(Some(parent_constant));
        }
        seen.pop();
        return Ok(Some(ResolvedConstant {
            class,
            constant: entry,
        }));
    }
    if let Some(parent) = parent_class(compiled, class)? {
        let resolved =
            lookup_constant_in_hierarchy_inner(compiled, parent, constant, caller_scope, seen)?;
        seen.pop();
        if resolved
            .as_ref()
            .is_some_and(|resolved| resolved.constant.flags.is_private)
        {
            return Ok(None);
        }
        return Ok(resolved);
    }
    seen.pop();
    Ok(None)
}

pub(super) fn validate_property_access(
    compiled: &CompiledUnit,
    stack: &CallStack,
    class: &php_ir::module::ClassEntry,
    property: &php_ir::module::ClassPropertyEntry,
) -> Result<(), String> {
    if property.flags.is_private {
        let scope = current_scope_class(compiled, stack);
        if scope.as_deref() != Some(normalize_class_name(&class.name).as_str()) {
            return Err(format!(
                "E_PHP_VM_PRIVATE_PROPERTY_ACCESS: Cannot access private property {}::${}",
                class.display_name, property.name
            ));
        }
    }
    if property.flags.is_protected {
        let Some(scope) = current_scope_class(compiled, stack) else {
            return Err(format!(
                "E_PHP_VM_PROTECTED_PROPERTY_ACCESS: Cannot access protected property {}::${}",
                class.display_name, property.name
            ));
        };
        if !protected_scope_is_related(compiled, &scope, &class.name)? {
            return Err(format!(
                "E_PHP_VM_PROTECTED_PROPERTY_ACCESS: Cannot access protected property {}::${}",
                class.display_name, property.name
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_property_access_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    class: &php_ir::module::ClassEntry,
    property: &php_ir::module::ClassPropertyEntry,
) -> Result<(), String> {
    if property.flags.is_private {
        let scope = current_scope_class(compiled, stack);
        if scope.as_deref() != Some(normalize_class_name(&class.name).as_str()) {
            return Err(format!(
                "E_PHP_VM_PRIVATE_PROPERTY_ACCESS: Cannot access private property {}::${}",
                class.display_name, property.name
            ));
        }
    }
    if property.flags.is_protected {
        let Some(scope) = current_scope_class(compiled, stack) else {
            return Err(format!(
                "E_PHP_VM_PROTECTED_PROPERTY_ACCESS: Cannot access protected property {}::${}",
                class.display_name, property.name
            ));
        };
        if !protected_scope_is_related_in_state(compiled, state, &scope, &class.name)? {
            return Err(format!(
                "E_PHP_VM_PROTECTED_PROPERTY_ACCESS: Cannot access protected property {}::${}",
                class.display_name, property.name
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_property_set_access(
    compiled: &CompiledUnit,
    stack: &CallStack,
    class: &php_ir::module::ClassEntry,
    property: &php_ir::module::ClassPropertyEntry,
) -> Result<(), String> {
    if property.flags.set_is_private {
        let scope = current_scope_class(compiled, stack);
        if scope.as_deref() != Some(normalize_class_name(&class.name).as_str()) {
            return Err(format!(
                "E_PHP_VM_PRIVATE_PROPERTY_SET_ACCESS: property {}::${} setter is private",
                class.name, property.name
            ));
        }
    }
    if property.flags.set_is_protected {
        let Some(scope) = current_scope_class(compiled, stack) else {
            return Err(format!(
                "E_PHP_VM_PROTECTED_PROPERTY_SET_ACCESS: property {}::${} setter is protected",
                class.name, property.name
            ));
        };
        if !protected_scope_is_related(compiled, &scope, &class.name)? {
            return Err(format!(
                "E_PHP_VM_PROTECTED_PROPERTY_SET_ACCESS: property {}::${} setter is protected",
                class.name, property.name
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_property_set_access_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    class: &php_ir::module::ClassEntry,
    property: &php_ir::module::ClassPropertyEntry,
) -> Result<(), String> {
    if property.flags.set_is_private {
        let scope = current_scope_class(compiled, stack);
        if scope.as_deref() != Some(normalize_class_name(&class.name).as_str()) {
            return Err(format!(
                "E_PHP_VM_PRIVATE_PROPERTY_SET_ACCESS: property {}::${} setter is private",
                class.name, property.name
            ));
        }
    }
    if property.flags.set_is_protected {
        let Some(scope) = current_scope_class(compiled, stack) else {
            return Err(format!(
                "E_PHP_VM_PROTECTED_PROPERTY_SET_ACCESS: property {}::${} setter is protected",
                class.name, property.name
            ));
        };
        if !protected_scope_is_related_in_state(compiled, state, &scope, &class.name)? {
            return Err(format!(
                "E_PHP_VM_PROTECTED_PROPERTY_SET_ACCESS: property {}::${} setter is protected",
                class.name, property.name
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_constant_access(
    compiled: &CompiledUnit,
    stack: &CallStack,
    class: &php_ir::module::ClassEntry,
    constant: &php_ir::module::ClassConstantEntry,
) -> Result<(), String> {
    if constant.flags.is_private {
        let scope = current_scope_class(compiled, stack);
        if scope.as_deref() != Some(normalize_class_name(&class.name).as_str()) {
            return Err(format!(
                "E_PHP_VM_PRIVATE_CLASS_CONSTANT_ACCESS: Cannot access private constant {}::{}",
                class.display_name, constant.name
            ));
        }
    }
    if constant.flags.is_protected {
        let Some(scope) = current_scope_class(compiled, stack) else {
            return Err(format!(
                "E_PHP_VM_PROTECTED_CLASS_CONSTANT_ACCESS: Cannot access protected constant {}::{}",
                class.display_name, constant.name
            ));
        };
        if !protected_scope_is_related(compiled, &scope, &class.name)? {
            return Err(format!(
                "E_PHP_VM_PROTECTED_CLASS_CONSTANT_ACCESS: Cannot access protected constant {}::{}",
                class.display_name, constant.name
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_property_write(
    class: &php_ir::module::ClassEntry,
    property: &php_ir::module::ClassPropertyEntry,
    object: &ObjectRef,
    stack: &CallStack,
    compiled: &CompiledUnit,
) -> Result<(), String> {
    if !(property.flags.is_readonly || class.flags.is_readonly) {
        return Ok(());
    }
    let scope = current_scope_class(compiled, stack);
    if scope.as_deref() != Some(normalize_class_name(&class.name).as_str()) {
        return Err(format!(
            "E_PHP_VM_READONLY_PROPERTY_WRITE: property {}::${} is readonly",
            class.name, property.name
        ));
    }
    let storage_name = property_storage_name(class, property);
    if !matches!(
        object.get_property(&storage_name),
        None | Some(Value::Uninitialized)
    ) {
        return Err(format!(
            "E_PHP_VM_READONLY_PROPERTY_WRITE: property {}::${} is already initialized",
            class.name, property.name
        ));
    }
    Ok(())
}

pub(super) fn lookup_property_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    property: &str,
) -> Result<Option<php_ir::module::ClassPropertyEntry>, String> {
    let Some(class) = lookup_class_in_state_ref(compiled, state, class_name) else {
        return Ok(None);
    };
    lookup_property_in_state_inner(compiled, state, class.as_ref(), property, &mut Vec::new())
}

pub(super) fn lookup_resolved_property_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: &Arc<php_ir::module::ClassEntry>,
    property: &str,
    caller_scope: Option<&str>,
) -> Result<Option<ResolvedPropertyOwned>, String> {
    if let Some(resolved) = lookup_private_property_in_state_caller_scope(
        compiled,
        state,
        class,
        property,
        caller_scope,
    )? {
        return Ok(Some(resolved));
    }
    lookup_resolved_property_in_state_inner(
        compiled,
        state,
        class,
        property,
        caller_scope,
        &mut Vec::new(),
    )
}

fn lookup_private_property_in_state_caller_scope(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: &php_ir::module::ClassEntry,
    property: &str,
    caller_scope: Option<&str>,
) -> Result<Option<ResolvedPropertyOwned>, String> {
    let Some(scope) = caller_scope else {
        return Ok(None);
    };
    if !class_is_subclass_of_in_state(compiled, state, &class.name, scope)?
        && normalize_class_name(&class.name) != normalize_class_name(scope)
    {
        return Ok(None);
    }
    let Some(scope_class) = lookup_class_in_state_ref(compiled, state, scope) else {
        return Ok(None);
    };
    let Some(scope_property) = scope_class
        .as_ref()
        .properties
        .iter()
        .find(|entry| entry.flags.is_private && entry.name == property)
        .cloned()
    else {
        return Ok(None);
    };
    Ok(Some(ResolvedPropertyOwned {
        class: scope_class.into_arc(),
        property: scope_property,
    }))
}

fn lookup_resolved_property_in_state_inner(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: &Arc<php_ir::module::ClassEntry>,
    property: &str,
    caller_scope: Option<&str>,
    seen: &mut Vec<String>,
) -> Result<Option<ResolvedPropertyOwned>, String> {
    let class_name = normalize_class_name(&class.name);
    if seen.iter().any(|name| name == &class_name) {
        return Err(format!(
            "E_PHP_VM_CLASS_INHERITANCE_CYCLE: class {} participates in an inheritance cycle",
            class.name
        ));
    }
    seen.push(class_name.clone());
    if let Some(entry) = class.properties.iter().find(|entry| entry.name == property) {
        if entry.flags.is_private
            && caller_scope.is_some_and(|scope| normalize_class_name(scope) != class_name)
            && class.parent.is_some()
            && let Some(parent_class) = class
                .parent
                .as_deref()
                .and_then(|parent| lookup_class_in_state_ref(compiled, state, parent))
                .map(ClassLookup::into_arc)
            && let Some(parent_property) = lookup_resolved_property_in_state_inner(
                compiled,
                state,
                &parent_class,
                entry.name.as_str(),
                caller_scope,
                seen,
            )?
        {
            seen.pop();
            return Ok(Some(parent_property));
        }
        seen.pop();
        return Ok(Some(ResolvedPropertyOwned {
            class: Arc::clone(class),
            property: entry.clone(),
        }));
    }
    if let Some(parent_name) = class.parent.as_deref() {
        let Some(parent_class) = lookup_class_in_state_ref(compiled, state, parent_name) else {
            return Err(format!(
                "E_PHP_VM_UNKNOWN_PARENT_CLASS: class {} extends missing class {}",
                class.name, parent_name
            ));
        };
        let parent_class = parent_class.into_arc();
        let resolved = lookup_resolved_property_in_state_inner(
            compiled,
            state,
            &parent_class,
            property,
            caller_scope,
            seen,
        )?;
        seen.pop();
        return Ok(resolved);
    }
    seen.pop();
    Ok(None)
}

fn lookup_property_in_state_inner(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: &php_ir::module::ClassEntry,
    property: &str,
    seen: &mut Vec<String>,
) -> Result<Option<php_ir::module::ClassPropertyEntry>, String> {
    let class_name = normalize_class_name(&class.name);
    if seen.iter().any(|name| name == &class_name) {
        return Err(format!(
            "E_PHP_VM_CLASS_INHERITANCE_CYCLE: class {} participates in an inheritance cycle",
            class.name
        ));
    }
    seen.push(class_name);
    if let Some(property) = class.properties.iter().find(|entry| entry.name == property) {
        seen.pop();
        return Ok(Some(property.clone()));
    }
    if let Some(parent_class) = class
        .parent
        .as_deref()
        .and_then(|parent| lookup_class_in_state_ref(compiled, state, parent))
    {
        let resolved =
            lookup_property_in_state_inner(compiled, state, parent_class.as_ref(), property, seen)?;
        seen.pop();
        return Ok(resolved);
    }
    seen.pop();
    Ok(None)
}
