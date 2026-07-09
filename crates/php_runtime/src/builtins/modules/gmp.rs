//! Bounded BigInt-backed GMP facade.

use super::core::{argument_value_error, arity_error, int_arg, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{ArrayKey, PhpArray, PhpString, Value};
use num_bigint::{BigInt, BigUint};
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::cmp::Ordering;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("gmp_abs", builtin_gmp_abs, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_add", builtin_gmp_add, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_and", builtin_gmp_and, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "gmp_binomial",
        builtin_gmp_binomial,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("gmp_cmp", builtin_gmp_cmp, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_com", builtin_gmp_com, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_div", builtin_gmp_div_q, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_div_q", builtin_gmp_div_q, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_div_qr", builtin_gmp_div_qr, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_div_r", builtin_gmp_div_r, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_divexact", builtin_gmp_div_q, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_export", builtin_gmp_export, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_fact", builtin_gmp_fact, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_gcd", builtin_gmp_gcd, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_gcdext", builtin_gmp_gcdext, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "gmp_hamdist",
        builtin_gmp_hamdist,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("gmp_import", builtin_gmp_import, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_init", builtin_gmp_init, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_intval", builtin_gmp_intval, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_invert", builtin_gmp_invert, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_jacobi", builtin_gmp_jacobi, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "gmp_kronecker",
        builtin_gmp_kronecker,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("gmp_lcm", builtin_gmp_lcm, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "gmp_legendre",
        builtin_gmp_legendre,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("gmp_mod", builtin_gmp_mod, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_mul", builtin_gmp_mul, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_neg", builtin_gmp_neg, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "gmp_nextprime",
        builtin_gmp_nextprime,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("gmp_or", builtin_gmp_or, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "gmp_perfect_power",
        builtin_gmp_perfect_power,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "gmp_perfect_square",
        builtin_gmp_perfect_square,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "gmp_popcount",
        builtin_gmp_popcount,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("gmp_pow", builtin_gmp_pow, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_powm", builtin_gmp_powm, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "gmp_prob_prime",
        builtin_gmp_prob_prime,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "gmp_random_bits",
        builtin_gmp_random_bits,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "gmp_random_range",
        builtin_gmp_random_range,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "gmp_random_seed",
        builtin_gmp_random_seed,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("gmp_root", builtin_gmp_root, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "gmp_rootrem",
        builtin_gmp_rootrem,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("gmp_scan0", builtin_gmp_scan0, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_scan1", builtin_gmp_scan1, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_sign", builtin_gmp_sign, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_sqrt", builtin_gmp_sqrt, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "gmp_sqrtrem",
        builtin_gmp_sqrtrem,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("gmp_strval", builtin_gmp_strval, BuiltinCompatibility::Php),
    BuiltinEntry::new("gmp_sub", builtin_gmp_sub, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "gmp_testbit",
        builtin_gmp_testbit,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("gmp_xor", builtin_gmp_xor, BuiltinCompatibility::Php),
];

fn builtin_gmp_init(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("gmp_init", "one or two arguments"));
    }
    let base = optional_int("gmp_init", &args, 1, 0)?;
    Ok(gmp_value(parse_gmp("gmp_init", &args[0], base)?))
}

fn builtin_gmp_import(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error("gmp_import", "one to three arguments"));
    }
    let bytes = string_arg("gmp_import", &args[0])?;
    let word_size = optional_int("gmp_import", &args, 1, 1)?;
    if word_size <= 0 {
        return Err(argument_value_error(
            "gmp_import",
            "#2 ($word_size)",
            "must be greater than 0",
        ));
    }
    Ok(gmp_value(BigInt::from(BigUint::from_bytes_be(
        bytes.as_bytes(),
    ))))
}

fn builtin_gmp_export(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error("gmp_export", "one to three arguments"));
    }
    let word_size = optional_int("gmp_export", &args, 1, 1)?;
    if word_size <= 0 {
        return Err(argument_value_error(
            "gmp_export",
            "#2 ($word_size)",
            "must be greater than 0",
        ));
    }
    let value = parse_gmp("gmp_export", &args[0], 10)?;
    let bytes = value
        .abs()
        .to_biguint()
        .map_or_else(Vec::new, |value| value.to_bytes_be());
    Ok(Value::string(bytes))
}

fn builtin_gmp_strval(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("gmp_strval", "one or two arguments"));
    }
    let base = optional_int("gmp_strval", &args, 1, 10)?;
    if !valid_output_base(base) {
        return Err(argument_value_error(
            "gmp_strval",
            "#2 ($base)",
            "must be between 2 and 62, or -2 and -36",
        ));
    }
    Ok(Value::string(bigint_to_radix(
        &parse_gmp("gmp_strval", &args[0], 10)?,
        base,
    )))
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
            Ok(gmp_value(value))
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
    builtin_gmp_and,
    "gmp_and",
    |left: BigInt, right: BigInt| Ok(left & right)
);
gmp_binary!(builtin_gmp_or, "gmp_or", |left: BigInt, right: BigInt| Ok(
    left | right
));
gmp_binary!(
    builtin_gmp_xor,
    "gmp_xor",
    |left: BigInt, right: BigInt| Ok(left ^ right)
);

fn builtin_gmp_div_q(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error("gmp_div_q", "two or three arguments"));
    }
    let (quotient, _) = gmp_divide("gmp_div_q", &args[0], &args[1])?;
    Ok(gmp_value(quotient))
}

