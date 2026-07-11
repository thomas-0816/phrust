use super::prelude::*;

pub(super) fn class_is_or_extends(
    compiled: &CompiledUnit,
    class_name: &str,
    ancestor_name: &str,
) -> Result<bool, String> {
    let ancestor_name = normalize_class_name(ancestor_name);
    let Some(mut class) = compiled.lookup_class(class_name) else {
        return Ok(false);
    };
    let mut seen = Vec::new();
    loop {
        let current = normalize_class_name(&class.name);
        if current == ancestor_name {
            return Ok(true);
        }
        if seen.iter().any(|name| name == &current) {
            return Err(format!(
                "E_PHP_VM_CLASS_INHERITANCE_CYCLE: class {} participates in an inheritance cycle",
                class.name
            ));
        }
        seen.push(current);
        if let Some(parent) = class.parent.as_deref() {
            let parent = normalize_class_name(parent);
            if internal_runtime_class_entry(&parent).is_some() {
                return Ok(internal_runtime_class_is_or_extends(
                    &parent,
                    &ancestor_name,
                ));
            }
        }
        let Some(parent) = parent_class(compiled, class)? else {
            return Ok(false);
        };
        class = parent;
    }
}

pub(super) fn class_is_or_implements(
    compiled: &CompiledUnit,
    class_name: &str,
    target_name: &str,
) -> Result<bool, String> {
    if class_is_or_extends(compiled, class_name, target_name)? {
        return Ok(true);
    }
    class_implements_interface(compiled, class_name, target_name, &mut Vec::new())
}

pub(super) fn class_extends_php_token(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: &php_ir::module::ClassEntry,
) -> bool {
    let mut parent = class.parent.clone();
    let mut seen = HashSet::new();
    while let Some(parent_name) = parent {
        let normalized = normalize_class_name(&parent_name);
        if is_php_token_runtime_class(&normalized) {
            return true;
        }
        if !seen.insert(normalized.clone()) {
            return false;
        }
        parent = lookup_class_in_state(compiled, state, &normalized)
            .and_then(|entry| entry.parent.clone());
    }
    false
}

pub(super) fn internal_runtime_parent_name(class_name: &str) -> Option<String> {
    let class_name = normalize_class_name(class_name);
    if is_spl_iterator_runtime_class(&class_name) {
        match class_name.as_str() {
            "recursivearrayiterator" => Some(normalize_class_name("ArrayIterator")),
            _ => spl_iterator_class(&class_name).parent,
        }
    } else if is_spl_container_runtime_class(&class_name) {
        spl_container_class(&class_name).parent
    } else if is_spl_heap_runtime_class(&class_name) {
        spl_heap_class(&class_name).parent
    } else if is_spl_file_runtime_class(&class_name) {
        spl_file_class(&class_name).parent
    } else if internal_throwable_instanceof(&class_name, "throwable").is_some() {
        internal_throwable_parent(&class_name).map(normalize_class_name)
    } else {
        None
    }
}

pub(super) fn internal_runtime_class_is_or_extends(class_name: &str, ancestor_name: &str) -> bool {
    let mut class_name = normalize_class_name(class_name);
    let ancestor_name = normalize_class_name(ancestor_name);
    let mut seen = Vec::new();
    loop {
        if class_name == ancestor_name {
            return true;
        }
        if seen.iter().any(|name| name == &class_name) {
            return false;
        }
        seen.push(class_name.clone());
        let parent = internal_runtime_parent_name(&class_name);
        let Some(parent) = parent else {
            return false;
        };
        class_name = parent;
    }
}

