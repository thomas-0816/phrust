//! Native reference identity, typed-property constraints, and ownership transfer.
//!
//! Direct reference descriptors remain authoritative while generated native
//! code executes. `ReferenceCell` exists only as a cold identity sidecar for
//! PHP-visible aliases and typed-property constraint recovery.

use super::*;

pub(super) fn direct_reference_payload_is_total(
    direct_value_slots: &[php_jit::JitNativeValueSlot],
    mut encoded: i64,
) -> bool {
    let mut visited = [usize::MAX; 16];
    let mut visited_count = 0_usize;

    loop {
        if let Some(constant) = php_jit::jit_decode_constant(encoded) {
            return matches!(
                constant,
                u32::MAX | php_jit::JIT_VALUE_FALSE | php_jit::JIT_VALUE_TRUE
            );
        }
        let Some(tag) = php_jit::jit_runtime_value_tag(encoded) else {
            // Zero is also the demand-zero payload of a descriptor whose
            // recursive publication has not completed. Without a separate
            // initialized bit it cannot be admitted as a proven integer zero.
            return encoded != 0;
        };
        let Some(index) = NativeRequestColdState::direct_value_index(encoded) else {
            return false;
        };
        let Some(slot) = direct_value_slots
            .get(index)
            .copied()
            .filter(|slot| slot.refcount != 0)
        else {
            return false;
        };

        if tag == php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG {
            if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                || slot.flags != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
                || native_reference_state(slot.reserved)
                    != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED
                || visited_count == visited.len()
                || visited[..visited_count].contains(&index)
            {
                return false;
            }
            visited[visited_count] = index;
            visited_count += 1;
            encoded = slot.payload as i64;
            continue;
        }

        return match tag {
            php_jit::JIT_VALUE_RUNTIME_TAG => {
                slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT
                    && slot.flags == php_jit::JIT_NATIVE_DIRECT_INT_ABI_VERSION
            }
            php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG => {
                slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
                    && slot.flags & php_jit::JIT_NATIVE_DIRECT_ARRAY_FLAGS_VERSION_MASK
                        == php_jit::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION
                    && slot.aux != 0
            }
            php_jit::JIT_VALUE_RUNTIME_OBJECT_TAG => {
                slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
            }
            php_jit::JIT_VALUE_RUNTIME_STRING_TAG => {
                slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_STRING
                    && slot.flags == php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION
                    && slot.aux != 0
            }
            php_jit::JIT_VALUE_RUNTIME_FLOAT_TAG => {
                slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT
            }
            php_jit::JIT_VALUE_RUNTIME_RESOURCE_TAG => {
                slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_RESOURCE
                    && slot.flags == php_jit::JIT_NATIVE_DIRECT_RESOURCE_ABI_VERSION
                    && slot.aux != 0
            }
            php_jit::JIT_VALUE_RUNTIME_CALLABLE_TAG => {
                slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE
                    && slot.flags == php_jit::JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION
                    && slot.aux != 0
            }
            php_jit::JIT_VALUE_RUNTIME_GENERATOR_TAG => {
                slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_GENERATOR
                    && slot.flags == php_jit::JIT_NATIVE_DIRECT_GENERATOR_ABI_VERSION
                    && slot.aux != 0
            }
            php_jit::JIT_VALUE_RUNTIME_FIBER_TAG => {
                slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER
                    && slot.flags == php_jit::JIT_NATIVE_DIRECT_FIBER_ABI_VERSION
                    && slot.aux != 0
            }
            _ => false,
        };
    }
}

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
        // Rebinding or unsetting a global can change the value shape observed
        // by a function whose optimizer admission was compiled against the
        // preceding canonical reference. Select its already-published
        // baseline entry at this cold mutation boundary; generated code never
        // validates the global identity or shape per invocation.
        for package in &self.dynamic_units {
            super::cold_dynamic_units::select_baseline_for_global_plan_functions(package);
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
        let payload_facts = if direct_reference_payload_is_total(&self.direct_value_slots, encoded)
        {
            php_jit::JIT_NATIVE_TRUSTED_GLOBAL_REFERENCE_PAYLOAD_TOTAL
        } else {
            0
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
            && previous.reserved == payload_facts
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
            reserved: payload_facts,
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