fn builtin_gmp_div_r(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error("gmp_div_r", "two or three arguments"));
    }
    let (_, remainder) = gmp_divide("gmp_div_r", &args[0], &args[1])?;
    Ok(gmp_value(remainder))
}

fn builtin_gmp_div_qr(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error("gmp_div_qr", "two or three arguments"));
    }
    let (quotient, remainder) = gmp_divide("gmp_div_qr", &args[0], &args[1])?;
    Ok(Value::packed_array(vec![
        gmp_value(quotient),
        gmp_value(remainder),
    ]))
}

fn builtin_gmp_mod(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("gmp_mod", "two arguments"));
    }
    let left = parse_gmp("gmp_mod", &args[0], 10)?;
    let modulus = parse_gmp("gmp_mod", &args[1], 10)?.abs();
    if modulus.is_zero() {
        return Err(argument_value_error(
            "gmp_mod",
            "#2 ($num2)",
            "must not be zero",
        ));
    }
    let mut remainder = left % &modulus;
    if remainder.is_negative() {
        remainder += &modulus;
    }
    Ok(gmp_value(remainder))
}

fn builtin_gmp_abs(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("gmp_abs", "one argument"));
    }
    Ok(gmp_value(parse_gmp("gmp_abs", &args[0], 10)?.abs()))
}

fn builtin_gmp_neg(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("gmp_neg", "one argument"));
    }
    Ok(gmp_value(-parse_gmp("gmp_neg", &args[0], 10)?))
}

fn builtin_gmp_com(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("gmp_com", "one argument"));
    }
    Ok(gmp_value(!parse_gmp("gmp_com", &args[0], 10)?))
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
    Ok(gmp_value(base.pow(exponent as u32)))
}

fn builtin_gmp_powm(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("gmp_powm", "three arguments"));
    }
    let base = parse_gmp("gmp_powm", &args[0], 10)?;
    let exponent = parse_gmp("gmp_powm", &args[1], 10)?;
    let modulus = parse_gmp("gmp_powm", &args[2], 10)?;
    if exponent.is_negative() || modulus.is_zero() {
        return Err(argument_value_error(
            "gmp_powm",
            "arguments",
            "must use a non-negative exponent and non-zero modulus",
        ));
    }
    Ok(gmp_value(modpow(base, exponent, modulus.abs())))
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

fn builtin_gmp_sign(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("gmp_sign", "one argument"));
    }
    Ok(Value::Int(
        match parse_gmp("gmp_sign", &args[0], 10)?.cmp(&BigInt::zero()) {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        },
    ))
}

fn builtin_gmp_gcd(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("gmp_gcd", "two arguments"));
    }
    Ok(gmp_value(gcd(
        parse_gmp("gmp_gcd", &args[0], 10)?,
        parse_gmp("gmp_gcd", &args[1], 10)?,
    )))
}

