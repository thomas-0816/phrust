//! Scalar conversion and comparison helpers for runtime/5 execution.

use crate::{
    PhpArray, PhpString, Value,
    numeric_string::{NumericStringKind, NumericStringValue, classify_php_string},
};
use std::cell::Cell;
use std::cmp::Ordering;

const DEFAULT_FLOAT_STRING_PRECISION: i32 = 14;

thread_local! {
    static FLOAT_STRING_PRECISION: Cell<i32> = const { Cell::new(DEFAULT_FLOAT_STRING_PRECISION) };
}

/// Numeric scalar produced by PHP-style scalar conversion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NumericValue {
    /// Integer value.
    Int(i64),
    /// Floating-point value.
    Float(f64),
}

/// Numeric operand conversion result for arithmetic operators.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArithmeticNumber {
    /// Converted numeric value.
    pub value: NumericValue,
    /// True when the operand was a leading numeric string and PHP must emit
    /// "A non-numeric value encountered" while still using the numeric prefix.
    pub leading_numeric_string: bool,
}

/// Resets request-local float-to-string precision to PHP's default.
pub fn reset_float_string_precision() {
    set_float_string_precision(DEFAULT_FLOAT_STRING_PRECISION);
}

/// Sets request-local float-to-string precision for INI-driven VM execution.
pub fn set_float_string_precision(precision: i32) {
    FLOAT_STRING_PRECISION.with(|cell| cell.set(precision.clamp(-1, 17)));
}

impl NumericValue {
    /// Returns the value as an `f64`.
    #[must_use]
    pub const fn as_f64(self) -> f64 {
        match self {
            Self::Int(value) => value as f64,
            Self::Float(value) => value,
        }
    }

    /// Returns true when this value is represented as a float.
    #[must_use]
    pub const fn is_float(self) -> bool {
        matches!(self, Self::Float(_))
    }
}

/// Converts a scalar to PHP truthiness for the documented MVP subset.
pub fn to_bool(value: &Value) -> Result<bool, String> {
    match value {
        Value::Null => Ok(false),
        Value::Bool(value) => Ok(*value),
        Value::Int(value) => Ok(*value != 0),
        Value::Float(value) => {
            // NAN is truthy in PHP: only exactly 0.0/-0.0 convert to false.
            let value = value.to_f64();
            Ok(value != 0.0)
        }
        Value::String(value) => Ok(!value.is_empty() && value.as_bytes() != b"0"),
        Value::Uninitialized => Err("cannot convert uninitialized value to bool".to_owned()),
        Value::Array(array) => Ok(!array.is_empty()),
        Value::Object(_) | Value::Resource(_) | Value::Fiber(_) | Value::Generator(_) => Ok(true),
        Value::Callable(_) => Err("callable truthiness is not implemented".to_owned()),
        Value::Reference(cell) => to_bool(&cell.borrow()),
    }
}

/// Converts a scalar to PHP string bytes for the documented MVP subset.
pub fn to_string(value: &Value) -> Result<PhpString, String> {
    match value {
        Value::Null | Value::Bool(false) => Ok(PhpString::from_bytes(Vec::new())),
        Value::Bool(true) => Ok(PhpString::from_test_str("1")),
        Value::Int(value) => Ok(PhpString::from_test_str(&value.to_string())),
        Value::Float(value) => Ok(PhpString::from_test_str(&float_to_php_string(
            value.to_f64(),
        ))),
        Value::String(value) => Ok(value.clone()),
        Value::Uninitialized => Err("cannot convert uninitialized value to string".to_owned()),
        Value::Array(_) => Err(
            "E_PHP_RUNTIME_ARRAY_TO_STRING_GAP: array to string conversion is not implemented"
                .to_owned(),
        ),
        Value::Object(_) | Value::Fiber(_) | Value::Generator(_) => Err(
            "E_PHP_RUNTIME_OBJECT_TO_STRING_GAP: object __toString conversion is not implemented"
                .to_owned(),
        ),
        Value::Resource(resource) => Ok(PhpString::from_test_str(&format!(
            "Resource id #{}",
            resource.id().get()
        ))),
        Value::Callable(_) => Err("callable to string conversion is not implemented".to_owned()),
        Value::Reference(cell) => to_string(&cell.borrow()),
    }
}

pub(crate) fn float_to_php_string(value: f64) -> String {
    if value.is_nan() {
        "NAN".to_owned()
    } else if value.is_infinite() {
        if value.is_sign_negative() {
            "-INF".to_owned()
        } else {
            "INF".to_owned()
        }
    } else if FLOAT_STRING_PRECISION.with(Cell::get) == 0 {
        format!("{value:.0}")
    } else if FLOAT_STRING_PRECISION.with(Cell::get) == -1 {
        value.to_string()
    } else if value != 0.0 {
        let abs = value.abs();
        if !(1e-4..1e14).contains(&abs) {
            return php_scientific_float_string(value);
        }
        php_decimal_float_string(value)
    } else if value.is_sign_negative() {
        "-0".to_owned()
    } else {
        "0".to_owned()
    }
}

