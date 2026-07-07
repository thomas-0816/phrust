//! Scalar object methods: methods directly on string, int, and float values.
//!
//! Implements the "Scalar Object Methods" RFC, providing a curated set
//! of methods on scalar values. Dispatch is based on the runtime type of
//! the receiver value.

use crate::builtins::string_intrinsics;
use crate::{FloatValue, PhpString, Value, value_type_name};

/// Dispatch a method call on a scalar value.
///
/// Returns the method result or an error message. The `method` parameter
/// should be the original (unlowered) method name; matching is case-insensitive.
pub fn dispatch_scalar_method(
    receiver: Value,
    method: &str,
    args: Vec<Value>,
) -> Result<Value, String> {
    let lowered = method.to_ascii_lowercase();
    match receiver {
        Value::String(s) => call_string_method(s, &lowered, args),
        Value::Int(i) => call_int_method(i, &lowered, args),
        Value::Float(f) => call_float_method(f, &lowered, args),
        _ => Err(format!(
            "E_PHP_VM_METHOD_CALL_NON_OBJECT: Call to a member function {method}() on {}",
            value_type_name(&receiver)
        )),
    }
}

// ---------------------------------------------------------------------------
// String methods
// ---------------------------------------------------------------------------

fn call_string_method(s: PhpString, method: &str, args: Vec<Value>) -> Result<Value, String> {
    match method {
        "trim" => string_trim(s, args),
        "upper" => {
            expect_arity("Str::upper", &args, 0)?;
            Ok(Value::String(string_intrinsics::strtoupper_ascii(&s)))
        }
        "lower" => {
            expect_arity("Str::lower", &args, 0)?;
            Ok(Value::String(string_intrinsics::strtolower_ascii(&s)))
        }
        "length" => {
            expect_arity("Str::length", &args, 0)?;
            Ok(Value::Int(s.len() as i64))
        }
        "contains" => {
            expect_arity("Str::contains", &args, 1)?;
            let needle = string_arg("Str::contains", &args[0])?;
            Ok(Value::Bool(
                php_source::byte_kernel::find_bytes(s.as_bytes(), needle.as_bytes()).is_some(),
            ))
        }
        "startswith" => {
            expect_arity("Str::startsWith", &args, 1)?;
            let prefix = string_arg("Str::startsWith", &args[0])?;
            Ok(Value::Bool(s.as_bytes().starts_with(prefix.as_bytes())))
        }
        "endswith" => {
            expect_arity("Str::endsWith", &args, 1)?;
            let suffix = string_arg("Str::endsWith", &args[0])?;
            Ok(Value::Bool(s.as_bytes().ends_with(suffix.as_bytes())))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method Str::{method} is not defined"
        )),
    }
}

fn string_trim(s: PhpString, args: Vec<Value>) -> Result<Value, String> {
    if args.len() > 1 {
        return Err(arity_error("Str::trim", "0 or 1 argument(s)"));
    }
    let bytes = s.as_bytes();
    let (start, end) = if let Some(mask_arg) = args.first() {
        let charlist = string_arg("Str::trim", mask_arg)?;
        let mask = trim_mask_from_charlist(charlist.as_bytes());
        let start = bytes
            .iter()
            .position(|byte| !mask[usize::from(*byte)])
            .unwrap_or(bytes.len());
        let end = bytes
            .iter()
            .rposition(|byte| !mask[usize::from(*byte)])
            .map(|pos| pos + 1)
            .unwrap_or(start);
        (start, end)
    } else {
        php_source::byte_kernel::trim_default_bounds(bytes)
    };
    if start == 0 && end == bytes.len() {
        Ok(Value::String(s))
    } else {
        Ok(Value::string(bytes[start..end].to_vec()))
    }
}

fn trim_mask_from_charlist(charlist: &[u8]) -> [bool; 256] {
    let mut mask = [false; 256];
    for &byte in charlist {
        mask[usize::from(byte)] = true;
    }
    mask
}

// ---------------------------------------------------------------------------
// Int methods
// ---------------------------------------------------------------------------