fn builtin_gmp_gcdext(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("gmp_gcdext", "two arguments"));
    }
    let (g, s, t) = extended_gcd(
        parse_gmp("gmp_gcdext", &args[0], 10)?,
        parse_gmp("gmp_gcdext", &args[1], 10)?,
    );
    let mut array = PhpArray::new();
    array.insert(ArrayKey::String(PhpString::from("g")), gmp_value(g));
    array.insert(ArrayKey::String(PhpString::from("s")), gmp_value(s));
    array.insert(ArrayKey::String(PhpString::from("t")), gmp_value(t));
    Ok(Value::Array(array))
}

fn builtin_gmp_lcm(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("gmp_lcm", "two arguments"));
    }
    let left = parse_gmp("gmp_lcm", &args[0], 10)?;
    let right = parse_gmp("gmp_lcm", &args[1], 10)?;
    if left.is_zero() || right.is_zero() {
        return Ok(gmp_value(BigInt::zero()));
    }
    let divisor = gcd(left.clone(), right.clone());
    Ok(gmp_value(((left / divisor) * right).abs()))
}

fn builtin_gmp_jacobi(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    binary_symbol("gmp_jacobi", args)
}

fn builtin_gmp_legendre(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    binary_symbol("gmp_legendre", args)
}

fn builtin_gmp_kronecker(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    binary_symbol("gmp_kronecker", args)
}

fn builtin_gmp_invert(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("gmp_invert", "two arguments"));
    }
    let value = parse_gmp("gmp_invert", &args[0], 10)?;
    let modulus = parse_gmp("gmp_invert", &args[1], 10)?;
    if modulus.is_zero() {
        return Ok(Value::Bool(false));
    }
    let (g, s, _) = extended_gcd(value, modulus.clone());
    if g != BigInt::one() {
        return Ok(Value::Bool(false));
    }
    let modulus = modulus.abs();
    let mut result = s % &modulus;
    if result.is_negative() {
        result += &modulus;
    }
    Ok(gmp_value(result))
}

fn builtin_gmp_sqrt(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("gmp_sqrt", "one argument"));
    }
    Ok(gmp_value(integer_nth_root(
        "gmp_sqrt",
        parse_gmp("gmp_sqrt", &args[0], 10)?,
        2,
    )?))
}

fn builtin_gmp_sqrtrem(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("gmp_sqrtrem", "one argument"));
    }
    let value = parse_gmp("gmp_sqrtrem", &args[0], 10)?;
    let root = integer_nth_root("gmp_sqrtrem", value.clone(), 2)?;
    let remainder = value - (&root * &root);
    Ok(Value::packed_array(vec![
        gmp_value(root),
        gmp_value(remainder),
    ]))
}

fn builtin_gmp_root(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("gmp_root", "two arguments"));
    }
    let nth = int_arg("gmp_root", &args[1])?;
    Ok(gmp_value(integer_nth_root(
        "gmp_root",
        parse_gmp("gmp_root", &args[0], 10)?,
        nth,
    )?))
}

fn builtin_gmp_rootrem(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("gmp_rootrem", "two arguments"));
    }
    let nth = int_arg("gmp_rootrem", &args[1])?;
    let value = parse_gmp("gmp_rootrem", &args[0], 10)?;
    let root = integer_nth_root("gmp_rootrem", value.clone(), nth)?;
    let remainder = value - root.pow(nth as u32);
    Ok(Value::packed_array(vec![
        gmp_value(root),
        gmp_value(remainder),
    ]))
}

fn builtin_gmp_perfect_square(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("gmp_perfect_square", "one argument"));
    }
    let value = parse_gmp("gmp_perfect_square", &args[0], 10)?;
    if value.is_negative() {
        return Ok(Value::Bool(false));
    }
    let root = integer_nth_root("gmp_perfect_square", value.clone(), 2)?;
    Ok(Value::Bool(&root * &root == value))
}

fn builtin_gmp_perfect_power(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("gmp_perfect_power", "one argument"));
    }
    let value = parse_gmp("gmp_perfect_power", &args[0], 10)?;
    if value <= BigInt::one() {
        return Ok(Value::Bool(true));
    }
    for nth in 2..=64 {
        let root = integer_nth_root("gmp_perfect_power", value.clone(), nth)?;
        if root > BigInt::one() && root.pow(nth as u32) == value {
            return Ok(Value::Bool(true));
        }
    }
    Ok(Value::Bool(false))
}