fn php_decimal_float_string(value: f64) -> String {
    let precision = FLOAT_STRING_PRECISION.with(Cell::get).clamp(1, 17) as usize;
    let abs = value.abs();
    let integer_digits = if abs >= 1.0 {
        abs.log10().floor() as usize + 1
    } else {
        0
    };
    let leading_fractional_zeros = if abs > 0.0 && abs < 1.0 {
        (-abs.log10().floor() as usize).saturating_sub(1)
    } else {
        0
    };
    let decimals = precision
        .saturating_sub(integer_digits)
        .saturating_add(leading_fractional_zeros);
    let mut output = format!("{value:.decimals$}");
    if output == "-0" {
        return "0".to_owned();
    }
    if output.contains('.') {
        while output.ends_with('0') {
            output.pop();
        }
        if output.ends_with('.') {
            output.pop();
        }
    }
    if output == "-0" {
        "0".to_owned()
    } else {
        output
    }
}

fn php_scientific_float_string(value: f64) -> String {
    let precision = FLOAT_STRING_PRECISION.with(Cell::get).clamp(1, 17) as usize;
    let decimals = precision.saturating_sub(1);
    let mut output = format!("{value:.decimals$E}");
    if let Some(exponent_index) = output.find('E') {
        let mut mantissa = output[..exponent_index].to_owned();
        let exponent = &output[exponent_index + 1..];
        while mantissa.ends_with('0') {
            mantissa.pop();
        }
        if mantissa.ends_with('.') {
            mantissa.push('0');
        }
        let sign = exponent
            .strip_prefix('+')
            .map(|digits| ("+", digits))
            .or_else(|| exponent.strip_prefix('-').map(|digits| ("-", digits)))
            .unwrap_or(("+", exponent));
        let digits = sign.1.trim_start_matches('0');
        output = format!(
            "{}E{}{}",
            mantissa,
            sign.0,
            if digits.is_empty() { "0" } else { digits }
        );
    }
    output
}

/// Whether a float is exactly representable in the PHP int domain, mirroring
/// ZEND_DOUBLE_FITS_LONG (finite and within [i64::MIN, i64::MAX + 1)).
#[must_use]
pub fn float_fits_int(value: f64) -> bool {
    let min = i64::MIN as f64;
    value.is_finite() && value >= min && value < -min
}

/// Converts a float to a PHP int with zend_dval_to_lval semantics: non-finite
/// values become 0 and out-of-range values reduce modulo 2^64 into the i64
/// domain instead of saturating.
#[must_use]
pub fn php_float_to_int(value: f64) -> i64 {
    if float_fits_int(value) {
        return value as i64;
    }
    if !value.is_finite() {
        return 0;
    }
    let two_pow_64 = 2.0_f64.powi(64);
    let mut modulus = value % two_pow_64;
    if modulus < 0.0 {
        modulus += two_pow_64;
    }
    if modulus >= -(i64::MIN as f64) {
        modulus -= two_pow_64;
    }
    modulus as i64
}

/// Converts a value to an integer using explicit PHP cast rules in the subset.
pub fn to_int(value: &Value) -> Result<i64, String> {
    match value {
        Value::Null | Value::Bool(false) => Ok(0),
        Value::Bool(true) => Ok(1),
        Value::Int(value) => Ok(*value),
        Value::Float(value) => Ok(php_float_to_int(value.to_f64())),
        Value::String(value) => Ok(classify_php_string(value)
            .value
            .map_or(0, NumericStringValue::to_i64)),
        Value::Uninitialized => Err("cannot convert uninitialized value to int".to_owned()),
        Value::Array(array) => Ok(if array.is_empty() { 0 } else { 1 }),
        Value::Object(_) | Value::Fiber(_) | Value::Generator(_) => {
            Err("E_PHP_RUNTIME_OBJECT_NUMERIC_CONVERSION_GAP: object to int conversion is not implemented".to_owned())
        }
        Value::Resource(resource) => Ok(resource.id().get() as i64),
        Value::Callable(_) => Err("callable to int conversion is not implemented".to_owned()),
        Value::Reference(cell) => to_int(&cell.borrow()),
    }
}

/// Converts a value to a float using explicit PHP cast rules in the subset.
pub fn to_float(value: &Value) -> Result<f64, String> {
    match value {
        Value::Null | Value::Bool(false) => Ok(0.0),
        Value::Bool(true) => Ok(1.0),
        Value::Int(value) => Ok(*value as f64),
        Value::Float(value) => Ok(value.to_f64()),
        Value::String(value) => Ok(classify_php_string(value)
            .value
            .map_or(0.0, NumericStringValue::as_f64)),
        Value::Uninitialized => Err("cannot convert uninitialized value to float".to_owned()),
        Value::Array(array) => Ok(if array.is_empty() { 0.0 } else { 1.0 }),
        Value::Object(_) | Value::Fiber(_) | Value::Generator(_) => {
            Err("E_PHP_RUNTIME_OBJECT_NUMERIC_CONVERSION_GAP: object to float conversion is not implemented".to_owned())
        }
        Value::Resource(resource) => Ok(resource.id().get() as f64),
        Value::Callable(_) => Err("callable to float conversion is not implemented".to_owned()),
        Value::Reference(cell) => to_float(&cell.borrow()),
    }
}

