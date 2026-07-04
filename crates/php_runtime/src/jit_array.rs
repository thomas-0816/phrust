//! Read-only array helper surface for JIT fast paths.
//!
//! These helpers intentionally consume the public `PhpArray` facade instead of
//! exposing the backing storage. They are the only performance packed-array ABI
//! surface available to JIT code until a later safety audit allows anything
//! lower level.

use crate::{PhpArray, PhpArrayElementSummary, PhpArrayKind, Value};

/// Stable layout version for the performance read-only packed-array helper ABI.
pub const PHP_JIT_ARRAY_LAYOUT_VERSION: u64 = 0x0007_0014_0000_0001;

/// Helper status for a successful packed-array ABI operation.
pub const PHP_JIT_ARRAY_STATUS_OK: i32 = 0;
/// Helper status for any guard miss that must fall back to the interpreter.
pub const PHP_JIT_ARRAY_STATUS_FALLBACK: i32 = 1;
/// Helper status for an integer index outside the packed array length.
pub const PHP_JIT_ARRAY_STATUS_BOUNDS_EXIT: i32 = 2;
/// Helper status for layout, alias, or element-shape guard failure.
pub const PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT: i32 = 3;
/// Helper status for a record-shape lookup whose key symbol has no slot.
pub const PHP_JIT_ARRAY_STATUS_KEY_MISS_EXIT: i32 = 4;

/// Conservative failure reason for packed-array helper guards.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhpJitArrayAbiError {
    /// The value is not an array.
    NotArray,
    /// The array is not currently proven to have contiguous integer keys.
    NotPacked,
    /// The array contains reference cells.
    AliasedOrReferenced,
    /// At least one element is not an integer.
    NonIntElement,
    /// The requested index is outside the packed array length.
    OutOfBounds,
}

impl PhpJitArrayAbiError {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotArray => "not_array",
            Self::NotPacked => "not_packed",
            Self::AliasedOrReferenced => "aliased_or_referenced",
            Self::NonIntElement => "non_int_element",
            Self::OutOfBounds => "out_of_bounds",
        }
    }
}

/// Returns true when the caller and runtime agree on the array helper layout.
#[must_use]
pub const fn php_jit_array_layout_guard(version: u64) -> bool {
    version == PHP_JIT_ARRAY_LAYOUT_VERSION
}

/// Conservative read-only packed-int guard.
///
/// This rejects reference elements even though ordinary reads could observe
/// them safely today. Shared COW storage is accepted because the helper performs
/// a read-only fetch and does not expose mutable storage to generated code.
#[must_use]
pub fn php_jit_array_is_packed_ints(value: &Value) -> i32 {
    if validate_packed_int_array(value).is_ok() {
        PHP_JIT_ARRAY_STATUS_OK
    } else {
        PHP_JIT_ARRAY_STATUS_FALLBACK
    }
}

/// Returns the packed array length after the performance layout and alias guards.
pub fn php_jit_array_len(value: &Value, out: &mut usize) -> i32 {
    match php_jit_array_len_result(value) {
        Ok(length) => {
            *out = length;
            PHP_JIT_ARRAY_STATUS_OK
        }
        Err(_) => PHP_JIT_ARRAY_STATUS_FALLBACK,
    }
}

fn php_jit_array_len_result(value: &Value) -> Result<usize, PhpJitArrayAbiError> {
    let array = validate_packed_int_array(value)?;
    Ok(array.packed_len_fast().expect("validated packed array"))
}