fn builtin_gmp_prob_prime(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("gmp_prob_prime", "one or two arguments"));
    }
    let value = parse_gmp("gmp_prob_prime", &args[0], 10)?;
    Ok(Value::Int(if is_prime(&value) { 2 } else { 0 }))
}

fn builtin_gmp_nextprime(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("gmp_nextprime", "one argument"));
    }
    let mut value = parse_gmp("gmp_nextprime", &args[0], 10)? + 1;
    while !is_prime(&value) {
        value += 1;
    }
    Ok(gmp_value(value))
}

fn builtin_gmp_fact(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("gmp_fact", "one argument"));
    }
    let n = parse_gmp("gmp_fact", &args[0], 10)?;
    let n = n.to_u64().ok_or_else(|| {
        argument_value_error("gmp_fact", "#1 ($num)", "must fit a non-negative integer")
    })?;
    let mut result = BigInt::one();
    for value in 2..=n {
        result *= value;
    }
    Ok(gmp_value(result))
}

fn builtin_gmp_binomial(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("gmp_binomial", "two arguments"));
    }
    let n = parse_gmp("gmp_binomial", &args[0], 10)?;
    let n = n.to_u64().ok_or_else(|| {
        argument_value_error("gmp_binomial", "#1 ($n)", "must fit a non-negative integer")
    })?;
    let k = int_arg("gmp_binomial", &args[1])?;
    if k < 0 {
        return Ok(gmp_value(BigInt::zero()));
    }
    let k = (k as u64).min(n.saturating_sub(k as u64));
    let mut result = BigInt::one();
    for i in 0..k {
        result *= n - i;
        result /= i + 1;
    }
    Ok(gmp_value(result))
}

fn builtin_gmp_testbit(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("gmp_testbit", "two arguments"));
    }
    let index = bit_index("gmp_testbit", &args[1])?;
    let value = parse_gmp("gmp_testbit", &args[0], 10)?;
    Ok(Value::Bool(
        ((value >> index) & BigInt::one()) == BigInt::one(),
    ))
}

fn builtin_gmp_popcount(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("gmp_popcount", "one argument"));
    }
    let value = parse_gmp("gmp_popcount", &args[0], 10)?;
    if value.is_negative() {
        return Ok(Value::Int(-1));
    }
    Ok(Value::Int(count_ones(value)))
}

fn builtin_gmp_hamdist(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("gmp_hamdist", "two arguments"));
    }
    let left = parse_gmp("gmp_hamdist", &args[0], 10)?;
    let right = parse_gmp("gmp_hamdist", &args[1], 10)?;
    if left.is_negative() || right.is_negative() {
        return Ok(Value::Int(-1));
    }
    Ok(Value::Int(count_ones(left ^ right)))
}

fn builtin_gmp_scan1(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    scan_bit("gmp_scan1", args, true)
}

fn builtin_gmp_scan0(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    scan_bit("gmp_scan0", args, false)
}

fn builtin_gmp_random_bits(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("gmp_random_bits", "one argument"));
    }
    let bits = int_arg("gmp_random_bits", &args[0])?;
    if bits < 0 {
        return Err(argument_value_error(
            "gmp_random_bits",
            "#1 ($bits)",
            "must be non-negative",
        ));
    }
    Ok(gmp_value(BigInt::zero()))
}

fn builtin_gmp_random_range(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("gmp_random_range", "two arguments"));
    }
    let min = parse_gmp("gmp_random_range", &args[0], 10)?;
    let max = parse_gmp("gmp_random_range", &args[1], 10)?;
    if min > max {
        return Err(argument_value_error(
            "gmp_random_range",
            "arguments",
            "minimum must be less than or equal to maximum",
        ));
    }
    Ok(gmp_value(min))
}

fn builtin_gmp_random_seed(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("gmp_random_seed", "one argument"));
    }
    parse_gmp("gmp_random_seed", &args[0], 10)?;
    Ok(Value::Null)
}

