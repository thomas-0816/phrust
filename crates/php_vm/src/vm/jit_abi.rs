// Audited native-tier ABI surface (docs/performance/cranelift/
// safety-audit.md); compiled only under the JIT features, which the
// pre-ADR-0020 CLI gate never covered.
use crate::deopt::GuardKind;
use php_ir::module::normalize_class_name;
use php_runtime::api::PhpString;
use php_runtime::api::Value;

// Stable property-access side-exit statuses used by native helpers.
pub(super) const JIT_PROPERTY_LOAD_STATUS_CLASS_EXIT: i32 = 21;
pub(super) const JIT_PROPERTY_LOAD_STATUS_LAYOUT_EXIT: i32 = 22;
pub(super) const JIT_PROPERTY_LOAD_STATUS_UNINITIALIZED_EXIT: i32 = 23;
pub(super) const JIT_PROPERTY_LOAD_STATUS_STORAGE_EXIT: i32 = 24;

static JIT_RUNTIME_HELPER_TABLE: php_jit::JitRuntimeHelperTable =
    php_jit::JitRuntimeHelperTable::new(php_jit::jit_default_helper_dispatch);

pub(super) fn jit_runtime_helper_table() -> &'static php_jit::JitRuntimeHelperTable {
    &JIT_RUNTIME_HELPER_TABLE
}

/// Typed native call trampoline entry. Target compilation/lookup is requested
/// explicitly; this boundary never invokes dense, rich, or IR dispatch.
#[allow(unsafe_code)]
pub(super) extern "C" fn jit_native_call_dispatch_abi(
    _vm_context: u64,
    frame: *mut php_jit::JitNativeCallFrame,
    out: *mut php_jit::JitCallResult,
) -> i32 {
    if frame.is_null() || out.is_null() {
        return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: The generated caller owns both records for this synchronous
        // call and the pointers were checked for null above.
        let frame = unsafe { &*frame };
        if frame.abi_version != php_jit::JIT_RUNTIME_ABI_VERSION
            || frame.struct_size as usize != std::mem::size_of::<php_jit::JitNativeCallFrame>()
            || (frame.argument_count != 0 && frame.arguments == 0)
        {
            return (
                php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
                php_jit::JitCallStatus::ABI_MISMATCH,
            );
        }
        (
            php_jit::JitCallStatus::COMPILE_REQUIRED.0 as i32,
            php_jit::JitCallStatus::COMPILE_REQUIRED,
        )
    }));
    let (status, call_status) = result.unwrap_or((
        php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        php_jit::JitCallStatus::RUNTIME_ERROR,
    ));
    // SAFETY: `out` is a checked, caller-owned result record.
    unsafe {
        out.write(php_jit::JitCallResult {
            status: call_status,
            detail: status as u32,
            value: php_jit::JitAbiSlot::default(),
        });
    }
    status
}

/// Native dynamic-code compiler boundary. A production VM context installs
/// the include/eval compiler and compile-once cache. Until that context is
/// present this returns `COMPILE_REQUIRED`; it never executes the source via a
/// bytecode, rich, or IR fallback.
#[allow(unsafe_code)]
pub(super) extern "C" fn jit_native_dynamic_code_abi(
    vm_context: u64,
    request: *mut php_jit::JitNativeDynamicCodeRequest,
    out: *mut php_jit::JitCallResult,
) -> i32 {
    if request.is_null() || out.is_null() {
        return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
    }
    let status = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: Generated code owns this request for the synchronous call.
        let request = unsafe { &*request };
        if request.abi_version != php_jit::JIT_RUNTIME_ABI_VERSION
            || request.struct_size as usize
                != std::mem::size_of::<php_jit::JitNativeDynamicCodeRequest>()
        {
            php_jit::JitCallStatus::ABI_MISMATCH
        } else if vm_context == 0 {
            php_jit::JitCallStatus::COMPILE_REQUIRED
        } else {
            // The VM context is deliberately opaque at this ABI boundary. Its
            // owning executor installs the native compiler callback; treating
            // an unknown handle as executable would violate handle safety.
            php_jit::JitCallStatus::RUNTIME_ERROR
        }
    }))
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR);
    // SAFETY: `out` is a checked caller-owned result record.
    unsafe {
        out.write(php_jit::JitCallResult {
            status,
            detail: status.0,
            value: php_jit::JitAbiSlot::default(),
        });
    }
    status.0 as i32
}

