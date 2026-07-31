//! Explicit compatibility for already-materialized Fiber values.
//!
//! Direct Fibers remain authoritative native records. Only Fiber objects
//! that crossed a cold Value boundary enter this module.

use super::*;
use php_runtime::api::Value;

impl<'a> NativeRequestColdState<'a> {
    pub(super) fn native_fiber_receiver(
        &mut self,
        encoded: i64,
    ) -> Result<Option<NativeFiberReceiver>, String> {
        let encoded = self.dereference_direct_encoding(encoded);
        if self.direct_fiber_index(encoded).is_some() {
            return Ok(Some(NativeFiberReceiver::Direct(encoded)));
        }
        if self.native_encoded_value_kind(encoded) != Some(NativeEncodedValueKind::Fiber) {
            return Ok(None);
        }
        match self.decode_baseline_value(encoded)? {
            Value::Fiber(fiber) => Ok(Some(NativeFiberReceiver::Materialized(fiber))),
            _ => Ok(None),
        }
    }

    pub(super) fn fiber_receiver_id(&self, fiber: &NativeFiberReceiver) -> Result<u64, String> {
        match fiber {
            NativeFiberReceiver::Direct(encoded) => Self::direct_value_index(*encoded)
                .map(|index| index as u64)
                .ok_or_else(|| "native Fiber identity is missing".to_owned()),
            NativeFiberReceiver::Materialized(fiber) => Ok(self
                .baseline_values
                .direct_fiber_handles
                .get(&fiber.id())
                .map_or_else(|| fiber.id(), |index| u64::from(*index))),
        }
    }

    pub(super) fn fiber_receiver_state(
        &self,
        fiber: &NativeFiberReceiver,
    ) -> Result<php_runtime::api::FiberState, String> {
        match fiber {
            NativeFiberReceiver::Direct(encoded) => self
                .native_fiber_state(*encoded)
                .ok_or_else(|| "native Fiber state is missing".to_owned()),
            NativeFiberReceiver::Materialized(fiber) => Ok(fiber.state()),
        }
    }

    pub(super) fn set_fiber_receiver_state(
        &mut self,
        fiber: &NativeFiberReceiver,
        state: php_runtime::api::FiberState,
    ) -> Result<(), String> {
        match fiber {
            NativeFiberReceiver::Direct(encoded) => self.set_native_fiber_state(*encoded, state),
            NativeFiberReceiver::Materialized(fiber) => {
                fiber.set_state(state);
                Ok(())
            }
        }
    }

    pub(super) fn fiber_receiver_callable(
        &mut self,
        fiber: &NativeFiberReceiver,
    ) -> Result<i64, String> {
        match fiber {
            NativeFiberReceiver::Direct(encoded) => self
                .native_fiber_callable(*encoded)
                .ok_or_else(|| "native Fiber callable is missing".to_owned()),
            NativeFiberReceiver::Materialized(fiber) => {
                self.encode_baseline_value(fiber.callable())
            }
        }
    }

    pub(super) fn fiber_receiver_return_value(
        &mut self,
        fiber: &NativeFiberReceiver,
    ) -> Result<Option<i64>, String> {
        match fiber {
            NativeFiberReceiver::Direct(encoded) => {
                let value = self
                    .native_fiber_return_value(*encoded)
                    .ok_or_else(|| "native Fiber return slot is missing".to_owned())?;
                value
                    .map(|value| {
                        self.duplicate_authoritative_native_value(value)?
                            .ok_or_else(|| {
                                "direct Fiber return value is not authoritative native data"
                                    .to_owned()
                            })
                    })
                    .transpose()
            }
            NativeFiberReceiver::Materialized(fiber) => fiber
                .return_value()
                .map(|value| self.encode_baseline_value(value))
                .transpose(),
        }
    }

    pub(super) fn terminate_fiber_receiver(
        &mut self,
        fiber: &NativeFiberReceiver,
        return_value: Option<i64>,
    ) -> Result<(), String> {
        match fiber {
            NativeFiberReceiver::Direct(encoded) => {
                self.terminate_native_fiber(*encoded, return_value)
            }
            NativeFiberReceiver::Materialized(fiber) => {
                let return_value = return_value
                    .map(|value| self.decode_baseline_value(value))
                    .transpose()?;
                fiber.terminate(return_value);
                Ok(())
            }
        }
    }
    fn publish_native_fiber(
        &mut self,
        callable: i64,
        state: php_runtime::api::FiberState,
        return_value: Option<i64>,
        materialized: Option<php_runtime::api::FiberRef>,
    ) -> Result<i64, String> {
        let index = match self.reserve_direct_value_slot() {
            Ok(index) => index,
            Err(error) => {
                let _ = self.release(callable);
                if let Some(return_value) = return_value {
                    let _ = self.release(return_value);
                }
                return Err(error);
            }
        };
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .expect("direct Fiber index is bounded by the native value arena");
        let identity = materialized.as_ref().map(php_runtime::api::FiberRef::id);
        let owner = Box::into_raw(Box::new(NativeDirectFiber {
            state,
            callable,
            return_value,
        }));
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: if materialized.is_some() {
                php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_FIBER
            } else {
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER
            },
            flags: php_jit::JIT_NATIVE_DIRECT_FIBER_ABI_VERSION,
            payload: identity.unwrap_or(0),
            aux: owner as usize as u64,
            ..php_jit::JitNativeValueSlot::default()
        };
        if let Some(fiber) = materialized {
            self.baseline_values
                .direct_fiber_handles
                .insert(fiber.id(), index as u32);
            self.baseline_values.direct_fiber_cells.insert(index, fiber);
        }
        Ok(php_jit::jit_encode_typed_runtime_value(
            runtime_index,
            php_jit::JIT_VALUE_RUNTIME_FIBER_TAG,
        ))
    }

    pub(super) fn encode_native_fiber(&mut self, callable: i64) -> Result<i64, String> {
        self.retain(callable)?;
        self.publish_native_fiber(
            callable,
            php_runtime::api::FiberState::NotStarted,
            None,
            None,
        )
    }

    pub(super) fn encode_native_fiber_owner(
        &mut self,
        fiber: php_runtime::api::FiberRef,
    ) -> Result<i64, String> {
        if let Some(index) = self
            .baseline_values
            .direct_fiber_handles
            .get(&fiber.id())
            .copied()
        {
            let slot = self
                .direct_value_slots
                .get_mut(index as usize)
                .filter(|slot| {
                    slot.refcount != 0
                        && matches!(
                            slot.kind,
                            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER
                                | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_FIBER
                        )
                })
                .ok_or_else(|| "native Fiber identity points at a dead slot".to_owned())?;
            slot.refcount = slot
                .refcount
                .checked_add(1)
                .ok_or_else(|| "native Fiber refcount overflow".to_owned())?;
            let runtime_index = index
                .checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
                .ok_or_else(|| "native Fiber handle overflow".to_owned())?;
            return Ok(php_jit::jit_encode_typed_runtime_value(
                runtime_index,
                php_jit::JIT_VALUE_RUNTIME_FIBER_TAG,
            ));
        }
        let callable = self.encode_baseline_value(fiber.callable())?;
        let return_value = match fiber
            .return_value()
            .map(|value| self.encode_baseline_value(value))
        {
            Some(Ok(value)) => Some(value),
            Some(Err(error)) => {
                let _ = self.release(callable);
                return Err(error);
            }
            None => None,
        };
        self.publish_native_fiber(callable, fiber.state(), return_value, Some(fiber))
    }
}