fn binary_symbol(name: &'static str, args: Vec<Value>) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error(name, "two arguments"));
    }
    let left = parse_gmp(name, &args[0], 10)?;
    let right = parse_gmp(name, &args[1], 10)?;
    Ok(Value::Int(kronecker_symbol(left, right)))
}

fn optional_int(
    name: &str,
    args: &[Value],
    index: usize,
    default: i64,
) -> Result<i64, BuiltinError> {
    args.get(index)
        .map(|value| int_arg(name, value))
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn gmp_value(value: BigInt) -> Value {
    Value::string(value.to_str_radix(10))
}

fn gmp_divide(name: &str, left: &Value, right: &Value) -> Result<(BigInt, BigInt), BuiltinError> {
    let left = parse_gmp(name, left, 10)?;
    let right = parse_gmp(name, right, 10)?;
    if right.is_zero() {
        return Err(argument_value_error(name, "#2 ($num2)", "must not be zero"));
    }
    let quotient = &left / &right;
    let remainder = left - (&quotient * right);
    Ok((quotient, remainder))
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
        } else if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
            text = rest;
            2
        } else if text.len() > 1 && text.starts_with('0') {
            8
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

fn valid_output_base(base: i64) -> bool {
    (2..=62).contains(&base) || (-36..=-2).contains(&base)
}

fn bigint_to_radix(value: &BigInt, base: i64) -> String {
    let uppercase = base < 0;
    let radix = base.unsigned_abs() as usize;
    let digits: &[u8] = if uppercase {
        b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    } else {
        b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
    };
    if value.is_zero() {
        return "0".to_owned();
    }
    let negative = value.is_negative();
    let mut value = value.abs();
    let radix_big = BigInt::from(radix);
    let mut out = Vec::new();
    while !value.is_zero() {
        let remainder = (&value % &radix_big).to_usize().unwrap_or_default();
        out.push(digits[remainder] as char);
        value /= &radix_big;
    }
    if negative {
        out.push('-');
    }
    out.iter().rev().collect()
}

fn gcd(mut left: BigInt, mut right: BigInt) -> BigInt {
    left = left.abs();
    right = right.abs();
    while !right.is_zero() {
        let remainder = left % &right;
        left = right;
        right = remainder;
    }
    left
}

fn extended_gcd(left: BigInt, right: BigInt) -> (BigInt, BigInt, BigInt) {
    let (mut old_r, mut r) = (left, right);
    let (mut old_s, mut s) = (BigInt::one(), BigInt::zero());
    let (mut old_t, mut t) = (BigInt::zero(), BigInt::one());
    while !r.is_zero() {
        let quotient = &old_r / &r;
        (old_r, r) = (r.clone(), old_r - &quotient * r);
        (old_s, s) = (s.clone(), old_s - &quotient * s);
        (old_t, t) = (t.clone(), old_t - quotient * t);
    }
    if old_r.is_negative() {
        (-old_r, -old_s, -old_t)
    } else {
        (old_r, old_s, old_t)
    }
}

fn integer_nth_root(name: &str, value: BigInt, nth: i64) -> Result<BigInt, BuiltinError> {
    if nth <= 0 {
        return Err(argument_value_error(
            name,
            "#2 ($nth)",
            "must be greater than 0",
        ));
    }
    if value.is_negative() {
        return Err(argument_value_error(
            name,
            "#1 ($num)",
            "must be non-negative",
        ));
    }
    if value <= BigInt::one() {
        return Ok(value);
    }
    let mut low = BigInt::zero();
    let mut high = value.clone();
    let one = BigInt::one();
    while low <= high {
        let mid = (&low + &high) >> 1usize;
        let powered = mid.pow(nth as u32);
        if powered <= value {
            low = &mid + &one;
        } else if mid.is_zero() {
            break;
        } else {
            high = &mid - &one;
        }
    }
    Ok(high)
}

fn modpow(mut base: BigInt, mut exponent: BigInt, modulus: BigInt) -> BigInt {
    let mut result = BigInt::one();
    base %= &modulus;
    while exponent > BigInt::zero() {
        if (&exponent & BigInt::one()) == BigInt::one() {
            result = (result * &base) % &modulus;
        }
        exponent >>= 1usize;
        base = (&base * &base) % &modulus;
    }
    result
}

fn is_prime(value: &BigInt) -> bool {
    let Some(value) = value.to_u64() else {
        return false;
    };
    if value < 2 {
        return false;
    }
    if value == 2 {
        return true;
    }
    if value % 2 == 0 {
        return false;
    }
    let mut divisor = 3;
    while divisor * divisor <= value {
        if value % divisor == 0 {
            return false;
        }
        divisor += 2;
    }
    true
}

fn bit_index(name: &str, value: &Value) -> Result<usize, BuiltinError> {
    let index = int_arg(name, value)?;
    if index < 0 {
        return Err(argument_value_error(
            name,
            "#2 ($index)",
            "must be non-negative",
        ));
    }
    Ok(index as usize)
}

fn count_ones(mut value: BigInt) -> i64 {
    let mut count = 0;
    while value > BigInt::zero() {
        if (&value & BigInt::one()) == BigInt::one() {
            count += 1;
        }
        value >>= 1usize;
    }
    count
}

fn scan_bit(name: &str, args: Vec<Value>, find_one: bool) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error(name, "two arguments"));
    }
    let value = parse_gmp(name, &args[0], 10)?;
    let start = bit_index(name, &args[1])?;
    for index in start..4096 {
        let bit_set = ((&value >> index) & BigInt::one()) == BigInt::one();
        if bit_set == find_one {
            return Ok(Value::Int(index as i64));
        }
    }
    Ok(Value::Int(-1))
}