pub(super) fn jit_guard_kind_for_side_exit(reason: php_jit::SideExitReason) -> Option<GuardKind> {
    match reason {
        php_jit::SideExitReason::TypeMismatch => Some(GuardKind::QuickeningType),
        php_jit::SideExitReason::Overflow => Some(GuardKind::IntAdd),
        php_jit::SideExitReason::UnsupportedValue => Some(GuardKind::RegionAssumption),
        php_jit::SideExitReason::GuardFailed => Some(GuardKind::RegionAssumption),
        php_jit::SideExitReason::HelperStatus => Some(GuardKind::BuiltinCall),
        php_jit::SideExitReason::ExceptionPending | php_jit::SideExitReason::AbiMismatch => None,
    }
}

#[allow(unsafe_code)]
pub(super) extern "C" fn jit_array_len_abi(value_ptr: usize, out: *mut i64) -> i32 {
    if value_ptr == 0 || out.is_null() {
        return php_runtime::experimental::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT;
    }
    // SAFETY: Cranelift passes a pointer to a `PreparedArg.value` owned by the
    // active VM call frame and invokes this helper synchronously.
    let value = unsafe { &*(value_ptr as *const Value) };
    let mut length = 0_usize;
    let status = php_runtime::experimental::php_jit_array_len(value, &mut length);
    if status != php_runtime::experimental::PHP_JIT_ARRAY_STATUS_OK {
        return php_runtime::experimental::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT;
    }
    let Ok(length) = i64::try_from(length) else {
        return php_runtime::experimental::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT;
    };
    // SAFETY: The pointer was checked for null and points to the native caller's
    // stack-owned output slot for the duration of this synchronous helper call.
    unsafe {
        *out = length;
    }
    php_runtime::experimental::PHP_JIT_ARRAY_STATUS_OK
}

#[allow(unsafe_code)]
pub(super) extern "C" fn jit_array_fetch_int_slow_abi(
    value_ptr: usize,
    index: i64,
    out: *mut i64,
) -> i32 {
    if value_ptr == 0 || out.is_null() {
        return php_runtime::experimental::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT;
    }
    let Ok(index) = usize::try_from(index) else {
        return php_runtime::experimental::PHP_JIT_ARRAY_STATUS_BOUNDS_EXIT;
    };
    // SAFETY: Cranelift passes a pointer to a `PreparedArg.value` owned by the
    // active VM call frame and invokes this helper synchronously.
    let value = unsafe { &*(value_ptr as *const Value) };
    // SAFETY: Native handles allocate the out slot on the Rust stack and pass a
    // non-null pointer for the duration of this call.
    let out = unsafe { &mut *out };
    php_runtime::experimental::php_jit_array_fetch_int_slow(value, index, out)
}

#[allow(unsafe_code)]
pub(super) extern "C" fn jit_strlen_known_abi(value_ptr: usize, out: *mut i64) -> i32 {
    if value_ptr == 0 || out.is_null() {
        return php_jit::JIT_HELPER_STATUS_FALLBACK;
    }
    // SAFETY: Cranelift passes a pointer to a `PreparedArg.value` owned by the
    // active VM call frame and invokes this helper synchronously.
    let value = unsafe { &*(value_ptr as *const Value) };
    let effective = match value {
        Value::Reference(cell) => cell.get(),
        other => other.clone(),
    };
    let Value::String(string) = effective else {
        return php_jit::JIT_HELPER_STATUS_FALLBACK;
    };
    let Ok(length) = i64::try_from(string.len()) else {
        return php_jit::JIT_HELPER_STATUS_FALLBACK;
    };
    // SAFETY: The pointer was checked for null and points to the native caller's
    // stack-owned output slot for the duration of this synchronous helper call.
    unsafe {
        *out = length;
    }
    php_jit::JIT_HELPER_STATUS_OK
}

