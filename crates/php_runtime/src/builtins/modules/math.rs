//! Math builtin registry slice.

use super::core::{
    argument_type_error, argument_value_error, arity_error, conversion_error, deref_value,
    expect_arity, group_decimal_integer, int_arg, min_max_builtin, numeric_f64_arg,
    php_float_debug_string, string_arg,
};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::numeric_string::{NumericStringKind, NumericStringValue, classify_php_string};
use crate::{NumericValue, PhpString, Value, to_number, value_type_name};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("abs", builtin_abs, BuiltinCompatibility::Php),
    BuiltinEntry::new("acos", builtin_acos, BuiltinCompatibility::Php),
    BuiltinEntry::new("acosh", builtin_acosh, BuiltinCompatibility::Php),
    BuiltinEntry::new("asin", builtin_asin, BuiltinCompatibility::Php),
    BuiltinEntry::new("asinh", builtin_asinh, BuiltinCompatibility::Php),
    BuiltinEntry::new("atan", builtin_atan, BuiltinCompatibility::Php),
    BuiltinEntry::new("atan2", builtin_atan2, BuiltinCompatibility::Php),
    BuiltinEntry::new("atanh", builtin_atanh, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "base_convert",
        builtin_base_convert,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("bindec", builtin_bindec, BuiltinCompatibility::Php),
    BuiltinEntry::new("ceil", builtin_ceil, BuiltinCompatibility::Php),
    BuiltinEntry::new("cos", builtin_cos, BuiltinCompatibility::Php),
    BuiltinEntry::new("cosh", builtin_cosh, BuiltinCompatibility::Php),
    BuiltinEntry::new("decbin", builtin_decbin, BuiltinCompatibility::Php),
    BuiltinEntry::new("dechex", builtin_dechex, BuiltinCompatibility::Php),
    BuiltinEntry::new("decoct", builtin_decoct, BuiltinCompatibility::Php),
    BuiltinEntry::new("deg2rad", builtin_deg2rad, BuiltinCompatibility::Php),
    BuiltinEntry::new("exp", builtin_exp, BuiltinCompatibility::Php),
    BuiltinEntry::new("expm1", builtin_expm1, BuiltinCompatibility::Php),
    BuiltinEntry::new("floor", builtin_floor, BuiltinCompatibility::Php),
    BuiltinEntry::new("fdiv", builtin_fdiv, BuiltinCompatibility::Php),
    BuiltinEntry::new("fmod", builtin_fmod, BuiltinCompatibility::Php),
    BuiltinEntry::new("fpow", builtin_fpow, BuiltinCompatibility::Php),
    BuiltinEntry::new("getrandmax", builtin_getrandmax, BuiltinCompatibility::Php),
    BuiltinEntry::new("hexdec", builtin_hexdec, BuiltinCompatibility::Php),
    BuiltinEntry::new("hypot", builtin_hypot, BuiltinCompatibility::Php),
    BuiltinEntry::new("intdiv", builtin_intdiv, BuiltinCompatibility::Php),
    BuiltinEntry::new("is_finite", builtin_is_finite, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "is_infinite",
        builtin_is_infinite,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("is_nan", builtin_is_nan, BuiltinCompatibility::Php),
    BuiltinEntry::new("log", builtin_log, BuiltinCompatibility::Php),
    BuiltinEntry::new("log10", builtin_log10, BuiltinCompatibility::Php),
    BuiltinEntry::new("log1p", builtin_log1p, BuiltinCompatibility::Php),
    BuiltinEntry::new("max", builtin_max, BuiltinCompatibility::Php),
    BuiltinEntry::new("min", builtin_min, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "number_format",
        builtin_number_format,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("octdec", builtin_octdec, BuiltinCompatibility::Php),
    BuiltinEntry::new("pi", builtin_pi, BuiltinCompatibility::Php),
    BuiltinEntry::new("pow", builtin_pow, BuiltinCompatibility::Php),
    BuiltinEntry::new("rad2deg", builtin_rad2deg, BuiltinCompatibility::Php),
    BuiltinEntry::new("round", builtin_round, BuiltinCompatibility::Php),
    BuiltinEntry::new("sin", builtin_sin, BuiltinCompatibility::Php),
    BuiltinEntry::new("sinh", builtin_sinh, BuiltinCompatibility::Php),
    BuiltinEntry::new("sqrt", builtin_sqrt, BuiltinCompatibility::Php),
    BuiltinEntry::new("tan", builtin_tan, BuiltinCompatibility::Php),
    BuiltinEntry::new("tanh", builtin_tanh, BuiltinCompatibility::Php),
];

const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";

#[derive(Clone, Copy)]
struct ParsedBaseNumber {
    int_value: u128,
    float_value: f64,
    overflowed: bool,
}

fn unary_float_builtin(
    name: &str,
    args: &[Value],
    function: impl FnOnce(f64) -> f64,
) -> BuiltinResult {
    expect_arity(name, args, 1)?;
    Ok(Value::float(function(numeric_f64_arg(name, &args[0])?)))
}

fn emit_null_numeric_deprecation(
    context: &mut BuiltinContext<'_>,
    name: &str,
    span: RuntimeSourceSpan,
) {
    context.php_deprecation(
        format!(
            "E_PHP_RUNTIME_{}_NULL_NUMERIC_ARG",
            name.to_ascii_uppercase()
        ),
        format!("{name}(): Passing null to parameter #1 ($num) of type int|float is deprecated"),
        span,
    );
}

fn numeric_f64_math_arg(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: &Value,
    span: RuntimeSourceSpan,
) -> Result<f64, BuiltinError> {
    if matches!(deref_value(value), Value::Null) {
        emit_null_numeric_deprecation(context, name, span);
    }
    numeric_f64_arg(name, value)
}

fn round_numeric_arg(
    context: &mut BuiltinContext<'_>,
    value: &Value,
    span: RuntimeSourceSpan,
) -> Result<f64, BuiltinError> {
    if matches!(deref_value(value), Value::Null) {
        emit_null_numeric_deprecation(context, "round", span);
    }
    if matches!(deref_value(value), Value::Resource(_)) {
        return Err(argument_type_error(
            "round",
            "#1 ($num)",
            "int|float",
            value,
        ));
    }
    to_number(value)
        .map(|number| number.as_f64())
        .map_err(|_| argument_type_error("round", "#1 ($num)", "int|float", value))
}

fn pow_numeric_arg(lhs: &Value, rhs: &Value, value: &Value) -> Result<NumericValue, BuiltinError> {
    if matches!(deref_value(value), Value::Resource(_)) {
        return Err(pow_unsupported_operand_types(lhs, rhs));
    }
    to_number(value).map_err(|_| pow_unsupported_operand_types(lhs, rhs))
}

fn pow_unsupported_operand_types(lhs: &Value, rhs: &Value) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_UNSUPPORTED_OPERAND_TYPES",
        format!(
            "Unsupported operand types: {} ** {}",
            pow_operand_type_name(lhs),
            pow_operand_type_name(rhs)
        ),
    )
}