/// Converts a value to a PHP numeric value for arithmetic operators.
pub fn to_number(value: &Value) -> Result<NumericValue, String> {
    match value {
        Value::Null | Value::Bool(false) => Ok(NumericValue::Int(0)),
        Value::Bool(true) => Ok(NumericValue::Int(1)),
        Value::Int(value) => Ok(NumericValue::Int(*value)),
        Value::Float(value) => Ok(NumericValue::Float(value.to_f64())),
        Value::String(value) => arithmetic_numeric_string(value),
        Value::Uninitialized => Err("cannot convert uninitialized value to number".to_owned()),
        Value::Array(_) => Err("array to number conversion is not implemented".to_owned()),
        Value::Object(_) | Value::Fiber(_) | Value::Generator(_) => {
            Err("E_PHP_RUNTIME_OBJECT_NUMERIC_CONVERSION_GAP: object to number conversion is not implemented".to_owned())
        }
        Value::Resource(resource) => Ok(NumericValue::Int(resource.id().get() as i64)),
        Value::Callable(_) => Err("callable to number conversion is not implemented".to_owned()),
        Value::Reference(cell) => to_number(&cell.borrow()),
    }
}

/// Converts a value to a PHP numeric arithmetic operand and preserves whether
/// a leading numeric string warning is required.
pub fn to_arithmetic_number(value: &Value) -> Result<ArithmeticNumber, String> {
    match value {
        Value::String(value) => arithmetic_numeric_string_with_warning(value),
        Value::Reference(cell) => to_arithmetic_number(&cell.borrow()),
        _ => to_number(value).map(|value| ArithmeticNumber {
            value,
            leading_numeric_string: false,
        }),
    }
}

/// Converts a value to PHP truthiness.
pub fn to_bool_php(value: &Value) -> Result<bool, String> {
    to_bool(value)
}

/// Converts a value to PHP string bytes for currently supported runtime values.
pub fn to_string_php(value: &Value) -> Result<PhpString, String> {
    to_string(value)
}

/// Converts a value to a PHP integer cast result.
pub fn to_int_php(value: &Value) -> Result<i64, String> {
    to_int(value)
}

/// Converts a value to a PHP float cast result.
pub fn to_float_php(value: &Value) -> Result<f64, String> {
    to_float(value)
}

/// Converts a value to a PHP numeric value for arithmetic operators.
pub fn to_number_php(value: &Value) -> Result<NumericValue, String> {
    to_number(value)
}

/// Converts a value to a PHP numeric arithmetic operand and warning flag.
pub fn to_arithmetic_number_php(value: &Value) -> Result<ArithmeticNumber, String> {
    to_arithmetic_number(value)
}

/// Converts a value to a PHP array cast result for currently supported values.
pub fn to_array_php(value: &Value) -> Result<PhpArray, String> {
    match value {
        Value::Null => Ok(PhpArray::new()),
        Value::Array(array) => Ok(array.clone()),
        Value::Reference(cell) => to_array_php(&cell.borrow()),
        Value::Object(_) | Value::Fiber(_) | Value::Generator(_) => Err(
            "E_PHP_RUNTIME_OBJECT_TO_ARRAY_GAP: object to array conversion is not implemented"
                .to_owned(),
        ),
        Value::Uninitialized => Err("cannot convert uninitialized value to array".to_owned()),
        Value::Callable(_) => Err("callable to array conversion is not implemented".to_owned()),
        scalar => {
            let mut array = PhpArray::new();
            array.append(scalar.clone());
            Ok(array)
        }
    }
}

/// Object casts are not implemented in the runtime value model yet.
pub fn to_object_php(value: &Value) -> Result<Value, String> {
    match value {
        Value::Object(_) => Ok(value.clone()),
        Value::Reference(cell) => to_object_php(&cell.borrow()),
        _ => Err(
            "E_PHP_RUNTIME_OBJECT_CAST_GAP: object cast conversion is not implemented".to_owned(),
        ),
    }
}

/// Strict identity for runtime-semantics runtime values.
pub fn identical(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(left), Value::Bool(right)) => left == right,
        (Value::Int(left), Value::Int(right)) => left == right,
        (Value::Float(left), Value::Float(right)) => left.to_f64() == right.to_f64(),
        (Value::String(left), Value::String(right)) => left == right,
        (Value::Array(left), Value::Array(right)) => arrays_identical(left, right),
        (Value::Object(left), Value::Object(right)) => left.id() == right.id(),
        (Value::Resource(left), Value::Resource(right)) => left.id() == right.id(),
        (Value::Callable(left), Value::Callable(right)) => left == right,
        (Value::Reference(left), Value::Reference(right)) if left.ptr_eq(right) => true,
        (Value::Reference(left), right) => identical(&left.get(), right),
        (left, Value::Reference(right)) => identical(left, &right.get()),
        _ => false,
    }
}

