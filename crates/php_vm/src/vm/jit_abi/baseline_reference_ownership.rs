//! Baseline-only reference identity, typed-property constraints, and ownership transfer.
//!
//! Direct reference descriptors remain authoritative while generated native
//! code executes. `ReferenceCell` exists only as a cold identity sidecar for
//! PHP-visible aliases and typed-property constraint recovery.

use super::*;
use php_runtime::api::Value;

impl<'a> NativeRequestColdState<'a> {
    /// Replaces an authoritative direct-reference payload. `replacement` is
    /// moved into the reference slot; the previous payload owner is released.
    pub(super) fn replace_direct_reference_payload_owned(
        &mut self,
        reference: i64,
        replacement: i64,
    ) -> Result<bool, String> {
        let Some(index) = Self::direct_value_index(reference) else {
            return Ok(false);
        };
        let Some(slot) = self.direct_value_slots.get(index).copied().filter(|slot| {
            slot.refcount != 0
                && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
                && native_reference_state(slot.reserved)
                    != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
        }) else {
            return Ok(false);
        };
        self.cross_unit_stable_values.remove(&index);
        self.direct_value_slots[index].payload = replacement as u64;
        self.direct_value_slots[index].reserved =
            php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED
                | (slot.reserved & php_jit::JIT_NATIVE_REFERENCE_TYPED_PROPERTY_GUARD);
        self.release(slot.payload as i64)?;
        Ok(true)
    }

    fn register_typed_static_reference(
        &mut self,
        reference: &php_runtime::api::ReferenceCell,
        declaration: &NativeStaticPropertyDeclaration,
        property: &str,
    ) {
        let Some(type_) = declaration.type_.clone() else {
            return;
        };
        let constraint = NativeTypedStaticReferenceConstraint {
            owner_display_name: declaration.owner_display_name.clone(),
            property: property.to_owned(),
            type_,
        };
        let constraints = self
            .typed_static_reference_constraints
            .entry(reference.gc_debug_id())
            .or_default();
        if !constraints.iter().any(|candidate| {
            candidate.owner_display_name == constraint.owner_display_name
                && candidate.property == constraint.property
                && candidate.type_ == constraint.type_
        }) {
            constraints.push(constraint);
        }
        if let Some(index) = self
            .baseline_values
            .direct_reference_cells
            .iter()
            .find_map(|(index, candidate)| candidate.ptr_eq(reference).then_some(*index))
            && let Some(slot) = self.direct_value_slots.get_mut(index)
        {
            slot.reserved |= php_jit::JIT_NATIVE_REFERENCE_TYPED_PROPERTY_GUARD;
        }
    }

    pub(super) fn register_typed_native_property_reference(
        &mut self,
        encoded: i64,
        owner_display_name: String,
        property: &str,
        type_: php_ir::IrReturnType,
    ) -> Result<(), String> {
        let index = Self::direct_value_index(encoded)
            .ok_or_else(|| "typed property reference is not a direct handle".to_owned())?;
        let reference = self
            .baseline_values
            .direct_reference_cells
            .entry(index)
            .or_insert_with(|| php_runtime::api::ReferenceCell::new(Value::Uninitialized))
            .clone();
        let constraint = NativeTypedStaticReferenceConstraint {
            owner_display_name,
            property: property.to_owned(),
            type_,
        };
        let constraints = self
            .typed_static_reference_constraints
            .entry(reference.gc_debug_id())
            .or_default();
        if !constraints.iter().any(|candidate| {
            candidate.owner_display_name == constraint.owner_display_name
                && candidate.property == constraint.property
                && candidate.type_ == constraint.type_
        }) {
            constraints.push(constraint);
        }
        let slot = self
            .direct_value_slots
            .get_mut(index)
            .ok_or_else(|| "typed property reference slot disappeared".to_owned())?;
        slot.reserved |= php_jit::JIT_NATIVE_REFERENCE_TYPED_PROPERTY_GUARD;
        Ok(())
    }