fn pow_operand_type_name(value: &Value) -> String {
    match deref_value(value) {
        Value::Object(object) => object.display_name(),
        other => value_type_name(&other).to_owned(),
    }
}

fn emit_zero_negative_power_deprecation(
    context: &mut BuiltinContext<'_>,
    base: NumericValue,
    exponent: NumericValue,
    span: RuntimeSourceSpan,
) {
    if base.as_f64() == 0.0 && exponent.as_f64() < 0.0 {
        context.php_deprecation(
            "E_PHP_RUNTIME_POW_ZERO_NEGATIVE_EXPONENT",
            "Power of base 0 and negative exponent is deprecated",
            span,
        );
    }
}

fn parse_base_number(
    context: &mut BuiltinContext<'_>,
    name: &str,
    argument: &str,
    value: &Value,
    base: u32,
    span: RuntimeSourceSpan,
) -> Result<ParsedBaseNumber, BuiltinError> {
    if matches!(deref_value(value), Value::Resource(_)) {
        return Err(argument_type_error(name, argument, "string", value));
    }
    let string = string_arg(name, value)?;
    let mut int_value = 0_u128;
    let mut float_value = 0.0_f64;
    let mut overflowed = false;
    let mut ignored_invalid_characters = false;
    let text = string.to_string_lossy();
    let text = text.trim();
    for (index, character) in text.chars().enumerate() {
        if index == 1 && text.starts_with('0') && base_prefix_matches(character, base) {
            continue;
        }
        let Some(digit) = character.to_digit(36) else {
            ignored_invalid_characters = true;
            continue;
        };
        if digit >= base {
            ignored_invalid_characters = true;
            continue;
        }
        float_value = (float_value * f64::from(base)) + f64::from(digit);
        if !overflowed {
            match int_value
                .checked_mul(u128::from(base))
                .and_then(|value| value.checked_add(u128::from(digit)))
            {
                Some(value) => int_value = value,
                None => overflowed = true,
            }
        }
    }
    if ignored_invalid_characters {
        context.php_deprecation(
            format!(
                "E_PHP_RUNTIME_{}_INVALID_BASE_DIGITS",
                name.to_ascii_uppercase()
            ),
            "Invalid characters passed for attempted conversion, these have been ignored",
            span,
        );
    }
    Ok(ParsedBaseNumber {
        int_value,
        float_value,
        overflowed,
    })
}

fn base_prefix_matches(character: char, base: u32) -> bool {
    matches!(
        (character, base),
        ('x' | 'X', 16) | ('b' | 'B', 2) | ('o' | 'O', 8)
    )
}