/// Loose equality for runtime-semantics comparison cases.
pub fn equal(left: &Value, right: &Value) -> Result<bool, String> {
    match (left, right) {
        (Value::Array(left), Value::Array(right)) => arrays_equal(left, right),
        (Value::Array(_), _) | (_, Value::Array(_)) => Ok(false),
        (Value::Object(left), Value::Object(right)) => objects_equal(left, right),
        (Value::Object(_), _) | (_, Value::Object(_)) => Ok(false),
        (Value::Resource(left), Value::Resource(right)) => Ok(left.id() == right.id()),
        (Value::Resource(_), _) | (_, Value::Resource(_)) => Ok(false),
        (Value::Reference(left), Value::Reference(right)) if left.ptr_eq(right) => Ok(true),
        (Value::Reference(left), right) => equal(&left.get(), right),
        (left, Value::Reference(right)) => equal(left, &right.get()),
        _ => Ok(compare(left, right)? == Ordering::Equal),
    }
}

/// Strict identity using PHP runtime semantics.
pub fn identical_php(left: &Value, right: &Value) -> bool {
    identical(left, right)
}

/// Loose equality using PHP runtime semantics.
pub fn equal_php(left: &Value, right: &Value) -> Result<bool, String> {
    equal(left, right)
}

/// Loose comparison for runtime-semantics comparison cases.
pub fn compare(left: &Value, right: &Value) -> Result<Ordering, String> {
    match (left, right) {
        (Value::Reference(left), Value::Reference(right)) if left.ptr_eq(right) => {
            return Ok(Ordering::Equal);
        }
        (Value::Reference(left), right) => return compare(&left.get(), right),
        (left, Value::Reference(right)) => return compare(left, &right.get()),
        (Value::String(left), Value::String(right)) => {
            if let (Some(left), Some(right)) = (
                comparison_numeric_string(left),
                comparison_numeric_string(right),
            ) {
                return compare_numbers(left, right);
            }
            return Ok(left.as_bytes().cmp(right.as_bytes()));
        }
        (Value::Bool(_), _) | (_, Value::Bool(_)) | (Value::Null, _) | (_, Value::Null) => {
            return Ok(to_bool(left)?.cmp(&to_bool(right)?));
        }
        _ => {}
    }

    match (left, right) {
        (Value::Int(_) | Value::Float(_), Value::Int(_) | Value::Float(_)) => {
            compare_numbers(to_number(left)?, to_number(right)?)
        }
        (Value::String(_), Value::Int(_) | Value::Float(_))
        | (Value::Int(_) | Value::Float(_), Value::String(_)) => {
            compare_number_and_string(left, right)
        }
        (Value::String(left), Value::String(right)) => Ok(left.as_bytes().cmp(right.as_bytes())),
        (Value::Array(left), Value::Array(right)) => arrays_compare(left, right),
        (Value::Array(_), _) => Ok(Ordering::Greater),
        (_, Value::Array(_)) => Ok(Ordering::Less),
        (Value::Object(left), Value::Object(right)) => objects_compare(left, right),
        (Value::Object(_), _) => Ok(Ordering::Greater),
        (_, Value::Object(_)) => Ok(Ordering::Less),
        (Value::Resource(left), Value::Resource(right)) => Ok(left.id().cmp(&right.id())),
        (Value::Resource(_), _) => Ok(Ordering::Greater),
        (_, Value::Resource(_)) => Ok(Ordering::Less),
        _ => Err(format!(
            "loose comparison is not implemented for {} and {}",
            type_name(left),
            type_name(right)
        )),
    }
}

/// Loose ordering using PHP runtime semantics.
pub fn compare_php(left: &Value, right: &Value) -> Result<Ordering, String> {
    compare(left, right)
}

fn compare_numbers(left: NumericValue, right: NumericValue) -> Result<Ordering, String> {
    if left.as_f64().is_nan() || right.as_f64().is_nan() {
        return Ok(Ordering::Greater);
    }
    let Some(ordering) = left.as_f64().partial_cmp(&right.as_f64()) else {
        return Err("cannot compare NaN numeric values".to_owned());
    };
    Ok(ordering)
}

fn compare_number_and_string(left: &Value, right: &Value) -> Result<Ordering, String> {
    match (left, right) {
        (Value::String(string), number) => {
            if let Some(string) = comparison_numeric_string(string) {
                return compare_numbers(string, to_number(number)?);
            }
            if is_nan_number(number) {
                return Ok(Ordering::Greater);
            }
            Ok(string.as_bytes().cmp(to_string(number)?.as_bytes()))
        }
        (number, Value::String(string)) => {
            if let Some(string) = comparison_numeric_string(string) {
                return compare_numbers(to_number(number)?, string);
            }
            if is_nan_number(number) {
                return Ok(Ordering::Less);
            }
            Ok(to_string(number)?.as_bytes().cmp(string.as_bytes()))
        }
        _ => unreachable!("compare_number_and_string requires one string and one number"),
    }
}

fn is_nan_number(value: &Value) -> bool {
    matches!(value, Value::Float(value) if value.to_f64().is_nan())
}

