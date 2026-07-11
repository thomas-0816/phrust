#[cfg(feature = "jit-cranelift")]
use crate::deopt::GuardKind;
#[cfg(any(
    feature = "jit-cranelift",
    all(feature = "jit-copy-patch", unix, target_arch = "aarch64")
))]
use php_ir::module::normalize_class_name;
#[cfg(feature = "jit-cranelift")]
use php_runtime::PhpString;
#[cfg(any(
    feature = "jit-cranelift",
    all(feature = "jit-copy-patch", unix, target_arch = "aarch64")
))]
use php_runtime::Value;

// The property-access status codes are shared by both native tiers'
// property-load helpers (Cranelift and copy-and-patch) and reused by the
// copy-and-patch store commit core (`jit_property_store_commit`), so the three
// the shared cores return are available whenever either tier is compiled.
// LAYOUT_EXIT is only produced/attributed on the Cranelift path, so it stays
// gated there.
#[cfg(any(
    feature = "jit-cranelift",
    all(feature = "jit-copy-patch", unix, target_arch = "aarch64")
))]
pub(super) const JIT_PROPERTY_LOAD_STATUS_CLASS_EXIT: i32 = 21;
#[cfg(feature = "jit-cranelift")]
pub(super) const JIT_PROPERTY_LOAD_STATUS_LAYOUT_EXIT: i32 = 22;
#[cfg(any(
    feature = "jit-cranelift",
    all(feature = "jit-copy-patch", unix, target_arch = "aarch64")
))]
pub(super) const JIT_PROPERTY_LOAD_STATUS_UNINITIALIZED_EXIT: i32 = 23;
#[cfg(any(
    feature = "jit-cranelift",
    all(feature = "jit-copy-patch", unix, target_arch = "aarch64")
))]
pub(super) const JIT_PROPERTY_LOAD_STATUS_STORAGE_EXIT: i32 = 24;

#[cfg(feature = "jit-cranelift")]
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

#[cfg(feature = "jit-cranelift")]
pub(super) extern "C" fn jit_array_len_abi(value_ptr: usize, out: *mut i64) -> i32 {
    if value_ptr == 0 || out.is_null() {
        return php_runtime::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT;
    }
    // SAFETY: Cranelift passes a pointer to a `PreparedArg.value` owned by the
    // active VM call frame and invokes this helper synchronously.
    let value = unsafe { &*(value_ptr as *const Value) };
    let mut length = 0_usize;
    let status = php_runtime::php_jit_array_len(value, &mut length);
    if status != php_runtime::PHP_JIT_ARRAY_STATUS_OK {
        return php_runtime::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT;
    }
    let Ok(length) = i64::try_from(length) else {
        return php_runtime::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT;
    };
    // SAFETY: The pointer was checked for null and points to the native caller's
    // stack-owned output slot for the duration of this synchronous helper call.
    unsafe {
        *out = length;
    }
    php_runtime::PHP_JIT_ARRAY_STATUS_OK
}

#[cfg(feature = "jit-cranelift")]
pub(super) extern "C" fn jit_array_fetch_int_slow_abi(
    value_ptr: usize,
    index: i64,
    out: *mut i64,
) -> i32 {
    if value_ptr == 0 || out.is_null() {
        return php_runtime::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT;
    }
    let Ok(index) = usize::try_from(index) else {
        return php_runtime::PHP_JIT_ARRAY_STATUS_BOUNDS_EXIT;
    };
    // SAFETY: Cranelift passes a pointer to a `PreparedArg.value` owned by the
    // active VM call frame and invokes this helper synchronously.
    let value = unsafe { &*(value_ptr as *const Value) };
    // SAFETY: Native handles allocate the out slot on the Rust stack and pass a
    // non-null pointer for the duration of this call.
    let out = unsafe { &mut *out };
    php_runtime::php_jit_array_fetch_int_slow(value, index, out)
}

#[cfg(feature = "jit-cranelift")]
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

#[cfg(feature = "jit-cranelift")]
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

#[cfg(feature = "jit-cranelift")]
pub(super) extern "C" fn jit_record_array_lookup_abi(
    array_ptr: usize,
    key_ptr: usize,
    out: *mut usize,
) -> i32 {
    if array_ptr == 0 || key_ptr == 0 || out.is_null() {
        return php_runtime::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT;
    }
    // SAFETY: Cranelift passes pointers to `PreparedArg.value` slots owned by
    // the active VM call frame and invokes this helper synchronously.
    let array = unsafe { &*(array_ptr as *const Value) };
    // SAFETY: Same as above for the key operand.
    let key = unsafe { &*(key_ptr as *const Value) };
    match php_runtime::php_jit_record_array_lookup(array, key) {
        Ok(value) => {
            let result = Box::new(value);
            // SAFETY: The out pointer was checked for null and points to the
            // native caller's stack-owned output slot. The VM reclaims the
            // boxed value immediately after the native call returns.
            unsafe {
                *out = Box::into_raw(result) as usize;
            }
            php_runtime::PHP_JIT_ARRAY_STATUS_OK
        }
        Err(status) => status,
    }
}

#[cfg(feature = "jit-cranelift")]
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

/// Shared monomorphic property-load fetch core reused by both native tiers'
/// property-load helpers: the Cranelift [`jit_property_load_monomorphic_fast`]
/// and the copy-and-patch `copy_patch_property_load_abi` (in
/// `crate::copy_patch_bridge`). It performs the *layout guard* — the value must
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
#[cfg(any(
    feature = "jit-cranelift",
    all(feature = "jit-copy-patch", unix, target_arch = "aarch64")
))]
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

/// Monomorphic property-*store* commit core used by the copy-and-patch
/// property-store helper (`copy_patch_property_store_abi` in
/// `crate::copy_patch_bridge`). The write-side mirror of
/// [`jit_property_load_fetch`]: it performs the same layout guard — the value
/// must be an object whose runtime class equals the metadata's expected
/// receiver class, so the recognition-time facts (declared, untyped,
/// non-readonly, hook-free, symmetric-visibility public slot) provably hold for
/// the instance being written — and then commits exactly one name-keyed write
/// to the declared property's storage.
///
/// The store only proceeds when the slot currently holds a plain initialized
/// value: an absent slot (`unset()` re-arms `__set`), a reference-holding slot
/// (the write must go through the cell so aliases observe it), an uninitialized
/// marker, or a concurrently borrowed storage all side-exit *before any write*,
/// so the interpreter performs the exact store with full semantics. On `Err`
/// nothing was written. It never invokes a hook/`__set`, frees, or re-enters
/// the VM; the single mutation goes through the runtime's own
/// interior-mutability layer (`try_set_property`), the same storage cell the
/// interpreter writes.
#[cfg(all(feature = "jit-copy-patch", unix, target_arch = "aarch64"))]
pub(crate) fn jit_property_store_commit(
    value: &Value,
    metadata: &php_jit::JitPropertyStoreMetadata,
    new_value: Value,
) -> Result<(), i32> {
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
    let Ok(Some(current)) = object.try_get_property(&metadata.storage_name) else {
        return Err(JIT_PROPERTY_LOAD_STATUS_STORAGE_EXIT);
    };
    if matches!(current, Value::Reference(_)) || current.is_uninitialized() {
        return Err(JIT_PROPERTY_LOAD_STATUS_STORAGE_EXIT);
    }
    if object
        .try_set_property(metadata.storage_name.clone(), new_value)
        .is_err()
    {
        return Err(JIT_PROPERTY_LOAD_STATUS_STORAGE_EXIT);
    }
    Ok(())
}

#[cfg(feature = "jit-cranelift")]
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