fn base_digit_value(name: &str, argument: &str, value: &Value) -> Result<u32, BuiltinError> {
    let base = int_arg(name, value)?;
    if !(2..=36).contains(&base) {
        return Err(argument_value_error(
            name,
            argument,
            "must be between 2 and 36 (inclusive)",
        ));
    }
    Ok(base as u32)
}

fn format_u128_base(mut value: u128, base: u32) -> String {
    if value == 0 {
        return "0".to_owned();
    }
    let mut output = Vec::new();
    while value > 0 {
        let digit = (value % u128::from(base)) as usize;
        output.push(DIGITS[digit]);
        value /= u128::from(base);
    }
    output.reverse();
    String::from_utf8(output).expect("base digits are ascii")
}

fn format_f64_base(mut value: f64, base: u32) -> String {
    if !value.is_finite() || value <= 0.0 {
        return "0".to_owned();
    }
    let base_f64 = f64::from(base);
    let mut output = Vec::new();
    while value >= 1.0 && output.len() < 4096 {
        let next = (value / base_f64).floor();
        let digit = (value - (next * base_f64)).round().clamp(0.0, 35.0) as usize;
        output.push(DIGITS[digit]);
        value = next;
    }
    output.reverse();
    String::from_utf8(output).expect("base digits are ascii")
}

fn parsed_base_as_value(parsed: ParsedBaseNumber) -> Value {
    if !parsed.overflowed && parsed.int_value <= i64::MAX as u128 {
        Value::Int(parsed.int_value as i64)
    } else {
        Value::float(parsed.float_value)
    }
}

fn decimal_float_to_base_int(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: &Value,
    number: f64,
    label: String,
    span: RuntimeSourceSpan,
) -> Result<i64, BuiltinError> {
    if !number.is_finite() || number < i64::MIN as f64 || number >= 9_223_372_036_854_775_808.0 {
        return Err(argument_type_error(name, "#1 ($num)", "int", value));
    }
    if number.fract() != 0.0 {
        context.php_deprecation(
            format!(
                "E_PHP_RUNTIME_{}_FLOAT_TO_INT_PRECISION",
                name.to_ascii_uppercase()
            ),
            format!("Implicit conversion from {label} to int loses precision"),
            span,
        );
    }
    Ok(number as i64)
}

fn decimal_to_base_arg(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: &Value,
    span: RuntimeSourceSpan,
) -> Result<i64, BuiltinError> {
    match deref_value(value) {
        Value::Float(float) => {
            let number = float.to_f64();
            decimal_float_to_base_int(
                context,
                name,
                value,
                number,
                format!("float {}", php_float_debug_string(float, -1)),
                span,
            )
        }
        Value::String(string) => {
            let classified = classify_php_string(&string);
            match (classified.kind, classified.value) {
                (NumericStringKind::IntString, Some(NumericStringValue::Int(number))) => Ok(number),
                (NumericStringKind::IntString, Some(NumericStringValue::Float(number))) => {
                    decimal_float_to_base_int(
                        context,
                        name,
                        value,
                        number,
                        format!("float-string \"{}\"", string.to_string_lossy()),
                        span,
                    )
                }
                (NumericStringKind::FloatString, Some(NumericStringValue::Float(number))) => {
                    decimal_float_to_base_int(
                        context,
                        name,
                        value,
                        number,
                        format!("float-string \"{}\"", string.to_string_lossy()),
                        span,
                    )
                }
                (NumericStringKind::FloatString, Some(NumericStringValue::Int(number))) => {
                    Ok(number)
                }
                _ => Err(argument_type_error(name, "#1 ($num)", "int", value)),
            }
        }
        other => int_arg(name, &other),
    }
}

fn decimal_to_base_builtin(
    context: &mut BuiltinContext<'_>,
    name: &str,
    args: &[Value],
    base: u32,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity(name, args, 1)?;
    Ok(Value::string(format_u128_base(
        decimal_to_base_arg(context, name, &args[0], span)? as u64 as u128,
        base,
    )))
}

fn round_scaled(value: f64, mode: i64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    match mode {
        5 => return value.ceil(),
        6 => return value.floor(),
        7 => return value.trunc(),
        8 => return value.signum() * value.abs().ceil(),
        _ => {}
    }
    let sign = if value.is_sign_negative() { -1.0 } else { 1.0 };
    let absolute = value.abs();
    let lower = absolute.floor();
    let fraction = absolute - lower;
    let rounded = if fraction < 0.5 {
        lower
    } else if fraction > 0.5 {
        lower + 1.0
    } else {
        match mode {
            1 => lower + 1.0,
            2 => lower,
            3 => {
                if (lower as i128).rem_euclid(2) == 0 {
                    lower
                } else {
                    lower + 1.0
                }
            }
            4 => {
                if (lower as i128).rem_euclid(2) == 0 {
                    lower + 1.0
                } else {
                    lower
                }
            }
            _ => lower + 1.0,
        }
    };
    sign * rounded
}