#[allow(unsafe_code)]
pub(super) extern "C" fn jit_count_known_abi(value_ptr: usize, out: *mut i64) -> i32 {
    if value_ptr == 0 || out.is_null() {
        return php_jit::JIT_HELPER_STATUS_FALLBACK;
    }
    // SAFETY: Cranelift passes a pointer to a `PreparedArg.value` owned by the
    // active VM call frame and invokes this helper synchronously.
    let value = unsafe { &*(value_ptr as *const Value) };
    let effective = match value {
        Value::Reference(cell) => cell.get(),
        other => other.clone(),
    };
    let Value::Array(array) = effective else {
        return php_jit::JIT_HELPER_STATUS_FALLBACK;
    };
    let Ok(length) = i64::try_from(array.len()) else {
        return php_jit::JIT_HELPER_STATUS_FALLBACK;
    };
    // SAFETY: The pointer was checked for null and points to the native caller's
    // stack-owned output slot for the duration of this synchronous helper call.
    unsafe {
        *out = length;
    }
    php_jit::JIT_HELPER_STATUS_OK
}

#[allow(unsafe_code)]
pub(super) extern "C" fn jit_record_array_lookup_abi(
    array_ptr: usize,
    key_ptr: usize,
    out: *mut usize,
) -> i32 {
    if array_ptr == 0 || key_ptr == 0 || out.is_null() {
        return php_runtime::experimental::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT;
    }
    // SAFETY: Cranelift passes pointers to `PreparedArg.value` slots owned by
    // the active VM call frame and invokes this helper synchronously.
    let array = unsafe { &*(array_ptr as *const Value) };
    // SAFETY: Same as above for the key operand.
    let key = unsafe { &*(key_ptr as *const Value) };
    match php_runtime::experimental::php_jit_record_array_lookup(array, key) {
        Ok(value) => {
            let result = Box::new(value);
            // SAFETY: The out pointer was checked for null and points to the
            // native caller's stack-owned output slot. The VM reclaims the
            // boxed value immediately after the native call returns.
            unsafe {
                *out = Box::into_raw(result) as usize;
            }
            php_runtime::experimental::PHP_JIT_ARRAY_STATUS_OK
        }
        Err(status) => status,
    }
}

#[allow(unsafe_code)]
pub(super) extern "C" fn jit_concat_string_string_fast(
    lhs_ptr: usize,
    rhs_ptr: usize,
    out: *mut usize,
) -> i32 {
    if lhs_ptr == 0 || rhs_ptr == 0 || out.is_null() {
        return php_jit::JIT_HELPER_STATUS_FALLBACK;
    }
    // SAFETY: Cranelift passes pointers to `PreparedArg.value` slots owned by
    // the active VM call frame and invokes this helper synchronously.
    let lhs = unsafe { &*(lhs_ptr as *const Value) };
    // SAFETY: Same as above for the right operand.
    let rhs = unsafe { &*(rhs_ptr as *const Value) };
    let lhs = match lhs {
        Value::Reference(cell) => cell.get(),
        other => other.clone(),
    };
    let rhs = match rhs {
        Value::Reference(cell) => cell.get(),
        other => other.clone(),
    };
    let (Value::String(lhs), Value::String(rhs)) = (lhs, rhs) else {
        return php_jit::JIT_HELPER_STATUS_FALLBACK;
    };
    let Some(capacity) = lhs.len().checked_add(rhs.len()) else {
        return php_jit::JIT_HELPER_STATUS_OVERFLOW;
    };
    let mut bytes = Vec::new();
    if bytes.try_reserve_exact(capacity).is_err() {
        return php_jit::JIT_HELPER_STATUS_FALLBACK;
    }
    bytes.extend_from_slice(lhs.as_bytes());
    bytes.extend_from_slice(rhs.as_bytes());
    let result = Box::new(Value::String(PhpString::from_bytes(bytes)));
    // SAFETY: The out pointer was checked for null and points to the native
    // caller's stack-owned output slot. The VM reclaims the boxed value
    // immediately after the native call returns successfully.
    unsafe {
        *out = Box::into_raw(result) as usize;
    }
    php_jit::JIT_HELPER_STATUS_OK
}

