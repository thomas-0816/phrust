//! Bounded BigInt-backed bcmath MVP.

use super::core::{argument_value_error, arity_error, int_arg, string_arg};
use crate::Value;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use num_bigint::{BigInt, Sign};
use num_traits::{One, Signed, Zero};
use std::cmp::Ordering;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("bcadd", builtin_bcadd, BuiltinCompatibility::Php),
    BuiltinEntry::new("bccomp", builtin_bccomp, BuiltinCompatibility::Php),
    BuiltinEntry::new("bcdiv", builtin_bcdiv, BuiltinCompatibility::Php),
    BuiltinEntry::new("bcmod", builtin_bcmod, BuiltinCompatibility::Php),
    BuiltinEntry::new("bcmul", builtin_bcmul, BuiltinCompatibility::Php),
    BuiltinEntry::new("bcpow", builtin_bcpow, BuiltinCompatibility::Php),
    BuiltinEntry::new("bcpowmod", builtin_bcpowmod, BuiltinCompatibility::Php),
    BuiltinEntry::new("bcscale", builtin_bcscale, BuiltinCompatibility::Php),
    BuiltinEntry::new("bcsqrt", builtin_bcsqrt, BuiltinCompatibility::Php),
    BuiltinEntry::new("bcsub", builtin_bcsub, BuiltinCompatibility::Php),
];

#[derive(Clone, Debug, Eq, PartialEq)]
struct Decimal {
    units: BigInt,
    scale: usize,
}

fn builtin_bcadd(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    binary_decimal(
        "bcadd",
        context.bcmath_scale(),
        args,
        |left, right, scale| {
            let common = left.scale.max(right.scale);
            let value = left.units_scaled(common) + right.units_scaled(common);
            Ok(Decimal {
                units: rescale_units(value, common, scale),
                scale,
            }
            .format())
        },
    )
}

fn builtin_bcsub(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    binary_decimal(
        "bcsub",
        context.bcmath_scale(),
        args,
        |left, right, scale| {
            let common = left.scale.max(right.scale);
            let value = left.units_scaled(common) - right.units_scaled(common);
            Ok(Decimal {
                units: rescale_units(value, common, scale),
                scale,
            }
            .format())
        },
    )
}

fn builtin_bcmul(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    binary_decimal(
        "bcmul",
        context.bcmath_scale(),
        args,
        |left, right, scale| {
            let raw_scale = left.scale + right.scale;
            Ok(Decimal {
                units: rescale_units(left.units * right.units, raw_scale, scale),
                scale,
            }
            .format())
        },
    )
}

fn builtin_bcdiv(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    binary_decimal(
        "bcdiv",
        context.bcmath_scale(),
        args,
        |left, right, scale| {
            if right.units.is_zero() {
                return Err(argument_value_error(
                    "bcdiv",
                    "#2 ($num2)",
                    "must not be zero",
                ));
            }
            let mut numerator = left.units;
            let mut denominator = right.units;
            let exponent = scale as isize + right.scale as isize - left.scale as isize;
            if exponent >= 0 {
                numerator *= pow10(exponent as usize);
            } else {
                denominator *= pow10((-exponent) as usize);
            }
            Ok(Decimal {
                units: numerator / denominator,
                scale,
            }
            .format())
        },
    )
}

fn builtin_bcmod(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("bcmod", "two or three arguments"));
    }
    let left = parse_decimal_arg("bcmod", &args[0])?;
    let right = parse_decimal_arg("bcmod", &args[1])?;
    if right.units.is_zero() {
        return Err(argument_value_error(
            "bcmod",
            "#2 ($num2)",
            "must not be zero",
        ));
    }
    let scale = scale_arg("bcmod", args.get(2), context.bcmath_scale())?;
    let common = left.scale.max(right.scale);
    Ok(Value::string(
        Decimal {
            units: left.units_scaled(common) % right.units_scaled(common),
            scale: 0,
        }
        .rescaled(scale)
        .format(),
    ))
}

fn builtin_bcpow(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("bcpow", "two or three arguments"));
    }
    let base = parse_decimal_arg("bcpow", &args[0])?;
    let exponent_text = string_arg("bcpow", &args[1])?.to_string_lossy();
    let exponent = exponent_text.trim().parse::<u32>().map_err(|_| {
        argument_value_error("bcpow", "#2 ($exponent)", "must be a non-negative integer")
    })?;
    let scale = scale_arg("bcpow", args.get(2), context.bcmath_scale())?;
    let units = base.units.pow(exponent);
    let raw_scale = base.scale.saturating_mul(exponent as usize);
    Ok(Value::string(
        Decimal {
            units: rescale_units(units, raw_scale, scale),
            scale,
        }
        .format(),
    ))
}

