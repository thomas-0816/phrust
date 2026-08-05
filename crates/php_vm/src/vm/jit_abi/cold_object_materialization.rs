//! Cold materialization of object aliases that escaped native storage.
//!
//! Optimizing property access remains on authoritative native slots.

//! Cold object/constant materialization outside generated execution.

use super::*;
use php_runtime::api::Value;

pub(super) fn native_runtime_class_with_owner(
    context: &NativeRequestColdState<'_>,
    owner_unit: Option<usize>,
    class: &php_ir::module::ClassEntry,
) -> Result<php_runtime::api::ClassEntry, String> {
    use php_runtime::api as runtime;

    let owner_ir_unit = |owner: Option<usize>| -> Option<&php_ir::IrUnit> {
        match owner {
            None => Some(&*context.unit),
            Some(unit) if context.current_dynamic_unit == Some(unit) => Some(&*context.unit),
            Some(unit) => context
                .dynamic_units
                .get(unit)
                .map(|package| package.compiled.unit()),
        }
    };
    let mut lineage = Vec::new();
    let mut current = Some((owner_unit, class));
    let mut visited = std::collections::BTreeSet::new();
    while let Some((owner, candidate)) = current {
        if !visited.insert(candidate.name.clone()) {
            return Err(format!(
                "native class hierarchy for {} contains a cycle",
                class.display_name
            ));
        }
        let parent = candidate.parent.clone();
        lineage.push((owner, candidate));
        current = match parent.as_deref() {
            None => None,
            Some(parent) => {
                let parent = normalize_class_name(parent);
                let resolved = owner_ir_unit(owner)
                    .into_iter()
                    .flat_map(|unit| &unit.classes)
                    .find(|class| class.name == parent)
                    .map(|class| (owner, class))
                    .or_else(|| {
                        native_external_class_ref(context, &parent)
                            .map(|(unit, class)| (Some(unit), class))
                    });
                if resolved.is_none() && !native_internal_class_is_available(&parent) {
                    return Err(format!(
                        "native class hierarchy for {} has unresolved parent {}",
                        candidate.display_name,
                        candidate
                            .parent_display_name
                            .as_deref()
                            .unwrap_or(parent.as_str())
                    ));
                }
                resolved
            }
        };
    }
    lineage.reverse();
    let properties = lineage
        .iter()
        .flat_map(|(owner, class)| {
            class
                .properties
                .iter()
                .map(move |property| (*owner, property))
        })
        .map(|(owner, property)| {
            let default = property
                .default
                .and_then(|constant| owner_ir_unit(owner)?.constants.get(constant.index()))
                .map(|value| native_runtime_constant_value(context, value))
                .transpose()?
                .unwrap_or_else(|| {
                    if property.flags.is_typed {
                        Value::Uninitialized
                    } else {
                        Value::Null
                    }
                });
            Ok(runtime::ClassPropertyEntry {
                name: property.name.clone(),
                default,
                type_: property.type_.as_ref().map(native_runtime_type),
                flags: runtime::ClassPropertyFlags {
                    is_static: property.flags.is_static,
                    is_private: property.flags.is_private,
                    is_protected: property.flags.is_protected,
                    set_is_private: property.flags.set_is_private,
                    set_is_protected: property.flags.set_is_protected,
                    is_readonly: property.flags.is_readonly,
                    is_typed: property.flags.is_typed,
                },
                hooks: runtime::ClassPropertyHooks {
                    get_function_id: property.hooks.get.map(|function| function.raw()),
                    set_function_id: property.hooks.set.map(|function| function.raw()),
                    backed: property.hooks.backed,
                },
                attributes: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let runtime_class = runtime::ClassEntry {
        name: class.name.clone().into(),
        parent: class.parent.clone(),
        interfaces: class.interfaces.clone(),
        methods: lineage
            .iter()
            .flat_map(|(_, class)| &class.methods)
            .map(|method| runtime::ClassMethodEntry {
                name: method.name.clone(),
                origin_class: method.origin_class.clone(),
                function_id: method.function.raw(),
                flags: runtime::ClassMethodFlags {
                    is_static: method.flags.is_static,
                    is_private: method.flags.is_private,
                    is_protected: method.flags.is_protected,
                    is_abstract: method.flags.is_abstract,
                    is_final: method.flags.is_final,
                },
                attributes: Vec::new(),
            })
            .collect(),
        properties,
        constants: class
            .constants
            .iter()
            .filter_map(|constant| {
                let value = constant
                    .value
                    .and_then(|value| owner_ir_unit(owner_unit)?.constants.get(value.index()))
                    .and_then(|value| native_runtime_constant_value(context, value).ok())?;
                Some(runtime::ClassConstantEntry {
                    name: constant.name.clone(),
                    value,
                    flags: runtime::ClassConstantFlags {
                        is_private: constant.flags.is_private,
                        is_protected: constant.flags.is_protected,
                    },
                    attributes: Vec::new(),
                })
            })
            .collect(),
        enum_cases: class
            .enum_cases
            .iter()
            .map(|case| runtime::ClassEnumCaseEntry {
                name: case.name.clone(),
                value: case
                    .value
                    .and_then(|value| owner_ir_unit(owner_unit)?.constants.get(value.index()))
                    .and_then(|value| ir_constant_value(value).ok()),
                attributes: Vec::new(),
            })
            .collect(),
        attributes: Vec::new(),
        enum_backing_type: class.enum_backing_type.map(|backing| match backing {
            php_ir::module::ClassEnumBackingType::Int => runtime::ClassEnumBackingType::Int,
            php_ir::module::ClassEnumBackingType::String => runtime::ClassEnumBackingType::String,
        }),
        constructor_id: class.constructor.map(|function| function.raw()),
        flags: runtime::ClassFlags {
            has_complete_method_table: true,
            implements_countable: native_class_is_a(context, &class.name, "countable"),
            implements_traversable: native_class_is_a(context, &class.name, "traversable"),
            allows_dynamic_properties: lineage.iter().any(|(_, class)| {
                class.attributes.iter().any(|attribute| {
                    attribute
                        .resolved_name
                        .as_deref()
                        .or(attribute.fallback_name.as_deref())
                        .unwrap_or(&attribute.name)
                        .trim_start_matches('\\')
                        .eq_ignore_ascii_case("AllowDynamicProperties")
                })
            }),
            is_abstract: class.flags.is_abstract || class.flags.is_trait,
            is_final: class.flags.is_final,
            is_readonly: class.flags.is_readonly,
            is_interface: class.flags.is_interface,
            is_enum: class.flags.is_enum,
        },
    };
    Ok(runtime_class)
}

impl<'a> NativeRequestColdState<'a> {
    /// A materialized ReferenceCell can outlive a direct object handle and can
    /// later be reached by a cold semantic operation. Restore every native
    /// property slot before exposing that referenced object to Rust APIs.
    pub(super) fn materialize_referenced_object(
        &mut self,
        reference: &php_runtime::api::ReferenceCell,
    ) -> Result<(), String> {
        let mut value = reference.get();
        for _ in 0..16 {
            let Value::Reference(next) = value else {
                break;
            };
            value = next.get();
        }
        let Value::Object(object) = value else {
            return Ok(());
        };
        self.materialize_direct_object_alias(&object)
    }

    pub(super) fn materialize_direct_object_alias(
        &mut self,
        object: &php_runtime::api::ObjectRef,
    ) -> Result<(), String> {
        if object
            .native_declared_slots_view(object.class_layout_epoch())
            .is_none()
        {
            return Ok(());
        }
        let object_id = object.id();
        let index = self
            .baseline_values
            .direct_object_handles
            .get(&object_id)
            .copied()
            .and_then(|index| usize::try_from(index).ok())
            .filter(|index| {
                self.direct_value_slots.get(*index).is_some_and(|slot| {
                    slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
                        && php_jit::jit_native_object_property_view_is_published(slot.flags)
                }) && self
                    .direct_object_owner(*index)
                    .is_some_and(|candidate| candidate.id() == object_id)
            })
            .or_else(|| {
                let used = usize::try_from(*self.direct_value_next).ok()?;
                (0..used).find(|index| {
                    self.direct_value_slots.get(*index).is_some_and(|slot| {
                        slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
                            && php_jit::jit_native_object_property_view_is_published(slot.flags)
                    }) && self
                        .direct_object_owner(*index)
                        .is_some_and(|candidate| candidate.id() == object_id)
                })
            })
            .ok_or_else(|| format!("native object {object_id} has no live direct descriptor"))?;
        let was_dead = self.direct_value_slots[index].refcount == 0;
        if was_dead {
            // Recover a descriptor whose retained call-owner was decremented
            // to zero without the last-owner commit. It is revived only long
            // enough to materialize its escaped ObjectRef and retire cleanly.
            self.direct_value_slots[index].refcount = 1;
        }
        self.demote_direct_object_property_slots(index)?;
        if was_dead {
            self.release_direct_value_index(index)?;
        }
        Ok(())
    }
    #[track_caller]
    pub(super) fn promote_direct_object_property_slots(
        &mut self,
        index: usize,
    ) -> Result<bool, String> {
        let object = self
            .direct_object(index)
            .ok_or_else(|| format!("direct native object {index} has no stable owner"))?;
        let object_type_flags = (u32::from(object.is_native_countable())
            * php_jit::JIT_NATIVE_OBJECT_COUNTABLE)
            | (u32::from(object.is_native_traversable()) * php_jit::JIT_NATIVE_OBJECT_TRAVERSABLE)
            | (u32::from(object.class_name().eq_ignore_ascii_case("stdClass"))
                * php_jit::JIT_NATIVE_OBJECT_STDCLASS)
            | (u32::from(object.allows_native_dynamic_properties())
                * php_jit::JIT_NATIVE_OBJECT_ALLOWS_DYNAMIC_PROPERTIES);
        let layout_id = object.class_layout_epoch();
        if let Some((base, count)) = object.native_declared_slots_view(layout_id) {
            let slot = self
                .direct_value_slots
                .get_mut(index)
                .ok_or_else(|| format!("direct native object {index} slot is missing"))?;
            slot.flags = php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION | object_type_flags;
            slot.reserved = u32::try_from(count).unwrap_or(u32::MAX);
            slot.payload = layout_id;
            slot.aux = base as usize as u64;
            return Ok(true);
        }
        let Some((rust_slots, rust_dynamic)) = object.take_property_slots_for_native(layout_id)
        else {
            return Ok(false);
        };
        self.record_direct_object_promotion(std::panic::Location::caller());
        let mut native_slots: Vec<php_runtime::api::NativeDeclaredPropertySlot> =
            Vec::with_capacity(rust_slots.len());
        for slot in &rust_slots {
            let encoded = match slot {
                Some(value) => match self.encode_baseline_value(value.clone()) {
                    Ok(encoded) => php_runtime::api::NativeDeclaredPropertySlot {
                        initialized: 1,
                        reserved: 0,
                        value: encoded,
                    },
                    Err(error) => {
                        for slot in native_slots {
                            if slot.initialized != 0 {
                                let _ = self.release(slot.value);
                            }
                        }
                        let _ = object.restore_property_slots_from_native(
                            layout_id,
                            rust_slots,
                            rust_dynamic,
                        );
                        return Err(error);
                    }
                },
                None => php_runtime::api::NativeDeclaredPropertySlot::default(),
            };
            native_slots.push(encoded);
        }
        let mut native_dynamic =
            php_runtime::api::NativeDynamicPropertySlots::with_capacity(rust_dynamic.len());
        for (name, value) in &rust_dynamic {
            let encoded = match self.encode_baseline_value(value.clone()) {
                Ok(encoded) => encoded,
                Err(error) => {
                    for slot in native_slots.iter().filter(|slot| slot.initialized != 0) {
                        let _ = self.release(slot.value);
                    }
                    for slot in native_dynamic.values() {
                        if slot.slot.initialized != 0 {
                            let _ = self.release(slot.slot.value);
                        }
                    }
                    let _ = object.restore_property_slots_from_native(
                        layout_id,
                        rust_slots,
                        rust_dynamic,
                    );
                    return Err(error);
                }
            };
            native_dynamic.insert(
                name.clone(),
                Box::new(php_runtime::api::NativeDynamicPropertyCell {
                    slot: php_runtime::api::NativeDeclaredPropertySlot {
                        initialized: 1,
                        reserved: 0,
                        value: encoded,
                    },
                    insertion_order: 0,
                    next_insertion_order: std::ptr::null_mut(),
                }),
            );
        }
        if let Err((native_slots, native_dynamic)) = object.install_native_property_slots(
            layout_id,
            native_slots.into_boxed_slice(),
            native_dynamic,
        ) {
            for slot in native_slots.iter().filter(|slot| slot.initialized != 0) {
                let _ = self.release(slot.value);
            }
            for slot in native_dynamic.values() {
                if slot.slot.initialized != 0 {
                    let _ = self.release(slot.slot.value);
                }
            }
            let _ = object.restore_property_slots_from_native(layout_id, rust_slots, rust_dynamic);
            return Ok(false);
        }
        let Some((base, count)) = object.native_declared_slots_view(layout_id) else {
            return Err("native object slots disappeared during publication".to_owned());
        };
        let slot = self
            .direct_value_slots
            .get_mut(index)
            .ok_or_else(|| format!("direct native object {index} slot is missing"))?;
        slot.flags = php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION | object_type_flags;
        slot.reserved = u32::try_from(count).unwrap_or(u32::MAX);
        slot.payload = layout_id;
        slot.aux = base as usize as u64;
        Ok(true)
    }

    #[track_caller]
    pub(super) fn demote_direct_object_property_slots(
        &mut self,
        index: usize,
    ) -> Result<(), String> {
        let object = self
            .direct_object_owner(index)
            .ok_or_else(|| format!("direct native object {index} has no stable owner"))?;
        let descriptor = *self
            .direct_value_slots
            .get(index)
            .ok_or_else(|| format!("direct native object {index} slot is missing"))?;
        if !php_jit::jit_native_object_property_view_is_published(descriptor.flags) {
            return Ok(());
        }
        self.record_direct_object_demotion(std::panic::Location::caller());
        let layout_id = descriptor.payload;
        let Some((native_slots, native_dynamic)) = object.take_native_property_slots(layout_id)
        else {
            return Err(format!(
                "direct native object {index} lost its property-slot storage"
            ));
        };
        // Mark the descriptor cold before decoding so a self-referential
        // object slot does not recursively attempt the same demotion.
        if let Some(slot) = self.direct_value_slots.get_mut(index) {
            slot.flags = 0;
            slot.reserved = 0;
            slot.payload = object.id();
            slot.aux = 0;
        }
        let mut rust_slots = Vec::with_capacity(native_slots.len());
        for slot in &native_slots {
            if slot.initialized == 0 {
                rust_slots.push(None);
            } else {
                match self.decode_baseline_value(slot.value) {
                    Ok(value) => rust_slots.push(Some(value)),
                    Err(error) => {
                        let _ = object.install_native_property_slots(
                            layout_id,
                            native_slots,
                            native_dynamic,
                        );
                        if let Some(slot) = self.direct_value_slots.get_mut(index) {
                            *slot = descriptor;
                        }
                        return Err(error);
                    }
                }
            }
        }
        let mut rust_dynamic = std::collections::HashMap::with_capacity(native_dynamic.len());
        for (name, cell) in &native_dynamic {
            if cell.slot.initialized == 0 {
                continue;
            }
            match self.decode_baseline_value(cell.slot.value) {
                Ok(value) => {
                    rust_dynamic.insert(name.clone(), value);
                }
                Err(error) => {
                    let _ = object.install_native_property_slots(
                        layout_id,
                        native_slots,
                        native_dynamic,
                    );
                    if let Some(slot) = self.direct_value_slots.get_mut(index) {
                        *slot = descriptor;
                    }
                    return Err(error);
                }
            }
        }
        if !object.restore_property_slots_from_native(layout_id, rust_slots, rust_dynamic) {
            let _ = object.install_native_property_slots(layout_id, native_slots, native_dynamic);
            if let Some(slot) = self.direct_value_slots.get_mut(index) {
                *slot = descriptor;
            }
            return Err("failed to restore cold object property values".to_owned());
        }
        let mut release_error = None;
        for slot in native_slots.iter().filter(|slot| slot.initialized != 0) {
            if let Err(error) = self.release(slot.value) {
                release_error.get_or_insert(error);
            }
        }
        for cell in native_dynamic
            .values()
            .filter(|cell| cell.slot.initialized != 0)
        {
            if let Err(error) = self.release(cell.slot.value) {
                release_error.get_or_insert(error);
            }
        }
        if let Some(error) = release_error {
            return Err(error);
        }
        Ok(())
    }
}
