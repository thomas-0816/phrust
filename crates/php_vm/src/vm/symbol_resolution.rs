use super::prelude::*;

#[derive(Clone, Copy)]
pub(super) struct ResolvedMethod<'a> {
    pub(super) class: &'a php_ir::module::ClassEntry,
    pub(super) method: &'a php_ir::module::ClassMethodEntry,
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedMethodOwned {
    pub(super) class: Arc<php_ir::module::ClassEntry>,
    pub(super) method: php_ir::module::ClassMethodEntry,
}

#[derive(Clone, Copy)]
pub(super) struct ResolvedProperty<'a> {
    pub(super) class: &'a php_ir::module::ClassEntry,
    pub(super) property: &'a php_ir::module::ClassPropertyEntry,
}

#[derive(Clone)]
pub(super) struct ResolvedPropertyOwned {
    pub(super) class: Arc<php_ir::module::ClassEntry>,
    pub(super) property: php_ir::module::ClassPropertyEntry,
}

pub(super) enum ClassLookup {
    /// Boxed: `ClassEntry` is ~272 bytes and would dominate the enum size.
    Owned(Box<php_ir::module::ClassEntry>),
    /// Shared handle into a class table; cloning is a cheap refcount bump.
    Shared(Arc<php_ir::module::ClassEntry>),
}

impl ClassLookup {
    pub(super) fn as_ref(&self) -> &php_ir::module::ClassEntry {
        match self {
            Self::Owned(class) => class,
            Self::Shared(class) => class,
        }
    }