fn kronecker_symbol(a: BigInt, mut n: BigInt) -> i64 {
    if n.is_zero() {
        return if a.abs() == BigInt::one() { 1 } else { 0 };
    }
    if n == BigInt::one() {
        return 1;
    }
    let mut result = 1;
    if n.is_negative() {
        n = -n;
        if a.is_negative() {
            result = -result;
        }
    }
    let twos = factor_twos(&n);
    if twos > 0 {
        let two_symbol = kronecker_two_symbol(&a);
        if two_symbol == 0 {
            return 0;
        }
        if twos % 2 == 1 {
            result *= two_symbol;
        }
        n >>= twos;
    }
    if n == BigInt::one() {
        return result;
    }
    result * jacobi_odd_positive(a, n)
}

fn jacobi_odd_positive(mut a: BigInt, mut n: BigInt) -> i64 {
    debug_assert!(n > BigInt::zero());
    debug_assert!((&n & BigInt::one()) == BigInt::one());
    a = positive_mod(a, &n);
    let mut result = 1;
    while !a.is_zero() {
        while (&a & BigInt::one()).is_zero() {
            a >>= 1usize;
            let n_mod_8 = positive_mod_i64(&n, 8);
            if n_mod_8 == 3 || n_mod_8 == 5 {
                result = -result;
            }
        }
        std::mem::swap(&mut a, &mut n);
        if positive_mod_i64(&a, 4) == 3 && positive_mod_i64(&n, 4) == 3 {
            result = -result;
        }
        a = positive_mod(a, &n);
    }
    if n == BigInt::one() { result } else { 0 }
}

fn factor_twos(value: &BigInt) -> usize {
    let mut value = value.clone();
    let mut twos = 0;
    while !value.is_zero() && (&value & BigInt::one()).is_zero() {
        value >>= 1usize;
        twos += 1;
    }
    twos
}

fn kronecker_two_symbol(value: &BigInt) -> i64 {
    if (value & BigInt::one()).is_zero() {
        return 0;
    }
    match positive_mod_i64(value, 8) {
        1 | 7 => 1,
        3 | 5 => -1,
        _ => 0,
    }
}

fn positive_mod(value: BigInt, modulus: &BigInt) -> BigInt {
    let mut remainder = value % modulus;
    if remainder.is_negative() {
        remainder += modulus;
    }
    remainder
}

fn positive_mod_i64(value: &BigInt, modulus: i64) -> i64 {
    positive_mod(value.clone(), &BigInt::from(modulus))
        .to_i64()
        .unwrap_or_default()
}
