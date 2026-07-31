//! Exact runtime operations over authoritative native encodings.
//!
//! These fixed ABIs expose no cold coordinator or compatibility value plane.

use super::{
    NativeComparisonTraversal, NativeComparisonValue, NativeFixedCallablePlan,
    NativePreparedClosure, NativeRequestFastState, PreparedNativeRuntimeClass,
    PreparedNativeThrowableSite, native_reference_state,
};
use std::fmt::{self, Write};
use std::sync::Arc;

/// Appends an already-typed native string without recovering the cold runtime
/// coordinator or materializing a compatibility graph.
#[allow(unsafe_code)] // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
pub(crate) extern "C" fn jit_native_echo_bytes_abi(
    runtime: *mut NativeRequestFastState,
    bytes: *const u8,
    length: u64,
) {
    if runtime.is_null() || (bytes.is_null() && length != 0) {
        return;
    }
    let Ok(length) = usize::try_from(length) else {
        return;
    };
    // SAFETY: the request owner publishes a stable output pointer for the
    // activation lifetime; the optimizing string descriptor supplies an
    // immutable byte range of exactly `length` bytes for this call.
    let output = unsafe { (*runtime).output.as_mut() };
    let Some(output) = output else {
        return;
    };
    let bytes = if length == 0 {
        &[]
    } else {
        // SAFETY: validated non-null above; descriptor publication owns the
        // backing direct-string bytes for the duration of this synchronous call.
        unsafe { std::slice::from_raw_parts(bytes, length) }
    };
    output.write_fast_bytes(bytes);
}

/// Formats one SSA-proven PHP float and publishes the result directly in the
/// authoritative native string arena, without constructing a compatibility graph.
pub(crate) extern "C" fn jit_native_float_to_string_abi(
    runtime: *mut NativeRequestFastState,
    value: f64,
) -> php_jit::JitNativeControlResult {
    let rendered = NativeInlineString::from_float(value);
    // SAFETY: generated exact handlers receive the active request's stable
    // fast-state pointer. The direct publisher does not inspect cold state.
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    match unsafe { &mut *runtime }.publish_direct_string_with(rendered.length, |output| {
        output.copy_from_slice(rendered.as_bytes());
    }) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Classifies one authoritative native string with PHP's exact shared
/// numeric-string parser. This is a pure compiled handler: it receives no
/// request state, capability, operation ID, or compatibility graph.
///
/// # Safety contract
///
/// Generated code supplies the immutable byte range published by the direct
/// string descriptor. The descriptor remains live for this synchronous call.
#[allow(unsafe_code)] // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
pub(crate) extern "C" fn jit_native_numeric_string_abi(
    bytes: *const u8,
    length: u64,
) -> php_jit::JitNativeNumericStringResult {
    let Ok(length) = usize::try_from(length) else {
        return php_jit::JitNativeNumericStringResult::default();
    };
    let bytes = if length == 0 {
        &[]
    } else {
        // SAFETY: guaranteed by the published native descriptor contract
        // documented above.
        unsafe { std::slice::from_raw_parts(bytes, length) }
    };
    let classified = php_runtime::experimental::numeric_string::classify(bytes);
    use php_runtime::experimental::numeric_string::{NumericStringKind, NumericStringValue};
    match (classified.kind, classified.value) {
        (
            NumericStringKind::IntString | NumericStringKind::FloatString,
            Some(NumericStringValue::Int(value)),
        ) => php_jit::JitNativeNumericStringResult {
            kind: php_jit::JIT_NATIVE_NUMERIC_STRING_INT,
            payload: value as u64,
        },
        (
            NumericStringKind::IntString | NumericStringKind::FloatString,
            Some(NumericStringValue::Float(value)),
        ) => php_jit::JitNativeNumericStringResult {
            kind: php_jit::JIT_NATIVE_NUMERIC_STRING_FLOAT,
            payload: value.to_bits(),
        },
        (NumericStringKind::LeadingNumeric, Some(NumericStringValue::Int(value))) => {
            php_jit::JitNativeNumericStringResult {
                kind: php_jit::JIT_NATIVE_NUMERIC_STRING_LEADING_INT,
                payload: value as u64,
            }
        }
        (NumericStringKind::LeadingNumeric, Some(NumericStringValue::Float(value))) => {
            php_jit::JitNativeNumericStringResult {
                kind: php_jit::JIT_NATIVE_NUMERIC_STRING_LEADING_FLOAT,
                payload: value.to_bits(),
            }
        }
        _ => php_jit::JitNativeNumericStringResult {
            kind: php_jit::JIT_NATIVE_NUMERIC_STRING_NON_NUMERIC,
            payload: 0,
        },
    }
}

/// Computes PHP `fmod`'s IEEE floating-point remainder without request state,
/// an operation selector, or compatibility graph conversion.
pub(crate) extern "C" fn jit_native_fmod_f64_abi(dividend: f64, divisor: f64) -> f64 {
    dividend % divisor
}

/// Applies PHP's validated rounding mode without request state, compatibility
/// conversion, or a runtime operation selector.
pub(crate) extern "C" fn jit_native_round_f64_abi(value: f64, precision: i64, mode: i64) -> f64 {
    php_runtime::api::native_round_f64(value, precision, mode)
}

macro_rules! native_unary_math_abi {
    ($name:ident, $method:ident) => {
        pub(crate) extern "C" fn $name(value: f64) -> f64 {
            value.$method()
        }
    };
}

native_unary_math_abi!(jit_native_acos_f64_abi, acos);
native_unary_math_abi!(jit_native_acosh_f64_abi, acosh);
native_unary_math_abi!(jit_native_asin_f64_abi, asin);
native_unary_math_abi!(jit_native_asinh_f64_abi, asinh);
native_unary_math_abi!(jit_native_atan_f64_abi, atan);
native_unary_math_abi!(jit_native_atanh_f64_abi, atanh);
native_unary_math_abi!(jit_native_cos_f64_abi, cos);
native_unary_math_abi!(jit_native_cosh_f64_abi, cosh);
native_unary_math_abi!(jit_native_exp_f64_abi, exp);
native_unary_math_abi!(jit_native_expm1_f64_abi, exp_m1);
native_unary_math_abi!(jit_native_log_f64_abi, ln);
native_unary_math_abi!(jit_native_log10_f64_abi, log10);
native_unary_math_abi!(jit_native_log1p_f64_abi, ln_1p);
native_unary_math_abi!(jit_native_sin_f64_abi, sin);
native_unary_math_abi!(jit_native_sinh_f64_abi, sinh);
native_unary_math_abi!(jit_native_tan_f64_abi, tan);
native_unary_math_abi!(jit_native_tanh_f64_abi, tanh);

pub(crate) extern "C" fn jit_native_atan2_f64_abi(left: f64, right: f64) -> f64 {
    left.atan2(right)
}

pub(crate) extern "C" fn jit_native_deg2rad_f64_abi(value: f64) -> f64 {
    (value / 180.0) * std::f64::consts::PI
}

pub(crate) extern "C" fn jit_native_fpow_f64_abi(base: f64, exponent: f64) -> f64 {
    base.powf(exponent)
}

pub(crate) extern "C" fn jit_native_hypot_f64_abi(left: f64, right: f64) -> f64 {
    left.hypot(right)
}

pub(crate) extern "C" fn jit_native_rad2deg_f64_abi(value: f64) -> f64 {
    (value / std::f64::consts::PI) * 180.0
}

fn native_runtime_contract_violation() -> php_jit::JitNativeControlResult {
    php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::ABI_MISMATCH, 0, 0)
}