pub(super) fn class_implements_interface(
    compiled: &CompiledUnit,
    class_name: &str,
    interface_name: &str,
    seen: &mut Vec<String>,
) -> Result<bool, String> {
    let interface_name = normalize_class_name(interface_name);
    let Some(class) = compiled.lookup_class(class_name) else {
        return Ok(false);
    };
    let current = normalize_class_name(&class.name);
    if seen.iter().any(|name| name == &current) {
        return Err(format!(
            "E_PHP_VM_CLASS_INHERITANCE_CYCLE: class {} participates in an inheritance cycle",
            class.name
        ));
    }
    seen.push(current);
    for interface in &class.interfaces {
        if interface_or_extends(compiled, interface, &interface_name, &mut Vec::new())? {
            seen.pop();
            return Ok(true);
        }
    }
    if let Some(parent) = class.parent.as_deref() {
        let parent = normalize_class_name(parent);
        if internal_runtime_class_entry(&parent).is_some() {
            if interface_or_extends(compiled, &parent, &interface_name, &mut Vec::new())? {
                seen.pop();
                return Ok(true);
            }
            for interface in internal_class_interfaces(&parent) {
                if interface_or_extends(compiled, &interface, &interface_name, &mut Vec::new())? {
                    seen.pop();
                    return Ok(true);
                }
            }
        }
    }
    if let Some(parent) = parent_class(compiled, class)?
        && class_implements_interface(compiled, &parent.name, &interface_name, seen)?
    {
        seen.pop();
        return Ok(true);
    }
    seen.pop();
    Ok(false)
}

pub(super) fn collect_class_interface_display_names(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    seen: &mut Vec<(String, String)>,
) {
    let normalized = normalize_class_name(class_name);
    let Some(class) = lookup_class_in_state(compiled, state, &normalized) else {
        for interface in internal_class_interfaces(&normalized) {
            collect_interface_display_names(compiled, &interface, seen);
        }
        if let Some(parent) = internal_runtime_parent_name(&normalized) {
            collect_class_interface_display_names(compiled, state, &parent, seen);
        }
        return;
    };
    for interface in &class.interfaces {
        collect_interface_display_names(compiled, interface, seen);
    }
    if let Some(parent) = class.parent.as_deref() {
        collect_class_interface_display_names(compiled, state, parent, seen);
    }
}

pub(super) fn collect_interface_display_names(
    compiled: &CompiledUnit,
    interface_name: &str,
    seen: &mut Vec<(String, String)>,
) {
    let normalized = normalize_class_name(interface_name);
    if seen.iter().any(|(name, _)| name == &normalized) {
        return;
    }
    let display = compiled
        .lookup_class(&normalized)
        .map(|class| class.display_name.clone())
        .unwrap_or_else(|| interface_name.to_owned());
    seen.push((normalized.clone(), display));
    if let Some(interface) = compiled.lookup_class(&normalized) {
        for parent in &interface.interfaces {
            collect_interface_display_names(compiled, parent, seen);
        }
    } else {
        for parent in internal_class_interfaces(&normalized) {
            collect_interface_display_names(compiled, &parent, seen);
        }
    }
}

pub(super) fn interface_or_extends(
    compiled: &CompiledUnit,
    interface_name: &str,
    target_name: &str,
    seen: &mut Vec<String>,
) -> Result<bool, String> {
    let interface_name = normalize_class_name(interface_name);
    let target_name = normalize_class_name(target_name);
    if interface_name == target_name {
        return Ok(true);
    }
    let Some(interface) = compiled.lookup_class(&interface_name) else {
        for parent in internal_class_interfaces(&interface_name) {
            if interface_or_extends(compiled, &parent, &target_name, seen)? {
                return Ok(true);
            }
        }
        return Ok(false);
    };
    if seen.iter().any(|name| name == &interface_name) {
        return Err(format!(
            "E_PHP_VM_INTERFACE_INHERITANCE_CYCLE: interface {} participates in an inheritance cycle",
            interface.name
        ));
    }
    seen.push(interface_name);
    for parent in &interface.interfaces {
        if interface_or_extends(compiled, parent, &target_name, seen)? {
            seen.pop();
            return Ok(true);
        }
    }
    seen.pop();
    Ok(false)
}

pub(super) fn class_relation_subject_name(value: &Value) -> Option<String> {
    match value {
        Value::Reference(cell) => class_relation_subject_name(&cell.get()),
        Value::Object(object) => Some(normalize_class_name(&object.class_name())),
        Value::Fiber(_) => Some("fiber".to_owned()),
        Value::Callable(_) => Some("closure".to_owned()),
        _ => None,
    }
}