fn round_edge_case(integral: f64, exponent: f64, places: i32, half: bool) -> f64 {
    let adjusted = if half {
        integral + 0.5_f64.copysign(integral)
    } else {
        integral
    };
    if places > 0 {
        (adjusted / exponent).abs()
    } else {
        (adjusted * exponent).abs()
    }
}

fn php_round_helper(integral: f64, value: f64, exponent: f64, places: i32, mode: i64) -> f64 {
    let value_abs = value.abs();
    match mode {
        1 => {
            if value_abs >= round_edge_case(integral, exponent, places, true) {
                integral + 1.0_f64.copysign(integral)
            } else {
                integral
            }
        }
        2 => {
            if value_abs > round_edge_case(integral, exponent, places, true) {
                integral + 1.0_f64.copysign(integral)
            } else {
                integral
            }
        }
        3 => {
            let edge_case = round_edge_case(integral, exponent, places, true);
            if value_abs > edge_case || (value_abs == edge_case && integral % 2.0 != 0.0) {
                integral + 1.0_f64.copysign(integral)
            } else {
                integral
            }
        }
        4 => {
            let edge_case = round_edge_case(integral, exponent, places, true);
            if value_abs > edge_case || (value_abs == edge_case && integral % 2.0 == 0.0) {
                integral + 1.0_f64.copysign(integral)
            } else {
                integral
            }
        }
        5 => {
            if value > 0.0 && value_abs > round_edge_case(integral, exponent, places, false) {
                integral + 1.0
            } else {
                integral
            }
        }
        6 => {
            if value < 0.0 && value_abs > round_edge_case(integral, exponent, places, false) {
                integral - 1.0
            } else {
                integral
            }
        }
        7 => integral,
        8 => {
            if value_abs > round_edge_case(integral, exponent, places, false) {
                integral + 1.0_f64.copysign(integral)
            } else {
                integral
            }
        }
        _ => round_scaled(value, mode),
    }
}

fn php_round(value: f64, places: i64, mode: i64) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    let places = places.clamp(i64::from(i32::MIN) + 1, i64::from(i32::MAX)) as i32;
    let exponent = 10_f64.powi(places.abs());
    let scaled = if places > 0 {
        value * exponent
    } else {
        value / exponent
    };
    let mut tmp_value = if value >= 0.0 {
        scaled.floor()
    } else {
        scaled.ceil()
    };
    let tmp_value2 = if value >= 0.0 {
        tmp_value + 1.0
    } else {
        tmp_value - 1.0
    };
    if (if places > 0 {
        tmp_value2 / exponent
    } else {
        tmp_value2 * exponent
    }) == value
    {
        tmp_value = tmp_value2;
    }
    if tmp_value.abs() >= 1e16 {
        return value;
    }
    tmp_value = php_round_helper(tmp_value, value, exponent, places, mode);
    if places.abs() < 23 {
        if places > 0 {
            tmp_value / exponent
        } else {
            tmp_value * exponent
        }
    } else {
        let text = format!("{tmp_value:15.6}e{}", -places);
        text.trim().parse::<f64>().unwrap_or(value)
    }
}

fn rounding_mode_arg(value: &Value) -> Result<i64, BuiltinError> {
    if let Value::Object(object) = value
        && object.class_name().eq_ignore_ascii_case("RoundingMode")
        && let Some(Value::String(name)) = object.get_property("name")
    {
        return match name.to_string_lossy().as_str() {
            "HalfAwayFromZero" => Ok(1),
            "HalfTowardsZero" => Ok(2),
            "HalfEven" => Ok(3),
            "HalfOdd" => Ok(4),
            "TowardsZero" => Ok(7),
            "AwayFromZero" => Ok(8),
            "NegativeInfinity" => Ok(6),
            "PositiveInfinity" => Ok(5),
            _ => Err(math_value_error(
                "round(): Argument #3 ($mode) must be a valid rounding mode (RoundingMode::*)",
            )),
        };
    }
    int_arg("round", value)
}

fn round_precision_arg(
    context: &mut BuiltinContext<'_>,
    value: &Value,
    span: RuntimeSourceSpan,
) -> Result<i64, BuiltinError> {
    match deref_value(value) {
        Value::Float(float) => {
            let number = float.to_f64();
            if number.fract() != 0.0 {
                context.php_deprecation(
                    "E_PHP_RUNTIME_ROUND_PRECISION_FLOAT_TO_INT",
                    format!(
                        "Implicit conversion from float {} to int loses precision",
                        php_float_debug_string(float, -1)
                    ),
                    span,
                );
            }
            int_arg("round", value)
        }
        Value::String(string) => {
            let classified = classify_php_string(&string);
            if classified.kind == NumericStringKind::FloatString
                && let Some(NumericStringValue::Float(number)) = classified.value
                && number.fract() != 0.0
            {
                context.php_deprecation(
                    "E_PHP_RUNTIME_ROUND_PRECISION_FLOAT_STRING_TO_INT",
                    format!(
                        "Implicit conversion from float-string \"{}\" to int loses precision",
                        string.to_string_lossy()
                    ),
                    span,
                );
            }
            int_arg("round", value)
        }
        other => int_arg("round", &other),
    }
}