fn native_compound_comparison_bool(value: bool) -> php_jit::JitNativeControlResult {
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(if value {
        php_jit::JIT_VALUE_TRUE
    } else {
        php_jit::JIT_VALUE_FALSE
    }))
}

fn native_exact_comparison_order(
    fast: &NativeRequestFastState,
    left: i64,
    right: i64,
) -> Option<(std::cmp::Ordering, bool)> {
    let mut traversal = NativeComparisonTraversal::default();
    let ordering = fast.native_values_compare(left, right, &mut traversal)?;
    Some((ordering, traversal.unordered))
}

/// Exact strict identity over the authoritative native value graph.
pub(crate) extern "C" fn jit_native_identical_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &*runtime };
    fast.native_values_identical(left, right, &mut NativeComparisonTraversal::default())
        .map_or_else(
            native_runtime_contract_violation,
            native_compound_comparison_bool,
        )
}

/// Exact strict non-identity over the authoritative native value graph.
pub(crate) extern "C" fn jit_native_not_identical_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &*runtime };
    fast.native_values_identical(left, right, &mut NativeComparisonTraversal::default())
        .map(|identical| !identical)
        .map_or_else(
            native_runtime_contract_violation,
            native_compound_comparison_bool,
        )
}

/// Exact loose equality over the authoritative native value graph.
pub(crate) extern "C" fn jit_native_equal_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &*runtime };
    fast.native_values_equal(left, right, &mut NativeComparisonTraversal::default())
        .map_or_else(
            native_runtime_contract_violation,
            native_compound_comparison_bool,
        )
}

/// Exact loose inequality over the authoritative native value graph.
pub(crate) extern "C" fn jit_native_not_equal_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &*runtime };
    fast.native_values_equal(left, right, &mut NativeComparisonTraversal::default())
        .map(|equal| !equal)
        .map_or_else(
            native_runtime_contract_violation,
            native_compound_comparison_bool,
        )
}

/// Exact PHP less-than comparison over the authoritative native value graph.
pub(crate) extern "C" fn jit_native_less_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &*runtime };
    let Some((ordering, unordered)) = native_exact_comparison_order(fast, left, right) else {
        return native_runtime_contract_violation();
    };
    native_compound_comparison_bool(!unordered && ordering.is_lt())
}

/// Exact PHP less-than-or-equal comparison over the native value graph.
pub(crate) extern "C" fn jit_native_less_equal_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &*runtime };
    let Some((ordering, unordered)) = native_exact_comparison_order(fast, left, right) else {
        return native_runtime_contract_violation();
    };
    native_compound_comparison_bool(!unordered && !ordering.is_gt())
}

/// Exact PHP greater-than comparison over the authoritative native value graph.
pub(crate) extern "C" fn jit_native_greater_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &*runtime };
    let Some((ordering, unordered)) = native_exact_comparison_order(fast, left, right) else {
        return native_runtime_contract_violation();
    };
    native_compound_comparison_bool(!unordered && ordering.is_gt())
}

/// Exact PHP greater-than-or-equal comparison over the native value graph.
pub(crate) extern "C" fn jit_native_greater_equal_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &*runtime };
    let Some((ordering, unordered)) = native_exact_comparison_order(fast, left, right) else {
        return native_runtime_contract_violation();
    };
    native_compound_comparison_bool(!unordered && !ordering.is_lt())
}

/// Exact PHP three-way comparison over the authoritative native value graph.
pub(crate) extern "C" fn jit_native_spaceship_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &*runtime };
    let Some((ordering, _)) = native_exact_comparison_order(fast, left, right) else {
        return native_runtime_contract_violation();
    };
    php_jit::JitNativeControlResult::returning(match ordering {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    })
}