pub(super) fn class_relation_config_fingerprint(compiled: &CompiledUnit) -> String {
    format!(
        "unit:{}:strict:{}",
        compiled.unit().id.raw(),
        compiled.unit().strict_types
    )
}

pub(super) fn object_instanceof(
    compiled: &CompiledUnit,
    value: &Value,
    class_name: &str,
) -> Result<bool, String> {
    match value {
        Value::Reference(cell) => object_instanceof(compiled, &cell.get(), class_name),
        Value::Fiber(_) => Ok(normalize_class_name(class_name) == "fiber"),
        Value::Callable(_) => Ok(is_closure_runtime_class(class_name)),
        Value::Object(object) => {
            if is_std_class_runtime_class(&object.class_name())
                && is_std_class_runtime_class(class_name)
            {
                return Ok(true);
            }
            if let Some(result) = internal_hash_context_instanceof(&object.class_name(), class_name)
            {
                return Ok(result);
            }
            if let Some(result) = internal_php_token_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) =
                internal_throwable_instanceof(&object.class_name_handle(), class_name)
            {
                return Ok(result);
            }
            if class_is_or_implements(compiled, &object.class_name(), class_name)? {
                return Ok(true);
            }
            if let Some(spl_class) = spl_runtime_marker(object) {
                if let Some(result) = internal_spl_iterator_instanceof(&spl_class, class_name) {
                    return Ok(result);
                }
                if let Some(result) = internal_spl_container_instanceof(&spl_class, class_name) {
                    return Ok(result);
                }
                if let Some(result) = internal_spl_heap_instanceof(&spl_class, class_name) {
                    return Ok(result);
                }
                if let Some(result) = internal_spl_file_instanceof(&spl_class, class_name) {
                    return Ok(result);
                }
            }
            if let Some(result) = internal_spl_iterator_instanceof(&object.class_name(), class_name)
            {
                return Ok(result);
            }
            if let Some(result) =
                internal_spl_container_instanceof(&object.class_name(), class_name)
            {
                return Ok(result);
            }
            if let Some(result) = internal_spl_heap_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_spl_file_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_date_time_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_sqlite_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_pdo_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_mysqli_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_redis_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_memcached_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_soap_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_fileinfo_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_phar_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_zip_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_gd_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) =
                internal_extension_resource_instanceof(&object.class_name(), class_name)
            {
                return Ok(result);
            }
            class_is_or_implements(compiled, &object.class_name(), class_name)
        }
        _ => Ok(false),
    }
}

pub(super) fn object_instanceof_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    value: &Value,
    class_name: &str,
) -> Result<bool, String> {
    match value {
        Value::Reference(cell) => {
            object_instanceof_in_state(compiled, state, &cell.get(), class_name)
        }
        Value::Fiber(_) => Ok(normalize_class_name(class_name) == "fiber"),
        Value::Callable(_) => Ok(is_closure_runtime_class(class_name)),
        Value::Object(object) => {
            if let Some(result) = internal_hash_context_instanceof(&object.class_name(), class_name)
            {
                return Ok(result);
            }
            if let Some(result) = internal_php_token_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) =
                internal_throwable_instanceof(&object.class_name_handle(), class_name)
            {
                return Ok(result);
            }
            if let Some(result) = internal_spl_iterator_instanceof(&object.class_name(), class_name)
            {
                return Ok(result);
            }
            if let Some(result) =
                internal_spl_container_instanceof(&object.class_name(), class_name)
            {
                return Ok(result);
            }
            if let Some(result) = internal_spl_file_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_date_time_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_sqlite_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_pdo_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_mysqli_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_redis_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_memcached_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_soap_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_fileinfo_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_phar_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_zip_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) = internal_gd_instanceof(&object.class_name(), class_name) {
                return Ok(result);
            }
            if let Some(result) =
                internal_extension_resource_instanceof(&object.class_name(), class_name)
            {
                return Ok(result);
            }
            class_is_a_in_state(compiled, state, &object.class_name(), class_name)
        }
        _ => Ok(false),
    }
}