fn math_value_error(message: impl Into<String>) -> BuiltinError {
    BuiltinError::new("E_PHP_RUNTIME_BUILTIN_VALUE", message.into())
}

fn number_format_separator(
    value: Option<&Value>,
    default: &'static str,
) -> Result<PhpString, BuiltinError> {
    match value.map(deref_value) {
        None | Some(Value::Null) => Ok(PhpString::from_test_str(default)),
        Some(value) => string_arg("number_format", &value),
    }
}

fn pow10_u128(power: usize) -> Option<u128> {
    let mut value = 1_u128;
    for _ in 0..power {
        value = value.checked_mul(10)?;
    }
    Some(value)
}

fn format_number_format_parts(
    units: u128,
    decimals: usize,
    decimal_separator: &str,
    thousands_separator: &str,
) -> String {
    let scale = pow10_u128(decimals).expect("number_format decimals are capped for exact units");
    let integer = units / scale;
    let fraction = units % scale;
    let mut grouped = group_decimal_integer(&integer.to_string(), thousands_separator);
    if decimals > 0 {
        grouped.push_str(decimal_separator);
        grouped.push_str(&format!("{fraction:0decimals$}"));
    }
    grouped
}

fn number_format_float_units(value: f64, decimals: usize) -> Option<u128> {
    let scale = 10_f64.powi(decimals.try_into().ok()?);
    if !scale.is_finite() {
        return None;
    }
    let scaled = value.abs() * scale;
    if !scaled.is_finite() {
        return None;
    }
    if scaled > 9_007_199_254_740_992.0 {
        return None;
    }
    let epsilon = f64::EPSILON * scaled.abs().max(1.0) * 8.0;
    let rounded = (scaled + 0.5 + epsilon).floor();
    if !(0.0..=(u128::MAX as f64)).contains(&rounded) {
        return None;
    }
    Some(rounded as u128)
}

