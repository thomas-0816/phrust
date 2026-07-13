use super::prelude::*;

pub(super) fn resolve_static_class_name(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    class_name: &str,
) -> Result<CompiledClass, String> {
    // Returns a shared handle: `ClassEntry` is a large struct (method/property
    // tables), and this resolver runs on every `Name::`/`self::`/`static::`
    // access — deep-cloning it here was a top CPU hotspot on the WordPress
    // request profile.
    match normalize_class_name(class_name).as_str() {
        "self" => {
            let Some(scope) = current_scope_class(compiled, stack) else {
                // Reference wording (PHP 8.5.7): thrown when a deferred
                // closure/global `self` reference is resolved without scope.
                return Err(
                    "E_PHP_VM_INVALID_STATIC_SCOPE: Cannot use \"self\" in the global scope"
                        .to_owned(),
                );
            };
            lookup_class_in_state(compiled, state, &scope)
                .ok_or_else(|| format!("E_PHP_VM_UNKNOWN_CLASS: class {scope} is not defined"))
        }
        "static" => {
            let Some(called) = current_called_class_display(compiled, stack)
                .or_else(|| current_scope_class(compiled, stack))
            else {
                return Err(
                    "E_PHP_VM_INVALID_STATIC_SCOPE: Cannot use \"static\" in the global scope"
                        .to_owned(),
                );
            };
            lookup_class_in_state(compiled, state, &called)
                .or_else(|| {
                    current_this_called_class(compiled, state, stack, &called)
                        .map(CompiledClass::owned)
                })
                .ok_or_else(|| format!("E_PHP_VM_UNKNOWN_CLASS: class {called} is not defined"))
        }
        "parent" => {
            let Some(scope) = current_scope_class(compiled, stack) else {
                return Err(
                    "E_PHP_VM_INVALID_STATIC_SCOPE: Cannot use \"parent\" in the global scope"
                        .to_owned(),
                );
            };
            let Some(class) = lookup_class_in_state(compiled, state, &scope) else {
                return Err(format!(
                    "E_PHP_VM_UNKNOWN_CLASS: class {scope} is not defined"
                ));
            };
            if let Some(parent) = class.parent.as_deref()
                && let Some(parent) = internal_runtime_class_entry(&normalize_class_name(parent))
            {
                return Ok(CompiledClass::owned(parent));
            }
            let Some(parent_name) = class.parent.as_deref() else {
                return Err(format!(
                    "E_PHP_VM_NO_PARENT_CLASS: class {} has no parent",
                    class.name
                ));
            };
            lookup_class_in_state(compiled, state, parent_name).ok_or_else(|| {
                format!(
                    "E_PHP_VM_UNKNOWN_PARENT_CLASS: class {} extends missing class {}",
                    class.name, parent_name
                )
            })
        }
        _ => lookup_class_in_state(compiled, state, class_name)
            .or_else(|| {
                internal_runtime_class_entry(&normalize_class_name(class_name))
                    .map(CompiledClass::owned)
            })
            .ok_or_else(|| format!("E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not defined")),
    }
}

pub(super) fn is_special_static_class_name(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "self" | "static" | "parent"
    )
}

pub(super) fn static_class_autoload_name(
    compiled: &CompiledUnit,
    class_name: &str,
    span: IrSpan,
) -> String {
    let display_name = display_class_name(class_name);
    static_class_name_from_source_span(compiled, span)
        .filter(|source_name| normalize_class_name(source_name) == normalize_class_name(class_name))
        .unwrap_or(display_name)
}

pub(super) fn static_class_name_from_source_span(
    compiled: &CompiledUnit,
    span: IrSpan,
) -> Option<String> {
    if span == IrSpan::default() {
        return None;
    }
    let file = compiled.unit().files.get(span.file.index())?;
    let source = fs::read_to_string(&file.path).ok()?;
    let start = usize::try_from(span.start).ok()?;
    let end = usize::try_from(span.end).ok()?.min(source.len());
    if start >= end || start >= source.len() || !source.is_char_boundary(start) {
        return None;
    }
    let end = if source.is_char_boundary(end) {
        end
    } else {
        source[..end].char_indices().last()?.0
    };
    let slice = source.get(start..end)?;
    let before_static_access = slice.split_once("::")?.0.trim_end();
    let name_start = before_static_access
        .char_indices()
        .rev()
        .find_map(|(index, ch)| (!is_php_class_name_char(ch)).then_some(index + ch.len_utf8()))
        .unwrap_or(0);
    let candidate = before_static_access
        .get(name_start..)?
        .trim_start_matches('\\');
    (!candidate.is_empty() && !is_special_static_class_name(candidate))
        .then(|| candidate.to_owned())
}