pub(super) fn iterator_function_accepts_source(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    value: &Value,
) -> Result<bool, String> {
    match effective_value(value) {
        Value::Array(_) | Value::Generator(_) => Ok(true),
        Value::Object(object) => {
            if let Some(spl_class) = spl_runtime_marker(&object) {
                return Ok(internal_spl_iterator_instanceof(&spl_class, "Traversable")
                    .or_else(|| internal_spl_container_instanceof(&spl_class, "Traversable"))
                    .or_else(|| internal_spl_heap_instanceof(&spl_class, "Traversable"))
                    .or_else(|| internal_spl_file_instanceof(&spl_class, "Traversable"))
                    .unwrap_or(false));
            }
            object_instanceof_in_state(compiled, state, &Value::Object(object), "Traversable")
        }
        _ => Ok(false),
    }
}

pub(super) fn class_display_name(
    compiled: &CompiledUnit,
    normalized_class: &str,
) -> Option<String> {
    compiled
        .unit()
        .classes
        .iter()
        .find(|class| normalize_class_name(&class.name) == normalized_class)
        .map(|class| class.display_name.clone())
}

pub(super) fn method_display_name(
    compiled: &CompiledUnit,
    method: &php_ir::module::ClassMethodEntry,
) -> String {
    compiled
        .unit()
        .functions
        .get(method.function.index())
        .and_then(|function| {
            function
                .name
                .split_once("::")
                .map(|(_, name)| name.to_owned())
        })
        .unwrap_or_else(|| method.name.clone())
}

pub(super) fn visible_class_methods(
    compiled: &CompiledUnit,
    stack: &CallStack,
    state: &ExecutionState,
    class: &php_ir::module::ClassEntry,
) -> Vec<String> {
    let scope = current_scope_class(compiled, stack);
    let mut methods = Vec::new();
    let mut seen = BTreeSet::new();
    for class in class_hierarchy(compiled, state, class) {
        for method in &class.methods {
            let normalized = normalize_method_name(&method.name);
            if seen.contains(&normalized) {
                continue;
            }
            seen.insert(normalized);
            if class_member_visible(
                compiled,
                scope.as_deref(),
                &class,
                method.flags.is_private,
                method.flags.is_protected,
            ) {
                methods.push(method_display_name(compiled, method));
            }
        }
    }
    methods
}

pub(super) fn visible_class_vars(
    compiled: &CompiledUnit,
    stack: &CallStack,
    state: &mut ExecutionState,
    class: &php_ir::module::ClassEntry,
) -> PhpArray {
    let scope = current_scope_class(compiled, stack);
    let mut array = PhpArray::new();
    let mut classes = class_hierarchy(compiled, state, class);
    classes.reverse();
    for class in classes {
        for property in &class.properties {
            if class_member_visible(
                compiled,
                scope.as_deref(),
                &class,
                property.flags.is_private,
                property.flags.is_protected,
            ) {
                let value = static_property_default(compiled, state, stack, &class, property)
                    .unwrap_or(Value::Uninitialized);
                array.insert(php_string_key(&property.name), value);
            }
        }
    }
    array
}

fn class_hierarchy(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: &php_ir::module::ClassEntry,
) -> Vec<Arc<php_ir::module::ClassEntry>> {
    // Shared handles: entries are large (method/property tables) and callers
    // only read; one clone for the starting borrow, none per ancestor.
    let mut classes = Vec::new();
    let mut current = Some(Arc::new(class.clone()));
    let mut seen = BTreeSet::new();
    while let Some(class) = current {
        let normalized = normalize_class_name(&class.name);
        if !seen.insert(normalized) {
            break;
        }
        current = class
            .parent
            .as_deref()
            .and_then(|parent| lookup_class_in_state(compiled, state, parent));
        classes.push(class);
    }
    classes
}

pub(super) fn class_member_visible(
    compiled: &CompiledUnit,
    scope: Option<&str>,
    declaring_class: &php_ir::module::ClassEntry,
    is_private: bool,
    is_protected: bool,
) -> bool {
    if is_private {
        return scope.is_some_and(|scope| {
            normalize_class_name(scope) == normalize_class_name(&declaring_class.name)
        });
    }
    if is_protected {
        return scope.is_some_and(|scope| {
            class_is_or_extends(compiled, scope, &declaring_class.name).unwrap_or(false)
        });
    }
    true
}