    pub(super) fn bind_typed_static_reference(
        &mut self,
        reference: &php_runtime::api::ReferenceCell,
        declaration: &NativeStaticPropertyDeclaration,
        property: &str,
    ) -> Result<(), String> {
        let Some(type_) = declaration.type_.as_ref() else {
            return Ok(());
        };
        let current = reference.get();
        let actual = native_assignment_type_name(&current);
        let effective = native_coerce_call_argument(current.clone(), type_, self.unit.strict_types);
        let existing = self
            .typed_static_reference_constraints
            .get(&reference.gc_debug_id())
            .and_then(|constraints| {
                constraints.iter().find(|constraint| {
                    constraint.owner_display_name != declaration.owner_display_name
                        || constraint.property != property
                        || constraint.type_ != *type_
                })
            });
        if let Some(existing) = existing
            && (effective != current
                || !native_value_matches_ir_type_in_context(self, &effective, type_))
        {
            return Err(format!(
                "E_PHP_THROW:TypeError:Reference with value of type {actual} held by {} is not compatible with {}",
                typed_static_reference_constraint_description(existing),
                typed_static_property_description(declaration, property)
            ));
        }
        if !native_value_matches_ir_type_in_context(self, &effective, type_) {
            return Err(format!(
                "E_PHP_THROW:TypeError:Cannot assign {actual} to property {}::${property} of type {}",
                declaration.owner_display_name,
                native_ir_type_name(type_)
            ));
        }
        if existing.is_none() && effective != current {
            reference.set(effective);
        }
        self.register_typed_static_reference(reference, declaration, property);
        Ok(())
    }

    pub(super) fn native_encoded_values_identical(&self, left: i64, right: i64) -> Option<bool> {
        let left = self.dereference_direct_encoding(left);
        let right = self.dereference_direct_encoding(right);
        if left == right {
            return Some(true);
        }
        let left_kind = self.native_encoded_value_kind(left)?;
        let right_kind = self.native_encoded_value_kind(right)?;
        if left_kind != right_kind {
            return Some(false);
        }
        match left_kind {
            NativeEncodedValueKind::Null
            | NativeEncodedValueKind::Uninitialized
            | NativeEncodedValueKind::Bool(_) => Some(true),
            NativeEncodedValueKind::Int => {
                Some(self.native_encoded_int(left)? == self.native_encoded_int(right)?)
            }
            NativeEncodedValueKind::Float => Some(
                self.native_encoded_float(left)?.to_bits()
                    == self.native_encoded_float(right)?.to_bits(),
            ),
            NativeEncodedValueKind::String => {
                Some(self.native_string_bytes(left)? == self.native_string_bytes(right)?)
            }
            // Coercion never manufactures containers, objects, resources, or
            // executable identities. Equal values of these kinds therefore
            // retain the same authoritative handle and were caught above.
            NativeEncodedValueKind::Array
            | NativeEncodedValueKind::Object
            | NativeEncodedValueKind::Resource
            | NativeEncodedValueKind::Callable
            | NativeEncodedValueKind::Generator
            | NativeEncodedValueKind::Fiber
            | NativeEncodedValueKind::Reference => Some(false),
        }
    }

