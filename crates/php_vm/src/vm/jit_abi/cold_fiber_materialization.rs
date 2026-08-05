//! Explicit compatibility for already-materialized Fiber values.
//!
//! Direct Fibers remain authoritative native records. Only Fiber objects
//! that crossed a cold Value boundary enter this module.

use super::*;
impl<'a> NativeRequestColdState<'a> {
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
