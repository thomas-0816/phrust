use super::prelude::*;

pub(super) fn property_hook_is_active(
    state: &ExecutionState,
    object: &ObjectRef,
    class: &php_ir::module::ClassEntry,
    property: &php_ir::module::ClassPropertyEntry,
) -> bool {
    let class_name = normalize_class_name(&class.name);
    state.property_hook_stack.iter().any(|active| {
        active.object_id == object.id()
            && active.class_name == class_name
            && active.property == property.name
    })
}

pub(super) fn property_storage_name(
    class: &php_ir::module::ClassEntry,
    property: &php_ir::module::ClassPropertyEntry,
) -> String {
    if property.flags.is_private {
        format!("private:{}:{}", class.display_name, property.name)
    } else {
        property.name.clone()
    }
}

#[cfg(feature = "jit-cranelift")]
pub(super) fn property_load_pre_guard_status(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    value: &Value,
    metadata: &php_jit::JitPropertyLoadMetadata,
) -> Option<i32> {
    if state.lookup_epoch().raw() != metadata.layout_version {
        return Some(JIT_PROPERTY_LOAD_STATUS_LAYOUT_EXIT);
    }
    let effective = match value {
        Value::Reference(cell) => cell.get(),
        other => other.clone(),
    };
    let Value::Object(object) = effective else {
        return Some(JIT_PROPERTY_LOAD_STATUS_CLASS_EXIT);
    };
    if normalize_class_name(&object.class_name()) != metadata.receiver_class {
        return Some(JIT_PROPERTY_LOAD_STATUS_CLASS_EXIT);
    }
    let Some(class) = lookup_class_in_state(compiled, state, &object.class_name()) else {
        return Some(JIT_PROPERTY_LOAD_STATUS_CLASS_EXIT);
    };
    if class.id.raw() != metadata.class_id {
        return Some(JIT_PROPERTY_LOAD_STATUS_CLASS_EXIT);
    }
    None
}

pub(super) fn property_fetch_callsite(
    compiled: &CompiledUnit,
    function: FunctionId,
    block: BlockId,
    instruction: InstrId,
) -> String {
    let unit = compiled.unit();
    let function_name = unit
        .functions
        .get(function.index())
        .map(|entry| entry.name.as_str())
        .unwrap_or("<unknown>");
    format!(
        "unit{}:{}:b{}:i{}",
        unit.id.raw(),
        function_name,
        block.raw(),
        instruction.raw()
    )
}

pub(super) fn method_call_callsite(
    compiled: &CompiledUnit,
    function: FunctionId,
    block: BlockId,
    instruction: InstrId,
) -> String {
    let unit = compiled.unit();
    let function_name = unit
        .functions
        .get(function.index())
        .map(|entry| entry.name.as_str())
        .unwrap_or("<unknown>");
    format!(
        "unit{}:{}:b{}:i{}",
        unit.id.raw(),
        function_name,
        block.raw(),
        instruction.raw()
    )
}

pub(super) fn class_has_public_magic_get(
    compiled: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
) -> bool {
    lookup_method_in_hierarchy(compiled, class, "__get", None)
        .ok()
        .flatten()
        .is_some_and(|resolved| {
            !resolved.method.flags.is_static
                && !resolved.method.flags.is_private
                && !resolved.method.flags.is_protected
        })
}

pub(super) fn class_has_public_magic_set(
    compiled: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
) -> bool {
    lookup_method_in_hierarchy(compiled, class, "__set", None)
        .ok()
        .flatten()
        .is_some_and(|resolved| {
            !resolved.method.flags.is_static
                && !resolved.method.flags.is_private
                && !resolved.method.flags.is_protected
        })
}

pub(super) fn class_allows_dynamic_properties(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: &php_ir::module::ClassEntry,
) -> bool {
    // Walk the receiver borrowed; only parent hops take a shared handle.
    let mut parent_holder;
    let mut entry = class;
    loop {
        if entry
            .attributes
            .iter()
            .any(attribute_is_allow_dynamic_properties)
        {
            return true;
        }
        let Some(parent) = entry
            .parent
            .as_deref()
            .and_then(|parent| lookup_class_in_state(compiled, state, parent))
        else {
            return false;
        };
        parent_holder = parent;
        entry = &parent_holder;
    }
}