fn call_int_method(i: i64, method: &str, args: Vec<Value>) -> Result<Value, String> {
    match method {
        "abs" => {
            expect_arity("Int::abs", &args, 0)?;
            Ok(
                i.checked_abs()
                    .map(Value::Int)
                    .unwrap_or_else(|| Value::float((i as f64).abs())),
            )
        }
        "pow" => {
            expect_arity("Int::pow", &args, 1)?;
            let exponent = int_arg("Int::pow", &args[0])?;
            let Ok(unsigned_exponent) = u32::try_from(exponent) else {
                return Ok(Value::float((i as f64).powf(exponent as f64)));
            };
            Ok(
                i.checked_pow(unsigned_exponent)
                    .map(Value::Int)
                    .unwrap_or_else(|| Value::float((i as f64).powf(exponent as f64))),
            )
        }
        "clamp" => {
            expect_arity("Int::clamp", &args, 2)?;
            let min = int_arg("Int::clamp", &args[0])?;
            let max = int_arg("Int::clamp", &args[1])?;
            if min > max {
                return Err(format!(
                    "E_PHP_VM_ARGUMENT_VALUE_ERROR: Int::clamp(): Argument #2 ($min) must be less than or equal to argument #3 ($max)"
                ));
            }
            Ok(Value::Int(i.clamp(min, max)))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method Int::{method} is not defined"
        )),
    }
}

// ---------------------------------------------------------------------------
// Float methods
// ---------------------------------------------------------------------------

fn call_float_method(f: FloatValue, method: &str, args: Vec<Value>) -> Result<Value, String> {
    let f64_val = f.to_f64();
    match method {
        "round" => {
            if args.len() > 1 {
                return Err(arity_error("Float::round", "0 or 1 argument(s)"));
            }
            let precision = args
                .first()
                .map(|v| int_arg("Float::round", v))
                .transpose()?
                .unwrap_or(0);
            let rounded = php_round(f64_val, precision, 1); // mode 1 = HalfAwayFromZero (PHP default)
            Ok(Value::float(rounded))
        }
        "ceil" => {
            expect_arity("Float::ceil", &args, 0)?;
            Ok(Value::float(f64_val.ceil()))
        }
        "floor" => {
            expect_arity("Float::floor", &args, 0)?;
            Ok(Value::float(f64_val.floor()))
        }
        "abs" => {
            expect_arity("Float::abs", &args, 0)?;
            Ok(Value::float(f64_val.abs()))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method Float::{method} is not defined"
        )),
    }
}

/// Simplified PHP-compatible round implementation for scalar methods.
///
/// Mirrors PHP's `_php_math_round` for the common case (HalfAwayFromZero).
fn php_round(value: f64, places: i64, mode: i64) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    // Only implement HalfAwayFromZero (mode 1) for v1
    let _ = mode;
    let places = places.clamp(i64::from(i32::MIN) + 1, i64::from(i32::MAX)) as i32;
    let exponent = 10_f64.powi(places.abs());
    let scaled = if places > 0 {
        value * exponent
    } else {
        value / exponent
    };
    let rounded = if scaled >= 0.0 {
        (scaled + 0.5).floor()
    } else {
        (scaled - 0.5).ceil()
    };
    let result = if places > 0 {
        rounded / exponent
    } else {
        rounded * exponent
    };
    if result.abs() >= 1e16 {
        value
    } else {
        result
    }
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn expect_arity(name: &str, args: &[Value], expected: usize) -> Result<(), String> {
    if args.len() != expected {
        Err(arity_error(name, &format!("{expected} argument(s)")))
    } else {
        Ok(())
    }
}

fn arity_error(name: &str, expected: &str) -> String {
    format!("E_PHP_VM_ARGUMENT_COUNT_ERROR: {name}() expects exactly {expected}")
}

fn string_arg(name: &str, value: &Value) -> Result<PhpString, String> {
    match value {
        Value::String(s) => Ok(s.clone()),
        _ => Err(format!(
            "E_PHP_VM_ARGUMENT_TYPE_ERROR: {name}(): Argument must be of type string"
        )),
    }
}

fn int_arg(name: &str, value: &Value) -> Result<i64, String> {
    match value {
        Value::Int(i) => Ok(*i),
        _ => Err(format!(
            "E_PHP_VM_ARGUMENT_TYPE_ERROR: {name}(): Argument must be of type int"
        )),
    }
}
