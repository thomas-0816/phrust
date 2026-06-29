//! Bounded BigInt-backed GMP facade.

use super::core::{argument_value_error, arity_error, int_arg, string_arg};
use crate::Value;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use num_bigint::BigInt;
use num_traits::{Signed, ToPrimitive, Zero};
use std::cmp::Ordering;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("gmp_abs", builtin_gmp_abs, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_add", builtin_gmp_add, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_cmp", builtin_gmp_cmp, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_div_q", builtin_gmp_div_q, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_init", builtin_gmp_init, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_intval", builtin_gmp_intval, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_mod", builtin_gmp_mod, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_mul", builtin_gmp_mul, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_neg", builtin_gmp_neg, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_pow", builtin_gmp_pow, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_strval", builtin_gmp_strval, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_sub", builtin_gmp_sub, BuiltinCompatibility::Php),
];

fn builtin_gmp_init(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("gmp_init", "one or two arguments"));
    }
    let base = args
        .get(1)
        .map(|value| int_arg("gmp_init", value))
        .transpose()?
        .unwrap_or(0);
    Ok(Value::string(
        parse_gmp("gmp_init", &args[0], base)?.to_str_radix(10),
    ))
}

fn builtin_gmp_strval(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("gmp_strval", "one or two arguments"));
    }
    let base = args
        .get(1)
        .map(|value| int_arg("gmp_strval", value))
        .transpose()?
        .unwrap_or(10);
    if !(2..=36).contains(&base) {
        return Err(argument_value_error(
            "gmp_strval",
            "#2 ($base)",
            "must be between 2 and 36",
        ));
    }
    Ok(Value::string(
        parse_gmp("gmp_strval", &args[0], 10)?.to_str_radix(base as u32),
    ))
}

fn builtin_gmp_intval(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("gmp_intval", "one argument"));
    }
    Ok(Value::Int(
        parse_gmp("gmp_intval", &args[0], 10)?
            .to_i64()
            .unwrap_or_default(),
    ))
}

macro_rules! gmp_binary {
    ($name:ident, $php_name:literal, $operation:expr) => {
        fn $name(
            _context: &mut BuiltinContext<'_>,
            args: Vec<Value>,
            _span: RuntimeSourceSpan,
        ) -> BuiltinResult {
            if args.len() != 2 {
                return Err(arity_error($php_name, "two arguments"));
            }
            let left = parse_gmp($php_name, &args[0], 10)?;
            let right = parse_gmp($php_name, &args[1], 10)?;
            let value: BigInt = $operation(left, right)?;
            Ok(Value::string(value.to_str_radix(10)))
        }
    };
}

gmp_binary!(
    builtin_gmp_add,
    "gmp_add",
    |left: BigInt, right: BigInt| Ok(left + right)
);
gmp_binary!(
    builtin_gmp_sub,
    "gmp_sub",
    |left: BigInt, right: BigInt| Ok(left - right)
);
gmp_binary!(
    builtin_gmp_mul,
    "gmp_mul",
    |left: BigInt, right: BigInt| Ok(left * right)
);
gmp_binary!(
    builtin_gmp_div_q,
    "gmp_div_q",
    |left: BigInt, right: BigInt| {
        if right.is_zero() {
            Err(argument_value_error(
                "gmp_div_q",
                "#2 ($num2)",
                "must not be zero",
            ))
        } else {
            Ok(left / right)
        }
    }
);
gmp_binary!(builtin_gmp_mod, "gmp_mod", |left: BigInt, right: BigInt| {
    if right.is_zero() {
        Err(argument_value_error(
            "gmp_mod",
            "#2 ($num2)",
            "must not be zero",
        ))
    } else {
        Ok(left % right)
    }
});

fn builtin_gmp_abs(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("gmp_abs", "one argument"));
    }
    Ok(Value::string(
        parse_gmp("gmp_abs", &args[0], 10)?.abs().to_str_radix(10),
    ))
}

fn builtin_gmp_neg(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("gmp_neg", "one argument"));
    }
    Ok(Value::string(
        (-parse_gmp("gmp_neg", &args[0], 10)?).to_str_radix(10),
    ))
}

fn builtin_gmp_pow(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("gmp_pow", "two arguments"));
    }
    let base = parse_gmp("gmp_pow", &args[0], 10)?;
    let exponent = int_arg("gmp_pow", &args[1])?;
    if exponent < 0 {
        return Err(argument_value_error(
            "gmp_pow",
            "#2 ($exponent)",
            "must be non-negative",
        ));
    }
    Ok(Value::string(base.pow(exponent as u32).to_str_radix(10)))
}

fn builtin_gmp_cmp(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("gmp_cmp", "two arguments"));
    }
    let left = parse_gmp("gmp_cmp", &args[0], 10)?;
    let right = parse_gmp("gmp_cmp", &args[1], 10)?;
    Ok(Value::Int(match left.cmp(&right) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }))
}

fn parse_gmp(name: &str, value: &Value, base: i64) -> Result<BigInt, BuiltinError> {
    match value {
        Value::Int(value) => Ok(BigInt::from(*value)),
        _ => {
            let text = string_arg(name, value)?.to_string_lossy();
            parse_gmp_text(&text, base)
                .ok_or_else(|| argument_value_error(name, "number", "must be an integer string"))
        }
    }
}

fn parse_gmp_text(text: &str, base: i64) -> Option<BigInt> {
    let mut text = text.trim();
    let negative = if let Some(rest) = text.strip_prefix('-') {
        text = rest;
        true
    } else {
        false
    };
    if let Some(rest) = text.strip_prefix('+') {
        text = rest;
    }
    let effective_base = if base == 0 {
        if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            text = rest;
            16
        } else {
            10
        }
    } else {
        base
    };
    if !(2..=36).contains(&effective_base) {
        return None;
    }
    let mut value = BigInt::parse_bytes(text.as_bytes(), effective_base as u32)?;
    if negative {
        value = -value;
    }
    Some(value)
}