fn number_format_rounded_integer_digits(
    value: &Value,
    decimals: i64,
) -> Result<String, BuiltinError> {
    let places = decimals.unsigned_abs() as usize;
    let Some(scale) = pow10_u128(places) else {
        return Ok("0".to_owned());
    };
    match deref_value(value) {
        Value::Int(int) => {
            let absolute = i128::from(int).unsigned_abs();
            Ok((((absolute + (scale / 2)) / scale) * scale).to_string())
        }
        other => {
            let raw_number = numeric_f64_arg("number_format", &other)?;
            let number = raw_number.abs();
            if !number.is_finite() {
                return Ok("0".to_owned());
            }
            if (raw_number.is_sign_negative() || number < 9_223_372_036_854_775_808.0)
                && number.fract() == 0.0
                && (0.0..=(u128::MAX as f64)).contains(&number)
            {
                let absolute = number as u128;
                return Ok((((absolute + (scale / 2)) / scale) * scale).to_string());
            }
            let scale_f64 = scale as f64;
            let rounded = ((number / scale_f64) + 0.5).floor() * scale_f64;
            if !rounded.is_finite() || rounded == 0.0 {
                return Ok("0".to_owned());
            }
            Ok(format!("{rounded:.0}"))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_abs(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("abs", &args, 1)?;
    if matches!(deref_value(&args[0]), Value::Null) {
        emit_null_numeric_deprecation(context, "abs", span);
    }
    Ok(
        match to_number(&args[0]).map_err(|message| conversion_error("abs", message))? {
            NumericValue::Int(value) => value
                .checked_abs()
                .map(Value::Int)
                .unwrap_or_else(|| Value::float((value as f64).abs())),
            NumericValue::Float(value) => Value::float(value.abs()),
        },
    )
}

pub(in crate::builtins::modules) fn builtin_acos(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("acos", &args, f64::acos)
}

pub(in crate::builtins::modules) fn builtin_acosh(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("acosh", &args, f64::acosh)
}

pub(in crate::builtins::modules) fn builtin_asin(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("asin", &args, f64::asin)
}

pub(in crate::builtins::modules) fn builtin_asinh(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("asinh", &args, f64::asinh)
}

pub(in crate::builtins::modules) fn builtin_atan(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("atan", &args, f64::atan)
}

pub(in crate::builtins::modules) fn builtin_atan2(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("atan2", &args, 2)?;
    Ok(Value::float(
        numeric_f64_arg("atan2", &args[0])?.atan2(numeric_f64_arg("atan2", &args[1])?),
    ))
}

pub(in crate::builtins::modules) fn builtin_atanh(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("atanh", &args, f64::atanh)
}

pub(in crate::builtins::modules) fn builtin_base_convert(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("base_convert", &args, 3)?;
    let from_base = base_digit_value("base_convert", "#2 ($from_base)", &args[1])?;
    let to_base = base_digit_value("base_convert", "#3 ($to_base)", &args[2])?;
    let parsed = parse_base_number(
        context,
        "base_convert",
        "#1 ($num)",
        &args[0],
        from_base,
        span,
    )?;
    let output = if parsed.overflowed {
        format_f64_base(parsed.float_value, to_base)
    } else {
        format_u128_base(parsed.int_value, to_base)
    };
    Ok(Value::string(output))
}

pub(in crate::builtins::modules) fn builtin_bindec(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("bindec", &args, 1)?;
    Ok(parsed_base_as_value(parse_base_number(
        context,
        "bindec",
        "#1 ($binary_string)",
        &args[0],
        2,
        span,
    )?))
}

pub(in crate::builtins::modules) fn builtin_decbin(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    decimal_to_base_builtin(context, "decbin", &args, 2, span)
}

pub(in crate::builtins::modules) fn builtin_min(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    min_max_builtin("min", args, false)
}

pub(in crate::builtins::modules) fn builtin_max(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    min_max_builtin("max", args, true)
}

pub(in crate::builtins::modules) fn builtin_round(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=3).contains(&args.len()) {
        return Err(arity_error("round", "one to three argument(s)"));
    }
    let value = round_numeric_arg(context, &args[0], span.clone())?;
    let precision = args
        .get(1)
        .map(|value| round_precision_arg(context, value, span.clone()))
        .transpose()?
        .unwrap_or(0);
    let mode = args.get(2).map(rounding_mode_arg).transpose()?.unwrap_or(1);
    if !(1..=8).contains(&mode) {
        return Err(math_value_error(
            "round(): Argument #3 ($mode) must be a valid rounding mode (RoundingMode::*)",
        ));
    }
    Ok(Value::float(php_round(value, precision, mode)))
}

pub(in crate::builtins::modules) fn builtin_floor(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("floor", &args, 1)?;
    Ok(Value::float(
        numeric_f64_math_arg(context, "floor", &args[0], span)?.floor(),
    ))
}

pub(in crate::builtins::modules) fn builtin_ceil(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("ceil", &args, 1)?;
    Ok(Value::float(
        numeric_f64_math_arg(context, "ceil", &args[0], span)?.ceil(),
    ))
}

pub(in crate::builtins::modules) fn builtin_cos(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("cos", &args, f64::cos)
}

pub(in crate::builtins::modules) fn builtin_cosh(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("cosh", &args, f64::cosh)
}

pub(in crate::builtins::modules) fn builtin_dechex(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    decimal_to_base_builtin(context, "dechex", &args, 16, span)
}

pub(in crate::builtins::modules) fn builtin_decoct(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    decimal_to_base_builtin(context, "decoct", &args, 8, span)
}

pub(in crate::builtins::modules) fn builtin_deg2rad(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("deg2rad", &args, |value| {
        (value / 180.0) * std::f64::consts::PI
    })
}

pub(in crate::builtins::modules) fn builtin_exp(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("exp", &args, f64::exp)
}

pub(in crate::builtins::modules) fn builtin_expm1(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("expm1", &args, f64::exp_m1)
}

pub(in crate::builtins::modules) fn builtin_sqrt(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("sqrt", &args, 1)?;
    Ok(Value::float(numeric_f64_arg("sqrt", &args[0])?.sqrt()))
}

pub(in crate::builtins::modules) fn builtin_pow(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("pow", &args, 2)?;
    let base = pow_numeric_arg(&args[0], &args[1], &args[0])?;
    let exponent = pow_numeric_arg(&args[0], &args[1], &args[1])?;
    emit_zero_negative_power_deprecation(context, base, exponent, span);
    if let (NumericValue::Int(base), NumericValue::Int(exponent)) = (base, exponent)
        && let Ok(unsigned_exponent) = u32::try_from(exponent)
        && let Some(value) = base.checked_pow(unsigned_exponent)
    {
        return Ok(Value::Int(value));
    }
    let result = base.as_f64().powf(exponent.as_f64());
    if let (NumericValue::Int(base), NumericValue::Int(exponent)) = (base, exponent)
        && base < 0
        && exponent > 0
        && exponent % 2 != 0
        && result.is_infinite()
        && result.is_sign_positive()
    {
        return Ok(Value::float(f64::NEG_INFINITY));
    }
    Ok(Value::float(result))
}

pub(in crate::builtins::modules) fn builtin_fpow(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("fpow", &args, 2)?;
    Ok(Value::float(
        numeric_f64_arg("fpow", &args[0])?.powf(numeric_f64_arg("fpow", &args[1])?),
    ))
}

pub(in crate::builtins::modules) fn builtin_getrandmax(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("getrandmax", &args, 0)?;
    Ok(Value::Int(i32::MAX.into()))
}

pub(in crate::builtins::modules) fn builtin_hexdec(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("hexdec", &args, 1)?;
    Ok(parsed_base_as_value(parse_base_number(
        context,
        "hexdec",
        "#1 ($hex_string)",
        &args[0],
        16,
        span,
    )?))
}

pub(in crate::builtins::modules) fn builtin_hypot(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("hypot", &args, 2)?;
    Ok(Value::float(
        numeric_f64_arg("hypot", &args[0])?.hypot(numeric_f64_arg("hypot", &args[1])?),
    ))
}

pub(in crate::builtins::modules) fn builtin_intdiv(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("intdiv", &args, 2)?;
    let dividend = int_arg("intdiv", &args[0])?;
    let divisor = int_arg("intdiv", &args[1])?;
    if divisor == 0 {
        return Err(math_value_error("Division by zero"));
    }
    if dividend == i64::MIN && divisor == -1 {
        return Err(math_value_error(
            "Division of PHP_INT_MIN by -1 is not an integer",
        ));
    }
    Ok(Value::Int(dividend / divisor))
}

pub(in crate::builtins::modules) fn builtin_fmod(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("fmod", &args, 2)?;
    let dividend = numeric_f64_arg("fmod", &args[0])?;
    let divisor = numeric_f64_arg("fmod", &args[1])?;
    if divisor == 0.0 {
        return Ok(Value::float(f64::NAN));
    }
    Ok(Value::float(dividend % divisor))
}

pub(in crate::builtins::modules) fn builtin_fdiv(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("fdiv", &args, 2)?;
    let dividend = numeric_f64_arg("fdiv", &args[0])?;
    let divisor = numeric_f64_arg("fdiv", &args[1])?;
    Ok(Value::float(dividend / divisor))
}

pub(in crate::builtins::modules) fn builtin_is_finite(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_finite", &args, 1)?;
    Ok(Value::Bool(
        numeric_f64_arg("is_finite", &args[0])?.is_finite(),
    ))
}

pub(in crate::builtins::modules) fn builtin_is_infinite(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_infinite", &args, 1)?;
    Ok(Value::Bool(
        numeric_f64_arg("is_infinite", &args[0])?.is_infinite(),
    ))
}

pub(in crate::builtins::modules) fn builtin_is_nan(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_nan", &args, 1)?;
    Ok(Value::Bool(numeric_f64_arg("is_nan", &args[0])?.is_nan()))
}

pub(in crate::builtins::modules) fn builtin_log(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error("log", "one to two argument(s)"));
    }
    let value = numeric_f64_arg("log", &args[0])?;
    let result = match args.get(1) {
        Some(base) => {
            let base = numeric_f64_arg("log", base)?;
            if base <= 0.0 {
                return Err(math_value_error(
                    "log(): Argument #2 ($base) must be greater than 0",
                ));
            }
            value.log(base)
        }
        None => value.ln(),
    };
    Ok(Value::float(result))
}