/// Monomorphic property-load fetch core used by
/// [`jit_property_load_monomorphic_fast`]. It performs the *layout guard*: the value must
/// be an object whose runtime class equals the metadata's expected receiver
/// class, so a polymorphic/subclass instance reaching the same site is rejected
/// rather than read at a wrong slot — and reads the declared property by its
/// runtime storage name.
///
/// Returns `Ok(value)` with the property's initialized value, or `Err(status)`
/// with the specific side-exit status: a non-object or class mismatch is
/// [`JIT_PROPERTY_LOAD_STATUS_CLASS_EXIT`], an absent property is
/// [`JIT_PROPERTY_LOAD_STATUS_STORAGE_EXIT`], and an uninitialized typed property
/// is [`JIT_PROPERTY_LOAD_STATUS_UNINITIALIZED_EXIT`] (the interpreter then throws
/// the exact `Error`). It only reads a declared property slot — it never mutates,
/// invokes a hook/`__get` (those shapes are excluded at recognition time), or
/// re-enters the VM.
pub(crate) fn jit_property_load_fetch(
    value: &Value,
    metadata: &php_jit::JitPropertyLoadMetadata,
) -> Result<Value, i32> {
    let effective = match value {
        Value::Reference(cell) => cell.get(),
        other => other.clone(),
    };
    let Value::Object(object) = effective else {
        return Err(JIT_PROPERTY_LOAD_STATUS_CLASS_EXIT);
    };
    if normalize_class_name(&object.class_name()) != metadata.receiver_class {
        return Err(JIT_PROPERTY_LOAD_STATUS_CLASS_EXIT);
    }
    let Some(value) = object.get_property(&metadata.storage_name) else {
        return Err(JIT_PROPERTY_LOAD_STATUS_STORAGE_EXIT);
    };
    if value.is_uninitialized() {
        return Err(JIT_PROPERTY_LOAD_STATUS_UNINITIALIZED_EXIT);
    }
    Ok(value)
}

#[allow(unsafe_code)]
pub(super) extern "C" fn jit_property_load_monomorphic_fast(
    value_ptr: usize,
    metadata_ptr: usize,
    out: *mut usize,
) -> i32 {
    if value_ptr == 0 || metadata_ptr == 0 || out.is_null() {
        return php_jit::JIT_HELPER_STATUS_FALLBACK;
    }
    // SAFETY: Cranelift passes a pointer to a `PreparedArg.value` owned by the
    // active VM call frame and invokes this helper synchronously.
    let value = unsafe { &*(value_ptr as *const Value) };
    // SAFETY: The VM passes a pointer to the handle-owned metadata for the
    // duration of this synchronous native invocation.
    let metadata = unsafe { &*(metadata_ptr as *const php_jit::JitPropertyLoadMetadata) };
    match jit_property_load_fetch(value, metadata) {
        Ok(value) => {
            let result = Box::new(value);
            // SAFETY: The out pointer was checked for null and points to the
            // native caller's stack-owned output slot. The VM reclaims the boxed
            // value immediately after the native call returns successfully.
            unsafe {
                *out = Box::into_raw(result) as usize;
            }
            php_jit::JIT_HELPER_STATUS_OK
        }
        Err(status) => status,
    }
}

#[cfg(test)]
mod tests {
    use super::{jit_native_call_dispatch_abi, jit_native_dynamic_code_abi};

    #[test]
    fn native_call_trampoline_requests_compile_without_interpreter_reentry() {
        let mut frame = php_jit::JitNativeCallFrame::default();
        frame.function_id = 3;
        frame.continuation_id = 7;
        let mut out = php_jit::JitCallResult::default();
        assert_eq!(
            jit_native_call_dispatch_abi(0, &mut frame, &mut out),
            php_jit::JitCallStatus::COMPILE_REQUIRED.0 as i32
        );
        assert_eq!(out.status, php_jit::JitCallStatus::COMPILE_REQUIRED);

        frame.abi_version = frame.abi_version.saturating_add(1);
        assert_eq!(
            jit_native_call_dispatch_abi(0, &mut frame, &mut out),
            php_jit::JitCallStatus::ABI_MISMATCH.0 as i32
        );
        assert_eq!(out.status, php_jit::JitCallStatus::ABI_MISMATCH);
    }

    #[test]
    fn native_dynamic_code_boundary_never_uses_first_execution_fallback() {
        let mut request = php_jit::JitNativeDynamicCodeRequest {
            kind: php_jit::JitNativeDynamicCodeKind::EVAL,
            ..php_jit::JitNativeDynamicCodeRequest::default()
        };
        let mut out = php_jit::JitCallResult::default();
        assert_eq!(
            jit_native_dynamic_code_abi(0, &mut request, &mut out),
            php_jit::JitCallStatus::COMPILE_REQUIRED.0 as i32
        );
        assert_eq!(out.status, php_jit::JitCallStatus::COMPILE_REQUIRED);
    }
}
