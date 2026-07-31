//! Explicit baseline iterator and materialized generator compatibility.
//!
//! Optimizing foreach and direct generators use native slots and CLIF
//! state machines. Rust `PhpArray`, `Value`, and snapshot iteration live
//! only in this one cold continuation module.

use super::*;

impl NativeGeneratorAdvance {
    fn into_entry(self) -> Result<Option<(i64, i64)>, String> {
        match self {
            Self::Yielded { key, value } => Ok(Some((key, value))),
            Self::Complete => Ok(None),
            Self::FiberSuspended { .. } => {
                Err("E_PHP_SUSPEND_FIBER:direct Generator suspended its Fiber".to_owned())
            }
        }
    }
}
use php_runtime::api::PhpString;
use php_runtime::api::Value;

pub(super) struct NativeSnapshotIteratorState {
    pub(super) entries: Vec<(Value, Value)>,
    pub(super) index: usize,
}

/// Baseline-only iterator state. The allocation is owned by one authoritative
/// direct value slot; there is no parallel request value table or mirror slot.
pub(super) enum NativeColdIterator {
    Array(Box<NativeArrayIteratorState>),
    Object(Box<NativeObjectIteratorState>),
    Snapshot(Box<NativeSnapshotIteratorState>),
    LiveArray(Box<NativeLiveArrayIteratorState>),
    User(Box<NativeUserIteratorState>),
    Generator(Box<BaselineGeneratorIteratorState>),
}

pub(super) struct NativeArrayIteratorState {
    pub(super) source: php_runtime::api::PhpArray,
    pub(super) index: usize,
    pub(super) direct: Option<Box<NativeDirectForeachState>>,
}

pub(super) struct NativeDirectForeachState {
    pub(super) view: Box<php_jit::JitNativeForeachView>,
    pub(super) entries: Box<[php_jit::JitNativeForeachEntry]>,
}

pub(super) struct NativeObjectIteratorState {
    pub(super) source: i64,
    pub(super) object: php_runtime::api::ObjectRef,
    pub(super) names: Vec<String>,
    pub(super) keys: Vec<i64>,
    pub(super) index: usize,
}

pub(super) struct NativeLiveArrayIteratorState {
    pub(super) source: i64,
    pub(super) global: Option<String>,
    pub(super) index: usize,
}

pub(super) struct NativeUserIteratorState {
    pub(super) object: php_runtime::api::ObjectRef,
    pub(super) started: bool,
    pub(super) valid: php_ir::FunctionId,
    pub(super) current: php_ir::FunctionId,
    pub(super) key: php_ir::FunctionId,
    pub(super) next: php_ir::FunctionId,
}

#[derive(Clone)]
pub(super) enum BaselineGeneratorDelegation {
    Array {
        entries: Vec<(Value, Value)>,
        index: usize,
    },
    Generator {
        generator: php_runtime::api::GeneratorRef,
        iterator: i64,
    },
}

pub(super) struct BaselineGeneratorIteratorState {
    pub(super) generator: php_runtime::api::GeneratorRef,
    pub(super) handle: Box<php_jit::JitFunctionHandle>,
    pub(super) arguments: Vec<i64>,
    pub(super) state: Box<Option<php_jit::JitDeoptState>>,
    pub(super) delegation: Option<BaselineGeneratorDelegation>,
    pub(super) yields_seen: u64,
    pub(super) finished: bool,
}

impl<'a> NativeRequestColdState<'a> {
    pub(super) fn mutate_array(
        &mut self,
        encoded: i64,
        mutate: impl FnOnce(&mut php_runtime::api::PhpArray),
    ) -> Result<(), String> {
        self.mutate_array_with(encoded, mutate)
    }

    pub(super) fn mutate_array_with<T>(
        &mut self,
        encoded: i64,
        mutate: impl FnOnce(&mut php_runtime::api::PhpArray) -> T,
    ) -> Result<T, String> {
        if encoded as u64 & php_jit::JIT_VALUE_RUNTIME_KIND_MASK
            == php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG
        {
            if let Some(index) = Self::direct_value_index(encoded) {
                let slot = self
                    .direct_value_slots
                    .get(index)
                    .copied()
                    .filter(|slot| slot.refcount != 0)
                    .ok_or_else(|| format!("direct native reference {index} is missing"))?;
                if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                    && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
                    && native_reference_state(slot.reserved)
                        != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
                {
                    // The direct payload, not the cold ReferenceCell sidecar,
                    // is authoritative until materialization. Mutate that
                    // payload in place so foreach-by-reference preserves both
                    // the reference identity and the array's native handle.
                    return self.mutate_array_with(slot.payload as i64, mutate);
                }
            }
            if let Value::Reference(reference) = self.decode_baseline_value(encoded)? {
                let mut value = reference.get();
                let Value::Array(array) = &mut value else {
                    return Err("native reference does not contain an array".to_owned());
                };
                let result = mutate(array);
                reference.set(value);
                return Ok(result);
            }
            return Err("native reference handle is unavailable".to_owned());
        }
        if let Some(index) = Self::direct_value_index(encoded) {
            let Value::Array(mut array) = self.baseline_decode_direct_array(index)? else {
                return Err("direct native value is not an array".to_owned());
            };
            let result = mutate(&mut array);
            self.replace_direct_array(index, array)?;
            return Ok(result);
        }
        Err("native value is not an array or array reference".to_owned())
    }

    pub(super) fn encode_snapshot_iterator(
        &mut self,
        entries: Vec<(Value, Value)>,
    ) -> Result<i64, String> {
        self.publish_cold_iterator(NativeColdIterator::Snapshot(Box::new(
            NativeSnapshotIteratorState { entries, index: 0 },
        )))
    }

    pub(super) fn encode_live_array_iterator(
        &mut self,
        source: i64,
        global: Option<String>,
    ) -> Result<i64, String> {
        self.retain(source)?;
        match self.publish_cold_iterator(NativeColdIterator::LiveArray(Box::new(
            NativeLiveArrayIteratorState {
                source,
                global,
                index: 0,
            },
        ))) {
            Ok(iterator) => Ok(iterator),
            Err(error) => {
                let _ = self.release(source);
                Err(error)
            }
        }
    }

    pub(super) fn encode_user_iterator(
        &mut self,
        object: php_runtime::api::ObjectRef,
    ) -> Result<i64, String> {
        let class_name = object.class_name();
        let valid = native_method_in_hierarchy(self, &class_name, "valid")
            .ok_or_else(|| "Iterator::valid() is missing".to_owned())?;
        let current = native_method_in_hierarchy(self, &class_name, "current")
            .ok_or_else(|| "Iterator::current() is missing".to_owned())?;
        let key = native_method_in_hierarchy(self, &class_name, "key")
            .ok_or_else(|| "Iterator::key() is missing".to_owned())?;
        let next = native_method_in_hierarchy(self, &class_name, "next")
            .ok_or_else(|| "Iterator::next() is missing".to_owned())?;
        self.publish_cold_iterator(NativeColdIterator::User(Box::new(
            NativeUserIteratorState {
                object,
                started: false,
                valid,
                current,
                key,
                next,
            },
        )))
    }