pub(in crate::builtins::modules) fn builtin_log10(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("log10", &args, f64::log10)
}

pub(in crate::builtins::modules) fn builtin_log1p(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("log1p", &args, f64::ln_1p)
}

pub(in crate::builtins::modules) fn builtin_octdec(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("octdec", &args, 1)?;
    Ok(parsed_base_as_value(parse_base_number(
        context,
        "octdec",
        "#1 ($octal_string)",
        &args[0],
        8,
        span,
    )?))
}

pub(in crate::builtins::modules) fn builtin_pi(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("pi", &args, 0)?;
    Ok(Value::float(std::f64::consts::PI))
}

pub(in crate::builtins::modules) fn builtin_rad2deg(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("rad2deg", &args, |value| {
        (value / std::f64::consts::PI) * 180.0
    })
}

pub(in crate::builtins::modules) fn builtin_sin(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("sin", &args, f64::sin)
}

pub(in crate::builtins::modules) fn builtin_sinh(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("sinh", &args, f64::sinh)
}

pub(in crate::builtins::modules) fn builtin_tan(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("tan", &args, f64::tan)
}

pub(in crate::builtins::modules) fn builtin_tanh(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unary_float_builtin("tanh", &args, f64::tanh)
}

pub(in crate::builtins::modules) fn builtin_number_format(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=4).contains(&args.len()) {
        return Err(arity_error("number_format", "one to four argument(s)"));
    }
    let decimals = args
        .get(1)
        .map(|value| int_arg("number_format", value))
        .transpose()?
        .unwrap_or(0);
    let decimal_separator = number_format_separator(args.get(2), ".")?;
    let thousands_separator = number_format_separator(args.get(3), ",")?;
    let decimal_separator = decimal_separator.to_string_lossy();
    let thousands_separator = thousands_separator.to_string_lossy();
    let sign_negative = match deref_value(&args[0]) {
        Value::Int(value) => value < 0,
        Value::Float(value) => value.to_f64().is_sign_negative(),
        other => numeric_f64_arg("number_format", &other)?.is_sign_negative(),
    };

    let mut grouped = if decimals < 0 {
        let digits = number_format_rounded_integer_digits(&args[0], decimals)?;
        group_decimal_integer(&digits, &thousands_separator)
    } else {
        let decimals = decimals as usize;
        match deref_value(&args[0]) {
            Value::Int(value) => {
                if let Some(scale) = pow10_u128(decimals)
                    && let Some(units) = i128::from(value).unsigned_abs().checked_mul(scale)
                {
                    format_number_format_parts(
                        units,
                        decimals,
                        &decimal_separator,
                        &thousands_separator,
                    )
                } else {
                    let rounded = format!("{:.*}", decimals, (value as f64).abs());
                    let (integer, fraction) = rounded.split_once('.').unwrap_or((&rounded, ""));
                    let mut grouped = group_decimal_integer(integer, &thousands_separator);
                    if decimals > 0 {
                        grouped.push_str(&decimal_separator);
                        grouped.push_str(fraction);
                    }
                    grouped
                }
            }
            other if decimals <= 18 => {
                let value = numeric_f64_arg("number_format", &other)?;
                match number_format_float_units(value, decimals) {
                    Some(units) => format_number_format_parts(
                        units,
                        decimals,
                        &decimal_separator,
                        &thousands_separator,
                    ),
                    None => {
                        let rounded = format!("{:.*}", decimals, value.abs());
                        let (integer, fraction) = rounded.split_once('.').unwrap_or((&rounded, ""));
                        let mut grouped = group_decimal_integer(integer, &thousands_separator);
                        if decimals > 0 {
                            grouped.push_str(&decimal_separator);
                            grouped.push_str(fraction);
                        }
                        grouped
                    }
                }
            }
            other => {
                let value = numeric_f64_arg("number_format", &other)?;
                let rounded = format!("{:.*}", decimals, value.abs());
                let (integer, fraction) = rounded.split_once('.').unwrap_or((&rounded, ""));
                let mut grouped = group_decimal_integer(integer, &thousands_separator);
                if decimals > 0 {
                    grouped.push_str(&decimal_separator);
                    grouped.push_str(fraction);
                }
                grouped
            }
        }
    };
    if sign_negative
        && grouped
            .bytes()
            .any(|byte| byte.is_ascii_digit() && byte != b'0')
    {
        grouped.insert(0, '-');
    }
    Ok(Value::string(grouped))
}