/// Consuming variant of [`call_args_from_php_array`]: argument arrays that
/// reach `call_user_func_array` as sole-owner temporaries (hook dispatch
/// building `array_slice(...)` results) move their values into the call
/// instead of cloning every element.
pub(super) fn class_name_from_object_or_string(value: &Value) -> Result<String, String> {
    if let Some(object) = object_from_value(value) {
        return Ok(object.class_name());
    }
    to_string(value).map(|name| name.to_string_lossy())
}

pub(super) fn dynamic_static_class_name_from_value(value: &Value) -> Result<String, String> {
    match effective_value(value) {
        Value::String(name) => Ok(display_class_name(&name.to_string_lossy())),
        Value::Object(object) => Ok(object.class_name()),
        other => Err(format!(
            "E_PHP_VM_INVALID_DYNAMIC_CLASS_NAME: class name must be string or object, {} given",
            value_type_name(&other)
        )),
    }
}

pub(super) fn lookup_method_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    method: &str,
) -> Result<Option<php_ir::module::ClassMethodEntry>, String> {
    lookup_resolved_method_in_state(compiled, state, class_name, method, None)
        .map(|resolved| resolved.map(|resolved| resolved.method))
}

pub(super) fn lookup_resolved_method_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    method: &str,
    caller_scope: Option<&str>,
) -> Result<Option<ResolvedMethodOwned>, String> {
    let Some(class) = lookup_class_in_state_ref(compiled, state, class_name) else {
        return Ok(None);
    };
    if let Some(resolved) = lookup_private_method_in_caller_scope_in_state(
        compiled,
        state,
        class.as_ref(),
        method,
        caller_scope,
    )? {
        return Ok(Some(resolved));
    }
    lookup_resolved_method_in_state_inner(
        compiled,
        state,
        class.into_arc(),
        method,
        caller_scope,
        &mut Vec::new(),
    )
}

fn lookup_private_method_in_caller_scope_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: &php_ir::module::ClassEntry,
    method: &str,
    caller_scope: Option<&str>,
) -> Result<Option<ResolvedMethodOwned>, String> {
    let Some(scope) = caller_scope else {
        return Ok(None);
    };
    if !class_is_a_in_state(compiled, state, &class.name, scope)? {
        return Ok(None);
    }
    let Some(scope_class) = lookup_class_in_state_ref(compiled, state, scope) else {
        return Ok(None);
    };
    let normalized = normalize_method_name(method);
    let Some(scope_method) = scope_class
        .as_ref()
        .methods
        .iter()
        .find(|entry| entry.flags.is_private && entry.name.eq_ignore_ascii_case(&normalized))
        .cloned()
    else {
        return Ok(None);
    };
    Ok(Some(ResolvedMethodOwned {
        class: scope_class.into_arc(),
        method: scope_method,
    }))
}

fn lookup_resolved_method_in_state_inner(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: Arc<php_ir::module::ClassEntry>,
    method: &str,
    caller_scope: Option<&str>,
    seen: &mut Vec<String>,
) -> Result<Option<ResolvedMethodOwned>, String> {
    let class_name = normalize_class_name(&class.name);
    if seen.iter().any(|name| name == &class_name) {
        return Err(format!(
            "E_PHP_VM_CLASS_INHERITANCE_CYCLE: class {} participates in an inheritance cycle",
            class.name
        ));
    }
    seen.push(class_name.clone());
    let normalized = normalize_method_name(method);
    if let Some(method) = class
        .methods
        .iter()
        .find(|entry| entry.name.eq_ignore_ascii_case(&normalized))
    {
        if method.flags.is_private
            && caller_scope.is_some_and(|scope| normalize_class_name(scope) != class_name)
            && class.parent.is_some()
            && let Some(parent_class) = class
                .parent
                .as_deref()
                .and_then(|parent| lookup_class_in_state_ref(compiled, state, parent))
            && let Some(parent_method) = lookup_resolved_method_in_state_inner(
                compiled,
                state,
                parent_class.into_arc(),
                method.name.as_str(),
                caller_scope,
                seen,
            )?
        {
            seen.pop();
            return Ok(Some(parent_method));
        }
        seen.pop();
        return Ok(Some(ResolvedMethodOwned {
            class: Arc::clone(&class),
            method: method.clone(),
        }));
    }
    if let Some(parent_class) = class
        .parent
        .as_deref()
        .and_then(|parent| lookup_class_in_state_ref(compiled, state, parent))
    {
        let resolved = lookup_resolved_method_in_state_inner(
            compiled,
            state,
            parent_class.into_arc(),
            method,
            caller_scope,
            seen,
        )?;
        seen.pop();
        return Ok(resolved);
    }
    seen.pop();
    Ok(None)
}