    fn coerce_typed_static_reference_encoded(
        &mut self,
        reference: &php_runtime::api::ReferenceCell,
        encoded: i64,
    ) -> Result<Option<i64>, String> {
        let Some(constraints) = self
            .typed_static_reference_constraints
            .get(&reference.gc_debug_id())
            .cloned()
        else {
            return Ok(None);
        };
        let actual = self.native_encoded_type_name(encoded);
        let mut selected: Option<(i64, NativeTypedStaticReferenceConstraint)> = None;
        for constraint in constraints {
            let candidate = self.coerce_native_call_argument_encoded(
                encoded,
                &constraint.type_,
                self.unit.strict_types,
            )?;
            let Some(candidate) = candidate else {
                if let Some((selected, _)) = selected {
                    self.release_if_live(selected)?;
                }
                return Ok(None);
            };
            if self.native_encoded_matches_ir_type(candidate, &constraint.type_) != Some(true) {
                self.release_if_live(candidate)?;
                if let Some((selected, _)) = selected {
                    self.release_if_live(selected)?;
                }
                return Err(format!(
                    "E_PHP_THROW:TypeError:Cannot assign {actual} to reference held by property {}::${} of type {}",
                    constraint.owner_display_name,
                    constraint.property,
                    native_ir_type_name(&constraint.type_)
                ));
            }
            if let Some((selected_value, selected_constraint)) = selected.as_ref() {
                if self.native_encoded_values_identical(*selected_value, candidate) != Some(true) {
                    self.release_if_live(candidate)?;
                    let error = inconsistent_typed_static_reference_assignment(
                        actual,
                        selected_constraint,
                        &constraint,
                    );
                    let selected_value = selected
                        .take()
                        .expect("selected typed reference candidate disappeared")
                        .0;
                    self.release_if_live(selected_value)?;
                    return Err(error);
                }
                self.release_if_live(candidate)?;
            } else {
                selected = Some((candidate, constraint));
            }
        }
        Ok(selected.map(|(candidate, _)| candidate))
    }

    pub(super) fn coerce_typed_static_reference_value(
        &self,
        reference: &php_runtime::api::ReferenceCell,
        value: Value,
    ) -> Result<Value, String> {
        let Some(constraints) = self
            .typed_static_reference_constraints
            .get(&reference.gc_debug_id())
        else {
            return Ok(value);
        };
        let actual = native_assignment_type_name(&value);
        let mut selected: Option<(Value, &NativeTypedStaticReferenceConstraint)> = None;
        for constraint in constraints {
            let candidate = native_coerce_call_argument(
                value.clone(),
                &constraint.type_,
                self.unit.strict_types,
            );
            if !native_value_matches_ir_type_in_context(self, &candidate, &constraint.type_) {
                return Err(format!(
                    "E_PHP_THROW:TypeError:Cannot assign {actual} to reference held by property {}::${} of type {}",
                    constraint.owner_display_name,
                    constraint.property,
                    native_ir_type_name(&constraint.type_)
                ));
            }
            if let Some((selected_value, selected_constraint)) = selected.as_ref() {
                if *selected_value != candidate {
                    return Err(inconsistent_typed_static_reference_assignment(
                        &actual,
                        selected_constraint,
                        constraint,
                    ));
                }
            } else {
                selected = Some((candidate, constraint));
            }
        }
        Ok(selected.map_or(value, |(candidate, _)| candidate))
    }

    pub(super) fn set_native_reference_value(
        &mut self,
        reference: &php_runtime::api::ReferenceCell,
        value: Value,
    ) -> Result<(), String> {
        let value = self.coerce_typed_static_reference_value(reference, value)?;
        reference.set(value);
        Ok(())
    }

    pub(super) fn typed_static_reference_auto_array_error(
        &self,
        reference: &php_runtime::api::ReferenceCell,
    ) -> Option<String> {
        let constraints = self
            .typed_static_reference_constraints
            .get(&reference.gc_debug_id())?;
        let empty_array = Value::Array(php_runtime::api::PhpArray::new());
        constraints.iter().find_map(|constraint| {
            (!native_value_matches_ir_type_in_context(
                self,
                &empty_array,
                &constraint.type_,
            ))
            .then(|| {
                format!(
                    "E_PHP_THROW:TypeError:Cannot auto-initialize an array inside a reference held by property {}::${} of type {}",
                    constraint.owner_display_name,
                    constraint.property,
                    native_ir_type_name(&constraint.type_)
                )
            })
        })
    }