/// Record-shaped array lookup with an interned string-key symbol guard.
///
/// Guards: the value must be a record-storage array, the key a string whose
/// symbol resolves to a shape slot, and the slot must not hold a reference
/// cell (read-only region restriction). Guard misses report the exact side
/// exit so the caller can fall back to the interpreter.
pub fn php_jit_record_array_lookup(array: &Value, key: &Value) -> Result<Value, i32> {
    let array = match array {
        Value::Reference(cell) => cell.get(),
        other => other.clone(),
    };
    let Value::Array(array) = array else {
        return Err(PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT);
    };
    let key = match key {
        Value::Reference(cell) => cell.get(),
        other => other.clone(),
    };
    let Value::String(key) = key else {
        return Err(PHP_JIT_ARRAY_STATUS_FALLBACK);
    };
    if array.record_slot_for_symbol(&key).is_none() {
        // Distinguish a shape/storage miss from a genuine key miss so side
        // exits blame the right guard.
        return match array.get(&crate::ArrayKey::String(key.clone())) {
            _ if !array.is_record_storage() => Err(PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT),
            _ => Err(PHP_JIT_ARRAY_STATUS_KEY_MISS_EXIT),
        };
    }
    let Some(value) = array.record_get_symbol(&key) else {
        return Err(PHP_JIT_ARRAY_STATUS_KEY_MISS_EXIT);
    };
    if matches!(value, Value::Reference(_)) {
        return Err(PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT);
    }
    Ok(value.clone())
}

/// Fetches an integer element through the safe runtime facade.
///
/// This helper is intentionally named `slow`: it is the ABI fallback that keeps
/// direct Rust `Vec` access out of JIT-generated code.
pub fn php_jit_array_fetch_int_slow(value: &Value, index: usize, out: &mut i64) -> i32 {
    match php_jit_array_fetch_int_slow_result(value, index) {
        Ok(value) => {
            *out = value;
            PHP_JIT_ARRAY_STATUS_OK
        }
        Err(error) => array_error_status(error),
    }
}

fn php_jit_array_fetch_int_slow_result(
    value: &Value,
    index: usize,
) -> Result<i64, PhpJitArrayAbiError> {
    let array = validate_packed_int_array(value)?;
    match array.packed_element_fast(index) {
        Some(Value::Int(value)) => Ok(*value),
        Some(_) => Err(PhpJitArrayAbiError::NonIntElement),
        None => Err(PhpJitArrayAbiError::OutOfBounds),
    }
}

fn validate_packed_int_array(value: &Value) -> Result<&PhpArray, PhpJitArrayAbiError> {
    let Value::Array(array) = value else {
        return Err(PhpJitArrayAbiError::NotArray);
    };
    let metadata = array.packed_metadata();
    if metadata.kind != PhpArrayKind::PackedList {
        return Err(PhpJitArrayAbiError::NotPacked);
    }
    if metadata.contains_references {
        return Err(PhpJitArrayAbiError::AliasedOrReferenced);
    }
    if metadata.element_summary == PhpArrayElementSummary::Mixed {
        return Err(PhpJitArrayAbiError::NonIntElement);
    }
    Ok(array)
}

