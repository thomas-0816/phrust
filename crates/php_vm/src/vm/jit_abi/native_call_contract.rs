//! Native function-boundary type and lifecycle contracts.
//!
//! These exact boundary services operate only on the authoritative native
//! encoding. They neither select an operation nor resolve or invoke PHP.

use super::*;

pub(in crate::vm) extern "C" fn jit_native_execution_poll_abi(
    runtime: *mut NativeRequestFastState,
) -> i32 {
    // SAFETY: publication supplies a stable request-owned fast-state pointer.
    #[allow(unsafe_code)] // Safety: the immutable contract table remains process-owned.
    let Some(fast) = (unsafe { runtime.as_mut() }) else {
        return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
    };
    let status = fast.execution_deadline.poll();
    if status != php_jit::JitCallStatus::CONTINUE.0 as i32 {
        return status;
    }
    // A loop that stays inside one call never crosses a request boundary, so
    // request-completion selection alone can never promote it. This safepoint
    // is the loop backedge evidence the tiering options always described.
    //
    // SAFETY: the poll runs synchronously inside one native activation. Hot
    // selection is opportunistic, so an activation published without a cold
    // coordinator simply skips it.
    #[allow(unsafe_code)]
    if let Some(context) = unsafe { try_active_baseline_cold_context() } {
        super::cold_dynamic_units::schedule_hot_native_functions_on_backedge(context);
    }
    status
}

// SAFETY: generated callers supply a live stack-owned output slot.
#[allow(unsafe_code)]
#[inline(always)]
fn write_native_value(out: *mut i64, value: i64) -> bool {
    unsafe { out.write(value) };
    true
}

pub(in crate::vm) extern "C" fn jit_native_declared_return_contract_abi(
    runtime: *mut NativeRequestFastState,
    encoded: i64,
    contract: u64,
    out: *mut i64,
) -> i32 {
    if contract == 0 {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    // SAFETY: generated code loads this pointer from the active compiled
    // unit's immutable process-owned contract table.
    #[allow(unsafe_code)] // Safety: the immutable contract table remains process-owned.
    let contract = unsafe {
        &*(contract as usize as *const crate::compiled_unit::PreparedNativeReturnContract)
    };
    with_baseline_native_context_for(runtime, "declared_return_contract", |context| {
        if context.native_encoded_exactly_matches_ir_type(encoded, &contract.type_) == Some(true) {
            return if write_native_value(out, encoded) {
                0
            } else {
                php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
            };
        }

        let strict_types = context.unit.strict_types;
        match context.coerce_native_call_argument_encoded(encoded, &contract.type_, strict_types) {
            Ok(Some(checked))
                if context.native_encoded_exactly_matches_ir_type(checked, &contract.type_)
                    == Some(true) =>
            {
                if contract.returns_by_ref {
                    match context.replace_direct_reference_payload_owned(encoded, checked) {
                        Ok(true) => {
                            return if write_native_value(out, encoded) {
                                0
                            } else {
                                php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                            };
                        }
                        Ok(false) => {
                            if context.release(checked).is_err() {
                                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                            }
                        }
                        Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                    }
                } else {
                    return if write_native_value(out, checked) {
                        0
                    } else {
                        let _ = context.release(checked);
                        php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                    };
                }
            }
            Ok(Some(checked)) => {
                if context.release(checked).is_err() {
                    return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                }
            }
            Ok(None) => {}
            Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }

        let message = format!(
            "{}(): Return value must be of type {}, {} returned",
            contract.function_name,
            native_ir_type_name(&contract.type_),
            context.native_encoded_type_name(encoded)
        );
        let throwable = encode_native_throwable_at(context, "TypeError", &message, contract.span);
        match throwable {
            Ok(encoded) if write_native_value(out, encoded) => {
                php_jit::JitCallStatus::THROW.0 as i32
            }
            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::vm) extern "C" fn jit_native_declared_argument_contract_abi(
    runtime: *mut NativeRequestFastState,
    encoded: i64,
    contract: u64,
    strict: i64,
    out: *mut i64,
) -> i32 {
    if contract == 0 {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    // SAFETY: generated code loads this pointer from the active compiled
    // unit's immutable process-owned contract table.
    #[allow(unsafe_code)] // Safety: the immutable contract table remains process-owned.
    let contract = unsafe {
        &*(contract as usize as *const crate::compiled_unit::PreparedNativeParameterContract)
    };
    with_baseline_native_context_for(runtime, "declared_argument_contract", |context| {
        let strict = strict != 0;
        let reference_index = contract
            .by_ref
            .then(|| NativeRequestColdState::direct_value_index(encoded))
            .flatten();
        if contract.by_ref {
            let Some(index) = reference_index else {
                return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
            };
            let Some(slot) = context.direct_value_slots.get_mut(index) else {
                return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
            };
            if slot.refcount == 0
                || slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                || slot.flags != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            {
                return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
            }
            if native_reference_state(slot.reserved)
                == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
            {
                slot.payload = php_jit::jit_encode_constant(u32::MAX) as u64;
                slot.reserved = php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED
                    | (slot.reserved & php_jit::JIT_NATIVE_REFERENCE_TYPED_PROPERTY_GUARD);
            }
        }
        let checked =
            match context.coerce_native_call_argument_encoded(encoded, &contract.type_, strict) {
                Ok(Some(checked)) => checked,
                Ok(None) | Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        if context.native_encoded_matches_ir_type(checked, &contract.type_) != Some(true) {
            let message = format!(
                "{}(): Argument #{} (${}) must be of type {}, {} given",
                contract.function_name,
                contract.position + 1,
                contract.parameter_name,
                native_ir_type_name(&contract.type_),
                context.native_encoded_type_name(checked)
            );
            if context.release(checked).is_err() {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
            let throwable =
                encode_native_throwable_at(context, "TypeError", &message, contract.span);
            return match throwable {
                Ok(encoded) if write_native_value(out, encoded) => {
                    php_jit::JitCallStatus::THROW.0 as i32
                }
                _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        let checked = if reference_index.is_some() {
            match context.replace_direct_reference_payload_owned(encoded, checked) {
                Ok(true) => encoded,
                Ok(false) => {
                    let _ = context.release(checked);
                    return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
                }
                Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            }
        } else {
            checked
        };
        if write_native_value(out, checked) {
            0
        } else {
            php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}