    pub(super) fn encode_object_iterator(
        &mut self,
        source: i64,
        object: php_runtime::api::ObjectRef,
        names: Vec<String>,
    ) -> Result<i64, String> {
        self.retain(source)?;
        let mut keys = Vec::with_capacity(names.len());
        for name in &names {
            match self.encode_native_string_owner(PhpString::from_bytes(name.as_bytes().to_vec())) {
                Ok(key) => keys.push(key),
                Err(error) => {
                    for key in keys {
                        let _ = self.release(key);
                    }
                    let _ = self.release(source);
                    return Err(error);
                }
            }
        }
        let cleanup_keys = keys.clone();
        match self.publish_cold_iterator(NativeColdIterator::Object(Box::new(
            NativeObjectIteratorState {
                source,
                object,
                names,
                keys,
                index: 0,
            },
        ))) {
            Ok(iterator) => Ok(iterator),
            Err(error) => {
                // Publication did not consume the object-source owner.
                // Encoded keys were freshly allocated above and have no
                // remaining observer on this error edge.
                for key in cleanup_keys {
                    let _ = self.release(key);
                }
                let _ = self.release(source);
                Err(error)
            }
        }
    }

    pub(super) fn encode_array_iterator(
        &mut self,
        source: php_runtime::api::PhpArray,
    ) -> Result<i64, String> {
        // A by-value foreach over an immutable COW snapshot can publish all
        // non-reference entries once. Ordinary loop iterations then advance a
        // request-owned ABI cursor without crossing back into Rust. Reference
        // elements remain on the semantic helper path because their value is
        // intentionally observed at each iteration.
        let snapshot = source
            .iter()
            .map(|(key, value)| {
                let key = match key {
                    php_runtime::api::ArrayKey::Int(key) => Value::Int(key),
                    php_runtime::api::ArrayKey::String(key) => Value::String(key.clone()),
                };
                (key, value.clone())
            })
            .collect::<Vec<_>>();
        let direct = if snapshot
            .iter()
            .any(|(_, value)| matches!(value, Value::Reference(_)))
        {
            None
        } else {
            let mut entries = Vec::with_capacity(snapshot.len());
            for (key, value) in snapshot {
                let key = match self.encode_baseline_value(key) {
                    Ok(key) => key,
                    Err(error) => {
                        self.release_direct_foreach_entries(&entries);
                        return Err(error);
                    }
                };
                let value = match self.encode_baseline_value(value) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = self.release(key);
                        self.release_direct_foreach_entries(&entries);
                        return Err(error);
                    }
                };
                entries.push(php_jit::JitNativeForeachEntry { key, value });
            }
            let entries = entries.into_boxed_slice();
            let view = Box::new(php_jit::JitNativeForeachView {
                cursor: 0,
                length: entries.len() as u64,
                entries: entries.as_ptr() as usize as u64,
            });
            Some(Box::new(NativeDirectForeachState { view, entries }))
        };
        self.publish_cold_iterator(NativeColdIterator::Array(Box::new(
            NativeArrayIteratorState {
                source,
                index: 0,
                direct,
            },
        )))
    }

    pub(super) fn release_direct_foreach_entries(
        &mut self,
        entries: &[php_jit::JitNativeForeachEntry],
    ) {
        for entry in entries {
            for encoded in [entry.key, entry.value] {
                let _ = self.release(encoded);
            }
        }
    }

    pub(super) fn encode_baseline_generator_iterator(
        &mut self,
        generator: php_runtime::api::GeneratorRef,
    ) -> Result<i64, String> {
        let function = php_ir::FunctionId::new(generator.function());
        let handle = ensure_native_entry(self, function)?;
        let arguments = generator
            .args()
            .into_iter()
            .map(|value| self.encode_baseline_value(value))
            .collect::<Result<Vec<_>, _>>()?;
        self.publish_cold_iterator(NativeColdIterator::Generator(Box::new(
            BaselineGeneratorIteratorState {
                generator,
                handle: Box::new(handle),
                arguments,
                state: Box::new(None),
                delegation: None,
                yields_seen: 0,
                finished: false,
            },
        )))
    }

    pub(super) fn baseline_generator_iterator(
        &mut self,
        generator: php_runtime::api::GeneratorRef,
    ) -> Result<i64, String> {
        if let Some(encoded) = self
            .baseline_generator_iterators
            .get(&generator.id())
            .copied()
        {
            return Ok(encoded);
        }
        let id = generator.id();
        let encoded = self.encode_baseline_generator_iterator(generator)?;
        self.baseline_generator_iterators.insert(id, encoded);
        Ok(encoded)
    }

    pub(super) fn resume_baseline_iterator(
        &mut self,
        encoded: i64,
        resume_kind: php_jit::JitNativeResumeInputKind,
        resume_value: i64,
    ) -> Result<Option<(Value, Value)>, String> {
        let index = Self::direct_value_index(encoded)
            .ok_or_else(|| "native value is not a foreach iterator handle".to_owned())?;
        let live = match self.cold_iterator(index) {
            Some(NativeColdIterator::LiveArray(iterator)) => {
                Some((iterator.source, iterator.index, iterator.global.clone()))
            }
            _ => None,
        };
        if let Some((source, cursor, live_global)) = live {
            let reference_entry = |array: &mut php_runtime::api::PhpArray| {
                let (key, value) = array
                    .iter()
                    .nth(cursor)
                    .map(|(key, value)| (key.clone(), value.clone()))?;
                let reference = match value {
                    Value::Reference(reference) => reference,
                    value => {
                        let reference = php_runtime::api::ReferenceCell::new(value);
                        array.insert(key.clone(), Value::Reference(reference.clone()));
                        reference
                    }
                };
                let key = match key {
                    php_runtime::api::ArrayKey::Int(key) => Value::Int(key),
                    php_runtime::api::ArrayKey::String(key) => Value::String(key),
                };
                Some((key, Value::Reference(reference)))
            };
            let entry = if let Some(global) = live_global {
                let Some(root) = self.baseline_values.inherited_globals.get(&global).cloned()
                else {
                    return Ok(None);
                };
                match root {
                    Value::Reference(reference) => {
                        let Value::Array(mut array) = reference.get() else {
                            return Ok(None);
                        };
                        let entry = reference_entry(&mut array);
                        reference.set(Value::Array(array));
                        entry
                    }
                    Value::Array(mut array) => {
                        let entry = reference_entry(&mut array);
                        self.baseline_values
                            .inherited_globals
                            .insert(global, Value::Array(array));
                        entry
                    }
                    _ => None,
                }
            } else {
                self.mutate_array_with(source, reference_entry)?
            };
            let Some(entry) = entry else {
                return Ok(None);
            };
            if let Some(NativeColdIterator::LiveArray(iterator)) = self.cold_iterator_mut(index) {
                iterator.index = iterator.index.saturating_add(1);
            }
            return Ok(Some(entry));
        }
        if let Some(NativeColdIterator::Snapshot(iterator)) = self.cold_iterator_mut(index) {
            let entry = iterator
                .entries
                .get(iterator.index)
                .cloned()
                .map(|(key, value)| {
                    let value = match value {
                        Value::Reference(reference) => reference.get(),
                        value => value,
                    };
                    (key, value)
                });
            iterator.index = iterator.index.saturating_add(usize::from(entry.is_some()));
            return Ok(entry);
        }
        let (generator, handle, arguments, state, delegation, finished) =
            match self.cold_iterator(index) {
                Some(NativeColdIterator::Generator(iterator)) => (
                    iterator.generator.clone(),
                    iterator.handle.clone(),
                    iterator.arguments.clone(),
                    iterator.state.clone(),
                    iterator.delegation.clone(),
                    iterator.finished,
                ),
                _ => return Err(format!("native foreach iterator {index} is missing")),
            };
        if finished {
            return Ok(None);
        }
        let mut effective_resume_kind = resume_kind;
        let mut effective_resume_value = resume_value;
        if let Some(delegation) = delegation {
            match delegation {
                BaselineGeneratorDelegation::Array {
                    entries,
                    index: cursor,
                } => {
                    if let Some((key, value)) = entries.get(cursor).cloned() {
                        if let Some(NativeColdIterator::Generator(iterator)) =
                            self.cold_iterator_mut(index)
                            && let Some(BaselineGeneratorDelegation::Array {
                                index: saved_cursor,
                                ..
                            }) = iterator.delegation.as_mut()
                        {
                            *saved_cursor = saved_cursor.saturating_add(1);
                        }
                        generator.suspend_forwarded(Some(key.clone()), value.clone());
                        if let Some(NativeColdIterator::Generator(iterator)) =
                            self.cold_iterator_mut(index)
                        {
                            iterator.yields_seen = iterator.yields_seen.saturating_add(1);
                        }
                        return Ok(Some((key, value)));
                    }
                    if let Some(NativeColdIterator::Generator(iterator)) =
                        self.cold_iterator_mut(index)
                    {
                        iterator.delegation = None;
                    }
                    effective_resume_kind = php_jit::JitNativeResumeInputKind::VALUE;
                    effective_resume_value = php_jit::jit_encode_constant(u32::MAX);
                }
                BaselineGeneratorDelegation::Generator {
                    generator: delegated,
                    iterator,
                } => {
                    if let Some((key, value)) = self.baseline_iterator_next(iterator)? {
                        generator.suspend_forwarded(Some(key.clone()), value.clone());
                        if let Some(NativeColdIterator::Generator(iterator)) =
                            self.cold_iterator_mut(index)
                        {
                            iterator.yields_seen = iterator.yields_seen.saturating_add(1);
                        }
                        return Ok(Some((key, value)));
                    }
                    effective_resume_kind = php_jit::JitNativeResumeInputKind::VALUE;
                    effective_resume_value = self
                        .encode_baseline_value(delegated.return_value().unwrap_or(Value::Null))?;
                    if let Some(NativeColdIterator::Generator(iterator)) =
                        self.cold_iterator_mut(index)
                    {
                        iterator.delegation = None;
                    }
                }
            }
        }
        let outcome = if let Some(state) = state.as_ref() {
            let runtime = self.native_runtime_ptr();
            handle.invoke_i64_suspension_resume_with_native_unwind_runtime(
                &arguments,
                state,
                effective_resume_kind,
                effective_resume_value,
                php_jit::JIT_RUNTIME_ABI_HASH,
                runtime,
                |types, value| native_catch_matches(self, types, value),
            )
        } else {
            let runtime = self.native_runtime_ptr();
            handle.invoke_i64_with_deopt_runtime(&arguments, php_jit::JIT_RUNTIME_ABI_HASH, runtime)
        }
        .map_err(|error| format!("native generator invocation failed: {error:?}"))?;
        match outcome {
            php_jit::JitI64InvokeOutcome::SideExit {
                status,
                value,
                state,
            } if status == php_jit::JitCallStatus::SUSPEND_GENERATOR.0 as i32 => {
                if state.suspend_kind == php_jit::JitNativeSuspendKind::GENERATOR_DELEGATE.0 {
                    let delegated = self.decode_baseline_value(state.delegation_handle as i64)?;
                    let delegation = match delegated {
                        Value::Array(array) => BaselineGeneratorDelegation::Array {
                            entries: array
                                .iter()
                                .map(|(key, value)| {
                                    let key = match key {
                                        php_runtime::api::ArrayKey::Int(value) => Value::Int(value),
                                        php_runtime::api::ArrayKey::String(value) => {
                                            Value::String(value.clone())
                                        }
                                    };
                                    (key, value.clone())
                                })
                                .collect(),
                            index: 0,
                        },
                        Value::Generator(delegated) => BaselineGeneratorDelegation::Generator {
                            iterator: self.baseline_generator_iterator(delegated.clone())?,
                            generator: delegated,
                        },
                        other => {
                            return Err(format!(
                                "yield from expects an array or Traversable, got {}",
                                native_value_type_name(&other)
                            ));
                        }
                    };
                    if let Some(NativeColdIterator::Generator(iterator)) =
                        self.cold_iterator_mut(index)
                    {
                        *iterator.state = Some(state);
                        iterator.delegation = Some(delegation);
                    }
                    return self.baseline_iterator_next(encoded);
                }
                let key = if state.suspend_flags & 1 != 0 {
                    Some(self.decode_baseline_value(state.yielded_key)?)
                } else {
                    None
                };
                let value = self.decode_baseline_value(value)?;
                generator.suspend(key, value.clone());
                if let Some(NativeColdIterator::Generator(iterator)) = self.cold_iterator_mut(index)
                {
                    *iterator.state = Some(state);
                }
                if let Some(NativeColdIterator::Generator(iterator)) = self.cold_iterator_mut(index)
                {
                    iterator.yields_seen = iterator.yields_seen.saturating_add(1);
                }
                let (key, value) = generator
                    .current()
                    .ok_or_else(|| "native generator suspension value is missing".to_owned())?;
                Ok(Some((key.unwrap_or(Value::Null), value)))
            }
            php_jit::JitI64InvokeOutcome::Returned(value)
            | php_jit::JitI64InvokeOutcome::SideExit {
                status: 1 | 2,
                value,
                ..
            } => {
                generator.close(Some(self.decode_baseline_value(value)?));
                if let Some(NativeColdIterator::Generator(iterator)) = self.cold_iterator_mut(index)
                {
                    iterator.finished = true;
                }
                Ok(None)
            }
            php_jit::JitI64InvokeOutcome::SideExit { status, .. } => {
                Err(format!("native generator returned status {status}"))
            }
        }
    }

    pub(super) fn baseline_iterator_next(
        &mut self,
        encoded: i64,
    ) -> Result<Option<(Value, Value)>, String> {
        if let Some(entry) = self.baseline_array_iterator_next(encoded) {
            return Ok(entry);
        }
        self.resume_baseline_iterator(
            encoded,
            php_jit::JitNativeResumeInputKind::VALUE,
            php_jit::jit_encode_constant(u32::MAX),
        )
    }

    #[allow(unsafe_code)]
    pub(super) fn object_iterator_next_encoded(
        &mut self,
        encoded: i64,
    ) -> Result<Option<(i64, i64)>, String> {
        let Some(index) = Self::direct_value_index(encoded) else {
            return Ok(None);
        };
        let entry = match self.cold_iterator(index) {
            Some(NativeColdIterator::Object(iterator)) => iterator
                .names
                .get(iterator.index)
                .zip(iterator.keys.get(iterator.index))
                .map(|(name, key)| (iterator.object.clone(), name.clone(), *key, iterator.index)),
            _ => return Ok(None),
        };
        let Some((object, name, key, cursor)) = entry else {
            return Ok(None);
        };

        let layout_id = object.class_layout_epoch();
        let (value, value_is_owned) = if let Some(slot_index) = object.declared_slot_index(&name) {
            if let Some((base, count)) = object.native_declared_slots_view(layout_id) {
                let slot_index = usize::try_from(slot_index)
                    .map_err(|_| "native object iterator property index overflow".to_owned())?;
                if slot_index >= count {
                    return Err("native object iterator property index is out of bounds".to_owned());
                }
                // SAFETY: native object property slots are immovable for the
                // lifetime of the iterator's transferred source owner.
                let slot = unsafe { *base.add(slot_index) };
                ((slot.initialized != 0).then_some(slot.value), false)
            } else {
                (
                    object
                        .get_property(&name)
                        .map(|value| self.encode_baseline_value(value))
                        .transpose()?,
                    true,
                )
            }
        } else {
            if let Some(slot) = object.native_dynamic_property_slot(layout_id, &name) {
                (
                    slot.and_then(|slot| (slot.initialized != 0).then_some(slot.value)),
                    false,
                )
            } else {
                (
                    object
                        .get_property(&name)
                        .map(|value| self.encode_baseline_value(value))
                        .transpose()?,
                    true,
                )
            }
        };

        let key = self
            .duplicate_authoritative_native_value(key)?
            .ok_or_else(|| "native object iterator key is not authoritative".to_owned())?;
        let value = match value {
            Some(value) if value_is_owned => value,
            Some(value) => match self.duplicate_authoritative_dereferenced_native_value(value)? {
                Some(value) => value,
                None => {
                    self.release(key)?;
                    return Err("native object iterator property is not authoritative".to_owned());
                }
            },
            None => php_jit::jit_encode_constant(u32::MAX),
        };
        if let Some(NativeColdIterator::Object(iterator)) = self.cold_iterator_mut(index) {
            iterator.index = cursor.saturating_add(1);
        }
        Ok(Some((key, value)))
    }

    pub(super) fn user_iterator_next_encoded(
        &mut self,
        encoded: i64,
    ) -> Result<Option<(i64, i64)>, String> {
        let index = Self::direct_value_index(encoded)
            .ok_or_else(|| "native value is not a user iterator handle".to_owned())?;
        let (object, started, valid, current, key, next) = match self.cold_iterator(index) {
            Some(NativeColdIterator::User(iterator)) => (
                iterator.object.clone(),
                iterator.started,
                iterator.valid,
                iterator.current,
                iterator.key,
                iterator.next,
            ),
            _ => return Err("native user iterator state is missing".to_owned()),
        };
        let receiver = self.encode_native_object_owner(object)?;
        let result = (|| -> Result<Option<(i64, i64)>, String> {
            if started {
                let advanced = invoke_native_method(self, next, &[receiver])?;
                self.release(advanced)?;
            }
            let validity = invoke_native_method(self, valid, &[receiver])?;
            let truthy = match self.decode_baseline_value(validity) {
                Ok(validity) => native_property_truthy(&validity),
                Err(error) => {
                    let _ = self.release(validity);
                    return Err(error);
                }
            };
            self.release(validity)?;
            if !truthy {
                return Ok(None);
            }

            let value = invoke_native_method(self, current, &[receiver])?;
            let key = match invoke_native_method(self, key, &[receiver]) {
                Ok(key) => key,
                Err(error) => {
                    let _ = self.release(value);
                    return Err(error.into());
                }
            };
            if let Some(NativeColdIterator::User(iterator)) = self.cold_iterator_mut(index) {
                iterator.started = true;
            }
            Ok(Some((key, value)))
        })();
        let release_receiver = self.release(receiver);
        match (result, release_receiver) {
            (Err(error), _) => Err(error),
            (Ok(Some((key, value))), Err(error)) => {
                let _ = self.release(key);
                let _ = self.release(value);
                Err(error)
            }
            (Ok(None), Err(error)) => Err(error),
            (Ok(result), Ok(())) => Ok(result),
        }
    }

    pub(super) fn iterator_next_encoded(
        &mut self,
        encoded: i64,
    ) -> Result<Option<(i64, i64)>, String> {
        if self.direct_generator_index(encoded).is_some() {
            return self.resume_direct_generator(
                encoded,
                php_jit::JitNativeResumeInputKind::VALUE,
                php_jit::jit_encode_constant(u32::MAX),
            );
        }
        if let Some(index) = Self::direct_value_index(encoded) {
            let iterator = *self
                .direct_value_slots
                .get(index)
                .ok_or_else(|| "direct foreach iterator slot is missing".to_owned())?;
            if iterator.refcount != 0
                && iterator.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FOREACH
            {
                let cursor = usize::try_from(iterator.aux)
                    .map_err(|_| "direct foreach cursor is invalid".to_owned())?;
                let live_reference =
                    iterator.reserved == php_jit::JIT_NATIVE_DIRECT_FOREACH_LIVE_REFERENCE;
                let source = if live_reference {
                    let reference_index = Self::direct_value_index(iterator.payload as i64)
                        .ok_or_else(|| {
                            "direct foreach live-source reference is invalid".to_owned()
                        })?;
                    let reference =
                        *self
                            .direct_value_slots
                            .get(reference_index)
                            .ok_or_else(|| {
                                "direct foreach live-source reference is missing".to_owned()
                            })?;
                    if reference.refcount == 0
                        || reference.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                    {
                        return Err(
                            "direct foreach live-source reference is unavailable".to_owned()
                        );
                    }
                    reference.payload as i64
                } else {
                    iterator.payload as i64
                };
                let source_index = Self::direct_value_index(source)
                    .ok_or_else(|| "direct foreach source handle is invalid".to_owned())?;
                let source = *self
                    .direct_value_slots
                    .get(source_index)
                    .ok_or_else(|| "direct foreach source slot is missing".to_owned())?;
                let length = if live_reference {
                    usize::try_from(source.payload)
                        .map_err(|_| "direct foreach source length is invalid".to_owned())?
                } else {
                    iterator.reserved as usize
                };
                if cursor >= length {
                    return Ok(None);
                }
                let base = self.direct_array_entries.as_ptr() as usize;
                let address = usize::try_from(source.aux)
                    .map_err(|_| "direct foreach entry address is invalid".to_owned())?;
                let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
                let start = address
                    .checked_sub(base)
                    .map(|offset| offset / entry_size)
                    .ok_or_else(|| "direct foreach entry range is invalid".to_owned())?;
                let entry = *self
                    .direct_array_entries
                    .get(start.saturating_add(cursor))
                    .ok_or_else(|| "direct foreach entry is missing".to_owned())?;
                let key = self
                    .duplicate_authoritative_native_value(entry.key)?
                    .ok_or_else(|| "direct foreach key is not authoritative".to_owned())?;
                let value = if live_reference {
                    match self.duplicate_authoritative_native_value(entry.value) {
                        Ok(Some(value)) => value,
                        Ok(None) => {
                            self.release(key)?;
                            return Err(
                                "direct foreach reference entry is not authoritative".to_owned()
                            );
                        }
                        Err(error) => {
                            self.release(key)?;
                            return Err(error);
                        }
                    }
                } else {
                    match self.duplicate_authoritative_dereferenced_native_value(entry.value) {
                        Ok(Some(value)) => value,
                        Ok(None) => match self.duplicate_dereferenced_native_value(entry.value) {
                            Ok(value) => value,
                            Err(error) => {
                                self.release(key)?;
                                return Err(error);
                            }
                        },
                        Err(error) => {
                            self.release(key)?;
                            return Err(error);
                        }
                    }
                };
                self.direct_value_slots[index].aux = iterator.aux.saturating_add(1);
                return Ok(Some((key, value)));
            }
        }
        if matches!(
            Self::direct_value_index(encoded).and_then(|index| self.cold_iterator(index)),
            Some(NativeColdIterator::Object(_))
        ) {
            return self.object_iterator_next_encoded(encoded);
        }
        if matches!(
            Self::direct_value_index(encoded).and_then(|index| self.cold_iterator(index)),
            Some(NativeColdIterator::User(_))
        ) {
            return self.user_iterator_next_encoded(encoded);
        }
        self.baseline_iterator_next(encoded)?
            .map(|(key, value)| {
                let key = self.encode_baseline_value(key)?;
                match self.encode_baseline_value(value) {
                    Ok(value) => Ok((key, value)),
                    Err(error) => {
                        self.release(key)?;
                        Err(error)
                    }
                }
            })
            .transpose()
    }

    pub(super) fn baseline_array_iterator_next(
        &mut self,
        encoded: i64,
    ) -> Option<Option<(Value, Value)>> {
        if let Some(index) = Self::direct_value_index(encoded) {
            let iterator = *self.direct_value_slots.get(index)?;
            if iterator.refcount == 0 {
                return None;
            }
            if iterator.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FOREACH {
                let cursor = usize::try_from(iterator.aux).ok()?;
                let live_reference =
                    iterator.reserved == php_jit::JIT_NATIVE_DIRECT_FOREACH_LIVE_REFERENCE;
                let source = if live_reference {
                    let reference = Self::direct_value_index(iterator.payload as i64)?;
                    let reference = *self.direct_value_slots.get(reference)?;
                    if reference.refcount == 0
                        || reference.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                    {
                        return None;
                    }
                    reference.payload as i64
                } else {
                    iterator.payload as i64
                };
                let source = Self::direct_value_index(source)?;
                let source = *self.direct_value_slots.get(source)?;
                let length = if live_reference {
                    usize::try_from(source.payload).ok()?
                } else {
                    iterator.reserved as usize
                };
                if cursor >= length {
                    return Some(None);
                }
                let base = self.direct_array_entries.as_ptr() as usize;
                let address = usize::try_from(source.aux).ok()?;
                let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
                let start = address.checked_sub(base)? / entry_size;
                let entry = *self.direct_array_entries.get(start.checked_add(cursor)?)?;
                self.direct_value_slots[index].aux = iterator.aux.saturating_add(1);
                let key = self.decode_baseline_value(entry.key).ok()?;
                let value = self.decode_baseline_value(entry.value).ok()?;
                return Some(Some((key, value)));
            }
            let NativeColdIterator::Array(iterator) = self.cold_iterator_mut(index)? else {
                return None;
            };
            return Some(
                iterator
                    .source
                    .next_pair_at_cursor(&mut iterator.index)
                    .map(|(key, value)| {
                        let key = match key {
                            php_runtime::api::ArrayKey::Int(key) => Value::Int(key),
                            php_runtime::api::ArrayKey::String(key) => Value::String(key),
                        };
                        let value = match value {
                            Value::Reference(reference) => reference.get(),
                            value => value,
                        };
                        (key, value)
                    }),
            );
        }
        None
    }

    pub(super) fn generator_can_rewind(&self, encoded: i64) -> bool {
        if let Some(index) = self.direct_generator_index(encoded) {
            return self.direct_generator(index).is_some_and(|generator| {
                matches!(generator.yields_seen, 0 | 1)
                    && generator.lifecycle != php_runtime::api::GeneratorState::Closed
            });
        }
        let Some(index) = Self::direct_value_index(encoded) else {
            return false;
        };
        self.cold_iterator(index).is_some_and(|value| match value {
            NativeColdIterator::Generator(iterator) => {
                matches!(iterator.yields_seen, 0 | 1) && !iterator.finished
            }
            NativeColdIterator::Array(_)
            | NativeColdIterator::Object(_)
            | NativeColdIterator::Snapshot(_)
            | NativeColdIterator::LiveArray(_)
            | NativeColdIterator::User(_) => false,
        })
    }

    pub(super) fn close_iterator(&mut self, encoded: i64) -> Result<(), String> {
        if let Some(index) = Self::direct_value_index(encoded) {
            return self.release_direct_value_index(index);
        }
        let index = php_jit::jit_decode_runtime_value(encoded)
            .ok_or_else(|| "native value is not a foreach iterator handle".to_owned())?;
        Err(format!(
            "native foreach iterator {index} is outside the authoritative direct slot plane"
        ))
    }
    pub(super) fn publish_native_generator_owned(
        &mut self,
        target: NativeExecutionTarget,
        arguments: Vec<i64>,
    ) -> Result<i64, String> {
        let index = match self.reserve_direct_value_slot() {
            Ok(index) => index,
            Err(error) => {
                for argument in arguments {
                    let _ = self.release(argument);
                }
                return Err(error);
            }
        };
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .expect("direct Generator index is bounded by the native value arena");
        let function = target.function;
        let argument_count = arguments.len();
        let owner = Box::into_raw(Box::new(NativeDirectGenerator {
            target,
            arguments,
            argument_count,
            handle: None,
            state: None,
            lifecycle: php_runtime::api::GeneratorState::Created,
            current_key: None,
            current_value: None,
            return_value: None,
            next_auto_key: 0,
            delegation: None,
            yields_seen: 0,
        }));
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_GENERATOR,
            flags: php_jit::JIT_NATIVE_DIRECT_GENERATOR_ABI_VERSION,
            payload: u64::from(function.raw()),
            aux: owner as usize as u64,
            ..php_jit::JitNativeValueSlot::default()
        };
        Ok(php_jit::jit_encode_typed_runtime_value(
            runtime_index,
            php_jit::JIT_VALUE_RUNTIME_GENERATOR_TAG,
        ))
    }

    pub(super) fn encode_native_generator_owner(
        &mut self,
        generator: php_runtime::api::GeneratorRef,
    ) -> Result<i64, String> {
        if let Some(index) = self
            .baseline_values
            .direct_generator_handles
            .get(&generator.id())
            .copied()
        {
            let slot = self
                .direct_value_slots
                .get_mut(index as usize)
                .filter(|slot| {
                    slot.refcount != 0
                        && matches!(
                            slot.kind,
                            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_GENERATOR
                                | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_GENERATOR
                                | php_jit::JIT_NATIVE_VALUE_VIEW_COLD_GENERATOR
                        )
                })
                .ok_or_else(|| {
                    "native Generator identity points at a dead activation".to_owned()
                })?;
            slot.refcount = slot
                .refcount
                .checked_add(1)
                .ok_or_else(|| "native Generator refcount overflow".to_owned())?;
            let runtime_index = index
                .checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
                .ok_or_else(|| "native Generator handle overflow".to_owned())?;
            return Ok(php_jit::jit_encode_typed_runtime_value(
                runtime_index,
                php_jit::JIT_VALUE_RUNTIME_GENERATOR_TAG,
            ));
        }
        self.publish_cold_generator(generator)
    }
    pub(super) fn resume_direct_generator(
        &mut self,
        encoded: i64,
        resume_kind: php_jit::JitNativeResumeInputKind,
        resume_value: i64,
    ) -> Result<Option<(i64, i64)>, String> {
        self.advance_direct_generator(encoded, resume_kind, resume_value)?
            .into_entry()
    }

    pub(super) fn advance_direct_generator(
        &mut self,
        encoded: i64,
        resume_kind: php_jit::JitNativeResumeInputKind,
        resume_value: i64,
    ) -> Result<NativeGeneratorAdvance, String> {
        let index = self
            .direct_generator_index(encoded)
            .ok_or_else(|| "native value is not a direct Generator".to_owned())?;
        let lifecycle = self
            .direct_generator(index)
            .map(|generator| generator.lifecycle)
            .ok_or_else(|| format!("direct Generator {index} is missing"))?;
        if lifecycle == php_runtime::api::GeneratorState::Closed {
            return Ok(NativeGeneratorAdvance::Complete);
        }
        if lifecycle == php_runtime::api::GeneratorState::Running {
            return Err(
                "E_PHP_THROW:Exception:Cannot resume an already running generator".to_owned(),
            );
        }

        let mut effective_resume_kind = resume_kind;
        let mut effective_resume_value = resume_value;
        let delegation = self
            .direct_generator(index)
            .and_then(|generator| generator.delegation.as_ref())
            .map(|delegation| match delegation {
                NativeGeneratorDelegation::Array { source, cursor } => (0_u8, *source, *cursor),
                NativeGeneratorDelegation::Generator { generator } => (1_u8, *generator, 0),
            });
        if let Some((kind, delegated, cursor)) = delegation {
            if kind == 0 {
                let entry = self
                    .direct_array_entries_for(delegated)
                    .and_then(|entries| entries.get(cursor))
                    .copied();
                if let Some(entry) = entry {
                    let key = self.duplicate_direct_generator_value(entry.key)?;
                    let value = match self
                        .duplicate_authoritative_dereferenced_native_value(entry.value)?
                    {
                        Some(value) => value,
                        None => {
                            self.release(key)?;
                            return Err(
                                "direct Generator delegation reached a cold array value".to_owned()
                            );
                        }
                    };
                    if let Some(NativeGeneratorDelegation::Array { cursor, .. }) = self
                        .direct_generator_mut(index)
                        .and_then(|generator| generator.delegation.as_mut())
                    {
                        *cursor = cursor.saturating_add(1);
                    }
                    return self
                        .replace_direct_generator_current_owned(index, Some(key), value, true)
                        .map(|(key, value)| NativeGeneratorAdvance::Yielded { key, value });
                }
                let delegation = self
                    .direct_generator_mut(index)
                    .and_then(|generator| generator.delegation.take());
                if let Some(NativeGeneratorDelegation::Array { source, .. }) = delegation {
                    self.release(source)?;
                }
                effective_resume_kind = php_jit::JitNativeResumeInputKind::VALUE;
                effective_resume_value = php_jit::jit_encode_constant(u32::MAX);
            } else {
                match self.advance_direct_generator(
                    delegated,
                    php_jit::JitNativeResumeInputKind::VALUE,
                    php_jit::jit_encode_constant(u32::MAX),
                )? {
                    NativeGeneratorAdvance::Yielded { key, value } => {
                        return self
                            .replace_direct_generator_current_owned(index, Some(key), value, true)
                            .map(|(key, value)| NativeGeneratorAdvance::Yielded { key, value });
                    }
                    NativeGeneratorAdvance::FiberSuspended {
                        value,
                        active,
                        mut parents,
                    } => {
                        parents.push(encoded);
                        return Ok(NativeGeneratorAdvance::FiberSuspended {
                            value,
                            active,
                            parents,
                        });
                    }
                    NativeGeneratorAdvance::Complete => {}
                }
                effective_resume_kind = php_jit::JitNativeResumeInputKind::VALUE;
                effective_resume_value = {
                    let child_index = self
                        .direct_generator_index(delegated)
                        .ok_or_else(|| "delegated direct Generator disappeared".to_owned())?;
                    let return_value = self
                        .direct_generator(child_index)
                        .and_then(|generator| generator.return_value)
                        .unwrap_or_else(|| php_jit::jit_encode_constant(u32::MAX));
                    self.duplicate_direct_generator_value(return_value)?
                };
                let delegation = self
                    .direct_generator_mut(index)
                    .and_then(|generator| generator.delegation.take());
                if let Some(NativeGeneratorDelegation::Generator { generator }) = delegation {
                    self.release(generator)?;
                }
            }
        }

        let (target, arguments, argument_count, saved_state, saved_handle, starting) = {
            let generator = self
                .direct_generator(index)
                .ok_or_else(|| format!("direct Generator {index} is missing"))?;
            (
                generator.target.clone(),
                generator.arguments.clone(),
                generator.argument_count,
                generator.state,
                generator.handle.clone(),
                generator.lifecycle == php_runtime::api::GeneratorState::Created,
            )
        };
        let handle = match saved_handle {
            Some(handle) => handle,
            None => self.run_in_native_execution_target(&target, |context| {
                ensure_native_entry(context, target.function)
            })?,
        };
        if let Some(generator) = self.direct_generator_mut(index) {
            generator.lifecycle = php_runtime::api::GeneratorState::Running;
            generator.handle = Some(handle.clone());
        }
        let invocation_handle = handle.clone();
        let resume_arguments = saved_state
            .is_some()
            .then(|| vec![php_jit::jit_encode_constant(u32::MAX); argument_count]);
        let outcome = self.run_in_native_execution_target(&target, |context| {
            let runtime = context.native_runtime_ptr();
            let outcome = if let Some(state) = saved_state.as_ref() {
                if context.completed_nested_fiber_call.as_ref().is_some_and(
                    |(function, continuation, _, _)| {
                        *function == state.function_id && *continuation == state.continuation_id
                    },
                ) {
                    invocation_handle.invoke_i64_same_artifact_transition_with_unwind_runtime(
                        state,
                        php_jit::JIT_RUNTIME_ABI_HASH,
                        runtime,
                        |types, value| native_catch_matches(context, types, value),
                    )
                } else {
                    invocation_handle.invoke_i64_suspension_resume_with_native_unwind_runtime(
                        resume_arguments.as_deref().unwrap_or_default(),
                        state,
                        effective_resume_kind,
                        effective_resume_value,
                        php_jit::JIT_RUNTIME_ABI_HASH,
                        runtime,
                        |types, value| native_catch_matches(context, types, value),
                    )
                }
            } else {
                invocation_handle.invoke_i64_with_deopt_runtime(
                    &arguments,
                    php_jit::JIT_RUNTIME_ABI_HASH,
                    runtime,
                )
            };
            resume_native_optimizing_exit_with_artifact(context, Some(invocation_handle), outcome)
                .map(|(artifact, outcome)| {
                    (
                        artifact.expect("Generator invocation always has an active artifact"),
                        outcome,
                    )
                })
                .map_err(|error| format!("native Generator invocation failed: {error:?}"))
        });
        if self.completed_nested_fiber_call.as_ref().is_some_and(
            |(function, continuation, _, _)| {
                saved_state.as_ref().is_some_and(|state| {
                    *function == state.function_id && *continuation == state.continuation_id
                })
            },
        ) {
            self.completed_nested_fiber_call = None;
        }
        let (handle, outcome) = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                if let Some(generator) = self.direct_generator_mut(index) {
                    generator.lifecycle = php_runtime::api::GeneratorState::Errored;
                }
                if !starting {
                    let _ = self.release(effective_resume_value);
                }
                return Err(error);
            }
        };
        if let Some(generator) = self.direct_generator_mut(index) {
            generator.handle = Some(handle);
        }
        if starting && let Some(generator) = self.direct_generator_mut(index) {
            // First entry transferred the bound owners into its native frame.
            generator.arguments.clear();
        }
        match outcome {
            php_jit::JitI64InvokeOutcome::SideExit {
                status,
                value,
                state,
            } if status == php_jit::JitCallStatus::SUSPEND_GENERATOR.0 as i32 => {
                if state.suspend_kind == php_jit::JitNativeSuspendKind::GENERATOR_DELEGATE.0 {
                    let source = state.delegation_handle as i64;
                    let source = self.duplicate_direct_generator_value(source)?;
                    let delegation = match self.native_encoded_value_kind(source) {
                        Some(NativeEncodedValueKind::Array)
                            if self.direct_array_entries_for(source).is_some() =>
                        {
                            NativeGeneratorDelegation::Array { source, cursor: 0 }
                        }
                        Some(NativeEncodedValueKind::Generator)
                            if self.direct_generator_index(source).is_some() =>
                        {
                            NativeGeneratorDelegation::Generator { generator: source }
                        }
                        _ => {
                            let type_name = self.native_encoded_type_name(source);
                            self.release(source)?;
                            return Err(format!(
                                "yield from expects an array or Traversable, got {type_name}"
                            ));
                        }
                    };
                    if let Some(generator) = self.direct_generator_mut(index) {
                        generator.state = Some(state);
                        generator.lifecycle = php_runtime::api::GeneratorState::Suspended;
                        generator.delegation = Some(delegation);
                    }
                    return self.advance_direct_generator(
                        encoded,
                        php_jit::JitNativeResumeInputKind::VALUE,
                        php_jit::jit_encode_constant(u32::MAX),
                    );
                }
                let key = if state.suspend_flags & 1 != 0 {
                    Some(self.duplicate_direct_generator_value(state.yielded_key)?)
                } else {
                    None
                };
                let value = self.duplicate_direct_generator_value(value)?;
                if let Some(generator) = self.direct_generator_mut(index) {
                    generator.state = Some(state);
                }
                self.replace_direct_generator_current_owned(index, key, value, false)
                    .map(|(key, value)| NativeGeneratorAdvance::Yielded { key, value })
            }
            php_jit::JitI64InvokeOutcome::Returned(value)
            | php_jit::JitI64InvokeOutcome::SideExit {
                status: 1 | 2,
                value,
                ..
            } => {
                let (old_key, old_value, old_return, old_delegation) = {
                    let generator = self
                        .direct_generator_mut(index)
                        .ok_or_else(|| format!("direct Generator {index} is missing"))?;
                    (
                        generator.current_key.take(),
                        generator.current_value.take(),
                        generator.return_value.replace(value),
                        generator.delegation.take(),
                    )
                };
                for owner in [old_key, old_value, old_return].into_iter().flatten() {
                    self.release(owner)?;
                }
                if let Some(delegation) = old_delegation {
                    self.release(match delegation {
                        NativeGeneratorDelegation::Array { source, .. } => source,
                        NativeGeneratorDelegation::Generator { generator } => generator,
                    })?;
                }
                if let Some(generator) = self.direct_generator_mut(index) {
                    generator.state = None;
                    generator.lifecycle = php_runtime::api::GeneratorState::Closed;
                }
                Ok(NativeGeneratorAdvance::Complete)
            }
            php_jit::JitI64InvokeOutcome::SideExit {
                status,
                value,
                mut state,
            } if status == php_jit::JitCallStatus::SUSPEND_FIBER.0 as i32 => {
                // The generated activation remains authoritative. Its nested
                // compiled-call link is consumed when the Fiber execution is
                // assembled; keeping the state on the Generator also makes
                // every live owner visible to normal root traversal.
                state.control_status = php_jit::JitCallStatus::CONTINUE;
                if let Some(generator) = self.direct_generator_mut(index) {
                    generator.state = Some(state);
                    generator.lifecycle = php_runtime::api::GeneratorState::Suspended;
                }
                Ok(NativeGeneratorAdvance::FiberSuspended {
                    value,
                    active: encoded,
                    parents: Vec::new(),
                })
            }
            php_jit::JitI64InvokeOutcome::SideExit { status, value, .. }
                if status == php_jit::JitCallStatus::THROW.0 as i32 =>
            {
                if let Some(generator) = self.direct_generator_mut(index) {
                    generator.state = None;
                    generator.lifecycle = php_runtime::api::GeneratorState::Errored;
                }
                let (class, message, _) = self
                    .decode_baseline_value(value)
                    .ok()
                    .and_then(crate::vm::native_exception_fields)
                    .unwrap_or_else(|| {
                        (
                            "Error".to_owned(),
                            "unknown native Generator exception".to_owned(),
                            "<unknown>".to_owned(),
                        )
                    });
                let _ = self.release(value);
                Err(format!("E_PHP_THROW:{class}:{message}"))
            }
            php_jit::JitI64InvokeOutcome::SideExit { status, value, .. }
                if status == php_jit::JitCallStatus::EXIT.0 as i32 =>
            {
                if let Some(generator) = self.direct_generator_mut(index) {
                    generator.state = None;
                    generator.lifecycle = php_runtime::api::GeneratorState::Closed;
                }
                Err(format!("E_PHP_EXIT:{value}"))
            }
            php_jit::JitI64InvokeOutcome::SideExit { status, .. }
                if status == php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32 =>
            {
                if let Some(generator) = self.direct_generator_mut(index) {
                    generator.lifecycle = php_runtime::api::GeneratorState::Errored;
                }
                if self.diagnostic.is_some() {
                    Err(NATIVE_RUNTIME_ERROR_MARKER.to_owned())
                } else {
                    Err("native Generator returned a runtime error".to_owned())
                }
            }
            php_jit::JitI64InvokeOutcome::SideExit { status, value, .. } => {
                if let Some(generator) = self.direct_generator_mut(index) {
                    generator.lifecycle = php_runtime::api::GeneratorState::Errored;
                }
                Err(format!(
                    "native Generator returned status {status} with {}",
                    self.native_encoded_type_name(value)
                ))
            }
        }
    }

    pub(super) fn propagate_direct_generator_fiber_advance(
        &mut self,
        mut active: i64,
        mut parents: Vec<i64>,
        mut advance: NativeGeneratorAdvance,
    ) -> Result<NativeGeneratorAdvance, String> {
        loop {
            match advance {
                NativeGeneratorAdvance::Yielded { mut key, mut value } => {
                    for parent in parents {
                        (key, value) = self
                            .direct_generator_index(parent)
                            .ok_or_else(|| {
                                "delegating direct Generator disappeared during Fiber resume"
                                    .to_owned()
                            })
                            .and_then(|index| {
                                self.replace_direct_generator_current_owned(
                                    index,
                                    Some(key),
                                    value,
                                    true,
                                )
                            })?;
                    }
                    return Ok(NativeGeneratorAdvance::Yielded { key, value });
                }
                NativeGeneratorAdvance::FiberSuspended {
                    value,
                    active,
                    parents: mut nested_parents,
                } => {
                    nested_parents.extend(parents);
                    return Ok(NativeGeneratorAdvance::FiberSuspended {
                        value,
                        active,
                        parents: nested_parents,
                    });
                }
                NativeGeneratorAdvance::Complete => {
                    if parents.is_empty() {
                        return Ok(NativeGeneratorAdvance::Complete);
                    }
                    let parent = parents.remove(0);
                    let return_value = {
                        let child_index = self.direct_generator_index(active).ok_or_else(|| {
                            "completed delegated direct Generator disappeared".to_owned()
                        })?;
                        let value = self
                            .direct_generator(child_index)
                            .and_then(|generator| generator.return_value)
                            .unwrap_or_else(|| php_jit::jit_encode_constant(u32::MAX));
                        self.duplicate_direct_generator_value(value)?
                    };
                    let parent_index = self.direct_generator_index(parent).ok_or_else(|| {
                        "delegating direct Generator disappeared during Fiber resume".to_owned()
                    })?;
                    let delegation = self
                        .direct_generator_mut(parent_index)
                        .and_then(|generator| generator.delegation.take());
                    match delegation {
                        Some(NativeGeneratorDelegation::Generator { generator })
                            if generator == active =>
                        {
                            self.release(generator)?;
                        }
                        Some(other) => {
                            if let Some(generator) = self.direct_generator_mut(parent_index) {
                                generator.delegation = Some(other);
                            }
                            self.release(return_value)?;
                            return Err(
                                "direct Generator Fiber continuation lost its delegation chain"
                                    .to_owned(),
                            );
                        }
                        None => {
                            self.release(return_value)?;
                            return Err(
                                "direct Generator Fiber continuation has no delegating parent"
                                    .to_owned(),
                            );
                        }
                    }
                    active = parent;
                    advance = self.advance_direct_generator(
                        parent,
                        php_jit::JitNativeResumeInputKind::VALUE,
                        return_value,
                    )?;
                }
            }
        }
    }
}