fn comparison_numeric_string(value: &PhpString) -> Option<NumericValue> {
    let classified = classify_php_string(value);
    match (classified.kind, classified.value) {
        (
            NumericStringKind::IntString | NumericStringKind::FloatString,
            Some(NumericStringValue::Int(value)),
        ) => Some(NumericValue::Int(value)),
        (
            NumericStringKind::IntString | NumericStringKind::FloatString,
            Some(NumericStringValue::Float(value)),
        ) => Some(NumericValue::Float(value)),
        _ => None,
    }
}

fn arithmetic_numeric_string(value: &PhpString) -> Result<NumericValue, String> {
    arithmetic_numeric_string_with_warning(value).map(|number| number.value)
}

fn arithmetic_numeric_string_with_warning(value: &PhpString) -> Result<ArithmeticNumber, String> {
    let classified = classify_php_string(value);
    match (classified.kind, classified.value) {
        (
            NumericStringKind::IntString
            | NumericStringKind::FloatString
            | NumericStringKind::LeadingNumeric,
            Some(NumericStringValue::Int(value)),
        ) => Ok(ArithmeticNumber {
            value: NumericValue::Int(value),
            leading_numeric_string: classified.kind == NumericStringKind::LeadingNumeric,
        }),
        (
            NumericStringKind::IntString
            | NumericStringKind::FloatString
            | NumericStringKind::LeadingNumeric,
            Some(NumericStringValue::Float(value)),
        ) => Ok(ArithmeticNumber {
            value: NumericValue::Float(value),
            leading_numeric_string: classified.kind == NumericStringKind::LeadingNumeric,
        }),
        _ => Err(
            "E_PHP_RUNTIME_NON_NUMERIC_STRING: non-numeric string cannot be used as a number"
                .to_owned(),
        ),
    }
}

fn arrays_identical(left: &crate::PhpArray, right: &crate::PhpArray) -> bool {
    left.len() == right.len()
        && left.iter().zip(right.iter()).all(
            |((left_key, left_value), (right_key, right_value))| {
                left_key == right_key && identical(left_value, right_value)
            },
        )
}