    /// Replaces one authoritative direct-reference payload from a borrowed
    /// assignment operand. Both the lvalue identity and the replacement stay
    /// in the native plane; a materialized reference or compatibility value
    /// rejects before any visible mutation.
    pub(super) fn store_plain_native_reference_payload(
        &mut self,
        reference: i64,
        value: i64,
    ) -> Result<bool, String> {
        let Some(index) = Self::direct_value_index(reference) else {
            return Ok(false);
        };
        let Some(slot) = self.direct_value_slots.get(index).copied().filter(|slot| {
            slot.refcount != 0
                && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
                && native_reference_state(slot.reserved)
                    != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
        }) else {
            return Ok(false);
        };
        let typed = slot.reserved & php_jit::JIT_NATIVE_REFERENCE_TYPED_PROPERTY_GUARD != 0;
        let replacement = if typed {
            let reference_cell = self
                .baseline_values
                .direct_reference_cells
                .get(&index)
                .cloned()
                .ok_or_else(|| {
                    "typed native reference lost its cold constraint identity".to_owned()
                })?;
            match self.coerce_typed_static_reference_encoded(&reference_cell, value)? {
                Some(replacement) => replacement,
                None => {
                    let Some(replacement) =
                        self.duplicate_authoritative_dereferenced_native_value(value)?
                    else {
                        return Ok(false);
                    };
                    replacement
                }
            }
        } else {
            let Some(replacement) =
                self.duplicate_authoritative_dereferenced_native_value(value)?
            else {
                return Ok(false);
            };
            replacement
        };
        if !self.replace_direct_reference_payload_owned(reference, replacement)? {
            self.release(replacement)?;
            return Ok(false);
        }
        self.mark_roots_dirty(RootMutationReason::RootedContainer);
        Ok(true)
    }

    /// Publishes the canonical request reference for every `global` binding.
    /// This is publication-time work: generated code consumes only the fixed
    /// trusted slot and direct native reference descriptor.
    pub(super) fn prepare_trusted_global_references(&mut self) -> Result<(), String> {
        self.ensure_native_global_references();
        let published_functions = self.published_native_functions();
        let mut binding_sites = Vec::new();
        for function in &published_functions {
            let Some(instructions) = self.prepared_continuation_instructions(*function) else {
                continue;
            };
            for (continuation, instruction) in instructions.iter().enumerate() {
                let Some(instruction) = instruction.as_ref() else {
                    continue;
                };
                let php_ir::InstructionKind::BindGlobal { name, .. } = &instruction.kind else {
                    continue;
                };
                binding_sites.push((function.raw(), continuation, name.clone()));
            }
        }
        for (function, continuation, name) in binding_sites {
            let encoded = self.native_request_local_handle(&name)?;
            let continuation = u32::try_from(continuation)
                .map_err(|_| "native global continuation exceeds the ABI".to_owned())?;
            self.publish_native_global_reference(function, continuation, &name, encoded)?;
        }
        let mut dimension_sites = Vec::new();
        for function in published_functions {
            let Some(sites) = self.compiled.prepared_native_global_sites(function) else {
                continue;
            };
            for (continuation, name) in sites.iter().enumerate() {
                let Some(name) = name.as_deref() else {
                    continue;
                };
                dimension_sites.push((function.raw(), continuation, name.to_owned()));
            }
        }
        for (function, continuation, name) in dimension_sites {
            // A direct `$GLOBALS["name"]` plan may refer to a symbol that is
            // not visible yet. Its direct reference starts as uninitialized,
            // so publication preserves PHP visibility while giving every
            // top-level local and dimension site one shared native identity.
            let encoded = self
                .native_request_local_handle(&name)
                .expect("constant native global must have a request reference");
            let continuation = u32::try_from(continuation)
                .expect("native global continuation index must fit the ABI");
            self.publish_native_global_reference(function, continuation, &name, encoded)
                .expect("constant native global plan must publish before execution");
        }
        Ok(())
    }