pub(super) fn is_php_class_name_char(ch: char) -> bool {
    ch == '\\' || ch == '_' || ch.is_ascii_alphanumeric()
}

pub(super) fn called_class_for_static_call(
    compiled: &CompiledUnit,
    stack: &CallStack,
    class_name: &str,
    resolved_class: &php_ir::module::ClassEntry,
) -> String {
    match normalize_class_name(class_name).as_str() {
        "self" | "static" | "parent" => current_called_class_display(compiled, stack)
            .or_else(|| current_scope_class_display(compiled, stack))
            .unwrap_or_else(|| resolved_class.display_name.clone()),
        _ => resolved_class.display_name.clone(),
    }
}

pub(super) fn method_lookup_scope_for_static_call(
    compiled: &CompiledUnit,
    stack: &CallStack,
    class_name: &str,
) -> Option<String> {
    if normalize_class_name(class_name) == "static" {
        None
    } else {
        current_scope_class(compiled, stack)
    }
}

pub(super) fn current_scope_class(compiled: &CompiledUnit, stack: &CallStack) -> Option<String> {
    current_scope_class_display(compiled, stack).map(|class| normalize_class_name(&class))
}

pub(super) fn current_scope_class_display(
    compiled: &CompiledUnit,
    stack: &CallStack,
) -> Option<String> {
    let frame = stack.current()?;
    if let Some(scope) = frame.scope_class.as_deref() {
        return Some(scope.to_owned());
    }
    let function = compiled.unit().functions.get(frame.function.index())?;
    function
        .flags
        .is_method
        .then(|| {
            function
                .name
                .split_once("::")
                .map(|(class, _)| class.to_owned())
        })
        .flatten()
}

pub(super) fn current_this_object(compiled: &CompiledUnit, stack: &CallStack) -> Option<ObjectRef> {
    let frame = stack.current()?;
    let function = compiled.unit().functions.get(frame.function.index())?;
    let local = function
        .locals
        .iter()
        .position(|name| name == "this")
        .map(|index| LocalId::new(index as u32))?;
    match frame.locals.get(local)? {
        Value::Object(object) => Some(object),
        _ => None,
    }
}

pub(super) fn scoped_static_call_this_object(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    declaring_class: &php_ir::module::ClassEntry,
    method: &php_ir::module::ClassMethodEntry,
) -> Option<ObjectRef> {
    if method.flags.is_static {
        return None;
    }
    current_this_object(compiled, stack).filter(|object| {
        class_is_a_in_state(compiled, state, &object.class_name(), &declaring_class.name)
            .unwrap_or(false)
    })
}

pub(super) fn current_called_class(compiled: &CompiledUnit, stack: &CallStack) -> Option<String> {
    current_called_class_display(compiled, stack).map(|class| normalize_class_name(&class))
}

pub(super) fn current_called_class_display(
    compiled: &CompiledUnit,
    stack: &CallStack,
) -> Option<String> {
    let frame = stack.current()?;
    frame
        .called_class
        .as_deref()
        .map(ToOwned::to_owned)
        .or_else(|| current_scope_class_display(compiled, stack))
}

pub(super) fn current_this_called_class(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    called_display: &str,
) -> Option<php_ir::module::ClassEntry> {
    let this_value = current_this_object(compiled, stack)?;
    (normalize_class_name(&this_value.display_name()) == normalize_class_name(called_display))
        .then(|| {
            lookup_class_in_state(compiled, state, &this_value.class_name())
                .map(|class| (*class).clone())
        })
        .flatten()
}
