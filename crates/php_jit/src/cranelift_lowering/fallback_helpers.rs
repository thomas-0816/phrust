pub(super) extern "C" fn test_native_exact_unary_fallback(
    _runtime: *mut std::ffi::c_void,
    _source: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_exact_arithmetic_fallback(
    _runtime: *mut std::ffi::c_void,
    _left: i64,
    _right: i64,
    _prepared_type_error: i64,
    _file: i64,
    _start: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
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

pub(super) extern "C" fn test_native_echo_bytes_fallback(
    _runtime: *mut std::ffi::c_void,
    _bytes: *const u8,
    _length: u64,
) {
}

pub(super) extern "C" fn test_native_undefined_variable_warning_fallback(
    _runtime: *mut std::ffi::c_void,
    _name: *const u8,
    _name_length: u64,
    _file: i64,
    _start: i64,
) -> i32 {
    0
}

pub(super) extern "C" fn test_native_undefined_array_key_warning_fallback(
    _runtime: *mut std::ffi::c_void,
    _key: i64,
    _file: i64,
    _start: i64,
) -> i32 {
    0
}

pub(super) extern "C" fn test_native_array_offset_warning_fallback(
    _runtime: *mut std::ffi::c_void,
    _value: i64,
    _file: i64,
    _start: i64,
) -> i32 {
    0
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

pub(super) extern "C" fn test_native_type_name_fallback(
    _runtime: *mut std::ffi::c_void,
    _value: i64,
    _debug: i64,
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
    _code: i64,
    _previous: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_static_property_contract_fallback(
    _runtime: *mut std::ffi::c_void,
    _contract: u64,
    _value: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_typed_static_reference_bind_fallback(
    _runtime: *mut std::ffi::c_void,
    _contract: u64,
    _reference: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_typed_reference_store_fallback(
    _runtime: *mut std::ffi::c_void,
    _reference: i64,
    _replacement: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_typed_reference_array_init_fallback(
    _runtime: *mut std::ffi::c_void,
    _reference: i64,
) -> crate::JitNativeControlResult {
    crate::JitNativeControlResult::control(crate::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

pub(super) extern "C" fn test_native_undefined_constant_fallback(
    _runtime: *mut std::ffi::c_void,
    _contract: u64,
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

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(super) extern "C" fn test_native_declared_argument_contract_fallback(
    _runtime: *mut std::ffi::c_void,
    value: i64,
    _contract: u64,
    _strict: i64,
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
pub(super) extern "C" fn test_native_declared_return_contract_fallback(
    _runtime: *mut std::ffi::c_void,
    value: i64,
    _contract: u64,
    out: *mut i64,
) -> i32 {
    if out.is_null() {
        crate::JitCallStatus::RUNTIME_ERROR.0 as i32
    } else {
        unsafe { out.write(value) };
        0
    }
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