fn builtin_bcpowmod(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=4).contains(&args.len()) {
        return Err(arity_error("bcpowmod", "three or four arguments"));
    }
    let base = integer_decimal_arg("bcpowmod", "#1 ($num)", &args[0])?;
    let mut exponent = integer_decimal_arg("bcpowmod", "#2 ($exponent)", &args[1])?;
    if exponent.sign() == Sign::Minus {
        return Err(argument_value_error(
            "bcpowmod",
            "#2 ($exponent)",
            "must be greater than or equal to 0",
        ));
    }
    let modulus = integer_decimal_arg("bcpowmod", "#3 ($modulus)", &args[2])?;
    if modulus.is_zero() {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_VALUE",
            "Modulo by zero",
        ));
    }
    let scale = scale_arg("bcpowmod", args.get(3), context.bcmath_scale())?;
    let modulus = modulus.abs();
    let two = BigInt::from(2_u8);
    let mut result = BigInt::one() % &modulus;
    let mut factor = base % &modulus;
    while !exponent.is_zero() {
        if (&exponent % &two).is_one() {
            result = (result * &factor) % &modulus;
        }
        exponent /= &two;
        if !exponent.is_zero() {
            factor = (&factor * &factor) % &modulus;
        }
    }
    Ok(Value::string(
        Decimal {
            units: result,
            scale: 0,
        }
        .rescaled(scale)
        .format(),
    ))
}

fn builtin_bcsqrt(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error("bcsqrt", "one or two arguments"));
    }
    let value = parse_decimal_arg("bcsqrt", &args[0])?;
    if value.units.sign() == Sign::Minus {
        return Err(argument_value_error(
            "bcsqrt",
            "#1 ($num)",
            "must be greater than or equal to 0",
        ));
    }
    let scale = scale_arg("bcsqrt", args.get(1), context.bcmath_scale())?;
    let numerator = value.units * pow10(scale.saturating_mul(2));
    let denominator = pow10(value.scale);
    let units = integer_sqrt_rational_floor(&numerator, &denominator);
    Ok(Value::string(Decimal { units, scale }.format()))
}

fn builtin_bccomp(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("bccomp", "two or three arguments"));
    }
    let scale = scale_arg("bccomp", args.get(2), context.bcmath_scale())?;
    let left = parse_decimal_arg("bccomp", &args[0])?.rescaled(scale);
    let right = parse_decimal_arg("bccomp", &args[1])?.rescaled(scale);
    Ok(Value::Int(match left.units.cmp(&right.units) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }))
}

fn builtin_bcscale(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("bcscale", "zero or one argument"));
    }
    if let Some(value) = args.first() {
        let scale = int_arg("bcscale", value)?;
        if scale < 0 {
            return Err(argument_value_error(
                "bcscale",
                "#1 ($scale)",
                "must be non-negative",
            ));
        }
        return Ok(Value::Int(context.set_bcmath_scale(scale as usize) as i64));
    }
    Ok(Value::Int(context.bcmath_scale() as i64))
}

fn binary_decimal(
    name: &'static str,
    default_scale: usize,
    args: Vec<Value>,
    operation: impl FnOnce(Decimal, Decimal, usize) -> Result<Vec<u8>, BuiltinError>,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error(name, "two or three arguments"));
    }
    let left = parse_decimal_arg(name, &args[0])?;
    let right = parse_decimal_arg(name, &args[1])?;
    let scale = scale_arg(name, args.get(2), default_scale)?;
    operation(left, right, scale).map(Value::string)
}

fn parse_decimal_arg(name: &str, value: &Value) -> Result<Decimal, BuiltinError> {
    let text = string_arg(name, value)?.to_string_lossy();
    parse_decimal(&text).ok_or_else(|| argument_value_error(name, "number", "must be decimal"))
}

fn integer_decimal_arg(name: &str, argument: &str, value: &Value) -> Result<BigInt, BuiltinError> {
    let decimal = parse_decimal_arg(name, value)?;
    if decimal.scale == 0 {
        return Ok(decimal.units);
    }
    let divisor = pow10(decimal.scale);
    if (&decimal.units % &divisor).is_zero() {
        Ok(decimal.units / divisor)
    } else {
        Err(argument_value_error(
            name,
            argument,
            "cannot have a fractional part",
        ))
    }
}

fn parse_decimal(text: &str) -> Option<Decimal> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (negative, rest) = match trimmed.as_bytes()[0] {
        b'-' => (true, &trimmed[1..]),
        b'+' => (false, &trimmed[1..]),
        _ => (false, trimmed),
    };
    let (integer, fraction) = rest.split_once('.').unwrap_or((rest, ""));
    if integer.is_empty() && fraction.is_empty() {
        return None;
    }
    if !integer.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let digits = format!(
        "{}{}",
        if integer.is_empty() { "0" } else { integer },
        fraction
    );
    let mut units = BigInt::parse_bytes(digits.as_bytes(), 10)?;
    if negative && !units.is_zero() {
        units = -units;
    }
    Some(Decimal {
        units,
        scale: fraction.len(),
    })
}

impl Decimal {
    fn units_scaled(&self, scale: usize) -> BigInt {
        if scale <= self.scale {
            rescale_units(self.units.clone(), self.scale, scale)
        } else {
            &self.units * pow10(scale - self.scale)
        }
    }

    fn rescaled(self, scale: usize) -> Self {
        Self {
            units: rescale_units(self.units, self.scale, scale),
            scale,
        }
    }