pub(super) fn attribute_is_allow_dynamic_properties(
    attribute: &php_ir::module::AttributeEntry,
) -> bool {
    [&attribute.resolved_name, &attribute.fallback_name]
        .into_iter()
        .filter_map(|name| name.as_deref())
        .chain(std::iter::once(attribute.name.as_str()))
        .any(|name| normalize_class_name(name) == "allowdynamicproperties")
}

pub(super) fn class_has_public_magic_call(
    compiled: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
) -> bool {
    lookup_method_in_hierarchy(compiled, class, "__call", None)
        .ok()
        .flatten()
        .is_some_and(|resolved| {
            !resolved.method.flags.is_static
                && !resolved.method.flags.is_private
                && !resolved.method.flags.is_protected
        })
}

pub(super) fn property_has_hooks_or_active(
    state: &ExecutionState,
    object: &ObjectRef,
    class: &php_ir::module::ClassEntry,
    property: &php_ir::module::ClassPropertyEntry,
) -> bool {
    property.hooks.get.is_some()
        || property.hooks.set.is_some()
        || property_hook_is_active(state, object, class, property)
}

pub(super) fn property_fetch_layout_metadata(
    receiver_class: &php_ir::module::ClassEntry,
    declaring_class: &php_ir::module::ClassEntry,
    property: &php_ir::module::ClassPropertyEntry,
    visibility_context: Option<&str>,
    layout_version: InvalidationEpoch,
    has_magic_get: bool,
    has_property_hooks: bool,
    dynamic_property_fallback: bool,
    typed_property_initialized: bool,
) -> PropertyFetchLayoutMetadata {
    PropertyFetchLayoutMetadata {
        class_id: receiver_class.id.raw(),
        layout_version: layout_version.raw(),
        property_slot_index: declaring_class
            .properties
            .iter()
            .position(|entry| entry.name == property.name)
            .map(|index| index as u32),
        visibility_context: visibility_context.map(str::to_owned),
        typed_property_initialized,
        has_property_hooks,
        has_magic_get,
        dynamic_property_fallback,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn property_assign_layout_metadata(
    receiver_class: &php_ir::module::ClassEntry,
    declaring_class: &php_ir::module::ClassEntry,
    property: &php_ir::module::ClassPropertyEntry,
    visibility_context: Option<&str>,
    layout_version: InvalidationEpoch,
    has_magic_set: bool,
    has_property_hooks: bool,
    dynamic_property_fallback: bool,
    reference_slot: bool,
) -> PropertyAssignLayoutMetadata {
    PropertyAssignLayoutMetadata {
        class_id: receiver_class.id.raw(),
        layout_version: layout_version.raw(),
        property_slot_index: declaring_class
            .properties
            .iter()
            .position(|entry| entry.name == property.name)
            .map(|index| index as u32),
        visibility_context: visibility_context.map(str::to_owned),
        typed_property: property.type_.is_some(),
        readonly_or_init_only: property.flags.is_readonly || declaring_class.flags.is_readonly,
        reference_slot,
        has_property_hooks,
        has_magic_set,
        dynamic_property_fallback,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn property_fetch_profile_observation(
    callsite: &str,
    property: &str,
    receiver_class: &str,
    class: &php_ir::module::ClassEntry,
    declared_property: Option<(
        &php_ir::module::ClassEntry,
        &php_ir::module::ClassPropertyEntry,
    )>,
    visibility_context: Option<&str>,
    layout_version: InvalidationEpoch,
    has_magic_get: bool,
    has_property_hook: bool,
    dynamic_property_fallback: bool,
    declared_visible_property: bool,
    uninitialized_typed_property: bool,
    non_eligible_reasons: Vec<&'static str>,
) -> PropertyFetchProfileObservation {
    let (declared_property_name, property_slot_index) =
        declared_property.map_or((None, None), |(declaring_class, property)| {
            (
                Some(property.name.clone()),
                declaring_class
                    .properties
                    .iter()
                    .position(|entry| entry.name == property.name),
            )
        });
    PropertyFetchProfileObservation {
        callsite: callsite.to_owned(),
        property: property.to_owned(),
        receiver_class: receiver_class.to_owned(),
        class_id: class.id.raw(),
        declared_property_name,
        visibility_context: visibility_context.map(str::to_owned),
        property_slot_index,
        class_layout_version: layout_version.raw(),
        has_magic_get,
        has_property_hook,
        dynamic_property_fallback,
        declared_visible_property,
        uninitialized_typed_property,
        non_eligible_reasons,
    }
}