    pub(super) fn native_reference_identity(&self, encoded: i64) -> Option<u64> {
        if encoded as u64 & php_jit::JIT_VALUE_RUNTIME_KIND_MASK
            != php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG
        {
            return None;
        }
        if let Some(index) = Self::direct_value_index(encoded) {
            return self
                .baseline_values
                .direct_reference_cells
                .get(&index)
                .map(php_runtime::api::ReferenceCell::gc_debug_id);
        }
        None
    }

    pub(super) fn direct_native_reference_cell(
        &self,
        encoded: i64,
    ) -> Option<php_runtime::api::ReferenceCell> {
        let index = Self::direct_value_index(encoded)?;
        self.baseline_values
            .direct_reference_cells
            .get(&index)
            .cloned()
    }

    pub(super) fn invalidate_native_global_reference(
        &mut self,
        reference_identity: u64,
    ) -> Result<(), String> {
        let mut retained = Self::take_trusted_global_reference_slots(
            &mut self.trusted_global_reference_slots,
            &mut self.trusted_global_reference_names,
            |slot, _| slot.reference_identity == reference_identity,
        );
        for package in &mut self.dynamic_units {
            retained.extend(Self::take_trusted_global_reference_slots(
                &mut package.runtime_state.trusted_global_reference_slots,
                &mut package.runtime_state.trusted_global_reference_names,
                |slot, _| slot.reference_identity == reference_identity,
            ));
        }
        for encoded in retained {
            self.release(encoded)?;
        }
        let global_handles = self
            .native_global_reference_handles
            .iter()
            .filter_map(|(name, encoded)| {
                (self.native_reference_identity(*encoded) == Some(reference_identity))
                    .then_some(name.clone())
            })
            .collect::<Vec<_>>();
        for name in global_handles {
            if let Some(encoded) = self.native_global_reference_handles.remove(&name) {
                self.release(encoded)?;
            }
        }
        Ok(())
    }

    fn take_trusted_global_reference_slots(
        slots: &mut [php_jit::JitNativeTrustedGlobalReferenceSlot],
        names: &mut std::collections::BTreeMap<usize, Box<str>>,
        mut remove: impl FnMut(&php_jit::JitNativeTrustedGlobalReferenceSlot, Option<&str>) -> bool,
    ) -> Vec<i64> {
        let removed = names
            .iter()
            .filter_map(|(index, name)| {
                let slot = slots.get(*index)?;
                (slot.state == php_jit::JIT_NATIVE_TRUSTED_GLOBAL_REFERENCE_PUBLISHED
                    && remove(slot, Some(name.as_ref())))
                .then_some(*index)
            })
            .collect::<Vec<_>>();
        removed
            .into_iter()
            .filter_map(|index| {
                names.remove(&index);
                let slot = slots.get_mut(index)?;
                let encoded = slot.encoded;
                *slot = php_jit::JitNativeTrustedGlobalReferenceSlot::default();
                Some(encoded)
            })
            .collect()
    }

    pub(super) fn republish_trusted_global_references_for_all_units(
        &mut self,
    ) -> Result<(), String> {
        self.prepare_trusted_request_locals();
        self.prepare_trusted_global_references()?;
        let active = self.current_dynamic_unit;
        for unit in 0..self.dynamic_units.len() {
            if Some(unit) == active {
                continue;
            }
            self.with_active_dynamic_unit(unit, None, |_| ())?;
        }
        Ok(())
    }