pub(super) fn class_is_subclass_of_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    target_name: &str,
) -> Result<bool, String> {
    let class_name = normalize_class_name(class_name);
    let target_name = normalize_class_name(target_name);
    if class_name == target_name {
        return Ok(false);
    }
    class_extends_in_state(compiled, state, &class_name, &target_name, &mut Vec::new())?
        .then_some(true)
        .map(Ok)
        .unwrap_or_else(|| {
            class_implements_in_state(compiled, state, &class_name, &target_name, &mut Vec::new())
        })
}

pub(super) fn class_is_a_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    target_name: &str,
) -> Result<bool, String> {
    let class_name = normalize_class_name(class_name);
    let target_name = normalize_class_name(target_name);
    if class_name == target_name {
        return Ok(class_exists_for_is_a(compiled, state, &class_name));
    }
    class_is_subclass_of_in_state(compiled, state, &class_name, &target_name)
}

pub(super) fn class_is_or_extends_internal_throwable_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
) -> Result<bool, String> {
    let mut current = normalize_class_name(class_name);
    let mut seen = Vec::new();
    loop {
        if internal_throwable_instanceof(&current, "throwable").is_some() {
            return Ok(true);
        }
        let Some(class) = lookup_class_in_state(compiled, state, &current) else {
            return Ok(false);
        };
        let normalized = normalize_class_name(&class.name);
        if seen.iter().any(|name| name == &normalized) {
            return Err(format!(
                "E_PHP_VM_CLASS_INHERITANCE_CYCLE: class {} participates in an inheritance cycle",
                class.name
            ));
        }
        seen.push(normalized);
        let Some(parent) = class.parent.as_deref() else {
            return Ok(false);
        };
        current = normalize_class_name(parent);
    }
}

pub(super) fn iterator_function_accepts_iterable(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    value: &Value,
) -> Result<bool, String> {
    match effective_value(value) {
        Value::Array(_) | Value::Generator(_) => Ok(true),
        Value::Object(object) => {
            let class_name = object.class_name();
            if let Some(spl_class) = spl_runtime_marker(&object) {
                return Ok(internal_spl_iterator_instanceof(&spl_class, "Traversable")
                    .or_else(|| internal_spl_container_instanceof(&spl_class, "Traversable"))
                    .or_else(|| internal_spl_heap_instanceof(&spl_class, "Traversable"))
                    .or_else(|| internal_spl_file_instanceof(&spl_class, "Traversable"))
                    .unwrap_or(false));
            }
            Ok(
                class_is_a_in_state(compiled, state, &class_name, "Iterator")?
                    || class_is_a_in_state(compiled, state, &class_name, "IteratorAggregate")?,
            )
        }
        _ => Ok(false),
    }
}

fn class_exists_for_is_a(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
) -> bool {
    lookup_class_in_state(compiled, state, class_name).is_some()
}

fn class_extends_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    target_name: &str,
    seen: &mut Vec<String>,
) -> Result<bool, String> {
    let Some(class) = lookup_class_in_state(compiled, state, class_name) else {
        return Ok(false);
    };
    let current = normalize_class_name(&class.name);
    if seen.iter().any(|name| name == &current) {
        return Err(format!(
            "E_PHP_VM_CLASS_INHERITANCE_CYCLE: class {} participates in an inheritance cycle",
            class.name
        ));
    }
    seen.push(current);
    let Some(parent) = class.parent.as_deref() else {
        seen.pop();
        return Ok(false);
    };
    let parent = normalize_class_name(parent);
    if parent == target_name {
        seen.pop();
        return Ok(true);
    }
    let result = class_extends_in_state(compiled, state, &parent, target_name, seen)?;
    seen.pop();
    Ok(result)
}