#[cfg(test)]
mod tests {
    use super::{number_format_rounded_integer_digits, php_round};
    use crate::Value;

    #[test]
    fn round_matches_php_prerounding_and_large_exponents() {
        assert_eq!(php_round(0.285, 2, 1), 0.29);
        assert_eq!(php_round(12.3456789000e10, 14, 1), 123456789000.0);
        assert_eq!(php_round(2e-23, 23, 1), 2e-23);
        assert_eq!(php_round(1e-23, 23, 1), 1e-23);
        assert_eq!(php_round(2e24, -24, 1), 2e24);
        assert_eq!(php_round(1e24, -24, 1), 1e24);
    }

    #[test]
    fn round_mode_numbers_match_php_constants() {
        assert_eq!(php_round(0.61, 0, 5), 1.0);
        assert_eq!(php_round(-0.61, 0, 5), -0.0);
        assert_eq!(php_round(0.61, 0, 6), 0.0);
        assert_eq!(php_round(-0.61, 0, 6), -1.0);
        assert_eq!(php_round(0.61, 0, 7), 0.0);
        assert_eq!(php_round(-0.61, 0, 7), -0.0);
        assert_eq!(php_round(0.61, 0, 8), 1.0);
        assert_eq!(php_round(-0.61, 0, 8), -1.0);
    }

    #[test]
    fn number_format_negative_precision_matches_large_negative_float_quirk() {
        assert_eq!(
            number_format_rounded_integer_digits(&Value::float(9_223_372_036_854_775_808.0), -5)
                .unwrap(),
            "9223372036854800384"
        );
        assert_eq!(
            number_format_rounded_integer_digits(&Value::float(9_223_372_036_854_774_784.0), -5)
                .unwrap(),
            "9223372036854800000"
        );
        assert_eq!(
            number_format_rounded_integer_digits(&Value::float(9_223_372_036_854_774_784.0), -1)
                .unwrap(),
            "9223372036854774780"
        );
        assert_eq!(
            number_format_rounded_integer_digits(&Value::float(-9_223_372_036_854_775_808.0), -5)
                .unwrap(),
            "9223372036854800000"
        );
        assert_eq!(
            number_format_rounded_integer_digits(&Value::float(-9_223_372_036_854_775_808.0), -1)
                .unwrap(),
            "9223372036854775810"
        );
    }
}