    fn format(&self) -> Vec<u8> {
        let negative = self.units.sign() == Sign::Minus;
        let mut digits = self.units.abs().to_str_radix(10);
        if self.scale == 0 {
            if negative && !self.units.is_zero() {
                digits.insert(0, '-');
            }
            return digits.into_bytes();
        }
        if digits.len() <= self.scale {
            digits.insert_str(0, &"0".repeat(self.scale + 1 - digits.len()));
        }
        let split = digits.len() - self.scale;
        digits.insert(split, '.');
        if negative && !self.units.is_zero() {
            digits.insert(0, '-');
        }
        digits.into_bytes()
    }
}

fn scale_arg(
    name: &str,
    value: Option<&Value>,
    default_scale: usize,
) -> Result<usize, BuiltinError> {
    let scale = value
        .map(|value| int_arg(name, value))
        .transpose()?
        .unwrap_or(default_scale as i64);
    if scale < 0 {
        Err(argument_value_error(name, "scale", "must be non-negative"))
    } else {
        Ok(scale as usize)
    }
}

fn rescale_units(units: BigInt, from: usize, to: usize) -> BigInt {
    if from == to {
        units
    } else if from < to {
        units * pow10(to - from)
    } else {
        units / pow10(from - to)
    }
}

fn pow10(power: usize) -> BigInt {
    let mut value = BigInt::one();
    for _ in 0..power {
        value *= 10;
    }
    value
}

fn integer_sqrt_rational_floor(numerator: &BigInt, denominator: &BigInt) -> BigInt {
    debug_assert!(!denominator.is_zero());
    if numerator.is_zero() {
        return BigInt::zero();
    }
    let mut low = BigInt::zero();
    let mut high = integer_sqrt_floor(numerator) + BigInt::one();
    while &low + BigInt::one() < high {
        let mid = (&low + &high) / 2_u8;
        if &mid * &mid * denominator <= *numerator {
            low = mid;
        } else {
            high = mid;
        }
    }
    low
}

fn integer_sqrt_floor(value: &BigInt) -> BigInt {
    debug_assert!(value.sign() != Sign::Minus);
    if value.is_zero() {
        return BigInt::zero();
    }
    let mut low = BigInt::zero();
    let mut high = value + BigInt::one();
    while &low + BigInt::one() < high {
        let mid = (&low + &high) / 2_u8;
        if &mid * &mid <= *value {
            low = mid;
        } else {
            high = mid;
        }
    }
    low
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OutputBuffer, PhpString};

    fn call(context: &mut BuiltinContext<'_>, name: &str, args: Vec<Value>) -> Value {
        ENTRIES
            .iter()
            .find(|entry| entry.name() == name)
            .expect("bcmath entry")
            .function()(context, args, RuntimeSourceSpan::default())
        .expect("bcmath succeeds")
    }

    fn string(value: &str) -> Value {
        Value::String(PhpString::from_test_str(value))
    }

    #[test]
    fn bcscale_updates_request_local_default_scale() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);

        assert_eq!(call(&mut context, "bcscale", vec![]), Value::Int(0));
        assert_eq!(
            call(&mut context, "bcadd", vec![string("1.2"), string("3.45")]),
            string("4")
        );
        assert_eq!(
            call(&mut context, "bcscale", vec![Value::Int(3)]),
            Value::Int(0)
        );
        assert_eq!(call(&mut context, "bcscale", vec![]), Value::Int(3));
        assert_eq!(
            call(&mut context, "bcadd", vec![string("1.2"), string("3.45")]),
            string("4.650")
        );
        assert_eq!(
            call(
                &mut context,
                "bcadd",
                vec![string("1.2"), string("3.45"), Value::Int(1)]
            ),
            string("4.6")
        );
        assert_eq!(
            call(&mut context, "bcscale", vec![Value::Int(0)]),
            Value::Int(3)
        );
        assert_eq!(call(&mut context, "bcscale", vec![]), Value::Int(0));
    }

    #[test]
    fn bcsqrt_truncates_to_requested_scale() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);

        assert_eq!(
            call(&mut context, "bcsqrt", vec![string("2"), Value::Int(4)]),
            string("1.4142")
        );
        assert_eq!(
            call(
                &mut context,
                "bcsqrt",
                vec![string("0.0004"), Value::Int(4)]
            ),
            string("0.0200")
        );
        assert_eq!(
            call(&mut context, "bcsqrt", vec![string("-0.00"), Value::Int(2)]),
            string("0.00")
        );
    }

    #[test]
    fn bcpowmod_accepts_decimal_integers_and_scales_result() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);

        assert_eq!(
            call(
                &mut context,
                "bcpowmod",
                vec![
                    string("2.0"),
                    string("10.00"),
                    string("1000"),
                    Value::Int(2)
                ]
            ),
            string("24.00")
        );
        assert_eq!(
            call(
                &mut context,
                "bcpowmod",
                vec![string("-2"), string("5"), string("7")]
            ),
            string("-4")
        );
        assert_eq!(
            call(
                &mut context,
                "bcpowmod",
                vec![string("5"), string("0"), string("-1"), Value::Int(3)]
            ),
            string("0.000")
        );
    }
}