pub(super) fn class_implements_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    target_name: &str,
    seen: &mut Vec<String>,
) -> Result<bool, String> {
    let target_name = normalize_class_name(target_name);
    let Some(class) = lookup_class_in_state(compiled, state, class_name) else {
        return Ok(false);
    };
    let current = normalize_class_name(&class.name);
    if seen.iter().any(|name| name == &current) {
        return Err(format!(
            "E_PHP_VM_CLASS_INHERITANCE_CYCLE: class {} participates in an inheritance cycle",
            class.name
        ));
    }
    seen.push(current);
    for interface in &class.interfaces {
        let interface = normalize_class_name(interface);
        if interface == target_name
            || interface_extends_in_state(
                compiled,
                state,
                &interface,
                &target_name,
                &mut Vec::new(),
            )?
        {
            seen.pop();
            return Ok(true);
        }
    }
    if let Some(parent) = class.parent.as_deref()
        && class_implements_in_state(compiled, state, parent, &target_name, seen)?
    {
        seen.pop();
        return Ok(true);
    }
    seen.pop();
    Ok(false)
}

pub(super) fn userland_arrayaccess_object(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    value: &Value,
) -> Result<Option<ObjectRef>, String> {
    let Value::Object(object) = effective_value(value) else {
        return Ok(None);
    };
    userland_arrayaccess_object_from_object(compiled, state, object)
}

pub(super) fn userland_arrayaccess_object_from_object(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    object: ObjectRef,
) -> Result<Option<ObjectRef>, String> {
    if spl_runtime_marker(&object).is_some_and(|class| is_spl_array_access_runtime_class(&class)) {
        return Ok(None);
    }
    if class_implements_in_state(
        compiled,
        state,
        &object.class_name(),
        "ArrayAccess",
        &mut Vec::new(),
    )? {
        Ok(Some(object))
    } else {
        Ok(None)
    }
}

pub(super) fn arrayaccess_object(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    value: &Value,
) -> Result<Option<ObjectRef>, String> {
    let Value::Object(object) = effective_value(value) else {
        return Ok(None);
    };
    if spl_runtime_marker(&object).is_some_and(|class| is_spl_array_access_runtime_class(&class)) {
        return Ok(Some(object));
    }
    userland_arrayaccess_object(compiled, state, &Value::Object(object))
}

fn interface_extends_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    interface_name: &str,
    target_name: &str,
    seen: &mut Vec<String>,
) -> Result<bool, String> {
    let interface_name = normalize_class_name(interface_name);
    let target_name = normalize_class_name(target_name);
    if interface_name == target_name {
        return Ok(true);
    }
    let Some(interface) = lookup_class_in_state(compiled, state, &interface_name) else {
        for parent in internal_class_interfaces(&interface_name) {
            if interface_extends_in_state(compiled, state, &parent, &target_name, seen)? {
                return Ok(true);
            }
        }
        return Ok(false);
    };
    if seen.iter().any(|name| name == &interface_name) {
        return Err(format!(
            "E_PHP_VM_INTERFACE_INHERITANCE_CYCLE: interface {} participates in an inheritance cycle",
            interface.name
        ));
    }
    seen.push(interface_name);
    for parent in &interface.interfaces {
        if interface_extends_in_state(compiled, state, parent, &target_name, seen)? {
            seen.pop();
            return Ok(true);
        }
    }
    seen.pop();
    Ok(false)
}

pub(super) fn declared_classes_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
) -> Vec<php_ir::module::ClassEntry> {
    let mut seen = BTreeSet::new();
    let mut classes = Vec::new();
    for class in compiled.unit().classes.iter().chain(
        state
            .dynamic_classes
            .iter()
            .map(|entry| entry.class.as_ref()),
    ) {
        let normalized = normalize_class_name(&class.name);
        if seen.insert(normalized) {
            classes.push(class.clone());
        }
    }
    classes
}
