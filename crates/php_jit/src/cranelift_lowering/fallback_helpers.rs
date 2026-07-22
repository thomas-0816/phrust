// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(super) extern "C" fn test_native_unary_fallback(
    _runtime: *mut std::ffi::c_void,
    op: u32,
    src: i64,
    out: *mut i64,
) -> i32 {
    if out.is_null() {
        return crate::JitCallStatus::RUNTIME_ERROR.0 as i32;
    }
    let value = match op {
        0 => src,
        1 => match src.checked_neg() {
            Some(value) => value,
            None => return crate::JitCallStatus::RUNTIME_ERROR.0 as i32,
        },
        2 => i64::from(src == 0),
        3 => !src,
        _ => return crate::JitCallStatus::ABI_MISMATCH.0 as i32,
    };
    // SAFETY: Cranelift owns this synchronous stack output slot.
    unsafe { out.write(value) };
    0
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(super) extern "C" fn test_native_binary_fallback(
    _runtime: *mut std::ffi::c_void,
    op: u32,
    lhs: i64,
    rhs: i64,
    _function: i64,
    _continuation: i64,
    out: *mut i64,
) -> i32 {
    if out.is_null() {
        return crate::JitCallStatus::RUNTIME_ERROR.0 as i32;
    }
    let value = match op {
        0 => lhs.checked_add(rhs),
        1 => lhs.checked_sub(rhs),
        2 => lhs.checked_mul(rhs),
        3 if rhs != 0 && lhs % rhs == 0 => Some(lhs / rhs),
        4 if rhs != 0 => Some(lhs % rhs),
        _ => None,
    };
    let Some(value) = value else {
        return crate::JitCallStatus::RECOMPILE_REQUESTED.0 as i32;
    };
    // SAFETY: Cranelift owns this synchronous stack output slot.
    unsafe { out.write(value) };
    0
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(super) extern "C" fn test_native_compare_fallback(
    _runtime: *mut std::ffi::c_void,
    op: u32,
    lhs: i64,
    rhs: i64,
    out: *mut i64,
) -> i32 {
    if out.is_null() {
        return crate::JitCallStatus::RUNTIME_ERROR.0 as i32;
    }
    let value = match op {
        0 | 2 => i64::from(lhs == rhs),
        1 | 3 => i64::from(lhs != rhs),
        4 => i64::from(lhs < rhs),
        5 => i64::from(lhs <= rhs),
        6 => i64::from(lhs > rhs),
        7 => i64::from(lhs >= rhs),
        8 => match lhs.cmp(&rhs) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        },
        _ => return crate::JitCallStatus::ABI_MISMATCH.0 as i32,
    };
    // SAFETY: Cranelift owns this synchronous stack output slot.
    unsafe { out.write(value) };
    0
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(super) extern "C" fn test_native_cast_fallback(
    _runtime: *mut std::ffi::c_void,
    op: u32,
    src: i64,
    out: *mut i64,
) -> i32 {
    if out.is_null() {
        return crate::JitCallStatus::RUNTIME_ERROR.0 as i32;
    }
    let value = match op {
        0 => i64::from(src != 0),
        1 => src,
        _ => return crate::JitCallStatus::RUNTIME_ERROR.0 as i32,
    };
    // SAFETY: Cranelift owns this synchronous stack output slot.
    unsafe { out.write(value) };
    0
}

pub(super) extern "C" fn test_native_echo_fallback(
    _runtime: *mut std::ffi::c_void,
    _src: i64,
) -> i32 {
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

pub(super) extern "C" fn test_native_echo_bytes_fallback(
    _runtime: *mut std::ffi::c_void,
    _bytes: *const u8,
    _length: u64,
) {
}

pub(super) extern "C" fn test_native_echo_int_fallback(
    _runtime: *mut std::ffi::c_void,
    _value: i64,
) {
}

pub(super) extern "C" fn test_native_echo_float_fallback(
    _runtime: *mut std::ffi::c_void,
    _value: f64,
) {
}

pub(super) extern "C" fn test_native_float_to_string_fallback(
    _runtime: *mut std::ffi::c_void,
    _value: f64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_float_to_int_fallback(
    _runtime: *mut std::ffi::c_void,
    _mode: u32,
    _function: u32,
    _continuation: u32,
    _value: f64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_object_class_name_fallback(
    _runtime: *mut std::ffi::c_void,
    _object: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_prepared_object_new_fallback(
    _runtime: *mut std::ffi::c_void,
    _prepared: u64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_plain_object_clone_fallback(
    _runtime: *mut std::ffi::c_void,
    _object: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_local_fetch_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    value: i64,
    _function: i64,
    _local: i64,
    _file: i64,
    _start: i64,
    out: *mut i64,
) -> i32 {
    if out.is_null() {
        crate::JitCallStatus::RUNTIME_ERROR.0 as i32
    } else {
        // SAFETY: Cranelift owns this synchronous stack output slot.
        unsafe { out.write(value) };
        0
    }
}

pub(super) extern "C" fn test_native_exception_new_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    message: i64,
    _function: i64,
    _continuation: i64,
    out: *mut i64,
) -> i32 {
    if out.is_null() {
        crate::JitCallStatus::RUNTIME_ERROR.0 as i32
    } else {
        // SAFETY: Cranelift owns this synchronous stack output slot.
        unsafe { out.write(message) };
        0
    }
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(super) extern "C" fn test_native_local_store_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _current: i64,
    value: i64,
    _function: i64,
    _local: i64,
    out: *mut i64,
) -> i32 {
    if !out.is_null() {
        unsafe { out.write(value) };
        0
    } else {
        crate::JitCallStatus::RUNTIME_ERROR.0 as i32
    }
}

pub(super) extern "C" fn test_native_value_release_fallback(
    _runtime: *mut std::ffi::c_void,
    _value: i64,
) -> i32 {
    0
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(super) extern "C" fn test_native_reference_bind_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    value: i64,
    _key: i64,
    _reserved: i64,
    out: *mut i64,
) -> i32 {
    if !out.is_null() {
        unsafe { out.write(value) };
        0
    } else {
        crate::JitCallStatus::RUNTIME_ERROR.0 as i32
    }
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(super) extern "C" fn test_native_argument_check_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    value: i64,
    _target_function: i64,
    _parameter_flags: i64,
    _caller_function: i64,
    _continuation: i64,
    out: *mut i64,
) -> i32 {
    if out.is_null() {
        crate::JitCallStatus::RUNTIME_ERROR.0 as i32
    } else {
        unsafe { out.write(value) };
        0
    }
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(super) extern "C" fn test_native_return_check_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    value: i64,
    _function: i64,
    out: *mut i64,
) -> i32 {
    if out.is_null() {
        crate::JitCallStatus::RUNTIME_ERROR.0 as i32
    } else {
        unsafe { out.write(value) };
        0
    }
}

pub(super) extern "C" fn test_native_array_new_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _out: *mut i64,
) -> i32 {
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

pub(super) extern "C" fn test_native_object_new_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _out: *mut i64,
) -> i32 {
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

pub(super) extern "C" fn test_native_property_fetch_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _object: i64,
    _function: i64,
    _continuation: i64,
    _out: *mut i64,
) -> i32 {
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

pub(super) extern "C" fn test_native_property_assign_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _object: i64,
    _value: i64,
    _function: i64,
    _continuation: i64,
    _out: *mut i64,
) -> i32 {
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(super) extern "C" fn test_native_object_clone_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    object: i64,
    out: *mut i64,
) -> i32 {
    if out.is_null() {
        crate::JitCallStatus::RUNTIME_ERROR.0 as i32
    } else {
        unsafe { out.write(object) };
        0
    }
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(super) extern "C" fn test_native_object_clone_with_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    object: i64,
    _replacements: i64,
    out: *mut i64,
) -> i32 {
    if out.is_null() {
        crate::JitCallStatus::RUNTIME_ERROR.0 as i32
    } else {
        unsafe { out.write(object) };
        0
    }
}

pub(super) extern "C" fn test_native_array_insert_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _array: i64,
    _key: i64,
    _value: i64,
    _out: *mut i64,
) -> i32 {
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

pub(super) extern "C" fn test_native_array_fetch_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _array: i64,
    _key: i64,
    out: *mut i64,
) -> i32 {
    if !out.is_null() {
        // SAFETY: test fallback follows the baseline value-helper ABI.
        unsafe { out.write(crate::jit_encode_constant(u32::MAX)) };
    }
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

pub(super) extern "C" fn test_native_array_unset_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _array: i64,
    _key: i64,
    _out: *mut i64,
) -> i32 {
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

pub(super) extern "C" fn test_native_array_spread_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _array: i64,
    _source: i64,
    _out: *mut i64,
) -> i32 {
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

pub(super) extern "C" fn test_native_foreach_init_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _source: i64,
    _function: i64,
    _local: i64,
    _out: *mut i64,
) -> i32 {
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

pub(super) extern "C" fn test_native_foreach_next_fallback(
    _runtime: *mut std::ffi::c_void,
    _iterator: i64,
    _key_out: *mut i64,
    _value_out: *mut i64,
    _has_out: *mut i64,
) -> i32 {
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

pub(super) extern "C" fn test_native_foreach_cleanup_fallback(
    _runtime: *mut std::ffi::c_void,
    _iterator: i64,
) -> i32 {
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

pub(super) extern "C" fn test_native_constant_fetch_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _function: i64,
    _instruction: i64,
    _out: *mut i64,
) -> i32 {
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(super) extern "C" fn test_native_truthy_fallback(
    _runtime: *mut std::ffi::c_void,
    src: i64,
    out: *mut i64,
) -> i32 {
    if out.is_null() {
        return crate::JitCallStatus::RUNTIME_ERROR.0 as i32;
    }
    // SAFETY: Cranelift owns this synchronous stack output slot.
    unsafe { out.write(i64::from(src != 0)) };
    0
}

pub(super) extern "C" fn test_native_type_predicate_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _src: i64,
    out: *mut i64,
) -> i32 {
    if out.is_null() {
        return crate::JitCallStatus::RUNTIME_ERROR.0 as i32;
    }
    // SAFETY: Cranelift owns this synchronous stack output slot.
    unsafe { out.write(crate::jit_encode_constant(crate::JIT_VALUE_FALSE)) };
    0
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(super) extern "C" fn test_native_stable_length_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _value: i64,
    _function: i64,
    _continuation: i64,
    _out: *mut i64,
) -> i32 {
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

pub(super) extern "C" fn test_native_string_predicate_fallback(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _haystack: i64,
    _needle: i64,
    _out: *mut i64,
) -> i32 {
    crate::JitCallStatus::ABI_MISMATCH.0 as i32
}

pub(super) extern "C" fn test_native_runtime_fatal_fallback(
    _runtime: *mut std::ffi::c_void,
    _function: u32,
    _instruction: u32,
) -> i32 {
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

pub(super) extern "C" fn test_native_execution_poll_fallback(
    _runtime: *mut std::ffi::c_void,
) -> i32 {
    0
}