fn arrays_equal(left: &crate::PhpArray, right: &crate::PhpArray) -> Result<bool, String> {
    if left.len() != right.len() {
        return Ok(false);
    }
    for (left_key, left_value) in left.iter() {
        let Some(right_value) = right.get(&left_key) else {
            return Ok(false);
        };
        if !equal(left_value, right_value)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn arrays_compare(left: &crate::PhpArray, right: &crate::PhpArray) -> Result<Ordering, String> {
    match left.len().cmp(&right.len()) {
        Ordering::Equal => {}
        ordering => return Ok(ordering),
    }
    for (left_key, left_value) in left.iter() {
        let Some(right_value) = right.get(&left_key) else {
            let right_key = right
                .iter()
                .map(|(key, _)| key)
                .next()
                .expect("equal length arrays are not empty when a key is missing");
            return Ok(compare_array_keys(&left_key, &right_key));
        };
        let ordering = compare(left_value, right_value)?;
        if ordering != Ordering::Equal {
            return Ok(ordering);
        }
    }
    Ok(Ordering::Equal)
}

fn compare_array_keys(left: &crate::ArrayKey, right: &crate::ArrayKey) -> Ordering {
    match (left, right) {
        (crate::ArrayKey::Int(left), crate::ArrayKey::Int(right)) => left.cmp(right),
        (crate::ArrayKey::String(left), crate::ArrayKey::String(right)) => {
            left.as_bytes().cmp(right.as_bytes())
        }
        (crate::ArrayKey::Int(_), crate::ArrayKey::String(_)) => Ordering::Less,
        (crate::ArrayKey::String(_), crate::ArrayKey::Int(_)) => Ordering::Greater,
    }
}

fn objects_equal(left: &crate::ObjectRef, right: &crate::ObjectRef) -> Result<bool, String> {
    if left.id() == right.id() {
        return Ok(true);
    }
    if left.class_name() != right.class_name() {
        return Ok(false);
    }
    let left_properties = left.properties_snapshot();
    let right_properties = right.properties_snapshot();
    if left_properties.len() != right_properties.len() {
        return Ok(false);
    }
    for ((left_name, left_value), (right_name, right_value)) in
        left_properties.iter().zip(right_properties.iter())
    {
        if left_name != right_name || !equal(left_value, right_value)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn objects_compare(left: &crate::ObjectRef, right: &crate::ObjectRef) -> Result<Ordering, String> {
    if objects_equal(left, right)? {
        return Ok(Ordering::Equal);
    }
    match left.class_name().cmp(&right.class_name()) {
        Ordering::Equal => {}
        ordering => return Ok(ordering),
    }
    let left_properties = left.properties_snapshot();
    let right_properties = right.properties_snapshot();
    match left_properties.len().cmp(&right_properties.len()) {
        Ordering::Equal => {}
        ordering => return Ok(ordering),
    }
    for ((left_name, left_value), (right_name, right_value)) in
        left_properties.iter().zip(right_properties.iter())
    {
        match left_name.cmp(right_name) {
            Ordering::Equal => {}
            ordering => return Ok(ordering),
        }
        let ordering = compare(left_value, right_value)?;
        if ordering != Ordering::Equal {
            return Ok(ordering);
        }
    }
    Ok(Ordering::Equal)
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::String(_) => "string",
        Value::Uninitialized => "uninitialized",
        Value::Array(_) => "array",
        Value::Object(_) | Value::Fiber(_) | Value::Generator(_) => "object",
        Value::Resource(_) => "resource",
        Value::Callable(_) => "callable",
        Value::Reference(_) => "reference",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{
        AttributeEntry, ClassConstantEntry, ClassEntry, ClassEnumCaseEntry, ClassFlags,
        ClassPropertyEntry, ClassPropertyFlags, ClassPropertyHooks, ObjectRef,
    };

    #[test]
    fn convert_truthiness_matches_scalar_mvp() {
        assert!(!to_bool(&Value::Null).unwrap());
        assert!(!to_bool(&Value::Bool(false)).unwrap());
        assert!(to_bool(&Value::Bool(true)).unwrap());
        assert!(!to_bool(&Value::Int(0)).unwrap());
        assert!(to_bool(&Value::Int(-1)).unwrap());
        assert!(!to_bool(&Value::string(b"".to_vec())).unwrap());
        assert!(!to_bool(&Value::string(b"0".to_vec())).unwrap());
        assert!(to_bool(&Value::string(b"00".to_vec())).unwrap());
        assert!(!to_bool(&Value::packed_array(Vec::new())).unwrap());
        assert!(to_bool(&Value::packed_array(vec![Value::Int(1)])).unwrap());
        // NAN is truthy: only exactly zero floats are false.
        assert!(to_bool(&Value::float(f64::NAN)).unwrap());
        assert!(!to_bool(&Value::float(0.0)).unwrap());
        assert!(!to_bool(&Value::float(-0.0)).unwrap());
    }

    #[test]
    fn float_to_int_uses_zend_dval_to_lval_semantics() {
        assert_eq!(php_float_to_int(f64::NAN), 0);
        assert_eq!(php_float_to_int(f64::INFINITY), 0);
        assert_eq!(php_float_to_int(f64::NEG_INFINITY), 0);
        assert_eq!(php_float_to_int(3.7), 3);
        assert_eq!(php_float_to_int(-3.7), -3);
        // Out-of-range floats reduce modulo 2^64 like the reference engine.
        assert_eq!(php_float_to_int(1e30), 5_076_964_154_930_102_272);
        assert_eq!(php_float_to_int(-1e30), -5_076_964_154_930_102_272);
        assert!(float_fits_int(9.2e18));
        assert!(!float_fits_int(9.3e18));
        assert!(!float_fits_int(f64::NAN));
        assert_eq!(to_int(&Value::float(f64::NAN)).unwrap(), 0);
    }

    #[test]
    fn convert_php_named_apis_cover_supported_casts_and_known_gaps() {
        assert!(to_bool_php(&Value::string(b"1".to_vec())).unwrap());
        assert_eq!(to_int_php(&Value::string(b"42x".to_vec())).unwrap(), 42);
        assert_eq!(to_float_php(&Value::string(b"1.5x".to_vec())).unwrap(), 1.5);
        assert_eq!(to_string_php(&Value::Bool(true)).unwrap().as_bytes(), b"1");
        assert_eq!(
            to_number_php(&Value::string(b"2".to_vec())).unwrap(),
            NumericValue::Int(2)
        );
        assert!(
            to_arithmetic_number_php(&Value::string(b"2x".to_vec()))
                .unwrap()
                .leading_numeric_string
        );

        let null_array = to_array_php(&Value::Null).unwrap();
        assert!(null_array.is_empty());
        let scalar_array = to_array_php(&Value::Int(7)).unwrap();
        assert_eq!(scalar_array.len(), 1);
        assert_eq!(
            to_object_php(&Value::Int(7)).unwrap_err(),
            "E_PHP_RUNTIME_OBJECT_CAST_GAP: object cast conversion is not implemented"
        );
    }

    #[test]
    fn convert_scalar_to_string_matches_echo_mvp() {
        assert_eq!(to_string(&Value::Null).unwrap().as_bytes(), b"");
        assert_eq!(to_string(&Value::Bool(false)).unwrap().as_bytes(), b"");
        assert_eq!(to_string(&Value::Bool(true)).unwrap().as_bytes(), b"1");
        assert_eq!(to_string(&Value::Int(42)).unwrap().as_bytes(), b"42");
        assert_eq!(to_string(&Value::float(1.5)).unwrap().as_bytes(), b"1.5");
        assert_eq!(
            to_string(&Value::string(b"x".to_vec())).unwrap().as_bytes(),
            b"x"
        );
    }

    #[test]
    fn objects_compare_by_properties_for_same_class() {
        let class = ClassEntry {
            name: "sample".to_string().into(),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: vec![ClassPropertyEntry {
                name: "class_value".to_string(),
                default: Value::Null,
                type_: None,
                flags: ClassPropertyFlags::default(),
                hooks: ClassPropertyHooks::default(),
                attributes: Vec::<AttributeEntry>::new(),
            }],
            constants: Vec::<ClassConstantEntry>::new(),
            enum_cases: Vec::<ClassEnumCaseEntry>::new(),
            attributes: Vec::<AttributeEntry>::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        };
        let low = ObjectRef::new(&class);
        low.set_property("class_value".to_string(), Value::Int(-5));
        let high = ObjectRef::new(&class);
        high.set_property("class_value".to_string(), Value::Int(11));

        assert_eq!(
            compare(&Value::Object(low), &Value::Object(high)).unwrap(),
            Ordering::Less
        );
    }

    #[test]
    fn convert_scalar_to_number_handles_plain_numeric_strings() {
        assert_eq!(to_number(&Value::Null).unwrap(), NumericValue::Int(0));
        assert_eq!(to_number(&Value::Bool(true)).unwrap(), NumericValue::Int(1));
        assert_eq!(
            to_number(&Value::string(b"12".to_vec())).unwrap(),
            NumericValue::Int(12)
        );
        assert_eq!(
            to_number(&Value::string(b"1.5".to_vec())).unwrap(),
            NumericValue::Float(1.5)
        );
        assert_eq!(
            to_number(&Value::string(b" 42".to_vec())).unwrap(),
            NumericValue::Int(42)
        );
        assert_eq!(
            to_number(&Value::string(b"12abc".to_vec())).unwrap(),
            NumericValue::Int(12)
        );
        assert!(to_number(&Value::string(b"abc".to_vec())).is_err());
    }

    #[test]
    fn arithmetic_number_preserves_leading_numeric_warning_flag() {
        let clean = to_arithmetic_number(&Value::string(b"42".to_vec())).unwrap();
        assert_eq!(clean.value, NumericValue::Int(42));
        assert!(!clean.leading_numeric_string);

        let leading = to_arithmetic_number(&Value::string(b"42abc".to_vec())).unwrap();
        assert_eq!(leading.value, NumericValue::Int(42));
        assert!(leading.leading_numeric_string);
    }

    #[test]
    fn numeric_casts_handle_non_numeric_strings_and_arrays() {
        reset_float_string_precision();
        assert_eq!(to_int(&Value::string(b"".to_vec())).unwrap(), 0);
        assert_eq!(to_int(&Value::string(b"42abc".to_vec())).unwrap(), 42);
        assert_eq!(to_int(&Value::string(b"abc".to_vec())).unwrap(), 0);
        assert_eq!(to_float(&Value::string(b"0.5x".to_vec())).unwrap(), 0.5);
        assert_eq!(to_float(&Value::packed_array(Vec::new())).unwrap(), 0.0);
        assert_eq!(
            to_int(&Value::packed_array(vec![Value::Int(1)])).unwrap(),
            1
        );
        assert_eq!(
            to_string(&Value::float(f64::INFINITY)).unwrap().as_bytes(),
            b"INF"
        );
        assert_eq!(
            to_string(&Value::float(f64::NEG_INFINITY))
                .unwrap()
                .as_bytes(),
            b"-INF"
        );
        assert_eq!(
            to_string(&Value::float(f64::NAN)).unwrap().as_bytes(),
            b"NAN"
        );
        assert_eq!(to_string(&Value::float(1.75)).unwrap().as_bytes(), b"1.75");
        assert_eq!(
            to_string(&Value::float(1.7000000000000002))
                .unwrap()
                .as_bytes(),
            b"1.7"
        );
        assert_eq!(
            to_string(&Value::float(1.0 / 23.0)).unwrap().as_bytes(),
            b"0.043478260869565"
        );
        assert_eq!(
            to_string(&Value::float(2.1_f64.powf(-10.0)))
                .unwrap()
                .as_bytes(),
            b"0.0005995246616609"
        );
        assert_eq!(
            to_string(&Value::float(0.1_f64.powf(10.0)))
                .unwrap()
                .as_bytes(),
            b"1.0E-10"
        );
        set_float_string_precision(0);
        assert_eq!(to_string(&Value::float(1.75)).unwrap().as_bytes(), b"2");
        set_float_string_precision(-1);
        assert_eq!(
            to_string(&Value::float(std::f64::consts::E))
                .unwrap()
                .as_bytes(),
            b"2.718281828459045"
        );
        reset_float_string_precision();
    }

    #[test]
    fn convert_comparison_handles_safe_scalar_mvp() {
        assert!(equal(&Value::Int(1), &Value::float(1.0)).unwrap());
        assert!(equal_php(&Value::Int(1), &Value::float(1.0)).unwrap());
        assert!(identical(&Value::Int(1), &Value::Int(1)));
        assert!(identical_php(&Value::Int(1), &Value::Int(1)));
        assert!(!identical(&Value::Int(1), &Value::float(1.0)));
        assert!(!identical(&Value::float(f64::NAN), &Value::float(f64::NAN)));
        assert_eq!(
            compare(&Value::string(b"2".to_vec()), &Value::Int(10)).unwrap(),
            Ordering::Less
        );
        assert_eq!(
            compare_php(&Value::string(b"2".to_vec()), &Value::Int(10)).unwrap(),
            Ordering::Less
        );
    }

    #[test]
    fn closure_callables_are_identical_by_handle() {
        let closure = Value::closure(crate::ClosurePayload::new(1, Vec::new()));
        let same_handle = closure.clone();
        let other = Value::closure(crate::ClosurePayload::new(1, Vec::new()));

        assert!(identical(&closure, &same_handle));
        assert!(!identical(&closure, &other));
    }

    #[test]
    fn compare_uses_php8_numeric_string_rules_without_arithmetic_errors() {
        assert!(equal(&Value::Int(42), &Value::string(b" 42".to_vec())).unwrap());
        assert!(!equal(&Value::Int(42), &Value::string(b"42abc".to_vec())).unwrap());
        assert!(!equal(&Value::Int(0), &Value::string(b"foo".to_vec())).unwrap());
        assert_eq!(
            compare(&Value::Int(0), &Value::string(b"foo".to_vec())).unwrap(),
            Ordering::Less
        );
        assert_eq!(
            compare(&Value::string(b"foo".to_vec()), &Value::Int(0)).unwrap(),
            Ordering::Greater
        );
        assert!(
            equal(
                &Value::string(b"0e123".to_vec()),
                &Value::string(b"0".to_vec())
            )
            .unwrap()
        );
        assert_eq!(
            compare(
                &Value::string(b"42abc".to_vec()),
                &Value::string(b"42".to_vec())
            )
            .unwrap(),
            Ordering::Greater
        );
        assert!(
            equal(
                &Value::float(f64::INFINITY),
                &Value::string(b"INF".to_vec())
            )
            .unwrap()
        );
        assert!(
            equal(
                &Value::float(f64::NEG_INFINITY),
                &Value::string(b"-INF".to_vec())
            )
            .unwrap()
        );
        assert!(!equal(&Value::float(f64::NAN), &Value::string(b"NAN".to_vec())).unwrap());
    }

    #[test]
    fn loose_nan_equality_is_false_without_runtime_error() {
        assert!(!equal(&Value::float(f64::NAN), &Value::float(f64::NAN)).unwrap());
        assert!(!equal(&Value::float(f64::NAN), &Value::Int(0)).unwrap());
    }

    #[test]
    fn compare_arrays_distinguishes_loose_and_strict_identity() {
        let first = {
            let mut array = crate::PhpArray::new();
            array.insert(crate::ArrayKey::Int(0), Value::string(b"1".to_vec()));
            array.insert(
                crate::ArrayKey::String(crate::PhpString::from("name")),
                Value::Int(2),
            );
            Value::Array(array)
        };
        let reordered = {
            let mut array = crate::PhpArray::new();
            array.insert(
                crate::ArrayKey::String(crate::PhpString::from("name")),
                Value::Int(2),
            );
            array.insert(crate::ArrayKey::Int(0), Value::Int(1));
            Value::Array(array)
        };

        assert!(equal(&first, &reordered).unwrap());
        assert!(!identical(&first, &reordered));
    }

    #[test]
    fn compare_arrays_orders_disjoint_integer_and_string_keys_consistently() {
        let string_keys = {
            let mut array = crate::PhpArray::new();
            array.insert(
                crate::ArrayKey::String(crate::PhpString::from("a")),
                Value::string("orange"),
            );
            array.insert(
                crate::ArrayKey::String(crate::PhpString::from("b")),
                Value::string("banana"),
            );
            array.insert(
                crate::ArrayKey::String(crate::PhpString::from("c")),
                Value::string("apple"),
            );
            Value::Array(array)
        };
        let integer_keys = {
            let mut array = crate::PhpArray::new();
            array.insert(crate::ArrayKey::Int(0), Value::string("first"));
            array.insert(crate::ArrayKey::Int(5), Value::string("second"));
            array.insert(crate::ArrayKey::Int(6), Value::string("third"));
            Value::Array(array)
        };

        assert_eq!(
            compare(&string_keys, &integer_keys).unwrap(),
            Ordering::Greater
        );
        assert_eq!(
            compare(&integer_keys, &string_keys).unwrap(),
            Ordering::Less
        );
    }

    #[test]
    fn compare_references_and_objects_use_value_and_handle_identity() {
        let cell = crate::ReferenceCell::new(Value::Int(1));
        let alias = Value::Reference(cell.clone());
        let same_alias = Value::Reference(cell);
        let other_reference = Value::Reference(crate::ReferenceCell::new(Value::Int(1)));

        assert!(identical(&alias, &same_alias));
        assert!(equal(&alias, &other_reference).unwrap());
        assert!(identical(&alias, &other_reference));

        let class = crate::ClassEntry {
            name: "Box".to_owned().into(),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: vec![ClassPropertyEntry {
                name: "value".to_owned(),
                default: Value::Int(1),
                type_: None,
                flags: ClassPropertyFlags::default(),
                hooks: ClassPropertyHooks::default(),
                attributes: Vec::new(),
            }],
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: crate::ClassFlags::default(),
        };
        let one = crate::ObjectRef::new(&class);
        let same_handle = one.clone();
        let clone = one.clone_shallow();

        assert!(identical(
            &Value::Object(one.clone()),
            &Value::Object(same_handle)
        ));
        assert!(!identical(
            &Value::Object(one.clone()),
            &Value::Object(clone.clone())
        ));
        assert!(equal(&Value::Object(one), &Value::Object(clone)).unwrap());
    }
}