    pub(super) fn into_arc(self) -> Arc<php_ir::module::ClassEntry> {
        match self {
            Self::Shared(class) => class,
            Self::Owned(class) => Arc::new(*class),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ResolvedConstant<'a> {
    pub(super) class: &'a php_ir::module::ClassEntry,
    pub(super) constant: &'a php_ir::module::ClassConstantEntry,
}

pub(super) struct ResolvedConstantOwned {
    pub(super) class: php_ir::module::ClassEntry,
    pub(super) constant: php_ir::module::ClassConstantEntry,
}

pub(super) fn validate_method_callable_in_state_scope(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    scope: Option<&str>,
    class: &php_ir::module::ClassEntry,
    method: &php_ir::module::ClassMethodEntry,
) -> Result<(), String> {
    if method.flags.is_abstract {
        return Err(format!(
            "E_PHP_VM_ABSTRACT_METHOD_CALL: Cannot call abstract method {}::{}()",
            class.display_name, method.name
        ));
    }
    if method.flags.is_private && scope != Some(normalize_class_name(&class.name).as_str()) {
        return Err(format!(
            "E_PHP_VM_PRIVATE_METHOD_ACCESS: Call to private method {}::{}() from {}",
            class.display_name,
            method.name,
            scope_description(scope)
        ));
    }
    if method.flags.is_protected {
        let allowed = match scope {
            Some(scope) => {
                protected_scope_is_related_in_state(compiled, state, scope, &class.name)?
            }
            None => false,
        };
        if !allowed {
            return Err(format!(
                "E_PHP_VM_PROTECTED_METHOD_ACCESS: Call to protected method {}::{}() from {}",
                class.display_name,
                method.name,
                scope_description(scope)
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_constructor_callable_in_state_scope(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    scope: Option<&str>,
    class: &php_ir::module::ClassEntry,
    method: &php_ir::module::ClassMethodEntry,
) -> Result<(), String> {
    if method.flags.is_abstract {
        return Err(format!(
            "E_PHP_VM_ABSTRACT_METHOD_CALL: Cannot call abstract method {}::{}()",
            class.display_name, method.name
        ));
    }
    if method.flags.is_private && scope != Some(normalize_class_name(&class.name).as_str()) {
        return Err(format!(
            "E_PHP_VM_PRIVATE_METHOD_ACCESS: Call to private {}::__construct() from {}",
            class.display_name,
            scope_description(scope)
        ));
    }
    if method.flags.is_protected {
        let allowed = match scope {
            Some(scope) => {
                protected_scope_is_related_in_state(compiled, state, scope, &class.name)?
            }
            None => false,
        };
        if !allowed {
            return Err(format!(
                "E_PHP_VM_PROTECTED_METHOD_ACCESS: Call to protected {}::__construct() from {}",
                class.display_name,
                scope_description(scope)
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_scoped_constructor_callable_in_state_scope(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    scope: Option<&str>,
    class: &php_ir::module::ClassEntry,
    method: &php_ir::module::ClassMethodEntry,
) -> Result<(), String> {
    if method.flags.is_abstract {
        return Err(format!(
            "E_PHP_VM_ABSTRACT_METHOD_CALL: Cannot call abstract method {}::{}()",
            class.display_name, method.name
        ));
    }
    if method.flags.is_private && scope != Some(normalize_class_name(&class.name).as_str()) {
        return Err(format!(
            "E_PHP_VM_PRIVATE_METHOD_ACCESS: Cannot call private {}::__construct()",
            class.display_name
        ));
    }
    if method.flags.is_protected {
        let allowed = match scope {
            Some(scope) => {
                protected_scope_is_related_in_state(compiled, state, scope, &class.name)?
            }
            None => false,
        };
        if !allowed {
            return Err(format!(
                "E_PHP_VM_PROTECTED_METHOD_ACCESS: Call to protected {}::__construct() from {}",
                class.display_name,
                scope_description(scope)
            ));
        }
    }
    Ok(())
}

pub(super) fn protected_scope_is_related(
    compiled: &CompiledUnit,
    scope: &str,
    declaring_class: &str,
) -> Result<bool, String> {
    Ok(class_is_or_extends(compiled, scope, declaring_class)?
        || class_is_or_extends(compiled, declaring_class, scope)?)
}

pub(super) fn protected_scope_is_related_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    scope: &str,
    declaring_class: &str,
) -> Result<bool, String> {
    Ok(
        class_is_a_in_state(compiled, state, scope, declaring_class)?
            || class_is_a_in_state(compiled, state, declaring_class, scope)?,
    )
}

/// Renders the calling scope for access-violation messages: PHP says
/// `"global scope"` outside a class and `"scope Class"` inside one.
pub(super) fn scope_description(scope: Option<&str>) -> String {
    match scope {
        Some(scope) => format!("scope {scope}"),
        None => "global scope".to_owned(),
    }
}

pub(super) fn lookup_method_in_hierarchy<'a>(
    compiled: &'a CompiledUnit,
    class: &'a php_ir::module::ClassEntry,
    method: &str,
    caller_scope: Option<&str>,
) -> Result<Option<ResolvedMethod<'a>>, String> {
    if let Some(resolved) =
        lookup_private_method_in_caller_scope(compiled, class, method, caller_scope)?
    {
        return Ok(Some(resolved));
    }
    lookup_method_in_hierarchy_inner(compiled, class, method, caller_scope, &mut Vec::new())
}

pub(super) fn lookup_private_method_in_caller_scope<'a>(
    compiled: &'a CompiledUnit,
    class: &'a php_ir::module::ClassEntry,
    method: &str,
    caller_scope: Option<&str>,
) -> Result<Option<ResolvedMethod<'a>>, String> {
    let Some(scope) = caller_scope else {
        return Ok(None);
    };
    if !class_is_or_extends(compiled, &class.name, scope)? {
        return Ok(None);
    }
    let Some(scope_class) = compiled.lookup_class(scope) else {
        return Ok(None);
    };
    let normalized = normalize_method_name(method);
    let Some(scope_method) = scope_class
        .methods
        .iter()
        .find(|entry| entry.flags.is_private && entry.name.eq_ignore_ascii_case(&normalized))
    else {
        return Ok(None);
    };
    Ok(Some(ResolvedMethod {
        class: scope_class,
        method: scope_method,
    }))
}

pub(super) fn lookup_method_in_hierarchy_inner<'a>(
    compiled: &'a CompiledUnit,
    class: &'a php_ir::module::ClassEntry,
    method: &str,
    caller_scope: Option<&str>,
    seen: &mut Vec<String>,
) -> Result<Option<ResolvedMethod<'a>>, String> {
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
            && let Some(parent) = parent_class(compiled, class)?
            && let Some(parent_method) = lookup_method_in_hierarchy_inner(
                compiled,
                parent,
                method.name.as_str(),
                caller_scope,
                seen,
            )?
        {
            seen.pop();
            return Ok(Some(parent_method));
        }
        seen.pop();
        return Ok(Some(ResolvedMethod { class, method }));
    }
    if let Some(parent) = parent_class(compiled, class)? {
        let resolved =
            lookup_method_in_hierarchy_inner(compiled, parent, method, caller_scope, seen)?;
        seen.pop();
        return Ok(resolved);
    }
    seen.pop();
    Ok(None)
}

pub(super) fn parent_class<'a>(
    compiled: &'a CompiledUnit,
    class: &php_ir::module::ClassEntry,
) -> Result<Option<&'a php_ir::module::ClassEntry>, String> {
    let Some(parent) = class.parent.as_deref() else {
        return Ok(None);
    };
    if internal_runtime_parent_is_known(class, parent) {
        return Ok(None);
    }
    if internal_runtime_class_entry(&normalize_class_name(parent)).is_some() {
        return Ok(None);
    }
    compiled.lookup_class(parent).map(Some).ok_or_else(|| {
        format!(
            "E_PHP_VM_UNKNOWN_PARENT_CLASS: class {} extends missing class {}",
            class.name, parent
        )
    })
}
