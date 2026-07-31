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

pub(super) extern "C" fn test_native_exact_unary_fallback(
    _runtime: *mut std::ffi::c_void,
    _source: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(super) extern "C" fn test_baseline_binary_fallback(
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

pub(super) extern "C" fn test_total_representation_binary_fallback(
    _runtime: *mut std::ffi::c_void,
    _lhs: i64,
    _rhs: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_exact_compare_fallback(
    _runtime: *mut u8,
    _left: i64,
    _right: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(
        crate::JitCallStatus::ABI_MISMATCH,
        0,
        crate::jit_encode_constant(u32::MAX),
    )
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

pub(super) extern "C" fn test_native_float_to_string_fallback(
    _runtime: *mut std::ffi::c_void,
    _value: f64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

// SAFETY: test artifacts pass an immutable byte range published by their
// direct-string arena. The zero-length case deliberately does not dereference
// the descriptor pointer.
#[allow(unsafe_code)]
pub(super) extern "C" fn test_native_numeric_string_fallback(
    bytes: *const u8,
    length: u64,
) -> crate::JitNativeNumericStringResult {
    let length = usize::try_from(length)
        .expect("native string length must fit the product target address width");
    let bytes = if length == 0 {
        &[]
    } else {
        // SAFETY: the optimizing string descriptor keeps this exact immutable
        // range alive for the synchronous pure handler call.
        unsafe { std::slice::from_raw_parts(bytes, length) }
    };
    let classified = php_runtime::experimental::numeric_string::classify(bytes);
    use php_runtime::experimental::numeric_string::{NumericStringKind, NumericStringValue};
    match (classified.kind, classified.value) {
        (
            NumericStringKind::IntString | NumericStringKind::FloatString,
            Some(NumericStringValue::Int(value)),
        ) => crate::JitNativeNumericStringResult {
            kind: crate::JIT_NATIVE_NUMERIC_STRING_INT,
            payload: value as u64,
        },
        (
            NumericStringKind::IntString | NumericStringKind::FloatString,
            Some(NumericStringValue::Float(value)),
        ) => crate::JitNativeNumericStringResult {
            kind: crate::JIT_NATIVE_NUMERIC_STRING_FLOAT,
            payload: value.to_bits(),
        },
        (NumericStringKind::LeadingNumeric, Some(NumericStringValue::Int(value))) => {
            crate::JitNativeNumericStringResult {
                kind: crate::JIT_NATIVE_NUMERIC_STRING_LEADING_INT,
                payload: value as u64,
            }
        }
        (NumericStringKind::LeadingNumeric, Some(NumericStringValue::Float(value))) => {
            crate::JitNativeNumericStringResult {
                kind: crate::JIT_NATIVE_NUMERIC_STRING_LEADING_FLOAT,
                payload: value.to_bits(),
            }
        }
        _ => crate::JitNativeNumericStringResult {
            kind: crate::JIT_NATIVE_NUMERIC_STRING_NON_NUMERIC,
            payload: 0,
        },
    }
}

pub(super) extern "C" fn test_native_fmod_f64_fallback(dividend: f64, divisor: f64) -> f64 {
    dividend % divisor
}

pub(super) extern "C" fn test_native_round_f64_fallback(
    value: f64,
    precision: i64,
    mode: i64,
) -> f64 {
    php_runtime::api::native_round_f64(value, precision, mode)
}

macro_rules! test_native_unary_math_fallback {
    ($name:ident, $method:ident) => {
        extern "C" fn $name(value: f64) -> f64 {
            value.$method()
        }
    };
}

test_native_unary_math_fallback!(test_native_acos_f64_fallback, acos);
test_native_unary_math_fallback!(test_native_acosh_f64_fallback, acosh);
test_native_unary_math_fallback!(test_native_asin_f64_fallback, asin);
test_native_unary_math_fallback!(test_native_asinh_f64_fallback, asinh);
test_native_unary_math_fallback!(test_native_atan_f64_fallback, atan);
test_native_unary_math_fallback!(test_native_atanh_f64_fallback, atanh);
test_native_unary_math_fallback!(test_native_cos_f64_fallback, cos);
test_native_unary_math_fallback!(test_native_cosh_f64_fallback, cosh);
test_native_unary_math_fallback!(test_native_exp_f64_fallback, exp);
test_native_unary_math_fallback!(test_native_expm1_f64_fallback, exp_m1);
test_native_unary_math_fallback!(test_native_log_f64_fallback, ln);
test_native_unary_math_fallback!(test_native_log10_f64_fallback, log10);
test_native_unary_math_fallback!(test_native_log1p_f64_fallback, ln_1p);
test_native_unary_math_fallback!(test_native_sin_f64_fallback, sin);
test_native_unary_math_fallback!(test_native_sinh_f64_fallback, sinh);
test_native_unary_math_fallback!(test_native_tan_f64_fallback, tan);
test_native_unary_math_fallback!(test_native_tanh_f64_fallback, tanh);

extern "C" fn test_native_atan2_f64_fallback(left: f64, right: f64) -> f64 {
    left.atan2(right)
}

extern "C" fn test_native_deg2rad_f64_fallback(value: f64) -> f64 {
    (value / 180.0) * std::f64::consts::PI
}

extern "C" fn test_native_fpow_f64_fallback(base: f64, exponent: f64) -> f64 {
    base.powf(exponent)
}

extern "C" fn test_native_hypot_f64_fallback(left: f64, right: f64) -> f64 {
    left.hypot(right)
}

extern "C" fn test_native_rad2deg_f64_fallback(value: f64) -> f64 {
    (value / std::f64::consts::PI) * 180.0
}

pub(super) fn test_native_pure_math_fallback(builtin: super::StablePureMathBuiltin) -> usize {
    use super::StablePureMathBuiltin;
    match builtin {
        StablePureMathBuiltin::Acos => test_native_acos_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Acosh => test_native_acosh_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Asin => test_native_asin_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Asinh => test_native_asinh_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Atan => test_native_atan_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Atan2 => test_native_atan2_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Atanh => test_native_atanh_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Cos => test_native_cos_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Cosh => test_native_cosh_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Deg2Rad => test_native_deg2rad_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Exp => test_native_exp_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Expm1 => test_native_expm1_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Fpow => test_native_fpow_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Hypot => test_native_hypot_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Log => test_native_log_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Log10 => test_native_log10_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Log1p => test_native_log1p_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Rad2Deg => test_native_rad2deg_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Sin => test_native_sin_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Sinh => test_native_sinh_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Tan => test_native_tan_f64_fallback as *const () as usize,
        StablePureMathBuiltin::Tanh => test_native_tanh_f64_fallback as *const () as usize,
    }
}

pub(super) extern "C" fn test_native_object_class_name_fallback(
    _runtime: *mut std::ffi::c_void,
    _object: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_object_cast_fallback(
    _runtime: *mut std::ffi::c_void,
    _source: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_array_cast_fallback(
    _runtime: *mut std::ffi::c_void,
    _source: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_int_cast_fallback(
    _runtime: *mut std::ffi::c_void,
    _source: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_float_cast_fallback(
    _runtime: *mut std::ffi::c_void,
    _source: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_string_cast_fallback(
    _runtime: *mut std::ffi::c_void,
    _source: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_prepared_object_new_fallback(
    _runtime: *mut std::ffi::c_void,
    _prepared: u64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_prepared_exception_new_fallback(
    _runtime: *mut std::ffi::c_void,
    _prepared: u64,
    _message: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_prepared_closure_new_fallback(
    _runtime: *mut std::ffi::c_void,
    _prepared: u64,
    _captures: *const i64,
    _implicit_this: i64,
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
    _state_out: *mut crate::JitDeoptState,
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

pub(super) extern "C" fn test_native_dynamic_property_fallback(
    _runtime: *mut std::ffi::c_void,
    _object: i64,
    _property: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RECOMPILE_REQUESTED, 0, 0)
}