    pub(super) fn reconcile_trusted_global_references(&mut self) -> Result<(), String> {
        let current = self
            .baseline_values
            .inherited_globals
            .iter()
            .filter_map(|(name, value)| match value {
                Value::Reference(reference) => Some((name.clone(), reference.gc_debug_id())),
                _ => None,
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut retained = Self::take_trusted_global_reference_slots(
            &mut self.trusted_global_reference_slots,
            &mut self.trusted_global_reference_names,
            |slot, name| {
                name.is_none_or(|name| current.get(name) != Some(&slot.reference_identity))
            },
        );
        for package in &mut self.dynamic_units {
            retained.extend(Self::take_trusted_global_reference_slots(
                &mut package.runtime_state.trusted_global_reference_slots,
                &mut package.runtime_state.trusted_global_reference_names,
                |slot, name| {
                    name.is_none_or(|name| current.get(name) != Some(&slot.reference_identity))
                },
            ));
        }
        let changed = !retained.is_empty();
        for encoded in retained {
            self.release(encoded)?;
        }
        if changed {
            self.republish_trusted_global_references_for_all_units()?;
        }
        Ok(())
    }

    fn publish_native_global_reference(
        &mut self,
        function: u32,
        continuation: u32,
        name: &str,
        encoded: i64,
    ) -> Result<(), String> {
        let Some(reference_identity) = self.native_reference_identity(encoded) else {
            return Err("native global binding reference handle has no reference cell".to_owned());
        };
        let Some(base) = self
            .trusted_property_function_offsets
            .get(function as usize)
            .copied()
            .and_then(|base| usize::try_from(base).ok())
        else {
            return Err("native global-binding function index is missing".to_owned());
        };
        let index = base
            .checked_add(continuation as usize)
            .ok_or_else(|| "native global-binding continuation index overflow".to_owned())?;
        let previous = self
            .trusted_global_reference_slots
            .get(index)
            .copied()
            .ok_or_else(|| "native global-binding continuation is missing".to_owned())?;
        if previous.state == php_jit::JIT_NATIVE_TRUSTED_GLOBAL_REFERENCE_PUBLISHED
            && previous.encoded == encoded
            && previous.reference_identity == reference_identity
            && self
                .trusted_global_reference_names
                .get(&index)
                .map(Box::as_ref)
                == Some(name)
        {
            return Ok(());
        }

        // The call result already owns one handle for the destination local.
        // The trusted slot owns another until replacement or request reset.
        self.retain(encoded)?;
        self.trusted_global_reference_slots[index] = php_jit::JitNativeTrustedGlobalReferenceSlot {
            encoded,
            reference_identity,
            state: php_jit::JIT_NATIVE_TRUSTED_GLOBAL_REFERENCE_PUBLISHED,
            reserved: 0,
            reserved_wide: 0,
        };
        self.trusted_global_reference_names
            .insert(index, name.into());
        if previous.state == php_jit::JIT_NATIVE_TRUSTED_GLOBAL_REFERENCE_PUBLISHED {
            self.release(previous.encoded)?;
        }
        Ok(())
    }

    pub(super) fn clear_trusted_global_references(&mut self) {
        let values = Self::take_trusted_global_reference_slots(
            &mut self.trusted_global_reference_slots,
            &mut self.trusted_global_reference_names,
            |_, _| true,
        );
        for encoded in values {
            let _ = self.release_if_live(encoded);
        }
    }
}

fn typed_static_reference_constraint_description(
    constraint: &NativeTypedStaticReferenceConstraint,
) -> String {
    format!(
        "property {}::${} of type {}",
        constraint.owner_display_name,
        constraint.property,
        native_ir_type_name(&constraint.type_)
    )
}

fn typed_static_property_description(
    declaration: &NativeStaticPropertyDeclaration,
    property: &str,
) -> String {
    format!(
        "property {}::${property} of type {}",
        declaration.owner_display_name,
        declaration
            .type_
            .as_ref()
            .map(native_ir_type_name)
            .unwrap_or_else(|| "mixed".to_owned())
    )
}

fn inconsistent_typed_static_reference_assignment(
    actual: &str,
    first: &NativeTypedStaticReferenceConstraint,
    second: &NativeTypedStaticReferenceConstraint,
) -> String {
    format!(
        "E_PHP_THROW:TypeError:Cannot assign {actual} to reference held by {} and {}, as this would result in an inconsistent type conversion",
        typed_static_reference_constraint_description(first),
        typed_static_reference_constraint_description(second)
    )
}