/// Returns the already-published class name of one direct object without
/// decoding a compatibility graph or entering generic property dispatch.
pub(crate) extern "C" fn jit_native_object_class_name_abi(
    runtime: *mut NativeRequestFastState,
    object: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: the generated call supplies one live direct object and the
    // request owner keeps both the fast state and slot-parallel owner alive.
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let Some(name) = (unsafe { &*runtime })
        .direct_object(object)
        .map(php_runtime::api::ObjectRef::display_name_handle)
    else {
        return php_jit::JitNativeControlResult::control(
            php_jit::JitCallStatus::ABI_MISMATCH,
            0,
            0,
        );
    };
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    match (unsafe { &mut *runtime }).publish_direct_string_bytes(name.as_bytes()) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Acquires one representation-complete callable directly from authoritative
/// native storage. Unsupported callable shapes request the instruction's one
/// baseline continuation; no generic operation dispatcher is entered.
pub(crate) extern "C" fn jit_native_acquire_callable_abi(
    runtime: *mut NativeRequestFastState,
    value: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: generated code passes the request-stable fast state and one
    // encoded value whose owner remains live for this synchronous call.
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    match (unsafe { &mut *runtime }).acquire_direct_callable(value) {
        Ok(Some(callable)) => php_jit::JitNativeControlResult::returning(callable),
        Ok(None) => native_runtime_contract_violation(),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Publishes one compile-time callable from its immutable target/signature
/// contract. Generated code passes the already-resolved function identity and
/// flags, so this boundary performs no symbol query or dynamic dispatch.
pub(crate) extern "C" fn jit_native_resolve_callable_abi(
    runtime: *mut NativeRequestFastState,
    name: *const u8,
    length: u64,
    function_id: u64,
    visible_arity: u64,
    flags: u64,
) -> php_jit::JitNativeControlResult {
    let Ok(length) = usize::try_from(length) else {
        return native_runtime_contract_violation();
    };
    let (Ok(function_id), Ok(visible_arity), Ok(flags)) = (
        u32::try_from(function_id),
        u32::try_from(visible_arity),
        u32::try_from(flags),
    ) else {
        return native_runtime_contract_violation();
    };
    let allowed_flags = php_jit::JIT_NATIVE_PREPARED_CALLABLE_FIRST_PARAMETER_BY_REFERENCE
        | php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_INT
        | php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_STRING
        | php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_RELEASABLE_SCALAR;
    if flags & !allowed_flags != 0 {
        return native_runtime_contract_violation();
    }
    if name.is_null() && length != 0 {
        return php_jit::JitNativeControlResult::control(
            php_jit::JitCallStatus::ABI_MISMATCH,
            0,
            0,
        );
    }
    // SAFETY: generated code passes a stack-backed immutable byte range that
    // remains live for this synchronous call. The zero-length case admits a
    // dangling non-null stack address but never dereferences it.
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let name = unsafe { std::slice::from_raw_parts(name, length) };
    // SAFETY: the generated entry receives the request-stable fast state.
    #[allow(unsafe_code)]
    match (unsafe { &mut *runtime }).publish_fixed_function_callable(
        name,
        NativeFixedCallablePlan {
            function: php_ir::FunctionId::new(function_id),
            visible_arity,
            has_receiver: false,
            first_parameter_by_reference: flags
                & php_jit::JIT_NATIVE_PREPARED_CALLABLE_FIRST_PARAMETER_BY_REFERENCE
                != 0,
            returns_int: flags & php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_INT != 0,
            returns_string: flags & php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_STRING != 0,
            returns_releasable_scalar: flags
                & php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_RELEASABLE_SCALAR
                != 0,
        },
    ) {
        Ok(callable) => php_jit::JitNativeControlResult::returning(callable),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

fn native_binary_runtime_error() -> php_jit::JitNativeControlResult {
    php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

fn count_native_array_entries(
    fast: &NativeRequestFastState,
    identity: usize,
    entries: &[php_jit::JitNativeDirectArrayEntry],
    active: &mut [usize; 64],
    depth: usize,
) -> Option<usize> {
    if depth == active.len() || active[..depth].contains(&identity) {
        return None;
    }
    active[depth] = identity;
    let mut count = entries.len();
    for entry in entries {
        match fast.native_comparison_value(entry.value) {
            Some(NativeComparisonValue::Array { identity, entries }) => {
                count = count.checked_add(count_native_array_entries(
                    fast,
                    identity,
                    entries,
                    active,
                    depth + 1,
                )?)?;
            }
            Some(_) => {}
            // Publication admits only authoritative direct-array graphs.
            // An opaque value here is an engine contract violation.
            None => return None,
        }
    }
    Some(count)
}

fn native_array_count(
    runtime: *mut NativeRequestFastState,
    value: i64,
    mode: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the active request's stable fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let recursive = if mode == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        false
    } else {
        match fast.native_comparison_value(mode) {
            Some(NativeComparisonValue::Int(0)) => false,
            Some(NativeComparisonValue::Int(1)) => true,
            _ => return native_runtime_contract_violation(),
        }
    };
    let Some(NativeComparisonValue::Array { identity, entries }) =
        fast.native_comparison_value(value)
    else {
        return native_runtime_contract_violation();
    };
    let count = if recursive {
        let mut active = [0; 64];
        count_native_array_entries(fast, identity, entries, &mut active, 0)
    } else {
        Some(entries.len())
    };
    let Some(count) = count.and_then(|count| i64::try_from(count).ok()) else {
        return native_runtime_contract_violation();
    };
    match fast.publish_direct_int(count) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Exact `count` over authoritative direct-array entries. Publication admits
/// only non-recursive direct-array forms to optimizing code; unsupported
/// modes, compatibility values, and cycles are contract violations here.
pub(crate) extern "C" fn jit_native_count_abi(
    runtime: *mut NativeRequestFastState,
    value: i64,
    mode: i64,
) -> php_jit::JitNativeControlResult {
    native_array_count(runtime, value, mode)
}

/// `sizeof` is a distinct fixed target with the same PHP array semantics.
pub(crate) extern "C" fn jit_native_sizeof_abi(
    runtime: *mut NativeRequestFastState,
    value: i64,
    mode: i64,
) -> php_jit::JitNativeControlResult {
    native_array_count(runtime, value, mode)
}

#[derive(Clone, Copy)]
enum NativeBinaryNumber {
    Int(i64),
    Float(f64),
}

impl NativeBinaryNumber {
    fn exact_integer(self) -> Option<i64> {
        match self {
            Self::Int(value) => Some(value),
            Self::Float(value) if php_runtime::api::float_fits_int(value) => {
                let integer = value as i64;
                ((integer as f64) == value).then_some(integer)
            }
            Self::Float(_) => None,
        }
    }
}

fn native_binary_number(value: NativeComparisonValue<'_>) -> Option<NativeBinaryNumber> {
    match value {
        NativeComparisonValue::Null => Some(NativeBinaryNumber::Int(0)),
        NativeComparisonValue::Bool(value) => Some(NativeBinaryNumber::Int(i64::from(value))),
        NativeComparisonValue::Int(value) => Some(NativeBinaryNumber::Int(value)),
        NativeComparisonValue::Float(value) => Some(NativeBinaryNumber::Float(value)),
        NativeComparisonValue::String(bytes) => {
            let classified = php_runtime::experimental::numeric_string::classify(bytes);
            if !matches!(
                classified.kind,
                php_runtime::experimental::numeric_string::NumericStringKind::IntString
                    | php_runtime::experimental::numeric_string::NumericStringKind::FloatString
            ) {
                return None;
            }
            classified.value.map(|value| match value {
                php_runtime::experimental::numeric_string::NumericStringValue::Int(value) => {
                    NativeBinaryNumber::Int(value)
                }
                php_runtime::experimental::numeric_string::NumericStringValue::Float(value) => {
                    NativeBinaryNumber::Float(value)
                }
            })
        }
        NativeComparisonValue::Array { .. }
        | NativeComparisonValue::Object(_)
        | NativeComparisonValue::Resource(_)
        | NativeComparisonValue::OpaqueIdentity(_) => None,
    }
}

fn publish_native_binary_number(
    fast: &mut NativeRequestFastState,
    value: NativeBinaryNumber,
) -> php_jit::JitNativeControlResult {
    let published = match value {
        NativeBinaryNumber::Int(value) => fast.publish_direct_int(value),
        NativeBinaryNumber::Float(value) => fast.publish_direct_float(value),
    };
    published.map_or_else(
        |_| native_binary_runtime_error(),
        php_jit::JitNativeControlResult::returning,
    )
}

fn native_binary_array_key_equal(fast: &NativeRequestFastState, left: i64, right: i64) -> bool {
    match (
        fast.native_comparison_value(left),
        fast.native_comparison_value(right),
    ) {
        (Some(NativeComparisonValue::Int(left)), Some(NativeComparisonValue::Int(right))) => {
            left == right
        }
        (Some(NativeComparisonValue::String(left)), Some(NativeComparisonValue::String(right))) => {
            left == right
        }
        _ => false,
    }
}

fn native_binary_array_union(
    fast: &mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> Result<i64, &'static str> {
    let (left_entries, left_length) = fast
        .stable_native_array_range(left)
        .ok_or("array-union left operand escaped its publication plan")?;
    let (right_entries, right_length) = fast
        .stable_native_array_range(right)
        .ok_or("array-union right operand escaped its publication plan")?;
    let entry_at = |entries: *const php_jit::JitNativeDirectArrayEntry, index: usize| {
        // SAFETY: both operand owners remain live throughout this
        // synchronous binary operation and native arena reservations do
        // not relocate their stable ranges.
        #[allow(unsafe_code)]
        unsafe {
            *entries.add(index)
        }
    };
    let key_in_range = |fast: &NativeRequestFastState,
                        entries: *const php_jit::JitNativeDirectArrayEntry,
                        length: usize,
                        key: i64| {
        (0..length)
            .any(|index| native_binary_array_key_equal(fast, entry_at(entries, index).key, key))
    };
    let appended = (0..right_length)
        .filter(|&right_index| {
            let key = entry_at(right_entries, right_index).key;
            !key_in_range(fast, left_entries, left_length, key)
                && !key_in_range(fast, right_entries, right_index, key)
        })
        .count();
    let Some(output_length) = left_length.checked_add(appended) else {
        return Err("native binary array union length overflow");
    };
    fast.publish_owned_direct_array_with(output_length, |fast, output_index| {
        let entry = if output_index < left_length {
            entry_at(left_entries, output_index)
        } else {
            let appended_index = output_index - left_length;
            let source_index = (0..right_length)
                .filter(|&right_index| {
                    let key = entry_at(right_entries, right_index).key;
                    !key_in_range(fast, left_entries, left_length, key)
                        && !key_in_range(fast, right_entries, right_index, key)
                })
                .nth(appended_index)
                .ok_or("native binary array union lost an appended entry")?;
            entry_at(right_entries, source_index)
        };
        fast.retain_direct_encoded(entry.key)?;
        if let Err(error) = fast.retain_direct_encoded(entry.value) {
            fast.rollback_direct_retain(entry.key);
            return Err(error);
        }
        Ok(entry)
    })
}

struct NativeInlineString {
    bytes: [u8; php_runtime::api::PHP_FLOAT_STRING_BUFFER_CAPACITY],
    length: usize,
}

impl NativeInlineString {
    fn new() -> Self {
        Self {
            bytes: [0; php_runtime::api::PHP_FLOAT_STRING_BUFFER_CAPACITY],
            length: 0,
        }
    }

    fn from_float(value: f64) -> Self {
        let mut output = Self::new();
        let rendered = php_runtime::api::float_to_php_string_bytes(value, &mut output.bytes);
        output.length = rendered.len();
        output
    }

    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}

impl Write for NativeInlineString {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.length.checked_add(value.len()).ok_or(fmt::Error)?;
        let output = self.bytes.get_mut(self.length..end).ok_or(fmt::Error)?;
        output.copy_from_slice(value.as_bytes());
        self.length = end;
        Ok(())
    }
}

enum NativeConcatPart {
    /// Raw native arena bytes remain stable for the synchronous exact call.
    Native {
        bytes: *const u8,
        length: usize,
    },
    Inline(NativeInlineString),
}

impl NativeConcatPart {
    fn from_value(fast: &NativeRequestFastState, value: i64) -> Option<Self> {
        let mut inline = NativeInlineString::new();
        match fast.native_comparison_value(value)? {
            NativeComparisonValue::Null | NativeComparisonValue::Bool(false) => {}
            NativeComparisonValue::Bool(true) => inline.write_str("1").ok()?,
            NativeComparisonValue::Int(value) => write!(inline, "{value}").ok()?,
            NativeComparisonValue::Float(value) => inline = NativeInlineString::from_float(value),
            NativeComparisonValue::String(value) => {
                return Some(Self::Native {
                    bytes: value.as_ptr(),
                    length: value.len(),
                });
            }
            NativeComparisonValue::Resource(value) => {
                write!(inline, "Resource id #{value}").ok()?
            }
            NativeComparisonValue::Array { .. }
            | NativeComparisonValue::Object(_)
            | NativeComparisonValue::OpaqueIdentity(_) => return None,
        }
        Some(Self::Inline(inline))
    }

    fn len(&self) -> usize {
        match self {
            Self::Native { length, .. } => *length,
            Self::Inline(value) => value.length,
        }
    }

    #[allow(unsafe_code)] // Safety: native source slots remain live and stable for the synchronous exact call.
    fn copy_to(&self, output: &mut [u8]) {
        match self {
            Self::Native { bytes, length } => {
                debug_assert_eq!(output.len(), *length);
                if *length != 0 {
                    unsafe {
                        std::ptr::copy_nonoverlapping(*bytes, output.as_mut_ptr(), *length);
                    }
                }
            }
            Self::Inline(value) => output.copy_from_slice(value.as_bytes()),
        }
    }
}

fn native_string_ranges(
    fast: &NativeRequestFastState,
    left: i64,
    right: i64,
) -> Option<((*const u8, usize), (*const u8, usize))> {
    match (
        fast.native_comparison_value(left)?,
        fast.native_comparison_value(right)?,
    ) {
        (NativeComparisonValue::String(left), NativeComparisonValue::String(right)) => {
            Some(((left.as_ptr(), left.len()), (right.as_ptr(), right.len())))
        }
        _ => None,
    }
}

pub(crate) extern "C" fn jit_native_array_union_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: publication proved two live direct arrays and reserved the
    // complete result capacity before the optimizing entry was selected.
    #[allow(unsafe_code)]
    native_binary_array_union(unsafe { &mut *runtime }, left, right).map_or_else(
        |_| native_binary_runtime_error(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_concat_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: generated code supplies the active request state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(left) = NativeConcatPart::from_value(fast, left) else {
        return native_binary_runtime_error();
    };
    let Some(right) = NativeConcatPart::from_value(fast, right) else {
        return native_binary_runtime_error();
    };
    let Some(length) = left.len().checked_add(right.len()) else {
        return native_binary_runtime_error();
    };
    fast.publish_direct_string_with(length, |output| {
        let (left_output, right_output) = output.split_at_mut(left.len());
        left.copy_to(left_output);
        right.copy_to(right_output);
    })
    .map_or_else(
        |_| native_binary_runtime_error(),
        php_jit::JitNativeControlResult::returning,
    )
}

fn native_string_bitwise<const OPERATION: u8>(
    fast: &mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    let Some(((left_bytes, left_length), (right_bytes, right_length))) =
        native_string_ranges(fast, left, right)
    else {
        return native_binary_runtime_error();
    };
    let common = left_length.min(right_length);
    let output_length = if OPERATION == 1 {
        left_length.max(right_length)
    } else {
        common
    };
    fast.publish_direct_string_with(output_length, |output| {
        // SAFETY: publication keeps both stable source owners live for this
        // synchronous total native call.
        #[allow(unsafe_code)]
        let (left, right) = unsafe {
            (
                std::slice::from_raw_parts(left_bytes, left_length),
                std::slice::from_raw_parts(right_bytes, right_length),
            )
        };
        for (output, (left, right)) in output[..common].iter_mut().zip(left.iter().zip(right)) {
            *output = match OPERATION {
                0 => left & right,
                1 => left | right,
                2 => left ^ right,
                _ => unreachable!("fixed string-bit operation"),
            };
        }
        if OPERATION == 1 {
            output[common..].copy_from_slice(if left.len() > common {
                &left[common..]
            } else {
                &right[common..]
            });
        }
    })
    .map_or_else(
        |_| native_binary_runtime_error(),
        php_jit::JitNativeControlResult::returning,
    )
}

macro_rules! native_string_bitwise_abi {
    ($name:ident, $operation:literal) => {
        pub(crate) extern "C" fn $name(
            runtime: *mut NativeRequestFastState,
            left: i64,
            right: i64,
        ) -> php_jit::JitNativeControlResult {
            // SAFETY: generated code supplies the active request state.
            #[allow(unsafe_code)]
            native_string_bitwise::<$operation>(unsafe { &mut *runtime }, left, right)
        }
    };
}

native_string_bitwise_abi!(jit_native_bit_and_abi, 0);
native_string_bitwise_abi!(jit_native_bit_or_abi, 1);
native_string_bitwise_abi!(jit_native_bit_xor_abi, 2);

fn native_exact_unary_numeric(
    fast: &mut NativeRequestFastState,
    source: i64,
    negate: bool,
) -> php_jit::JitNativeControlResult {
    let Some(value) = fast
        .native_comparison_value(source)
        .and_then(native_binary_number)
    else {
        return native_runtime_contract_violation();
    };
    let result = if negate {
        match value {
            NativeBinaryNumber::Int(i64::MIN) => NativeBinaryNumber::Float(-(i64::MIN as f64)),
            NativeBinaryNumber::Int(value) => NativeBinaryNumber::Int(-value),
            NativeBinaryNumber::Float(value) => NativeBinaryNumber::Float(-value),
        }
    } else {
        value
    };
    publish_native_binary_number(fast, result)
}

pub(crate) extern "C" fn jit_native_unary_plus_abi(
    runtime: *mut NativeRequestFastState,
    source: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: generated code supplies the live request prefix for this fixed
    // synchronous exact operation.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    native_exact_unary_numeric(fast, source, false)
}

pub(crate) extern "C" fn jit_native_unary_minus_abi(
    runtime: *mut NativeRequestFastState,
    source: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: generated code supplies the live request prefix for this fixed
    // synchronous exact operation.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    native_exact_unary_numeric(fast, source, true)
}

pub(crate) extern "C" fn jit_native_bit_not_abi(
    runtime: *mut NativeRequestFastState,
    source: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: generated code supplies the live request prefix for this fixed
    // synchronous exact operation.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    match fast.native_comparison_value(source) {
        Some(NativeComparisonValue::String(bytes)) => {
            let source = (bytes.as_ptr(), bytes.len());
            fast.publish_direct_string_with(source.1, |output| {
                // SAFETY: the source slot is live and stable for this
                // synchronous exact call.
                #[allow(unsafe_code)]
                let source = unsafe { std::slice::from_raw_parts(source.0, source.1) };
                for (output, source) in output.iter_mut().zip(source) {
                    *output = !source;
                }
            })
            .map_or_else(
                |_| native_binary_runtime_error(),
                php_jit::JitNativeControlResult::returning,
            )
        }
        Some(NativeComparisonValue::Int(value)) => fast.publish_direct_int(!value).map_or_else(
            |_| native_binary_runtime_error(),
            php_jit::JitNativeControlResult::returning,
        ),
        Some(NativeComparisonValue::Float(value)) => {
            let Some(value) = NativeBinaryNumber::Float(value).exact_integer() else {
                return native_runtime_contract_violation();
            };
            fast.publish_direct_int(!value).map_or_else(
                |_| native_binary_runtime_error(),
                php_jit::JitNativeControlResult::returning,
            )
        }
        Some(
            NativeComparisonValue::Null
            | NativeComparisonValue::Bool(_)
            | NativeComparisonValue::Array { .. }
            | NativeComparisonValue::Object(_)
            | NativeComparisonValue::Resource(_)
            | NativeComparisonValue::OpaqueIdentity(_),
        )
        | None => native_runtime_contract_violation(),
    }
}

pub(super) fn publish_native_array_cast_entries(
    fast: &mut NativeRequestFastState,
    properties: Vec<(String, i64)>,
) -> Result<i64, &'static str> {
    let length = properties.len();
    let mut properties = properties.into_iter();
    fast.publish_owned_direct_array_with(length, |fast, _| {
        let (name, value) = properties
            .next()
            .ok_or("native array-cast property list is truncated")?;
        let published =
            if let Some(integer) = php_runtime::api::array_key_integer_bytes(name.as_bytes()) {
                fast.publish_direct_int(integer)
            } else {
                fast.publish_direct_string_bytes(name.as_bytes())
            };
        let key = published?;
        if let Err(error) = fast.retain_direct_encoded(value) {
            let _ = fast.discard_owned_direct_value(key);
            return Err(error);
        }
        Ok(php_jit::JitNativeDirectArrayEntry { key, value })
    })
}

/// Reads `get_object_vars()` and `get_mangled_object_vars()` directly from the
/// authoritative object property plane.
///
/// Mangled projection is scope independent. Visible projection is exact for
/// public/private properties and global-scope protected omission. The legacy
/// baseline currently grants protected access only to the declaring class,
/// while the native array-cast key intentionally does not encode that owner;
/// scoped objects containing an initialized protected slot therefore take the
/// single baseline continuation instead of guessing.
pub(super) fn native_object_vars_entries(
    fast: &NativeRequestFastState,
    mut object: i64,
    mangled: bool,
) -> Option<Vec<(String, i64)>> {
    for _ in 0..16 {
        let (_, slot) = fast.direct_slot(object)?;
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            && native_reference_state(slot.reserved)
                != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
        {
            object = slot.payload as i64;
            continue;
        }
        break;
    }
    let (_, descriptor) = fast.direct_slot(object)?;
    if descriptor.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
        || !php_jit::jit_native_object_property_view_is_published(descriptor.flags)
    {
        return None;
    }
    let owner = fast.direct_object(object)?;
    let scope_class = fast
        .current_execution_scope()
        .and_then(|scope| scope.scope_class.as_deref());
    owner.with_native_array_cast_view(
        descriptor.payload,
        |declared_names, declared, dynamic_order, dynamic| {
            let mut result = Vec::with_capacity(declared.len().saturating_add(dynamic_order.len()));
            for (name, slot) in declared_names.iter().zip(declared) {
                if slot.initialized == 0 {
                    continue;
                }
                if mangled || !name.starts_with('\0') {
                    result.push((name.clone(), slot.value));
                    continue;
                }
                let rest = name.strip_prefix('\0')?;
                let (owner_name, visible_name) = rest.split_once('\0')?;
                if owner_name == "*" {
                    if scope_class.is_some() {
                        return None;
                    }
                    continue;
                }
                if scope_class.is_some_and(|scope| scope.eq_ignore_ascii_case(owner_name)) {
                    result.push((visible_name.to_owned(), slot.value));
                }
            }
            result.extend(dynamic_order.iter().filter_map(|name| {
                dynamic
                    .get(name)
                    .filter(|cell| cell.slot.initialized != 0)
                    .map(|cell| (name.clone(), cell.slot.value))
            }));
            Some(result)
        },
    )?
}

/// Implements PHP's complete `(array)` conversion family over authoritative
/// native values. Arrays retain COW identity, objects expose property order
/// with PHP visibility key encoding, null becomes empty, and every other
/// admitted value becomes element zero.
pub(crate) extern "C" fn jit_native_array_cast_abi(
    runtime: *mut NativeRequestFastState,
    mut source: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    for _ in 0..16 {
        let Some((_, slot)) = fast.direct_slot(source) else {
            break;
        };
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            && native_reference_state(slot.reserved)
                != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
        {
            source = slot.payload as i64;
            continue;
        }
        if matches!(
            slot.kind,
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                | php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
        ) {
            return native_runtime_contract_violation();
        }
        break;
    }

    enum ArrayCastSource {
        Identity,
        Empty,
        Scalar,
        Object {
            owner: php_runtime::api::ObjectRef,
            layout_id: u64,
        },
    }
    let source_kind = match fast.native_comparison_value(source) {
        Some(NativeComparisonValue::Array { .. }) => ArrayCastSource::Identity,
        Some(NativeComparisonValue::Null) => ArrayCastSource::Empty,
        Some(NativeComparisonValue::Object(object)) => {
            let Some(layout_id) = object.layout_id else {
                return native_runtime_contract_violation();
            };
            ArrayCastSource::Object {
                owner: object.owner.clone(),
                layout_id,
            }
        }
        Some(NativeComparisonValue::OpaqueIdentity(_)) => {
            match fast.direct_slot(source).map(|(_, slot)| slot.kind) {
                Some(
                    php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER
                    | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_GENERATOR,
                ) => ArrayCastSource::Empty,
                Some(php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE) => ArrayCastSource::Scalar,
                _ => return native_runtime_contract_violation(),
            }
        }
        Some(
            NativeComparisonValue::Bool(_)
            | NativeComparisonValue::Int(_)
            | NativeComparisonValue::Float(_)
            | NativeComparisonValue::String(_)
            | NativeComparisonValue::Resource(_),
        ) => ArrayCastSource::Scalar,
        None => return native_runtime_contract_violation(),
    };

    let result = match source_kind {
        ArrayCastSource::Identity => {
            return match fast.retain_direct_encoded(source) {
                Ok(()) => php_jit::JitNativeControlResult::returning(source),
                Err(_) => php_jit::JitNativeControlResult::control(
                    php_jit::JitCallStatus::RUNTIME_ERROR,
                    0,
                    0,
                ),
            };
        }
        ArrayCastSource::Empty => fast.publish_retained_direct_array_from_iter(std::iter::empty()),
        ArrayCastSource::Scalar => fast.publish_retained_direct_array_from_iter(std::iter::once(
            php_jit::JitNativeDirectArrayEntry {
                key: 0,
                value: source,
            },
        )),
        ArrayCastSource::Object { owner, layout_id } => {
            let Some(properties) = owner.with_native_array_cast_view(
                layout_id,
                |declared_names, declared, dynamic_order, dynamic| {
                    declared_names
                        .iter()
                        .zip(declared)
                        .filter(|(_, slot)| slot.initialized != 0)
                        .map(|(name, slot)| (name.clone(), slot.value))
                        .chain(dynamic_order.iter().filter_map(|name| {
                            dynamic
                                .get(name)
                                .filter(|cell| cell.slot.initialized != 0)
                                .map(|cell| (name.clone(), cell.slot.value))
                        }))
                        .collect::<Vec<_>>()
                },
            ) else {
                return native_runtime_contract_violation();
            };
            publish_native_array_cast_entries(fast, properties)
        }
    };
    match result {
        Ok(array) => php_jit::JitNativeControlResult::returning(array),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Exact explicit integer cast for publication-admitted scalar and array
/// values. Object-like shapes are rejected before optimizer entry; strings
/// and every float payload use PHP's shared numeric conversion directly.
pub(crate) extern "C" fn jit_native_int_cast_abi(
    runtime: *mut NativeRequestFastState,
    source: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let value = match fast.native_comparison_value(source) {
        Some(NativeComparisonValue::Null) => 0,
        Some(NativeComparisonValue::Bool(value)) => i64::from(value),
        Some(NativeComparisonValue::Int(value)) => value,
        Some(NativeComparisonValue::Float(value)) => php_runtime::api::php_float_to_int(value),
        Some(NativeComparisonValue::String(bytes)) => {
            php_runtime::experimental::numeric_string::classify(bytes)
                .value
                .map_or(0, |value| value.to_i64())
        }
        Some(NativeComparisonValue::Array { entries, .. }) => i64::from(!entries.is_empty()),
        Some(NativeComparisonValue::Resource(value)) => value as i64,
        Some(NativeComparisonValue::Object(_) | NativeComparisonValue::OpaqueIdentity(_))
        | None => return native_runtime_contract_violation(),
    };
    match fast.publish_direct_int(value) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Exact explicit float cast for publication-admitted scalar and array values.
/// Object-like shapes are rejected before optimizer entry; native strings use
/// the shared numeric parser without reconstructing a runtime value.
pub(crate) extern "C" fn jit_native_float_cast_abi(
    runtime: *mut NativeRequestFastState,
    source: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let value = match fast.native_comparison_value(source) {
        Some(NativeComparisonValue::Null) => 0.0,
        Some(NativeComparisonValue::Bool(value)) => f64::from(u8::from(value)),
        Some(NativeComparisonValue::Int(value)) => value as f64,
        Some(NativeComparisonValue::Float(value)) => value,
        Some(NativeComparisonValue::String(bytes)) => {
            php_runtime::experimental::numeric_string::classify(bytes)
                .value
                .map_or(0.0, |value| value.as_f64())
        }
        Some(NativeComparisonValue::Array { entries, .. }) => {
            f64::from(u8::from(!entries.is_empty()))
        }
        Some(NativeComparisonValue::Resource(value)) => value as f64,
        Some(NativeComparisonValue::Object(_) | NativeComparisonValue::OpaqueIdentity(_))
        | None => return native_runtime_contract_violation(),
    };
    match fast.publish_direct_float(value) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Implements PHP's scalar `(string)` conversion family over authoritative
/// native values. Arrays and object-like values are assigned to baseline
/// before optimizer entry because their warning/`__toString` semantics require
/// the source span and full PHP call machinery.
pub(crate) extern "C" fn jit_native_string_cast_abi(
    runtime: *mut NativeRequestFastState,
    mut source: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    for _ in 0..16 {
        let Some((_, slot)) = fast.direct_slot(source) else {
            break;
        };
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            && native_reference_state(slot.reserved)
                != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
        {
            source = slot.payload as i64;
            continue;
        }
        if matches!(
            slot.kind,
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                | php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
        ) {
            return native_runtime_contract_violation();
        }
        break;
    }

    let rendered = match NativeConcatPart::from_value(fast, source) {
        Some(NativeConcatPart::Native { .. }) => {
            return match fast.retain_direct_encoded(source) {
                Ok(()) => php_jit::JitNativeControlResult::returning(source),
                Err(_) => php_jit::JitNativeControlResult::control(
                    php_jit::JitCallStatus::RUNTIME_ERROR,
                    0,
                    0,
                ),
            };
        }
        Some(NativeConcatPart::Inline(rendered)) => rendered,
        None => return native_runtime_contract_violation(),
    };
    match fast.publish_direct_string_with(rendered.length, |output| {
        output.copy_from_slice(rendered.as_bytes());
    }) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Coerces an already-returned scalar callback value to the exact replacement
/// string representation. This boundary deliberately has no baseline outcome:
/// replaying the containing operation would execute PHP-visible callback
/// effects twice. Admission proves a non-reference scalar return before the
/// callback is entered; a representation-contract violation is terminal.
pub(crate) extern "C" fn jit_native_callback_return_string_abi(
    runtime: *mut NativeRequestFastState,
    source: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let rendered = match NativeConcatPart::from_value(fast, source) {
        Some(NativeConcatPart::Native { .. }) => {
            return match fast.retain_direct_encoded(source) {
                Ok(()) => php_jit::JitNativeControlResult::returning(source),
                Err(_) => php_jit::JitNativeControlResult::control(
                    php_jit::JitCallStatus::RUNTIME_ERROR,
                    0,
                    0,
                ),
            };
        }
        Some(NativeConcatPart::Inline(rendered)) => rendered,
        None => {
            return php_jit::JitNativeControlResult::control(
                php_jit::JitCallStatus::RUNTIME_ERROR,
                0,
                0,
            );
        }
    };
    match fast.publish_direct_string_with(rendered.length, |output| {
        output.copy_from_slice(rendered.as_bytes());
    }) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

fn native_object_cast_stdclass() -> php_runtime::api::ObjectRef {
    static STDCLASS_NAME: std::sync::OnceLock<std::sync::Arc<str>> = std::sync::OnceLock::new();
    let class = php_runtime::api::ClassEntry {
        name: std::sync::Arc::clone(STDCLASS_NAME.get_or_init(|| std::sync::Arc::from("stdclass"))),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: php_runtime::api::ClassFlags {
            has_complete_method_table: true,
            ..php_runtime::api::ClassFlags::default()
        },
    };
    php_runtime::api::ObjectRef::from_layout_native_slots(&class, "stdClass", Box::new([]))
}

/// Implements PHP's complete `(object)` conversion family over authoritative
/// native values. Objects preserve identity, arrays become insertion-ordered
/// stdClass properties, null becomes an empty stdClass, and every other
/// admitted value is stored in the `scalar` property. No compatibility graph is
/// decoded or encoded on this path.
pub(crate) extern "C" fn jit_native_object_cast_abi(
    runtime: *mut NativeRequestFastState,
    mut source: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: generated code supplies the live request prefix and the exact
    // call is synchronous with the published native arenas.
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };

    for _ in 0..16 {
        let Some((_, slot)) = fast.direct_slot(source) else {
            break;
        };
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            && native_reference_state(slot.reserved)
                != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
        {
            source = slot.payload as i64;
            continue;
        }
        if matches!(
            slot.kind,
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                | php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
        ) {
            return native_runtime_contract_violation();
        }
        break;
    }

    enum ObjectCastSource {
        Identity,
        Empty,
        Scalar,
        Array {
            entries: *const php_jit::JitNativeDirectArrayEntry,
            length: usize,
        },
    }

    let source_kind = if let Some((entries, length)) = fast.stable_native_array_range(source) {
        ObjectCastSource::Array { entries, length }
    } else {
        match fast.native_comparison_value(source) {
            Some(NativeComparisonValue::Object(_)) => ObjectCastSource::Identity,
            Some(NativeComparisonValue::Null) => ObjectCastSource::Empty,
            Some(NativeComparisonValue::Array { .. }) => {
                return native_runtime_contract_violation();
            }
            Some(NativeComparisonValue::Float(value)) if value.is_nan() => {
                return native_runtime_contract_violation();
            }
            Some(
                NativeComparisonValue::Bool(_)
                | NativeComparisonValue::Int(_)
                | NativeComparisonValue::Float(_)
                | NativeComparisonValue::String(_)
                | NativeComparisonValue::OpaqueIdentity(_)
                | NativeComparisonValue::Resource(_),
            ) => ObjectCastSource::Scalar,
            None => return native_runtime_contract_violation(),
        }
    };

    if matches!(source_kind, ObjectCastSource::Identity) {
        return match fast.retain_direct_encoded(source) {
            Ok(()) => php_jit::JitNativeControlResult::returning(source),
            Err(_) => php_jit::JitNativeControlResult::control(
                php_jit::JitCallStatus::RUNTIME_ERROR,
                0,
                0,
            ),
        };
    }

    let object = native_object_cast_stdclass();
    let layout_id = object.class_layout_epoch();
    let mut retained = Vec::new();
    let mut properties = Vec::new();
    match source_kind {
        ObjectCastSource::Empty => {}
        ObjectCastSource::Scalar => properties.push(("scalar".to_owned(), source)),
        ObjectCastSource::Array { entries, length } => {
            let mut names = std::collections::HashSet::with_capacity(length);
            for index in 0..length {
                // SAFETY: the source owner remains live throughout this
                // synchronous cast and object allocation does not relocate
                // the request's stable native array arena.
                #[allow(unsafe_code)]
                let entry = unsafe { *entries.add(index) };
                let name = match fast.native_comparison_value(entry.key) {
                    Some(NativeComparisonValue::Int(key)) => key.to_string(),
                    Some(NativeComparisonValue::String(key)) => {
                        String::from_utf8_lossy(key).into_owned()
                    }
                    _ => return native_runtime_contract_violation(),
                };
                if !names.insert(name.clone()) {
                    return native_runtime_contract_violation();
                }
                properties.push((name, entry.value));
            }
        }
        ObjectCastSource::Identity => unreachable!("identity returned before allocation"),
    }

    for (name, value) in properties {
        if fast.retain_direct_encoded(value).is_err() {
            for retained_value in retained {
                fast.rollback_direct_retain(retained_value);
            }
            return php_jit::JitNativeControlResult::control(
                php_jit::JitCallStatus::RUNTIME_ERROR,
                0,
                0,
            );
        }
        retained.push(value);
        let slot = php_runtime::api::NativeDeclaredPropertySlot {
            initialized: 1,
            reserved: 0,
            value,
        };
        if object
            .set_native_dynamic_property(layout_id, name, slot)
            .is_err()
        {
            for retained_value in retained {
                fast.rollback_direct_retain(retained_value);
            }
            return php_jit::JitNativeControlResult::control(
                php_jit::JitCallStatus::RUNTIME_ERROR,
                0,
                0,
            );
        }
    }

    match fast.publish_direct_object(object) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            for retained_value in retained {
                fast.rollback_direct_retain(retained_value);
            }
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Allocates one object from a request-published immutable class layout. The
/// opaque plan pointer was resolved and validated before native execution;
/// this call performs no class lookup, flag check, or compatibility graph encode.
#[allow(unsafe_code)] // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
pub(crate) extern "C" fn jit_native_prepared_object_new_abi(
    runtime: *mut NativeRequestFastState,
    prepared: u64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: the active trusted-class table contains pointers owned by an Rc
    // in `runtime_class_cache`; request activation and scoped unit switching
    // keep that owner alive for this synchronous exact call.
    let prepared = unsafe { &*(prepared as usize as *const PreparedNativeRuntimeClass) };
    let result = (|| {
        let fast = unsafe { &mut *runtime };
        let slots = prepared.default_native_slots.clone();
        let mut retained = Vec::new();
        for slot in slots.iter().filter(|slot| slot.initialized != 0) {
            if let Err(error) = fast.retain_direct_encoded(slot.value) {
                for value in retained {
                    fast.rollback_direct_retain(value);
                }
                return Err(error);
            }
            retained.push(slot.value);
        }
        let object = php_runtime::api::ObjectRef::from_layout_native_slots(
            &prepared.entry,
            prepared.display_name.clone(),
            slots,
        );
        debug_assert_eq!(object.class_layout_epoch(), prepared.layout_id);
        match fast.publish_direct_object(object) {
            Ok(value) => Ok(value),
            Err(error) => {
                for value in retained {
                    fast.rollback_direct_retain(value);
                }
                Err(error)
            }
        }
    })();
    match result {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Constructs one internal throwable from immutable publication metadata and
/// authoritative native fields. The exact handler never recovers the cold
/// coordinator, decodes a compatibility graph, or resolves class/source metadata.
fn publish_prepared_throwable_trace(
    fast: &mut NativeRequestFastState,
    prepared: &PreparedNativeThrowableSite,
) -> Result<i64, &'static str> {
    if !prepared.include_function_frame {
        return fast.publish_owned_direct_array_from_iter(std::iter::empty());
    }

    let mut owned = Vec::with_capacity(4);
    let result = (|| {
        let function_key = fast.publish_direct_string_bytes(b"function")?;
        owned.push(function_key);
        let function = fast.publish_direct_string_bytes(prepared.function_name.as_bytes())?;
        owned.push(function);
        let arguments_key = fast.publish_direct_string_bytes(b"args")?;
        owned.push(arguments_key);
        let arguments = fast.native_func_get_args()?;
        owned.push(arguments);
        let entries = [
            php_jit::JitNativeDirectArrayEntry {
                key: function_key,
                value: function,
            },
            php_jit::JitNativeDirectArrayEntry {
                key: arguments_key,
                value: arguments,
            },
        ];
        // Ownership of every key/value moves into the frame on both success
        // and failure; `publish_owned_direct_array` performs rollback.
        owned.clear();
        let frame = fast.publish_owned_direct_array_from_iter(entries.into_iter())?;
        let entries = [php_jit::JitNativeDirectArrayEntry {
            key: 0,
            value: frame,
        }];
        fast.publish_owned_direct_array_from_iter(entries.into_iter())
    })();
    if result.is_err() {
        for value in owned.into_iter().rev() {
            let _ = fast.discard_owned_direct_value(value);
        }
    }
    result
}

#[allow(unsafe_code)] // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
pub(crate) extern "C" fn jit_native_prepared_exception_new_abi(
    runtime: *mut NativeRequestFastState,
    prepared: u64,
    message: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: publication owns the plan for the lifetime of the active unit
    // view. Optimizing lowering loads the pointer from the exact continuation
    // table and forwards it synchronously.
    let prepared = unsafe { &*(prepared as usize as *const PreparedNativeThrowableSite) };
    let fast = unsafe { &mut *runtime };
    let message = match fast.native_printf_scalar(message) {
        Some(php_runtime::api::NativePrintfScalar::String(bytes)) => bytes.to_vec(),
        Some(php_runtime::api::NativePrintfScalar::Int(value)) => value.to_string().into_bytes(),
        Some(php_runtime::api::NativePrintfScalar::Float(value)) => value.to_string().into_bytes(),
        Some(php_runtime::api::NativePrintfScalar::Bool(true)) => b"1".to_vec(),
        Some(
            php_runtime::api::NativePrintfScalar::Bool(false)
            | php_runtime::api::NativePrintfScalar::Null,
        ) => Vec::new(),
        None => return native_runtime_contract_violation(),
    };

    let mut owned = Vec::with_capacity(4);
    let result = (|| {
        let message = fast.publish_direct_string_bytes(&message)?;
        owned.push(message);
        let file = fast.publish_direct_string_bytes(&prepared.file)?;
        owned.push(file);
        let line = fast.publish_direct_int(prepared.line)?;
        if php_jit::jit_decode_runtime_value(line).is_some() {
            owned.push(line);
        }
        let trace = publish_prepared_throwable_trace(fast, prepared)?;
        owned.push(trace);

        let object = php_runtime::api::ObjectRef::from_layout_native_slots(
            &prepared.runtime_class,
            prepared.display_name.clone(),
            Box::new([]),
        );
        let layout_id = object.class_layout_epoch();
        let fields = [
            ("message", message),
            ("file", file),
            ("line", line),
            ("code", 0),
            ("previous", php_jit::jit_encode_constant(u32::MAX)),
            ("trace", trace),
        ];
        for (name, value) in fields {
            let slot = php_runtime::api::NativeDeclaredPropertySlot {
                initialized: 1,
                reserved: 0,
                value,
            };
            object
                .set_native_dynamic_property(layout_id, name.to_owned(), slot)
                .map_err(|_| "native throwable property publication failed")?;
        }
        fast.publish_direct_object(object)
    })();

    match result {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            for value in owned {
                let _ = fast.discard_owned_direct_value(value);
            }
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Allocates one closure from immutable callsite metadata and authoritative
/// native captures. Generated code has already applied by-value/by-reference
/// binding and transferred one owner per capture; this exact handler never
/// constructs a compatibility graph or enters the generic dynamic-code executor.
#[allow(unsafe_code)] // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
pub(crate) extern "C" fn jit_native_prepared_closure_new_abi(
    runtime: *mut NativeRequestFastState,
    prepared: u64,
    captures: *const i64,
    implicit_this: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: generated code loads this opaque pointer from the active
    // compiled unit's trusted closure-plan table.
    let prepared =
        unsafe { &*(prepared as usize as *const crate::compiled_unit::PreparedNativeClosureSite) };
    let fast = unsafe { &mut *runtime };
    let capture_values = if prepared.capture_descriptors.is_empty() {
        &[]
    } else {
        // SAFETY: lowering allocates one packed stack word per immutable
        // descriptor and keeps the stack slot live for this synchronous call.
        unsafe { std::slice::from_raw_parts(captures, prepared.capture_descriptors.len()) }
    };
    let Some(scope) = fast.current_execution_scope().cloned() else {
        for capture in capture_values.iter().copied() {
            fast.rollback_direct_retain(capture);
        }
        if prepared.binds_this {
            fast.rollback_direct_retain(implicit_this);
        }
        return php_jit::JitNativeControlResult::control(
            php_jit::JitCallStatus::ABI_MISMATCH,
            0,
            0,
        );
    };
    let scope_class = scope.scope_class;
    let context = php_runtime::api::ClosureContext {
        owner_unit: scope.unit,
        called_class: scope.called_class.or_else(|| scope_class.clone()),
        scope_class: scope_class.clone(),
        declaring_class: scope_class,
    };
    let closure = php_runtime::api::ClosurePayload::new(prepared.function.raw(), Vec::new())
        .with_debug(prepared.debug.clone())
        .with_context(context);
    let prepared_closure = NativePreparedClosure::new(
        closure,
        Arc::clone(&prepared.capture_descriptors),
        prepared.binds_this.then_some(implicit_this),
        capture_values.to_vec().into_boxed_slice(),
        prepared.fixed_visible_arity,
        prepared.first_parameter_by_reference,
        prepared.returns_int,
        prepared.returns_string,
        prepared.returns_releasable_scalar,
    );
    match fast.publish_prepared_closure_owned(prepared_closure) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Shallow-clones one direct object whose exact class was proven not to have
/// `__clone`; no runtime method lookup or compatibility decode occurs here.
pub(crate) extern "C" fn jit_native_plain_object_clone_abi(
    runtime: *mut NativeRequestFastState,
    object: i64,
) -> php_jit::JitNativeControlResult {
    enum PlainCloneOutcome {
        Returned(i64),
        ContractViolation,
        Error,
    }

    // SAFETY: the exact call executes synchronously with one published fast
    // state. All owner/slot pointers remain stable for its duration.
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let outcome = (|| {
        let fast = unsafe { &mut *runtime };
        let Some((_, descriptor)) = fast.direct_slot(object) else {
            return PlainCloneOutcome::Error;
        };
        let Some(object) = fast.direct_object(object).cloned() else {
            return PlainCloneOutcome::Error;
        };
        if !php_jit::jit_native_object_property_view_is_published(descriptor.flags) {
            return PlainCloneOutcome::ContractViolation;
        }
        let Some((slots, dynamic)) = object.clone_native_property_slots(descriptor.payload) else {
            return PlainCloneOutcome::ContractViolation;
        };
        let mut retained = Vec::new();
        for slot in slots.iter().filter(|slot| slot.initialized != 0).chain(
            dynamic
                .values()
                .filter(|cell| cell.slot.initialized != 0)
                .map(|cell| &cell.slot),
        ) {
            if fast.retain_direct_encoded(slot.value).is_err() {
                for value in retained {
                    fast.rollback_direct_retain(value);
                }
                return PlainCloneOutcome::Error;
            }
            retained.push(slot.value);
        }
        let clone = object.clone_shallow();
        if clone
            .install_native_property_slots(descriptor.payload, slots, dynamic)
            .is_err()
        {
            for value in retained {
                fast.rollback_direct_retain(value);
            }
            return PlainCloneOutcome::Error;
        }
        match fast.publish_direct_object(clone) {
            Ok(value) => PlainCloneOutcome::Returned(value),
            Err(_) => {
                for value in retained {
                    fast.rollback_direct_retain(value);
                }
                PlainCloneOutcome::Error
            }
        }
    })();
    match outcome {
        PlainCloneOutcome::Returned(value) => php_jit::JitNativeControlResult::returning(value),
        PlainCloneOutcome::ContractViolation => native_runtime_contract_violation(),
        PlainCloneOutcome::Error => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Resolves the stable authoritative cell for one undeclared property. A
/// missing stdClass name reserves an uninitialized tombstone; generated code
/// performs the complete operation directly on that cell.
pub(crate) extern "C" fn jit_native_dynamic_property_slot_abi(
    runtime: *mut NativeRequestFastState,
    object: i64,
    property: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(property) = fast.exact_dynamic_property_name(property) else {
        return native_runtime_contract_violation();
    };
    let Some(slot) = fast.exact_dynamic_property_slot_location(object, property) else {
        return native_runtime_contract_violation();
    };
    php_jit::JitNativeControlResult::returning(slot as usize as i64)
}

/// Resolves the stable authoritative cell for a non-mutating dynamic property
/// test. Missing ordinary names return the request's immutable absence cell;
/// declared visibility and `__isset` shapes take one baseline continuation.
pub(crate) extern "C" fn jit_native_dynamic_property_test_slot_abi(
    runtime: *mut NativeRequestFastState,
    object: i64,
    property: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(property) = fast.exact_dynamic_property_name(property) else {
        return native_runtime_contract_violation();
    };
    let property_pointer = property.as_ptr();
    let property_length = property.len();
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let property = unsafe {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(
            property_pointer,
            property_length,
        ))
    };
    let Some(slot) = fast.exact_dynamic_property_test_slot_location(object, property) else {
        return native_runtime_contract_violation();
    };
    php_jit::JitNativeControlResult::returning(slot as usize as i64)
}