const fn array_error_status(error: PhpJitArrayAbiError) -> i32 {
    match error {
        PhpJitArrayAbiError::OutOfBounds => PHP_JIT_ARRAY_STATUS_BOUNDS_EXIT,
        PhpJitArrayAbiError::NotArray
        | PhpJitArrayAbiError::NotPacked
        | PhpJitArrayAbiError::AliasedOrReferenced
        | PhpJitArrayAbiError::NonIntElement => PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PHP_JIT_ARRAY_LAYOUT_VERSION, PHP_JIT_ARRAY_STATUS_BOUNDS_EXIT,
        PHP_JIT_ARRAY_STATUS_FALLBACK, PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT, PHP_JIT_ARRAY_STATUS_OK,
        PhpJitArrayAbiError, php_jit_array_fetch_int_slow, php_jit_array_fetch_int_slow_result,
        php_jit_array_is_packed_ints, php_jit_array_layout_guard, php_jit_array_len,
        php_jit_array_len_result,
    };
    use crate::{ArrayKey, PhpArray, PhpString, ReferenceCell, Value};

    #[test]
    fn layout_guard_accepts_only_current_version() {
        assert!(php_jit_array_layout_guard(PHP_JIT_ARRAY_LAYOUT_VERSION));
        assert!(!php_jit_array_layout_guard(
            PHP_JIT_ARRAY_LAYOUT_VERSION + 1
        ));
    }

    #[test]
    fn packed_int_helpers_accept_read_only_int_arrays() {
        let value = Value::packed_array(vec![Value::Int(4), Value::Int(8)]);
        let mut length = 0;
        let mut fetched = 0;

        assert_eq!(
            php_jit_array_is_packed_ints(&value),
            PHP_JIT_ARRAY_STATUS_OK
        );
        assert_eq!(
            php_jit_array_len(&value, &mut length),
            PHP_JIT_ARRAY_STATUS_OK
        );
        assert_eq!(length, 2);
        assert_eq!(
            php_jit_array_fetch_int_slow(&value, 0, &mut fetched),
            PHP_JIT_ARRAY_STATUS_OK
        );
        assert_eq!(fetched, 4);
        assert_eq!(
            php_jit_array_fetch_int_slow(&value, 1, &mut fetched),
            PHP_JIT_ARRAY_STATUS_OK
        );
        assert_eq!(fetched, 8);
        assert_eq!(
            php_jit_array_fetch_int_slow(&value, 2, &mut fetched),
            PHP_JIT_ARRAY_STATUS_BOUNDS_EXIT
        );
        assert_eq!(
            php_jit_array_fetch_int_slow_result(&value, 2),
            Err(PhpJitArrayAbiError::OutOfBounds)
        );
    }

    #[test]
    fn packed_int_helpers_reject_mixed_and_non_int_arrays() {
        let mut mixed = PhpArray::from_packed(vec![Value::Int(1), Value::Int(2)]);
        mixed.insert(
            ArrayKey::String(PhpString::from_test_str("name")),
            Value::Int(3),
        );
        let mixed = Value::Array(mixed);
        let mut length = 0;
        assert_eq!(
            php_jit_array_is_packed_ints(&mixed),
            PHP_JIT_ARRAY_STATUS_FALLBACK
        );
        assert_eq!(
            php_jit_array_len(&mixed, &mut length),
            PHP_JIT_ARRAY_STATUS_FALLBACK
        );
        assert_eq!(
            php_jit_array_len_result(&mixed),
            Err(PhpJitArrayAbiError::NotPacked)
        );

        let non_int = Value::packed_array(vec![Value::Int(1), Value::string("two")]);
        let mut fetched = 0;
        assert_eq!(
            php_jit_array_is_packed_ints(&non_int),
            PHP_JIT_ARRAY_STATUS_FALLBACK
        );
        assert_eq!(
            php_jit_array_fetch_int_slow(&non_int, 1, &mut fetched),
            PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT
        );
        assert_eq!(
            php_jit_array_fetch_int_slow_result(&non_int, 1),
            Err(PhpJitArrayAbiError::NonIntElement)
        );
    }

    #[test]
    fn packed_int_helpers_accept_shared_read_only_arrays_but_reject_references() {
        let array = PhpArray::from_packed(vec![Value::Int(1)]);
        let shared = array.clone();
        let value = Value::Array(array);
        let mut length = 0;
        assert!(shared.is_shared());
        assert_eq!(
            php_jit_array_is_packed_ints(&value),
            PHP_JIT_ARRAY_STATUS_OK
        );
        assert_eq!(
            php_jit_array_len(&value, &mut length),
            PHP_JIT_ARRAY_STATUS_OK
        );
        assert_eq!(length, 1);

        let referenced =
            Value::packed_array(vec![Value::Reference(ReferenceCell::new(Value::Int(2)))]);
        let mut fetched = 0;
        assert_eq!(
            php_jit_array_is_packed_ints(&referenced),
            PHP_JIT_ARRAY_STATUS_FALLBACK
        );
        assert_eq!(
            php_jit_array_fetch_int_slow(&referenced, 0, &mut fetched),
            PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT
        );
        assert_eq!(
            php_jit_array_fetch_int_slow_result(&referenced, 0),
            Err(PhpJitArrayAbiError::AliasedOrReferenced)
        );
    }
}
