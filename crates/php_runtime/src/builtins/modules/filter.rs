//! Bounded filter extension MVP for common validation and sanitization.

use super::core::{
    argument_type_error, argument_value_error, arity_error, conversion_error, deref_value,
    float_arg, int_arg, string_arg,
};
use crate::builtins::context::PcreServiceAccess;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinRegistry,
    BuiltinResult, RuntimeSourceSpan,
};
use crate::{ArrayKey, PhpArray, PhpString, Value, pcre, to_bool, to_string};
use std::net::IpAddr;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "filter_input",
        builtin_filter_input,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "filter_has_var",
        builtin_filter_has_var,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "filter_input_array",
        builtin_filter_input_array,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "filter_var_array",
        builtin_filter_var_array,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "filter_list",
        builtin_filter_list,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("filter_id", builtin_filter_id, BuiltinCompatibility::Php),
    BuiltinEntry::new("filter_var", builtin_filter_var, BuiltinCompatibility::Php),
];

const FILTER_DEFAULT: i64 = 516;
const FILTER_UNSAFE_RAW: i64 = 516;
const FILTER_VALIDATE_BOOL: i64 = 258;
const FILTER_VALIDATE_INT: i64 = 257;
const FILTER_VALIDATE_FLOAT: i64 = 259;
const FILTER_VALIDATE_REGEXP: i64 = 272;
const FILTER_VALIDATE_URL: i64 = 273;
const FILTER_VALIDATE_EMAIL: i64 = 274;
const FILTER_VALIDATE_IP: i64 = 275;
const FILTER_VALIDATE_MAC: i64 = 276;
const FILTER_VALIDATE_DOMAIN: i64 = 277;
const FILTER_SANITIZE_STRING: i64 = 513;
const FILTER_SANITIZE_ENCODED: i64 = 514;
const FILTER_SANITIZE_SPECIAL_CHARS: i64 = 515;
const FILTER_SANITIZE_EMAIL: i64 = 517;
const FILTER_SANITIZE_URL: i64 = 518;
const FILTER_SANITIZE_NUMBER_INT: i64 = 519;
const FILTER_SANITIZE_NUMBER_FLOAT: i64 = 520;
const FILTER_SANITIZE_FULL_SPECIAL_CHARS: i64 = 522;
const FILTER_SANITIZE_ADD_SLASHES: i64 = 523;
const FILTER_CALLBACK: i64 = 1_024;
const FILTER_FLAG_NONE: i64 = 0;
const FILTER_REQUIRE_ARRAY: i64 = 16_777_216;
const FILTER_REQUIRE_SCALAR: i64 = 33_554_432;
const FILTER_FORCE_ARRAY: i64 = 67_108_864;
const FILTER_NULL_ON_FAILURE: i64 = 134_217_728;
const FILTER_FLAG_ALLOW_OCTAL: i64 = 1;
const FILTER_FLAG_ALLOW_HEX: i64 = 2;
const FILTER_FLAG_STRIP_LOW: i64 = 4;
const FILTER_FLAG_STRIP_HIGH: i64 = 8;
const FILTER_FLAG_ENCODE_LOW: i64 = 16;
const FILTER_FLAG_ENCODE_HIGH: i64 = 32;
const FILTER_FLAG_ENCODE_AMP: i64 = 64;
const FILTER_FLAG_NO_ENCODE_QUOTES: i64 = 128;
const FILTER_FLAG_EMPTY_STRING_NULL: i64 = 256;
const FILTER_FLAG_STRIP_BACKTICK: i64 = 512;
const FILTER_FLAG_ALLOW_FRACTION: i64 = 4_096;
const FILTER_FLAG_ALLOW_THOUSAND: i64 = 8_192;
const FILTER_FLAG_ALLOW_SCIENTIFIC: i64 = 16_384;
const FILTER_FLAG_IPV4: i64 = 1_048_576;
const FILTER_FLAG_IPV6: i64 = 2_097_152;
const FILTER_FLAG_NO_RES_RANGE: i64 = 4_194_304;
const FILTER_FLAG_NO_PRIV_RANGE: i64 = 8_388_608;
const FILTER_FLAG_GLOBAL_RANGE: i64 = 536_870_912;
const FILTER_FLAG_HOSTNAME: i64 = 1_048_576;
const FILTER_FLAG_EMAIL_UNICODE: i64 = 1_048_576;
const FILTER_FLAG_PATH_REQUIRED: i64 = 262_144;
const FILTER_FLAG_QUERY_REQUIRED: i64 = 524_288;
const INPUT_POST: i64 = 0;
const INPUT_GET: i64 = 1;
const INPUT_COOKIE: i64 = 2;
const INPUT_ENV: i64 = 4;
const INPUT_SERVER: i64 = 5;

const FILTER_NAMES: &[(&str, i64)] = &[
    ("int", FILTER_VALIDATE_INT),
    ("boolean", FILTER_VALIDATE_BOOL),
    ("float", FILTER_VALIDATE_FLOAT),
    ("validate_regexp", FILTER_VALIDATE_REGEXP),
    ("validate_domain", FILTER_VALIDATE_DOMAIN),
    ("validate_url", FILTER_VALIDATE_URL),
    ("validate_email", FILTER_VALIDATE_EMAIL),
    ("validate_ip", FILTER_VALIDATE_IP),
    ("validate_mac", FILTER_VALIDATE_MAC),
    ("string", FILTER_SANITIZE_STRING),
    ("stripped", FILTER_SANITIZE_STRING),
    ("encoded", FILTER_SANITIZE_ENCODED),
    ("special_chars", FILTER_SANITIZE_SPECIAL_CHARS),
    ("full_special_chars", FILTER_SANITIZE_FULL_SPECIAL_CHARS),
    ("unsafe_raw", FILTER_UNSAFE_RAW),
    ("email", FILTER_SANITIZE_EMAIL),
    ("url", FILTER_SANITIZE_URL),
    ("number_int", FILTER_SANITIZE_NUMBER_INT),
    ("number_float", FILTER_SANITIZE_NUMBER_FLOAT),
    ("add_slashes", FILTER_SANITIZE_ADD_SLASHES),
    ("callback", FILTER_CALLBACK),
];

#[derive(Clone, Debug)]
struct FilterOptions {
    flags: i64,
    min_range_int: Option<i64>,
    max_range_int: Option<i64>,
    min_range_float: Option<f64>,
    max_range_float: Option<f64>,
    callback: Option<String>,
    default_value: Option<Value>,
    decimal: Option<String>,
    separator: Option<String>,
    thousand: Option<String>,
    regexp: Option<PhpString>,
}

impl Default for FilterOptions {
    fn default() -> Self {
        Self {
            flags: FILTER_FLAG_NONE,
            min_range_int: None,
            max_range_int: None,
            min_range_float: None,
            max_range_float: None,
            callback: None,
            default_value: None,
            decimal: None,
            separator: None,
            thousand: None,
            regexp: None,
        }
    }
}

fn builtin_filter_var(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error("filter_var", "one to three argument(s)"));
    }
    let filter = args
        .get(1)
        .map(|value| int_arg("filter_var", value))
        .transpose()?
        .unwrap_or(FILTER_DEFAULT);
    let options = args
        .get(2)
        .map(|value| filter_options("filter_var", value))
        .transpose()?
        .unwrap_or_default();
    apply_filter(context, "filter_var", &args[0], filter, &options, span)
}

fn apply_filter(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: &Value,
    filter: i64,
    options: &FilterOptions,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if let Value::Array(array) = deref_value(value) {
        if options.flags & FILTER_REQUIRE_SCALAR != 0
            || options.flags & (FILTER_REQUIRE_ARRAY | FILTER_FORCE_ARRAY) == 0
        {
            return Ok(filter_failure(options));
        }
        let mut output = PhpArray::new();
        for (key, value) in array.iter() {
            output.insert(
                key.clone(),
                apply_filter_array_value(context, name, value, filter, options, span.clone())?,
            );
        }
        return Ok(Value::Array(output));
    }

    if options.flags & FILTER_REQUIRE_ARRAY != 0 {
        return Ok(filter_failure(options));
    }

    let filtered = apply_filter_scalar(context, name, value, filter, options, span)?;
    if options.flags & FILTER_FORCE_ARRAY != 0 {
        return Ok(Value::Array(PhpArray::from_packed(vec![filtered])));
    }
    Ok(filtered)
}

fn apply_filter_array_value(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: &Value,
    filter: i64,
    options: &FilterOptions,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    let Value::Array(array) = deref_value(value) else {
        return apply_filter_scalar(context, name, value, filter, options, span);
    };

    let mut output = PhpArray::new();
    for (key, value) in array.iter() {
        output.insert(
            key.clone(),
            apply_filter_array_value(context, name, value, filter, options, span.clone())?,
        );
    }
    Ok(Value::Array(output))
}

fn apply_filter_scalar(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: &Value,
    filter: i64,
    options: &FilterOptions,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    let failure = filter_failure(options);
    match filter {
        FILTER_DEFAULT => unsafe_raw(name, value, options.flags),
        FILTER_VALIDATE_EMAIL => validate_email(name, value, options.flags, failure),
        FILTER_VALIDATE_INT => validate_int(name, value, options, failure),
        FILTER_VALIDATE_FLOAT => validate_float(name, value, options, failure),
        FILTER_VALIDATE_REGEXP => validate_regexp(context, name, value, options, failure),
        FILTER_VALIDATE_URL => validate_url(name, value, options.flags, failure),
        FILTER_VALIDATE_IP => validate_ip(name, value, options.flags, failure),
        FILTER_VALIDATE_MAC => validate_mac(name, value, options, failure),
        FILTER_VALIDATE_DOMAIN => validate_domain(name, value, options.flags, failure),
        FILTER_VALIDATE_BOOL => validate_bool(name, value, options.flags, failure),
        FILTER_SANITIZE_STRING => sanitize_string(name, value, options.flags),
        FILTER_SANITIZE_ENCODED => sanitize_encoded(name, value, options.flags),
        FILTER_SANITIZE_EMAIL => sanitize(name, value, is_email_sanitize_byte),
        FILTER_SANITIZE_URL => sanitize(name, value, is_url_sanitize_byte),
        FILTER_SANITIZE_SPECIAL_CHARS => sanitize_special_chars(name, value, options.flags),
        FILTER_SANITIZE_FULL_SPECIAL_CHARS => {
            sanitize_full_special_chars(name, value, options.flags)
        }
        FILTER_SANITIZE_NUMBER_INT => sanitize(name, value, |byte| {
            byte.is_ascii_digit() || byte == b'+' || byte == b'-'
        }),
        FILTER_SANITIZE_NUMBER_FLOAT => sanitize_number_float(name, value, options.flags),
        FILTER_SANITIZE_ADD_SLASHES => sanitize_add_slashes(name, value),
        FILTER_CALLBACK => apply_callback_filter(context, name, value, options, span),
        _ if is_known_filter_id(filter) => Ok(failure),
        _ => {
            context.php_warning(
                "E_PHP_RUNTIME_FILTER_UNKNOWN_ID",
                format!("{name}(): Unknown filter with ID {filter}"),
                span,
            );
            Ok(failure)
        }
    }
}

fn is_known_filter_id(filter: i64) -> bool {
    FILTER_NAMES.iter().any(|(_, id)| *id == filter)
}

fn filter_failure(options: &FilterOptions) -> Value {
    if let Some(default_value) = options.default_value.clone() {
        default_value
    } else if options.flags & FILTER_NULL_ON_FAILURE != 0 {
        Value::Null
    } else {
        Value::Bool(false)
    }
}

fn validation_string_arg(name: &str, value: &Value) -> Result<Option<PhpString>, BuiltinError> {
    match string_arg(name, value) {
        Ok(input) => Ok(Some(input)),
        Err(_) if is_non_stringable_object(value) => Ok(None),
        Err(error) => Err(error),
    }
}

fn is_non_stringable_object(value: &Value) -> bool {
    matches!(
        deref_value(value),
        Value::Object(_) | Value::Fiber(_) | Value::Generator(_)
    )
}

fn validate_int(
    name: &str,
    value: &Value,
    options: &FilterOptions,
    failure: Value,
) -> BuiltinResult {
    let Some(input) = validation_string_arg(name, value)? else {
        return Ok(failure);
    };
    let text = input.to_string_lossy();
    let trimmed = text.trim();
    let Some(number) = parse_filter_int(trimmed, options.flags) else {
        return Ok(failure);
    };
    if options
        .min_range_int
        .is_some_and(|minimum| number < minimum)
        || options
            .max_range_int
            .is_some_and(|maximum| number > maximum)
    {
        return Ok(failure);
    }
    Ok(Value::Int(number))
}

fn parse_filter_int(trimmed: &str, flags: i64) -> Option<i64> {
    if trimmed.is_empty() {
        return None;
    }

    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        if flags & FILTER_FLAG_ALLOW_HEX == 0 || hex.is_empty() {
            return None;
        }
        return parse_filter_prefixed_int(hex, 16);
    }

    if let Some(octal) = trimmed
        .strip_prefix("0o")
        .or_else(|| trimmed.strip_prefix("0O"))
    {
        if flags & FILTER_FLAG_ALLOW_OCTAL == 0 || octal.is_empty() {
            return None;
        }
        return parse_filter_prefixed_int(octal, 8);
    }

    if trimmed.starts_with('0') && trimmed.len() > 1 {
        if flags & FILTER_FLAG_ALLOW_OCTAL == 0 {
            return None;
        }
        return parse_filter_prefixed_int(&trimmed[1..], 8);
    }

    let unsigned = trimmed
        .strip_prefix('+')
        .or_else(|| trimmed.strip_prefix('-'))
        .unwrap_or(trimmed);
    if unsigned.starts_with('0') && unsigned.len() > 1 {
        return None;
    }

    let number = trimmed.parse::<i64>().ok()?;
    trimmed
        .bytes()
        .enumerate()
        .all(|(index, byte)| byte.is_ascii_digit() || (index == 0 && matches!(byte, b'+' | b'-')))
        .then_some(number)
}

fn parse_filter_prefixed_int(digits: &str, radix: u32) -> Option<i64> {
    if digits.is_empty() {
        return None;
    }
    u64::from_str_radix(digits, radix)
        .ok()
        .map(|number| number as i64)
}

fn validate_float(
    name: &str,
    value: &Value,
    options: &FilterOptions,
    failure: Value,
) -> BuiltinResult {
    let Some(input) = validation_string_arg(name, value)? else {
        return Ok(failure);
    };
    let text = input.to_string_lossy();
    let trimmed = text.trim();
    let normalized = normalize_filter_float(name, trimmed, options)?;
    match normalized.parse::<f64>() {
        Ok(number)
            if number.is_finite()
                && !float_underflowed_to_zero(number, &normalized)
                && !options
                    .min_range_float
                    .is_some_and(|minimum| number < minimum)
                && !options
                    .max_range_float
                    .is_some_and(|maximum| number > maximum) =>
        {
            Ok(Value::float(number))
        }
        _ => Ok(failure),
    }
}

fn normalize_filter_float(
    name: &str,
    trimmed: &str,
    options: &FilterOptions,
) -> Result<String, BuiltinError> {
    let without_thousands = normalize_filter_float_thousand(name, trimmed, options)?;
    normalize_filter_float_decimal(name, &without_thousands, options.decimal.as_deref())
}

fn normalize_filter_float_thousand(
    name: &str,
    trimmed: &str,
    options: &FilterOptions,
) -> Result<String, BuiltinError> {
    if options.flags & FILTER_FLAG_ALLOW_THOUSAND == 0 {
        return Ok(trimmed.to_owned());
    }
    let thousand = options.thousand.as_deref().unwrap_or(",");
    if thousand.is_empty() {
        return Err(filter_value_error(format!(
            "{name}(): \"thousand\" option must not be empty"
        )));
    }
    if !has_valid_float_thousand_groups(trimmed, thousand, options.decimal.as_deref()) {
        return Ok(trimmed.to_owned());
    }
    Ok(trimmed.replace(thousand, ""))
}

fn has_valid_float_thousand_groups(input: &str, thousand: &str, decimal: Option<&str>) -> bool {
    if !input.contains(thousand) {
        return true;
    }

    let mantissa = input
        .split_once(['e', 'E'])
        .map(|(mantissa, _)| mantissa)
        .unwrap_or(input);
    let decimal = decimal.unwrap_or(".");
    let integer = mantissa
        .split_once(decimal)
        .map(|(integer, fractional)| {
            if fractional.contains(thousand) {
                ""
            } else {
                integer
            }
        })
        .unwrap_or(mantissa);
    if integer.is_empty() {
        return false;
    }

    let unsigned = integer.strip_prefix(['+', '-']).unwrap_or(integer);
    let groups: Vec<&str> = unsigned.split(thousand).collect();
    if groups.len() < 2 || groups[0].is_empty() || groups[0].len() > 3 {
        return false;
    }
    groups[0].bytes().all(|byte| byte.is_ascii_digit())
        && groups[1..]
            .iter()
            .all(|group| group.len() == 3 && group.bytes().all(|byte| byte.is_ascii_digit()))
}

fn float_underflowed_to_zero(number: f64, input: &str) -> bool {
    number == 0.0 && normalized_has_non_zero_digit(input)
}

fn normalized_has_non_zero_digit(input: &str) -> bool {
    input
        .split_once(['e', 'E'])
        .map(|(mantissa, _)| mantissa)
        .unwrap_or(input)
        .bytes()
        .any(|byte| matches!(byte, b'1'..=b'9'))
}

fn normalize_filter_float_decimal(
    name: &str,
    trimmed: &str,
    decimal: Option<&str>,
) -> Result<String, BuiltinError> {
    let Some(decimal) = decimal else {
        return Ok(trimmed.to_owned());
    };
    let mut chars = decimal.chars();
    let Some(decimal_char) = chars.next() else {
        return Err(filter_value_error(format!(
            "{name}(): \"decimal\" option must be one character long"
        )));
    };
    if chars.next().is_some() {
        return Err(filter_value_error(format!(
            "{name}(): \"decimal\" option must be one character long"
        )));
    }
    if decimal_char == '.' {
        return Ok(trimmed.to_owned());
    }
    if trimmed.contains('.') {
        return Ok(String::new());
    }
    Ok(trimmed.replace(decimal_char, "."))
}

fn builtin_filter_input(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=5).contains(&args.len()) {
        return Err(arity_error("filter_input", "two to five argument(s)"));
    }
    let source = int_arg("filter_input", &args[0])?;
    validate_input_source("filter_input", source)?;
    let name = string_arg("filter_input", &args[1])?.to_string_lossy();
    let filter = args
        .get(2)
        .map(|value| int_arg("filter_input", value))
        .transpose()?
        .unwrap_or(FILTER_DEFAULT);
    let options = args
        .get(3)
        .map(|value| filter_options("filter_input", value))
        .transpose()?
        .unwrap_or_default();
    let Some(value) = context.filter_input_value(source, &name) else {
        return Ok(filter_input_missing_value(&options));
    };
    apply_filter(context, "filter_input", &value, filter, &options, span)
}

fn filter_input_missing_value(options: &FilterOptions) -> Value {
    if let Some(default_value) = options.default_value.clone() {
        default_value
    } else if options.flags & FILTER_NULL_ON_FAILURE != 0 {
        Value::Bool(false)
    } else {
        Value::Null
    }
}

fn builtin_filter_has_var(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("filter_has_var", "two argument(s)"));
    }
    let source = int_arg("filter_has_var", &args[0])?;
    validate_input_source("filter_has_var", source)?;
    let name = string_arg("filter_has_var", &args[1])?.to_string_lossy();
    Ok(Value::Bool(
        context.filter_input_value(source, &name).is_some(),
    ))
}

fn builtin_filter_input_array(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error(
            "filter_input_array",
            "one to three argument(s)",
        ));
    }
    let source = int_arg("filter_input_array", &args[0])?;
    validate_input_source("filter_input_array", source)?;
    let Some(array) = context.filter_input_array(source) else {
        return Ok(Value::Null);
    };
    if array.is_empty() {
        return Ok(Value::Null);
    }
    let options = args.get(1);
    let add_empty = args
        .get(2)
        .map(|value| {
            to_bool(value).map_err(|message| conversion_error("filter_input_array", message))
        })
        .transpose()?
        .unwrap_or(true);
    filter_array(
        context,
        "filter_input_array",
        &array,
        options,
        add_empty,
        span,
    )
}

fn builtin_filter_var_array(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error("filter_var_array", "one to three argument(s)"));
    }
    let Value::Array(array) = deref_value(&args[0]) else {
        return Err(conversion_error(
            "filter_var_array",
            "argument must be of type array".to_owned(),
        ));
    };
    let options = args.get(1);
    let add_empty = args
        .get(2)
        .map(|value| {
            to_bool(value).map_err(|message| conversion_error("filter_var_array", message))
        })
        .transpose()?
        .unwrap_or(true);
    filter_array(
        context,
        "filter_var_array",
        &array,
        options,
        add_empty,
        span,
    )
}

fn builtin_filter_list(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !args.is_empty() {
        return Err(arity_error("filter_list", "zero argument(s)"));
    }
    Ok(Value::packed_array(
        FILTER_NAMES
            .iter()
            .map(|(name, _)| Value::String(PhpString::from_test_str(name)))
            .collect(),
    ))
}

fn builtin_filter_id(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("filter_id", "one argument(s)"));
    }
    let name = string_arg("filter_id", &args[0])?.to_string_lossy();
    Ok(FILTER_NAMES
        .iter()
        .find_map(|(filter_name, id)| (*filter_name == name).then_some(Value::Int(*id)))
        .unwrap_or(Value::Bool(false)))
}

fn filter_array(
    context: &mut BuiltinContext<'_>,
    name: &str,
    input: &PhpArray,
    options: Option<&Value>,
    add_empty: bool,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    match options.map(deref_value) {
        None | Some(Value::Null) => {
            filter_array_with_single_filter(context, name, input, FILTER_DEFAULT, span)
        }
        Some(Value::Int(filter)) => {
            filter_array_with_single_filter(context, name, input, filter, span)
        }
        Some(Value::Array(specs)) => {
            filter_array_with_specs(context, name, input, &specs, add_empty, span)
        }
        Some(other) => Err(argument_type_error(
            name,
            "#2 ($options)",
            "array|int",
            &other,
        )),
    }
}

fn filter_array_with_single_filter(
    context: &mut BuiltinContext<'_>,
    name: &str,
    input: &PhpArray,
    filter: i64,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !is_known_filter_id(filter) {
        warn_unknown_filter(context, name, filter, span);
        return Ok(Value::Bool(false));
    }
    let options = FilterOptions::default();
    let mut output = PhpArray::new();
    for (key, value) in input.iter() {
        output.insert(
            key.clone(),
            apply_filter_array_entry(context, name, value, filter, &options, span.clone())?,
        );
    }
    Ok(Value::Array(output))
}

fn apply_filter_array_entry(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: &Value,
    filter: i64,
    options: &FilterOptions,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if let Value::Reference(cell) = value {
        let filtered = apply_filter_array_value(context, name, &cell.get(), filter, options, span)?;
        cell.set(filtered);
        return Ok(Value::Reference(cell.clone()));
    }
    apply_filter_array_value(context, name, value, filter, options, span)
}

fn filter_array_with_specs(
    context: &mut BuiltinContext<'_>,
    name: &str,
    input: &PhpArray,
    specs: &PhpArray,
    add_empty: bool,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    let mut output = PhpArray::new();
    for (key, spec) in specs.iter() {
        if is_empty_spec_key(&key) {
            return Err(argument_value_error(
                name,
                "#2 ($options)",
                "cannot contain empty keys",
            ));
        }
        match input.get(&key) {
            Some(value) => {
                let filter = filter_spec_filter(name, spec)?;
                if !is_known_filter_id(filter) {
                    warn_unknown_filter(context, name, filter, span.clone());
                    output.insert(key.clone(), value.clone());
                    continue;
                }
                let options = filter_options(name, spec)?;
                output.insert(
                    key.clone(),
                    apply_filter(context, name, value, filter, &options, span.clone())?,
                );
            }
            None if add_empty => {
                output.insert(key.clone(), Value::Null);
            }
            None => {}
        }
    }
    Ok(Value::Array(output))
}

fn is_empty_spec_key(key: &ArrayKey) -> bool {
    matches!(key, ArrayKey::String(value) if value.as_bytes().is_empty())
}

fn warn_unknown_filter(
    context: &mut BuiltinContext<'_>,
    name: &str,
    filter: i64,
    span: RuntimeSourceSpan,
) {
    context.php_warning(
        "E_PHP_RUNTIME_FILTER_UNKNOWN_ID",
        format!("{name}(): Unknown filter with ID {filter}"),
        span,
    );
}

fn filter_spec_filter(name: &str, value: &Value) -> Result<i64, BuiltinError> {
    match deref_value(value) {
        Value::Array(array) => {
            let key = ArrayKey::String(PhpString::from_test_str("filter"));
            match array.get(&key) {
                Some(value) => int_arg(name, value),
                None => Ok(FILTER_DEFAULT),
            }
        }
        other => int_arg(name, &other),
    }
}

fn filter_options(name: &str, value: &Value) -> Result<FilterOptions, BuiltinError> {
    match deref_value(value) {
        Value::Array(array) => filter_options_from_array(name, &array),
        other => Ok(FilterOptions {
            flags: int_arg(name, &other)?,
            ..FilterOptions::default()
        }),
    }
}

fn filter_options_from_array(name: &str, array: &PhpArray) -> Result<FilterOptions, BuiltinError> {
    let mut options = FilterOptions::default();
    let flags_key = ArrayKey::String(PhpString::from_test_str("flags"));
    if let Some(value) = array.get(&flags_key) {
        options.flags = int_arg(name, value)?;
    }
    let options_key = ArrayKey::String(PhpString::from_test_str("options"));
    if let Some(value) = array.get(&options_key) {
        parse_filter_option_payload(name, value, &mut options)?;
    }
    Ok(options)
}

fn parse_filter_option_payload(
    name: &str,
    value: &Value,
    options: &mut FilterOptions,
) -> Result<(), BuiltinError> {
    match deref_value(value) {
        Value::Array(array) => {
            let min_key = ArrayKey::String(PhpString::from_test_str("min_range"));
            if let Some(value) = array.get(&min_key) {
                options.min_range_int = Some(int_arg(name, value)?);
                options.min_range_float = Some(float_arg(name, value)?);
            }
            let max_key = ArrayKey::String(PhpString::from_test_str("max_range"));
            if let Some(value) = array.get(&max_key) {
                options.max_range_int = Some(int_arg(name, value)?);
                options.max_range_float = Some(float_arg(name, value)?);
            }
            let decimal_key = ArrayKey::String(PhpString::from_test_str("decimal"));
            if let Some(value) = array.get(&decimal_key) {
                options.decimal = Some(string_arg(name, value)?.to_string_lossy());
            }
            let regexp_key = ArrayKey::String(PhpString::from_test_str("regexp"));
            if let Some(value) = array.get(&regexp_key) {
                options.regexp = Some(string_arg(name, value)?);
            }
            let separator_key = ArrayKey::String(PhpString::from_test_str("separator"));
            if let Some(value) = array.get(&separator_key) {
                options.separator = Some(string_arg(name, value)?.to_string_lossy());
            }
            let thousand_key = ArrayKey::String(PhpString::from_test_str("thousand"));
            if let Some(value) = array.get(&thousand_key) {
                options.thousand = Some(string_arg(name, value)?.to_string_lossy());
            }
            let default_key = ArrayKey::String(PhpString::from_test_str("default"));
            if let Some(value) = array.get(&default_key) {
                options.default_value = Some(value.clone());
            }
        }
        Value::String(value) => {
            options.callback = Some(value.to_string_lossy());
        }
        other => {
            if let Some(callable) = other.as_callable()
                && let crate::CallableValue::InternalBuiltin { name } = callable
            {
                options.callback = Some(name.clone());
            }
        }
    }
    Ok(())
}

fn apply_callback_filter(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: &Value,
    options: &FilterOptions,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    let Some(callback_name) = options.callback.as_deref() else {
        return Err(invalid_callback_error(name));
    };
    let Some(callback) = BuiltinRegistry::new().get(callback_name) else {
        return Err(invalid_callback_error(name));
    };
    callback.function()(context, vec![value.clone()], span)
}

fn invalid_callback_error(name: &str) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_FILTER_CALLBACK",
        format!("{name}(): Option must be a valid callback"),
    )
}

fn filter_value_error(message: impl Into<String>) -> BuiltinError {
    BuiltinError::new("E_PHP_RUNTIME_BUILTIN_VALUE", message.into())
}

fn validate_input_source(function: &str, source: i64) -> Result<(), BuiltinError> {
    match source {
        INPUT_POST | INPUT_GET | INPUT_COOKIE | INPUT_ENV | INPUT_SERVER => Ok(()),
        _ => Err(filter_value_error(format!(
            "{function}(): Argument #1 ($input_type) must be an INPUT_* constant"
        ))),
    }
}

fn validate_regexp(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: &Value,
    options: &FilterOptions,
    failure: Value,
) -> BuiltinResult {
    let Some(regexp) = options.regexp.as_ref() else {
        return Err(filter_value_error(format!(
            "{name}(): \"regexp\" option is missing"
        )));
    };
    let input = string_arg(name, value)?;
    let mut pcre_services = context.pcre_services();
    let compiled = match pcre_services.pcre_cache().compile(regexp) {
        Ok(compiled) => compiled,
        Err(error) => {
            pcre_services.set_preg_last_error(error.code(), pcre::preg_error_message(error.code()));
            return Ok(failure);
        }
    };
    match compiled.is_match(input.as_bytes()) {
        Ok(true) => Ok(Value::String(input)),
        Ok(false) => Ok(failure),
        Err(error) => {
            pcre_services.set_preg_last_error(error.code(), pcre::preg_error_message(error.code()));
            Ok(failure)
        }
    }
}

fn validate_email(name: &str, value: &Value, flags: i64, failure: Value) -> BuiltinResult {
    let Some(input) = validation_string_arg(name, value)? else {
        return Ok(failure);
    };
    if input.as_bytes().len() > 320 {
        return Ok(failure);
    }
    let string = input.to_string_lossy();
    let Some((local, domain)) = split_email_address(&string) else {
        return Ok(failure);
    };
    let allow_unicode = flags & FILTER_FLAG_EMAIL_UNICODE != 0;
    if !local.is_empty()
        && local.len() <= 64
        && is_valid_email_local(local, allow_unicode)
        && is_valid_email_domain(domain)
    {
        Ok(Value::String(input))
    } else {
        Ok(failure)
    }
}

fn split_email_address(input: &str) -> Option<(&str, &str)> {
    let mut quoted = false;
    let mut escaped = false;
    let mut at_index = None;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '@' if !quoted => {
                if at_index.is_some() {
                    return None;
                }
                at_index = Some(index);
            }
            _ => {}
        }
    }
    if quoted || escaped {
        return None;
    }
    let index = at_index?;
    Some((&input[..index], &input[index + 1..]))
}

fn is_valid_email_local(local: &str, allow_unicode: bool) -> bool {
    if let Some(quoted) = local.strip_prefix('"') {
        let Some(inner) = quoted.strip_suffix('"') else {
            return false;
        };
        return is_valid_email_quoted_local(inner, allow_unicode);
    }
    is_valid_email_dot_atom(local, allow_unicode)
}

fn is_valid_email_dot_atom(local: &str, allow_unicode: bool) -> bool {
    if local.starts_with('.') || local.ends_with('.') || local.contains("..") {
        return false;
    }
    local.split('.').all(|atom| {
        !atom.is_empty()
            && atom
                .chars()
                .all(|ch| is_valid_email_atom_char(ch, allow_unicode))
    })
}

fn is_valid_email_atom_char(ch: char, allow_unicode: bool) -> bool {
    matches!(
        ch,
        'A'..='Z'
            | 'a'..='z'
            | '0'..='9'
            | '!'
            | '#'
            | '$'
            | '%'
            | '&'
            | '\''
            | '*'
            | '+'
            | '-'
            | '/'
            | '='
            | '?'
            | '^'
            | '_'
            | '`'
            | '{'
            | '|'
            | '}'
            | '~'
    ) || (allow_unicode && !ch.is_ascii() && !ch.is_control() && !ch.is_whitespace())
}

fn is_valid_email_quoted_local(inner: &str, allow_unicode: bool) -> bool {
    let mut escaped = false;
    for ch in inner.chars() {
        if escaped {
            if ch == '\r' || ch == '\n' {
                return false;
            }
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' | '\r' | '\n' => return false,
            ch if ch.is_ascii_control() => return false,
            ch if !allow_unicode && !ch.is_ascii() => return false,
            _ => {}
        }
    }
    !escaped
}

fn is_valid_email_domain(domain: &str) -> bool {
    if let Some(literal) = domain
        .strip_prefix('[')
        .and_then(|domain| domain.strip_suffix(']'))
    {
        return is_valid_email_address_literal(literal);
    }
    if !domain.contains('.') {
        return false;
    }
    let Some(tld) = domain.rsplit('.').next() else {
        return false;
    };
    if tld.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    domain.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn is_valid_email_address_literal(literal: &str) -> bool {
    if let Some(ipv6) = literal.strip_prefix("IPv6:") {
        return ipv6.parse::<std::net::Ipv6Addr>().is_ok();
    }
    literal.parse::<std::net::Ipv4Addr>().is_ok()
}

fn validate_url(name: &str, value: &Value, flags: i64, failure: Value) -> BuiltinResult {
    let Some(input) = validation_string_arg(name, value)? else {
        return Ok(failure);
    };
    let string = input.to_string_lossy();
    let Some((scheme, rest)) = string.split_once(':') else {
        return Ok(failure);
    };
    let scheme = scheme.to_ascii_lowercase();
    let path_ok = flags & FILTER_FLAG_PATH_REQUIRED == 0 || url_rest_has_path(rest);
    let query_ok = flags & FILTER_FLAG_QUERY_REQUIRED == 0 || rest.contains('?');
    if is_valid_url_scheme(&scheme)
        && is_valid_url_rest(&scheme, rest)
        && path_ok
        && query_ok
        && !php_source::byte_kernel::contains_ascii_whitespace(input.as_bytes())
    {
        Ok(Value::String(input))
    } else {
        Ok(failure)
    }
}

fn is_valid_url_scheme(scheme: &str) -> bool {
    !scheme.is_empty()
        && scheme
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic())
        && scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-'))
}

fn is_valid_url_rest(scheme: &str, rest: &str) -> bool {
    if let Some(after_slashes) = rest.strip_prefix("//") {
        let authority = after_slashes
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default();
        if scheme == "file" && authority.is_empty() {
            return after_slashes.len() > authority.len();
        }
        return !authority.is_empty() && is_valid_url_authority(authority);
    }
    matches!(scheme, "mailto" | "news") && !rest.is_empty()
}

fn url_rest_has_path(rest: &str) -> bool {
    let Some(after_slashes) = rest.strip_prefix("//") else {
        return rest.contains('/');
    };
    let path_start = after_slashes
        .find(['/', '?', '#'])
        .map(|index| &after_slashes[index..])
        .unwrap_or_default();
    path_start.starts_with('/')
}

fn is_valid_url_authority(authority: &str) -> bool {
    if authority.is_empty() {
        return false;
    }
    let host_port = if let Some((userinfo, host)) = authority.rsplit_once('@') {
        if userinfo
            .bytes()
            .any(|byte| matches!(byte, b'\\' | b'[' | b']'))
        {
            return false;
        }
        host
    } else {
        authority
    };
    if let Some(rest) = host_port.strip_prefix('[') {
        let Some((host, port)) = rest.split_once(']') else {
            return false;
        };
        return port_is_valid(port) && host.parse::<std::net::Ipv6Addr>().is_ok();
    }
    if host_port
        .bytes()
        .any(|byte| matches!(byte, b'[' | b']' | b'\\'))
    {
        return false;
    }
    let Some(host) = url_host_without_port(host_port) else {
        return false;
    };
    is_valid_domain(host.as_bytes(), true)
}

fn url_host_without_port(host_port: &str) -> Option<&str> {
    let Some((host, port)) = host_port.rsplit_once(':') else {
        return Some(host_port);
    };
    if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
        return Some(host_port);
    }
    port_is_valid(&format!(":{port}")).then_some(host)
}

fn port_is_valid(port: &str) -> bool {
    port.is_empty()
        || port
            .strip_prefix(':')
            .is_some_and(|port| port.parse::<u16>().is_ok())
}

fn validate_ip(name: &str, value: &Value, flags: i64, failure: Value) -> BuiltinResult {
    let Some(input) = validation_string_arg(name, value)? else {
        return Ok(failure);
    };
    let string = input.to_string_lossy();
    match string.parse::<IpAddr>() {
        Ok(IpAddr::V4(address))
            if flags & FILTER_FLAG_IPV6 == 0 && ipv4_allowed_by_filter_flags(address, flags) =>
        {
            Ok(Value::String(input))
        }
        Ok(IpAddr::V6(address))
            if flags & FILTER_FLAG_IPV4 == 0 && ipv6_allowed_by_filter_flags(address, flags) =>
        {
            Ok(Value::String(input))
        }
        Ok(_) => Ok(failure),
        Err(_) => Ok(failure),
    }
}

fn ipv4_allowed_by_filter_flags(address: std::net::Ipv4Addr, flags: i64) -> bool {
    let number = u32::from(address);
    if flags & FILTER_FLAG_GLOBAL_RANGE != 0 && is_ipv4_non_global(number) {
        return false;
    }
    if flags & FILTER_FLAG_NO_PRIV_RANGE != 0 && is_ipv4_private(number) {
        return false;
    }
    if flags & FILTER_FLAG_NO_RES_RANGE != 0 && is_ipv4_reserved_for_no_res_range(number) {
        return false;
    }
    true
}

fn ipv6_allowed_by_filter_flags(address: std::net::Ipv6Addr, flags: i64) -> bool {
    let number = u128::from_be_bytes(address.octets());
    if flags & FILTER_FLAG_GLOBAL_RANGE != 0 && is_ipv6_non_global(number) {
        return false;
    }
    if flags & FILTER_FLAG_NO_PRIV_RANGE != 0 && ipv6_in_cidr(number, 0xfc00_u128 << 112, 7) {
        return false;
    }
    if flags & FILTER_FLAG_NO_RES_RANGE != 0 && is_ipv6_reserved_for_no_res_range(number) {
        return false;
    }
    true
}

fn is_ipv4_private(number: u32) -> bool {
    ipv4_in_cidr(number, [10, 0, 0, 0], 8)
        || ipv4_in_cidr(number, [172, 16, 0, 0], 12)
        || ipv4_in_cidr(number, [192, 168, 0, 0], 16)
}

fn is_ipv4_reserved_for_no_res_range(number: u32) -> bool {
    ipv4_in_cidr(number, [0, 0, 0, 0], 8)
        || ipv4_in_cidr(number, [127, 0, 0, 0], 8)
        || ipv4_in_cidr(number, [169, 254, 0, 0], 16)
        || ipv4_in_cidr(number, [240, 0, 0, 0], 4)
}

fn is_ipv4_non_global(number: u32) -> bool {
    ipv4_in_cidr(number, [0, 0, 0, 0], 8)
        || ipv4_in_cidr(number, [10, 0, 0, 0], 8)
        || ipv4_in_cidr(number, [100, 64, 0, 0], 10)
        || ipv4_in_cidr(number, [127, 0, 0, 0], 8)
        || ipv4_in_cidr(number, [169, 254, 0, 0], 16)
        || ipv4_in_cidr(number, [172, 16, 0, 0], 12)
        || ipv4_in_cidr(number, [192, 0, 0, 0], 24)
        || ipv4_in_cidr(number, [192, 0, 2, 0], 24)
        || ipv4_in_cidr(number, [192, 168, 0, 0], 16)
        || ipv4_in_cidr(number, [198, 18, 0, 0], 15)
        || ipv4_in_cidr(number, [198, 51, 100, 0], 24)
        || ipv4_in_cidr(number, [203, 0, 113, 0], 24)
        || ipv4_in_cidr(number, [240, 0, 0, 0], 4)
}

fn is_ipv6_reserved_for_no_res_range(number: u128) -> bool {
    number == 0
        || number == 1
        || ipv6_in_cidr(number, ipv6_base([0, 0, 0, 0, 0, 0xffff, 0, 0]), 96)
        || ipv6_in_cidr(number, 0xfe80_u128 << 112, 10)
}

fn is_ipv6_non_global(number: u128) -> bool {
    number == 0
        || number == 1
        || ipv6_in_cidr(number, ipv6_base([0, 0, 0, 0, 0, 0xffff, 0, 0]), 96)
        || ipv6_in_cidr(number, 0x0100_u128 << 112, 64)
        || ipv6_in_cidr(number, 0x2001_u128 << 112, 23)
        || ipv6_in_cidr(number, ipv6_base([0x2001, 0x0002, 0, 0, 0, 0, 0, 0]), 48)
        || ipv6_in_cidr(number, ipv6_base([0x2001, 0x0db8, 0, 0, 0, 0, 0, 0]), 32)
        || ipv6_in_cidr(number, ipv6_base([0x2001, 0x0010, 0, 0, 0, 0, 0, 0]), 28)
        || ipv6_in_cidr(number, 0xfc00_u128 << 112, 7)
        || ipv6_in_cidr(number, 0xfe80_u128 << 112, 10)
}

fn ipv4_in_cidr(number: u32, base: [u8; 4], prefix: u32) -> bool {
    let base = u32::from_be_bytes(base);
    let mask = u32::MAX << (32 - prefix);
    number & mask == base & mask
}

fn ipv6_in_cidr(number: u128, base: u128, prefix: u32) -> bool {
    let mask = u128::MAX << (128 - prefix);
    number & mask == base & mask
}

fn ipv6_base([a, b, c, d, e, f, g, h]: [u16; 8]) -> u128 {
    u128::from(a) << 112
        | u128::from(b) << 96
        | u128::from(c) << 80
        | u128::from(d) << 64
        | u128::from(e) << 48
        | u128::from(f) << 32
        | u128::from(g) << 16
        | u128::from(h)
}

fn validate_mac(
    name: &str,
    value: &Value,
    options: &FilterOptions,
    failure: Value,
) -> BuiltinResult {
    let Some(input) = validation_string_arg(name, value)? else {
        return Ok(failure);
    };
    let bytes = input.as_bytes();
    let Some((tokens, token_len, separator)) = mac_shape(bytes) else {
        return Ok(failure);
    };
    if let Some(expected) = options.separator.as_deref() {
        let mut chars = expected.chars();
        let Some(expected_separator) = chars.next() else {
            return Err(filter_value_error(format!(
                "{name}(): \"separator\" option must be one character long"
            )));
        };
        if chars.next().is_some() {
            return Err(filter_value_error(format!(
                "{name}(): \"separator\" option must be one character long"
            )));
        }
        if !expected_separator.is_ascii() || separator != expected_separator as u8 {
            return Ok(failure);
        }
    }
    for token in 0..tokens {
        let offset = token * (token_len + 1);
        if token < tokens - 1 && bytes[offset + token_len] != separator {
            return Ok(failure);
        }
        if !bytes[offset..offset + token_len]
            .iter()
            .all(|byte| byte.is_ascii_hexdigit())
        {
            return Ok(failure);
        }
    }
    Ok(Value::String(input))
}

fn mac_shape(bytes: &[u8]) -> Option<(usize, usize, u8)> {
    match bytes {
        [_, _, b'-', ..] if bytes.len() == 17 => Some((6, 2, b'-')),
        [_, _, b':', ..] if bytes.len() == 17 => Some((6, 2, b':')),
        [_, _, _, _, b'.', ..] if bytes.len() == 14 => Some((3, 4, b'.')),
        _ => None,
    }
}

fn validate_domain(name: &str, value: &Value, flags: i64, failure: Value) -> BuiltinResult {
    let Some(input) = validation_string_arg(name, value)? else {
        return Ok(failure);
    };
    if is_valid_domain(input.as_bytes(), flags & FILTER_FLAG_HOSTNAME != 0) {
        Ok(Value::String(input))
    } else {
        Ok(failure)
    }
}

fn is_valid_domain(bytes: &[u8], hostname: bool) -> bool {
    let bytes = bytes.strip_suffix(b".").unwrap_or(bytes);
    if bytes.is_empty() || bytes.len() > 253 || bytes[0] == b'.' {
        return false;
    }
    if hostname && !bytes[0].is_ascii_alphanumeric() {
        return false;
    }

    let mut label_len = 1usize;
    for index in 0..bytes.len() {
        let byte = bytes[index];
        if byte == b'.' {
            if bytes.get(index + 1) == Some(&b'.') {
                return false;
            }
            if hostname
                && (!bytes[index - 1].is_ascii_alphanumeric()
                    || !bytes.get(index + 1).is_some_and(u8::is_ascii_alphanumeric))
            {
                return false;
            }
            label_len = 1;
        } else {
            if label_len > 63 {
                return false;
            }
            if hostname
                && (byte != b'-' || index + 1 == bytes.len())
                && !byte.is_ascii_alphanumeric()
            {
                return false;
            }
            label_len += 1;
        }
    }
    true
}

fn validate_bool(_name: &str, value: &Value, flags: i64, failure: Value) -> BuiltinResult {
    if matches!(deref_value(value), Value::Null) {
        return Ok(if flags & FILTER_NULL_ON_FAILURE != 0 {
            Value::Null
        } else {
            failure
        });
    }
    let Ok(string_value) = to_string(value) else {
        return Ok(failure);
    };
    let string = string_value.to_string_lossy().to_ascii_lowercase();
    match string.as_str() {
        "1" | "true" | "on" | "yes" => Ok(Value::Bool(true)),
        "0" | "false" | "off" | "no" | "" => Ok(Value::Bool(false)),
        _ if flags & FILTER_NULL_ON_FAILURE != 0 => Ok(Value::Null),
        _ => Ok(failure),
    }
}

fn sanitize(name: &str, value: &Value, keep: impl Fn(u8) -> bool) -> BuiltinResult {
    let input = string_arg(name, value)?;
    Ok(Value::string(
        input
            .as_bytes()
            .iter()
            .copied()
            .filter(|byte| keep(*byte))
            .collect::<Vec<_>>(),
    ))
}

fn sanitize_number_float(name: &str, value: &Value, flags: i64) -> BuiltinResult {
    sanitize(name, value, |byte| {
        byte.is_ascii_digit()
            || matches!(byte, b'+' | b'-')
            || (byte == b'.' && flags & FILTER_FLAG_ALLOW_FRACTION != 0)
            || (byte == b',' && flags & FILTER_FLAG_ALLOW_THOUSAND != 0)
            || (matches!(byte, b'e' | b'E') && flags & FILTER_FLAG_ALLOW_SCIENTIFIC != 0)
    })
}

fn unsafe_raw(name: &str, value: &Value, flags: i64) -> BuiltinResult {
    let input = string_arg(name, value)?;
    if input.as_bytes().is_empty() && flags & FILTER_FLAG_EMPTY_STRING_NULL != 0 {
        return Ok(Value::Null);
    }
    let relevant_flags = FILTER_FLAG_STRIP_LOW
        | FILTER_FLAG_STRIP_HIGH
        | FILTER_FLAG_ENCODE_LOW
        | FILTER_FLAG_ENCODE_HIGH
        | FILTER_FLAG_ENCODE_AMP
        | FILTER_FLAG_STRIP_BACKTICK;
    if flags & relevant_flags == 0 {
        return Ok(Value::String(input));
    }
    let strip_low = flags & FILTER_FLAG_STRIP_LOW != 0;
    let strip_high = flags & FILTER_FLAG_STRIP_HIGH != 0;
    let strip_backtick = flags & FILTER_FLAG_STRIP_BACKTICK != 0;
    let encoded = encode_filter_entities(input.as_bytes(), |byte| {
        (flags & FILTER_FLAG_ENCODE_AMP != 0 && byte == b'&')
            || (flags & FILTER_FLAG_ENCODE_LOW != 0 && is_filter_low(byte))
            || (flags & FILTER_FLAG_ENCODE_HIGH != 0 && is_filter_high(byte))
    });
    let output = encoded
        .into_iter()
        .filter(|byte| {
            !(strip_low && is_filter_low(*byte)
                || strip_high && is_filter_high(*byte)
                || strip_backtick && *byte == b'`')
        })
        .collect::<Vec<_>>();
    if output.is_empty() && flags & FILTER_FLAG_EMPTY_STRING_NULL != 0 {
        Ok(Value::Null)
    } else {
        Ok(Value::string(output))
    }
}

fn sanitize_encoded(name: &str, value: &Value, flags: i64) -> BuiltinResult {
    let input = string_arg(name, value)?;
    let strip_low = flags & FILTER_FLAG_STRIP_LOW != 0;
    let strip_high = flags & FILTER_FLAG_STRIP_HIGH != 0;
    let stripped: Vec<u8> = input
        .as_bytes()
        .iter()
        .copied()
        .filter(|byte| !(strip_low && is_filter_low(*byte) || strip_high && is_filter_high(*byte)))
        .collect();
    Ok(Value::string(encode_filter_bytes(&stripped, |byte| {
        !(byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            || (flags & FILTER_FLAG_ENCODE_LOW != 0 && is_filter_low(byte))
            || (flags & FILTER_FLAG_ENCODE_HIGH != 0 && is_filter_high(byte))
    })))
}

fn sanitize_string(name: &str, value: &Value, flags: i64) -> BuiltinResult {
    let input = string_arg(name, value)?;
    let stripped = strip_filter_control_bytes(input.as_bytes(), flags);
    let encode_amp = flags & FILTER_FLAG_ENCODE_AMP != 0;
    let no_encode_quotes = flags & FILTER_FLAG_NO_ENCODE_QUOTES != 0;
    let encoded = encode_filter_entities(&stripped, |byte| {
        (encode_amp && byte == b'&')
            || (!no_encode_quotes && matches!(byte, b'\'' | b'"'))
            || (flags & FILTER_FLAG_ENCODE_LOW != 0 && is_filter_low(byte))
            || (flags & FILTER_FLAG_ENCODE_HIGH != 0 && is_filter_high(byte))
    });
    let output = strip_filter_tags(&encoded);
    if output.is_empty() && flags & FILTER_FLAG_EMPTY_STRING_NULL != 0 {
        Ok(Value::Null)
    } else {
        Ok(Value::string(output))
    }
}

fn sanitize_add_slashes(name: &str, value: &Value) -> BuiltinResult {
    let input = string_arg(name, value)?;
    let mut output = Vec::with_capacity(input.as_bytes().len());
    for byte in input.as_bytes() {
        match *byte {
            b'\0' => output.extend_from_slice(br"\0"),
            b'\'' | b'"' | b'\\' => {
                output.push(b'\\');
                output.push(*byte);
            }
            _ => output.push(*byte),
        }
    }
    Ok(Value::string(output))
}

fn strip_filter_control_bytes(input: &[u8], flags: i64) -> Vec<u8> {
    let strip_low = flags & FILTER_FLAG_STRIP_LOW != 0;
    let strip_high = flags & FILTER_FLAG_STRIP_HIGH != 0;
    let strip_backtick = flags & FILTER_FLAG_STRIP_BACKTICK != 0;
    input
        .iter()
        .copied()
        .filter(|byte| {
            !(strip_low && is_filter_low(*byte)
                || strip_high && is_filter_high(*byte)
                || strip_backtick && *byte == b'`')
        })
        .collect()
}

fn strip_filter_tags(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut in_tag = false;
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;

    for (index, byte) in input.iter().copied().enumerate() {
        if byte == b'\0' {
            continue;
        }
        if !in_tag {
            if byte == b'<' {
                in_tag = true;
                depth = 0;
                quote = None;
            } else {
                output.push(byte);
            }
            continue;
        }

        match byte {
            b'<' if quote.is_none() => depth += 1,
            b'>' if quote.is_none() => {
                if depth == 0 {
                    in_tag = false;
                } else {
                    depth -= 1;
                }
            }
            b'\'' | b'"' if index != 0 => match quote {
                Some(open) if open == byte => quote = None,
                None => quote = Some(byte),
                _ => {}
            },
            _ => {}
        }
    }

    output
}

fn sanitize_special_chars(name: &str, value: &Value, flags: i64) -> BuiltinResult {
    let input = string_arg(name, value)?;
    let mut output = Vec::new();
    let strip_low = flags & FILTER_FLAG_STRIP_LOW != 0;
    let strip_high = flags & FILTER_FLAG_STRIP_HIGH != 0;
    for byte in input.as_bytes() {
        if (strip_low && is_filter_low(*byte)) || (strip_high && is_filter_high(*byte)) {
            continue;
        }
        if matches!(*byte, b'<' | b'>' | b'&' | b'\'' | b'"')
            || (flags & FILTER_FLAG_ENCODE_LOW != 0 && is_filter_low(*byte))
            || (flags & FILTER_FLAG_ENCODE_HIGH != 0 && is_filter_high(*byte))
        {
            output.extend_from_slice(format!("&#{};", byte).as_bytes());
        } else {
            output.push(*byte);
        }
    }
    Ok(Value::string(output))
}

fn sanitize_full_special_chars(name: &str, value: &Value, flags: i64) -> BuiltinResult {
    let input = string_arg(name, value)?;
    let mut output = Vec::new();
    let strip_low = flags & FILTER_FLAG_STRIP_LOW != 0;
    let strip_high = flags & FILTER_FLAG_STRIP_HIGH != 0;
    let no_encode_quotes = flags & FILTER_FLAG_NO_ENCODE_QUOTES != 0;
    for byte in input.as_bytes() {
        if (strip_low && is_filter_low(*byte)) || (strip_high && is_filter_high(*byte)) {
            continue;
        }
        match *byte {
            b'<' => output.extend_from_slice(b"&lt;"),
            b'>' => output.extend_from_slice(b"&gt;"),
            b'&' => output.extend_from_slice(b"&amp;"),
            b'\'' if !no_encode_quotes => output.extend_from_slice(b"&#039;"),
            b'"' if !no_encode_quotes => output.extend_from_slice(b"&quot;"),
            byte if flags & FILTER_FLAG_ENCODE_LOW != 0 && is_filter_low(byte) => {
                output.extend_from_slice(format!("&#{};", byte).as_bytes());
            }
            byte if flags & FILTER_FLAG_ENCODE_HIGH != 0 && is_filter_high(byte) => {
                output.extend_from_slice(format!("&#{};", byte).as_bytes());
            }
            byte => output.push(byte),
        }
    }
    Ok(Value::string(output))
}

fn is_filter_low(byte: u8) -> bool {
    byte < 0x20
}

fn is_filter_high(byte: u8) -> bool {
    byte >= 0x7f
}

fn encode_filter_entities(input: &[u8], should_encode: impl Fn(u8) -> bool) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    for byte in input {
        if should_encode(*byte) {
            output.extend_from_slice(format!("&#{};", byte).as_bytes());
        } else {
            output.push(*byte);
        }
    }
    output
}

fn encode_filter_bytes(input: &[u8], should_encode: impl Fn(u8) -> bool) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    for byte in input {
        if should_encode(*byte) {
            output.extend_from_slice(format!("%{byte:02X}").as_bytes());
        } else {
            output.push(*byte);
        }
    }
    output
}

fn is_email_sanitize_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-=?^_`{|}~@.[]".contains(&byte)
}

fn is_url_sanitize_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"$-_.+!*'(),{}|\\^~[]`<>#%\";/?:@&=".contains(&byte)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClassEntry, ClassFlags, ObjectRef, OutputBuffer};

    fn call(name: &str, args: Vec<Value>) -> Value {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        ENTRIES
            .iter()
            .find(|entry| entry.name() == name)
            .expect("filter entry")
            .function()(&mut context, args, RuntimeSourceSpan::default())
        .expect("filter succeeds")
    }

    fn call_error(name: &str, args: Vec<Value>) -> BuiltinError {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        ENTRIES
            .iter()
            .find(|entry| entry.name() == name)
            .expect("filter entry")
            .function()(&mut context, args, RuntimeSourceSpan::default())
        .expect_err("filter should fail")
    }

    fn string(value: &str) -> Value {
        Value::String(PhpString::from_test_str(value))
    }

    fn bytes(value: &[u8]) -> Value {
        Value::String(PhpString::from_bytes(value.to_vec()))
    }

    fn object(display_name: &str) -> Value {
        let class = ClassEntry {
            name: display_name.to_ascii_lowercase().into(),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        };
        Value::Object(ObjectRef::new_with_display_name(&class, display_name))
    }

    fn string_key(value: &str) -> ArrayKey {
        ArrayKey::String(PhpString::from_test_str(value))
    }

    #[test]
    fn filter_list_and_filter_id_expose_common_filters() {
        assert_eq!(call("filter_id", vec![string("int")]), Value::Int(257));
        assert_eq!(
            call("filter_id", vec![string("number_float")]),
            Value::Int(520)
        );
        assert_eq!(
            call("filter_id", vec![string("callback")]),
            Value::Int(FILTER_CALLBACK)
        );
        assert_eq!(
            call("filter_id", vec![string("missing")]),
            Value::Bool(false)
        );

        let Value::Array(filters) = call("filter_list", vec![]) else {
            panic!("filter_list should return an array");
        };
        let names = filters
            .iter()
            .map(|(_, value)| match value {
                Value::String(name) => name.to_string_lossy(),
                other => panic!("expected string filter name, got {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "int",
                "boolean",
                "float",
                "validate_regexp",
                "validate_domain",
                "validate_url",
                "validate_email",
                "validate_ip",
                "validate_mac",
                "string",
                "stripped",
                "encoded",
                "special_chars",
                "full_special_chars",
                "unsafe_raw",
                "email",
                "url",
                "number_int",
                "number_float",
                "add_slashes",
                "callback",
            ]
        );
        assert!(filters.iter().any(|(_, value)| value == &string("int")));
        assert!(
            filters
                .iter()
                .any(|(_, value)| value == &string("number_float"))
        );
        assert!(
            filters
                .iter()
                .any(|(_, value)| value == &string("callback"))
        );
    }

    #[test]
    fn filter_var_respects_range_options() {
        let mut range = PhpArray::new();
        range.insert(string_key("min_range"), Value::Int(10));
        range.insert(string_key("max_range"), Value::Int(50));
        let mut options = PhpArray::new();
        options.insert(string_key("options"), Value::Array(range));

        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("42"),
                    Value::Int(FILTER_VALIDATE_INT),
                    Value::Array(options.clone()),
                ],
            ),
            Value::Int(42)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("5"),
                    Value::Int(FILTER_VALIDATE_INT),
                    Value::Array(options.clone()),
                ],
            ),
            Value::Bool(false)
        );

        let mut float_range = PhpArray::new();
        float_range.insert(string_key("min_range"), Value::float(1.0));
        float_range.insert(string_key("max_range"), Value::float(2.0));
        let mut float_options = PhpArray::new();
        float_options.insert(string_key("options"), Value::Array(float_range));

        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("1.25"),
                    Value::Int(FILTER_VALIDATE_FLOAT),
                    Value::Array(float_options.clone()),
                ],
            ),
            Value::float(1.25)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("2.5"),
                    Value::Int(FILTER_VALIDATE_FLOAT),
                    Value::Array(float_options),
                ],
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn filter_var_uses_default_option_on_validation_failure() {
        let mut payload = PhpArray::new();
        payload.insert(string_key("default"), Value::Int(321));
        let mut options = PhpArray::new();
        options.insert(string_key("options"), Value::Array(payload));

        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("123asd"),
                    Value::Int(FILTER_VALIDATE_INT),
                    Value::Array(options),
                ],
            ),
            Value::Int(321)
        );
    }

    #[test]
    fn filter_validation_objects_use_failure_and_default_paths() {
        assert_eq!(
            call(
                "filter_var",
                vec![object("stdClass"), Value::Int(FILTER_VALIDATE_EMAIL)],
            ),
            Value::Bool(false)
        );

        let mut payload = PhpArray::new();
        payload.insert(string_key("default"), Value::Int(2));
        let mut options = PhpArray::new();
        options.insert(string_key("options"), Value::Array(payload));

        assert_eq!(
            call(
                "filter_var",
                vec![
                    object("stdClass"),
                    Value::Int(FILTER_VALIDATE_INT),
                    Value::Array(options.clone()),
                ],
            ),
            Value::Int(2)
        );

        options.insert(string_key("flags"), Value::Int(FILTER_NULL_ON_FAILURE));
        assert_eq!(
            call(
                "filter_var",
                vec![
                    object("stdClass"),
                    Value::Int(FILTER_VALIDATE_INT),
                    Value::Array(options),
                ],
            ),
            Value::Int(2)
        );
    }

    #[test]
    fn filter_validate_float_respects_decimal_option() {
        let mut comma_payload = PhpArray::new();
        comma_payload.insert(string_key("decimal"), string(","));
        let mut comma_options = PhpArray::new();
        comma_options.insert(string_key("options"), Value::Array(comma_payload));

        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("1,234"),
                    Value::Int(FILTER_VALIDATE_FLOAT),
                    Value::Array(comma_options.clone()),
                ],
            ),
            Value::float(1.234)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("1.234"),
                    Value::Int(FILTER_VALIDATE_FLOAT),
                    Value::Array(comma_options),
                ],
            ),
            Value::Bool(false)
        );

        let mut invalid_payload = PhpArray::new();
        invalid_payload.insert(string_key("decimal"), string(".."));
        let mut invalid_options = PhpArray::new();
        invalid_options.insert(string_key("options"), Value::Array(invalid_payload));
        let error = call_error(
            "filter_var",
            vec![
                string("1.234"),
                Value::Int(FILTER_VALIDATE_FLOAT),
                Value::Array(invalid_options),
            ],
        );
        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_BUILTIN_VALUE");
        assert_eq!(
            error.message(),
            "filter_var(): \"decimal\" option must be one character long"
        );
    }

    #[test]
    fn filter_validate_float_respects_thousand_option() {
        let mut payload = PhpArray::new();
        payload.insert(string_key("thousand"), string(" "));
        let mut options = PhpArray::new();
        options.insert(string_key("flags"), Value::Int(FILTER_FLAG_ALLOW_THOUSAND));
        options.insert(string_key("options"), Value::Array(payload));

        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("1 234.567"),
                    Value::Int(FILTER_VALIDATE_FLOAT),
                    Value::Array(options),
                ],
            ),
            Value::float(1234.567)
        );

        let mut invalid_payload = PhpArray::new();
        invalid_payload.insert(string_key("thousand"), string(""));
        let mut invalid_options = PhpArray::new();
        invalid_options.insert(string_key("flags"), Value::Int(FILTER_FLAG_ALLOW_THOUSAND));
        invalid_options.insert(string_key("options"), Value::Array(invalid_payload));
        let error = call_error(
            "filter_var",
            vec![
                string("12345"),
                Value::Int(FILTER_VALIDATE_FLOAT),
                Value::Array(invalid_options),
            ],
        );
        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_BUILTIN_VALUE");
        assert_eq!(
            error.message(),
            "filter_var(): \"thousand\" option must not be empty"
        );
    }

    #[test]
    fn filter_validate_float_rejects_malformed_thousand_groups() {
        let mut options = PhpArray::new();
        options.insert(string_key("flags"), Value::Int(FILTER_FLAG_ALLOW_THOUSAND));

        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("1,234,567,890.1234567165"),
                    Value::Int(FILTER_VALIDATE_FLOAT),
                    Value::Array(options.clone()),
                ],
            ),
            Value::float(1_234_567_890.123_456_7)
        );
        for input in [
            "1234,567,890.1234567165",
            "1,234,567,89.1234567165",
            "1,234,567,8900.1234567165",
            "1,234,567,8900.123,456",
        ] {
            assert_eq!(
                call(
                    "filter_var",
                    vec![
                        string(input),
                        Value::Int(FILTER_VALIDATE_FLOAT),
                        Value::Array(options.clone()),
                    ],
                ),
                Value::Bool(false),
                "{input}"
            );
        }
    }

    #[test]
    fn filter_validate_float_rejects_underflow_to_zero() {
        assert_eq!(
            call(
                "filter_var",
                vec![string("1e-323"), Value::Int(FILTER_VALIDATE_FLOAT)],
            ),
            Value::float(1e-323)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![string("1e-324"), Value::Int(FILTER_VALIDATE_FLOAT)],
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![string("0e-324"), Value::Int(FILTER_VALIDATE_FLOAT)],
            ),
            Value::float(0.0)
        );
    }

    #[test]
    fn filter_validate_regexp_uses_options_pattern() {
        let mut regexp_payload = PhpArray::new();
        regexp_payload.insert(string_key("regexp"), string("/^d(.*)/"));
        let mut regexp_options = PhpArray::new();
        regexp_options.insert(string_key("options"), Value::Array(regexp_payload));

        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("data"),
                    Value::Int(FILTER_VALIDATE_REGEXP),
                    Value::Array(regexp_options),
                ],
            ),
            string("data")
        );

        let mut miss_payload = PhpArray::new();
        miss_payload.insert(string_key("regexp"), string("/^b(.*)/"));
        let mut miss_options = PhpArray::new();
        miss_options.insert(string_key("options"), Value::Array(miss_payload));
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("data"),
                    Value::Int(FILTER_VALIDATE_REGEXP),
                    Value::Array(miss_options),
                ],
            ),
            Value::Bool(false)
        );

        let error = call_error(
            "filter_var",
            vec![string("data"), Value::Int(FILTER_VALIDATE_REGEXP)],
        );
        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_BUILTIN_VALUE");
        assert_eq!(
            error.message(),
            "filter_var(): \"regexp\" option is missing"
        );
    }

    #[test]
    fn filter_validate_mac_respects_separator_option() {
        assert_eq!(
            call(
                "filter_var",
                vec![string("01-23-45-67-89-ab"), Value::Int(FILTER_VALIDATE_MAC),],
            ),
            string("01-23-45-67-89-ab")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![string("01:23:45:67:89:AB"), Value::Int(FILTER_VALIDATE_MAC),],
            ),
            string("01:23:45:67:89:AB")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![string("0123.4567.89ab"), Value::Int(FILTER_VALIDATE_MAC),],
            ),
            string("0123.4567.89ab")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![string("01:23:45-67:89:aB"), Value::Int(FILTER_VALIDATE_MAC),],
            ),
            Value::Bool(false)
        );

        let mut matching_payload = PhpArray::new();
        matching_payload.insert(string_key("separator"), string("-"));
        let mut matching_options = PhpArray::new();
        matching_options.insert(string_key("options"), Value::Array(matching_payload));
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("01-23-45-67-89-ab"),
                    Value::Int(FILTER_VALIDATE_MAC),
                    Value::Array(matching_options),
                ],
            ),
            string("01-23-45-67-89-ab")
        );

        let mut mismatch_payload = PhpArray::new();
        mismatch_payload.insert(string_key("separator"), string(":"));
        let mut mismatch_options = PhpArray::new();
        mismatch_options.insert(string_key("options"), Value::Array(mismatch_payload));
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("01-23-45-67-89-ab"),
                    Value::Int(FILTER_VALIDATE_MAC),
                    Value::Array(mismatch_options),
                ],
            ),
            Value::Bool(false)
        );

        let mut invalid_payload = PhpArray::new();
        invalid_payload.insert(string_key("separator"), string("--"));
        let mut invalid_options = PhpArray::new();
        invalid_options.insert(string_key("options"), Value::Array(invalid_payload));
        let error = call_error(
            "filter_var",
            vec![
                string("01-23-45-67-89-ab"),
                Value::Int(FILTER_VALIDATE_MAC),
                Value::Array(invalid_options),
            ],
        );
        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_BUILTIN_VALUE");
        assert_eq!(
            error.message(),
            "filter_var(): \"separator\" option must be one character long"
        );
    }

    #[test]
    fn filter_validate_domain_respects_hostname_flag() {
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("example.com"),
                    Value::Int(FILTER_VALIDATE_DOMAIN),
                    Value::Int(FILTER_FLAG_HOSTNAME),
                ],
            ),
            string("example.com")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("cont-ains.h-yph-en-s.com"),
                    Value::Int(FILTER_VALIDATE_DOMAIN),
                    Value::Int(FILTER_FLAG_HOSTNAME),
                ],
            ),
            string("cont-ains.h-yph-en-s.com")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string(
                        "kDTvHt1PPDgX5EiP2MwiXjcoWNOhhTuOVAUWJ3TmpBYCC9QoJV114LMYrV3Zl58.kDTvHt1PPDgX5EiP2MwiXjcoWNOhhTuOVAUWJ3TmpBYCC9QoJV114LMYrV3Zl58.kDTvHt1PPDgX5EiP2MwiXjcoWNOhhTuOVAUWJ3TmpBYCC9QoJV114LMYrV3Zl58.CQ1oT5Uq3jJt6Uhy3VH9u3Gi5YhfZCvZVKgLlaXNFhVKB1zJxvunR7SJa.com."
                    ),
                    Value::Int(FILTER_VALIDATE_DOMAIN),
                    Value::Int(FILTER_FLAG_HOSTNAME),
                ],
            ),
            string(
                "kDTvHt1PPDgX5EiP2MwiXjcoWNOhhTuOVAUWJ3TmpBYCC9QoJV114LMYrV3Zl58.kDTvHt1PPDgX5EiP2MwiXjcoWNOhhTuOVAUWJ3TmpBYCC9QoJV114LMYrV3Zl58.kDTvHt1PPDgX5EiP2MwiXjcoWNOhhTuOVAUWJ3TmpBYCC9QoJV114LMYrV3Zl58.CQ1oT5Uq3jJt6Uhy3VH9u3Gi5YhfZCvZVKgLlaXNFhVKB1zJxvunR7SJa.com."
            )
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string(
                        "toolongtoolongtoolongtoolongtoolongtoolongtoolongtoolongtoolongtoolong.com"
                    ),
                    Value::Int(FILTER_VALIDATE_DOMAIN),
                    Value::Int(FILTER_FLAG_HOSTNAME),
                ],
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("a.-bc.com"),
                    Value::Int(FILTER_VALIDATE_DOMAIN),
                    Value::Int(FILTER_FLAG_HOSTNAME),
                ],
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("ab.cd-.com"),
                    Value::Int(FILTER_VALIDATE_DOMAIN),
                    Value::Int(FILTER_FLAG_HOSTNAME),
                ],
            ),
            Value::Bool(false)
        );

        assert_eq!(
            call(
                "filter_var",
                vec![string("_example.com"), Value::Int(FILTER_VALIDATE_DOMAIN),],
            ),
            string("_example.com")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("_example.com"),
                    Value::Int(FILTER_VALIDATE_DOMAIN),
                    Value::Int(FILTER_FLAG_HOSTNAME),
                ],
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("test._example.com"),
                    Value::Int(FILTER_VALIDATE_DOMAIN),
                ],
            ),
            string("test._example.com")
        );
    }

    #[test]
    fn filter_validate_int_accepts_php_hex_and_octal_flags() {
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("0xff"),
                    Value::Int(FILTER_VALIDATE_INT),
                    Value::Int(FILTER_FLAG_ALLOW_HEX),
                ],
            ),
            Value::Int(255)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("0o16"),
                    Value::Int(FILTER_VALIDATE_INT),
                    Value::Int(FILTER_FLAG_ALLOW_OCTAL),
                ],
            ),
            Value::Int(14)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("08"),
                    Value::Int(FILTER_VALIDATE_INT),
                    Value::Int(FILTER_FLAG_ALLOW_OCTAL),
                ],
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("-0xff"),
                    Value::Int(FILTER_VALIDATE_INT),
                    Value::Int(FILTER_FLAG_ALLOW_HEX),
                ],
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("-07"),
                    Value::Int(FILTER_VALIDATE_INT),
                    Value::Int(FILTER_FLAG_ALLOW_OCTAL),
                ],
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("0xffffffffffffffff"),
                    Value::Int(FILTER_VALIDATE_INT),
                    Value::Int(FILTER_FLAG_ALLOW_HEX),
                ],
            ),
            Value::Int(-1)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("0x10000000000000000"),
                    Value::Int(FILTER_VALIDATE_INT),
                    Value::Int(FILTER_FLAG_ALLOW_HEX),
                ],
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn filter_validate_email_enforces_php_length_limits() {
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("valid@email.address"),
                    Value::Int(FILTER_VALIDATE_EMAIL)
                ],
            ),
            string("valid@email.address")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string(
                        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx@y.zz"
                    ),
                    Value::Int(FILTER_VALIDATE_EMAIL),
                ],
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string(&"x".repeat(8_000)),
                    Value::Int(FILTER_VALIDATE_EMAIL),
                ],
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn filter_validate_email_rejects_hyphen_edge_domain_labels() {
        for input in ["foo@-foo.com", "foo@foo-.com"] {
            assert_eq!(
                call(
                    "filter_var",
                    vec![string(input), Value::Int(FILTER_VALIDATE_EMAIL)],
                ),
                Value::Bool(false),
                "{input}"
            );
        }
    }

    #[test]
    fn filter_validate_email_matches_php_local_part_edges() {
        for input in [
            "[]()/@example.com",
            "e.x.a.m.p.l.e.@example.com",
            "foo@bar.123",
        ] {
            assert_eq!(
                call(
                    "filter_var",
                    vec![string(input), Value::Int(FILTER_VALIDATE_EMAIL)],
                ),
                Value::Bool(false),
                "{input}"
            );
        }
        assert_eq!(
            call(
                "filter_var",
                vec![string("-@foo.com"), Value::Int(FILTER_VALIDATE_EMAIL)],
            ),
            string("-@foo.com")
        );
    }

    #[test]
    fn filter_validate_email_honors_unicode_flag_and_literals() {
        for input in [
            "niceändsimple@example.com",
            "üser@[IPv6:2001:db8:1ff::a0b:dbd0]",
            "\"verî.uñusual.@.uñusual.com\"@example.com",
            "\"verî.(),:;<>[]\\\".VERÎ.\\\"verî@\\ \\\"verî\\\".unüsual\"@strange.example.com",
            "tést@[255.255.255.255]",
        ] {
            assert_eq!(
                call(
                    "filter_var",
                    vec![
                        string(input),
                        Value::Int(FILTER_VALIDATE_EMAIL),
                        Value::Int(FILTER_FLAG_EMAIL_UNICODE),
                    ],
                ),
                string(input),
                "{input}"
            );
        }
    }

    #[test]
    fn filter_validate_url_rejects_underscore_hostname_labels() {
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("http://exa-mple.com/path"),
                    Value::Int(FILTER_VALIDATE_URL),
                ],
            ),
            string("http://exa-mple.com/path")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("http://exa_mple.com/path"),
                    Value::Int(FILTER_VALIDATE_URL),
                ],
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn filter_validate_url_rejects_confusing_userinfo_authority() {
        for input in [
            "http://php.net\\@aliyun.com/aaa.do",
            "https://example.com:\\@test.com/",
            "https://user:\\epass@test.com",
            "http://t[est@127.0.0.1",
            "http://test[@2001:db8:3333:4444:5555:6666:1.2.3.4]",
        ] {
            assert_eq!(
                call(
                    "filter_var",
                    vec![string(input), Value::Int(FILTER_VALIDATE_URL)],
                ),
                Value::Bool(false),
                "{input}"
            );
        }
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("http://test@[2001:db8:3333:4444:5555:6666:1.2.3.4]"),
                    Value::Int(FILTER_VALIDATE_URL),
                ],
            ),
            string("http://test@[2001:db8:3333:4444:5555:6666:1.2.3.4]")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![string("http://test@[::1]"), Value::Int(FILTER_VALIDATE_URL)],
            ),
            string("http://test@[::1]")
        );
    }

    #[test]
    fn filter_validate_url_accepts_promoted_scheme_matrix() {
        for input in [
            "file:///tmp/test.c",
            "ftp://ftp.example.com/tmp/",
            "mailto:foo@bar.com",
            "news:news.php.net",
            "file://foo/bar",
        ] {
            assert_eq!(
                call(
                    "filter_var",
                    vec![string(input), Value::Int(FILTER_VALIDATE_URL)],
                ),
                string(input),
                "{input}"
            );
        }
        for input in [
            "http://example.com:qq",
            "http://example.com:-2",
            "http://example.com:65536",
            "http://example.com:65537",
            "aa:bb:cc:dd:ee:ff",
        ] {
            assert_eq!(
                call(
                    "filter_var",
                    vec![string(input), Value::Int(FILTER_VALIDATE_URL)],
                ),
                Value::Bool(false),
                "{input}"
            );
        }
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("http://www.example.com"),
                    Value::Int(FILTER_VALIDATE_URL),
                    Value::Int(FILTER_FLAG_PATH_REQUIRED),
                ],
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("http://www.example.com/path/at/the/server/"),
                    Value::Int(FILTER_VALIDATE_URL),
                    Value::Int(FILTER_FLAG_PATH_REQUIRED),
                ],
            ),
            string("http://www.example.com/path/at/the/server/")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("http://www.example.com/index.php?a=b&c=d"),
                    Value::Int(FILTER_VALIDATE_URL),
                    Value::Int(FILTER_FLAG_QUERY_REQUIRED),
                ],
            ),
            string("http://www.example.com/index.php?a=b&c=d")
        );
    }

    #[test]
    fn filter_validate_ip_honors_range_flags() {
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("192.168.0.1"),
                    Value::Int(FILTER_VALIDATE_IP),
                    Value::Int(FILTER_FLAG_NO_PRIV_RANGE),
                ],
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("100.64.0.0"),
                    Value::Int(FILTER_VALIDATE_IP),
                    Value::Int(FILTER_FLAG_NO_RES_RANGE),
                ],
            ),
            string("100.64.0.0")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("100.64.0.0"),
                    Value::Int(FILTER_VALIDATE_IP),
                    Value::Int(FILTER_FLAG_GLOBAL_RANGE),
                ],
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("185.85.0.29"),
                    Value::Int(FILTER_VALIDATE_IP),
                    Value::Int(FILTER_FLAG_GLOBAL_RANGE),
                ],
            ),
            string("185.85.0.29")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("FC00::1"),
                    Value::Int(FILTER_VALIDATE_IP),
                    Value::Int(FILTER_FLAG_IPV6 | FILTER_FLAG_NO_PRIV_RANGE),
                ],
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("::ffff:0:1"),
                    Value::Int(FILTER_VALIDATE_IP),
                    Value::Int(FILTER_FLAG_NO_RES_RANGE),
                ],
            ),
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("64:ff9b::"),
                    Value::Int(FILTER_VALIDATE_IP),
                    Value::Int(FILTER_FLAG_GLOBAL_RANGE),
                ],
            ),
            string("64:ff9b::")
        );
    }

    #[test]
    fn filter_callback_invokes_registered_builtin_string_callbacks() {
        let mut options = PhpArray::new();
        options.insert(string_key("options"), string("strtoupper"));

        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("abc"),
                    Value::Int(FILTER_CALLBACK),
                    Value::Array(options),
                ],
            ),
            string("ABC")
        );
    }

    #[test]
    fn filter_var_array_applies_per_key_specs() {
        let mut input = PhpArray::new();
        input.insert(string_key("a"), string("42"));
        input.insert(string_key("b"), string("x"));
        input.insert(string_key("c"), string("1.25"));

        let mut b_spec = PhpArray::new();
        b_spec.insert(string_key("filter"), Value::Int(FILTER_VALIDATE_INT));
        b_spec.insert(string_key("flags"), Value::Int(FILTER_NULL_ON_FAILURE));

        let mut specs = PhpArray::new();
        specs.insert(string_key("a"), Value::Int(FILTER_VALIDATE_INT));
        specs.insert(string_key("b"), Value::Array(b_spec));
        specs.insert(string_key("c"), Value::Int(FILTER_VALIDATE_FLOAT));

        let Value::Array(result) = call(
            "filter_var_array",
            vec![Value::Array(input), Value::Array(specs)],
        ) else {
            panic!("filter_var_array should return an array");
        };
        assert_eq!(result.get(&string_key("a")), Some(&Value::Int(42)));
        assert_eq!(result.get(&string_key("b")), Some(&Value::Null));
        assert_eq!(result.get(&string_key("c")), Some(&Value::float(1.25)));
    }

    #[test]
    fn filter_var_array_unknown_filter_modes_match_php_shapes() {
        let mut input = PhpArray::new();
        input.insert(string_key("test"), string("42"));

        assert_eq!(
            call(
                "filter_var_array",
                vec![Value::Array(input.clone()), Value::Int(-1)],
            ),
            Value::Bool(false)
        );

        let mut specs = PhpArray::new();
        specs.insert(string_key("test"), Value::Int(-1));
        let Value::Array(result) = call(
            "filter_var_array",
            vec![Value::Array(input), Value::Array(specs)],
        ) else {
            panic!("filter_var_array should return an array");
        };
        assert_eq!(result.get(&string_key("test")), Some(&string("42")));
    }

    #[test]
    fn filter_var_array_single_filter_recurses_and_writes_references() {
        let referenced = crate::ReferenceCell::new(Value::packed_array(vec![string("123foo")]));
        let input = PhpArray::from_packed(vec![Value::Reference(referenced.clone())]);

        let Value::Array(result) = call(
            "filter_var_array",
            vec![Value::Array(input), Value::Int(FILTER_VALIDATE_INT)],
        ) else {
            panic!("filter_var_array should return an array");
        };
        let Some(Value::Reference(result_ref)) = result.get(&ArrayKey::Int(0)) else {
            panic!("referenced array element should stay a reference");
        };
        assert!(result_ref.ptr_eq(&referenced));
        let Value::Array(filtered) = referenced.get() else {
            panic!("referenced array should be replaced with filtered array");
        };
        assert_eq!(filtered.get(&ArrayKey::Int(0)), Some(&Value::Bool(false)));
    }

    #[test]
    fn filter_var_array_rejects_invalid_options_shape_and_empty_spec_keys() {
        let error = call_error(
            "filter_var_array",
            vec![Value::Array(PhpArray::new()), string("")],
        );
        assert_eq!(
            error.message(),
            "filter_var_array(): Argument #2 ($options) must be of type array|int, string given"
        );

        let mut specs = PhpArray::new();
        specs.insert(string_key(""), Value::Int(FILTER_DEFAULT));
        let error = call_error(
            "filter_var_array",
            vec![Value::Array(PhpArray::new()), Value::Array(specs)],
        );
        assert_eq!(
            error.message(),
            "filter_var_array(): Argument #2 ($options) cannot contain empty keys"
        );
    }

    #[test]
    fn filter_array_flags_control_array_and_scalar_shape() {
        let input = Value::packed_array(vec![string("1"), string("x")]);
        let Value::Array(result) = call(
            "filter_var",
            vec![
                input,
                Value::Int(FILTER_VALIDATE_INT),
                Value::Int(FILTER_REQUIRE_ARRAY),
            ],
        ) else {
            panic!("FILTER_REQUIRE_ARRAY should map arrays");
        };
        assert_eq!(result.get(&ArrayKey::Int(0)), Some(&Value::Int(1)));
        assert_eq!(result.get(&ArrayKey::Int(1)), Some(&Value::Bool(false)));

        let Value::Array(result) = call(
            "filter_var",
            vec![
                string("7"),
                Value::Int(FILTER_VALIDATE_INT),
                Value::Int(FILTER_FORCE_ARRAY),
            ],
        ) else {
            panic!("FILTER_FORCE_ARRAY should wrap scalars");
        };
        assert_eq!(result.get(&ArrayKey::Int(0)), Some(&Value::Int(7)));
    }

    #[test]
    fn filter_require_array_recurses_into_nested_arrays() {
        let input = Value::packed_array(vec![
            string("1"),
            Value::packed_array(vec![string("2"), string("x")]),
        ]);
        let Value::Array(result) = call(
            "filter_var",
            vec![
                input,
                Value::Int(FILTER_VALIDATE_INT),
                Value::Int(FILTER_REQUIRE_ARRAY),
            ],
        ) else {
            panic!("FILTER_REQUIRE_ARRAY should map arrays");
        };
        assert_eq!(result.get(&ArrayKey::Int(0)), Some(&Value::Int(1)));
        let Some(Value::Array(nested)) = result.get(&ArrayKey::Int(1)) else {
            panic!("nested arrays should be preserved");
        };
        assert_eq!(nested.get(&ArrayKey::Int(0)), Some(&Value::Int(2)));
        assert_eq!(nested.get(&ArrayKey::Int(1)), Some(&Value::Bool(false)));
    }

    #[test]
    fn sanitize_special_chars_uses_decimal_entities() {
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("<data&sons>'\""),
                    Value::Int(FILTER_SANITIZE_SPECIAL_CHARS),
                ],
            ),
            string("&#60;data&#38;sons&#62;&#39;&#34;")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("<data&sons>'\""),
                    Value::Int(FILTER_SANITIZE_FULL_SPECIAL_CHARS),
                ],
            ),
            string("&lt;data&amp;sons&gt;&#039;&quot;")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("кириллица"),
                    Value::Int(FILTER_SANITIZE_SPECIAL_CHARS),
                    Value::Int(FILTER_FLAG_ENCODE_HIGH),
                ],
            ),
            string(
                "&#208;&#186;&#208;&#184;&#209;&#128;&#208;&#184;&#208;&#187;&#208;&#187;&#208;&#184;&#209;&#134;&#208;&#176;"
            )
        );
    }

    #[test]
    fn sanitize_encoded_percent_encodes_non_alnum_bytes() {
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("\"<br>blah</ph>"),
                    Value::Int(FILTER_SANITIZE_ENCODED),
                ],
            ),
            string("%22%3Cbr%3Eblah%3C%2Fph%3E")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("<data&sons>"),
                    Value::Int(FILTER_SANITIZE_ENCODED),
                    Value::Int(FILTER_FLAG_ENCODE_LOW),
                ],
            ),
            string("%3Cdata%26sons%3E")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![string("2.0.33"), Value::Int(FILTER_SANITIZE_ENCODED)],
            ),
            string("2.0.33")
        );
    }

    #[test]
    fn unsafe_raw_can_encode_ampersands() {
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("a&b&c"),
                    Value::Int(FILTER_UNSAFE_RAW),
                    Value::Int(FILTER_FLAG_ENCODE_AMP),
                ],
            ),
            string("a&#38;b&#38;c")
        );
    }

    #[test]
    fn unsafe_raw_can_convert_empty_string_to_null() {
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string(""),
                    Value::Int(FILTER_DEFAULT),
                    Value::Int(FILTER_FLAG_EMPTY_STRING_NULL),
                ],
            ),
            Value::Null
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("`"),
                    Value::Int(FILTER_UNSAFE_RAW),
                    Value::Int(FILTER_FLAG_STRIP_BACKTICK | FILTER_FLAG_EMPTY_STRING_NULL),
                ],
            ),
            Value::Null
        );
    }

    #[test]
    fn unsafe_raw_encodes_low_and_high_bytes() {
        assert_eq!(
            call(
                "filter_var",
                vec![
                    bytes(&[0x00, b'&', 0x7f, 0x80]),
                    Value::Int(FILTER_UNSAFE_RAW),
                    Value::Int(
                        FILTER_FLAG_ENCODE_LOW | FILTER_FLAG_ENCODE_HIGH | FILTER_FLAG_ENCODE_AMP
                    ),
                ],
            ),
            string("&#0;&#38;&#127;&#128;")
        );
    }

    #[test]
    fn sanitize_filters_strip_high_from_ascii_del() {
        assert_eq!(
            call(
                "filter_var",
                vec![
                    bytes(&[0x7f]),
                    Value::Int(FILTER_UNSAFE_RAW),
                    Value::Int(FILTER_FLAG_STRIP_HIGH),
                ],
            ),
            string("")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    bytes(&[0x7f]),
                    Value::Int(FILTER_SANITIZE_ENCODED),
                    Value::Int(FILTER_FLAG_STRIP_HIGH),
                ],
            ),
            string("")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    bytes(&[0x7f]),
                    Value::Int(FILTER_SANITIZE_SPECIAL_CHARS),
                    Value::Int(FILTER_FLAG_STRIP_HIGH),
                ],
            ),
            string("")
        );
    }

    #[test]
    fn sanitize_string_strips_nested_legacy_tags() {
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("!@#$%^&*()><<>+_\"'<br><p /><li />"),
                    Value::Int(FILTER_SANITIZE_STRING),
                ],
            ),
            string("!@#$%^&*()>")
        );
    }

    #[test]
    fn unsafe_raw_strips_backticks_independently() {
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("``a`b`c``"),
                    Value::Int(FILTER_UNSAFE_RAW),
                    Value::Int(FILTER_FLAG_STRIP_BACKTICK),
                ],
            ),
            string("abc")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("``a`b`c``"),
                    Value::Int(FILTER_UNSAFE_RAW),
                    Value::Int(FILTER_FLAG_STRIP_LOW | FILTER_FLAG_STRIP_BACKTICK),
                ],
            ),
            string("abc")
        );
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("``a`b`c``"),
                    Value::Int(FILTER_UNSAFE_RAW),
                    Value::Int(FILTER_FLAG_STRIP_LOW | FILTER_FLAG_STRIP_HIGH),
                ],
            ),
            string("``a`b`c``")
        );
    }

    #[test]
    fn sanitize_number_float_respects_flags() {
        assert_eq!(
            call(
                "filter_var",
                vec![
                    string("12.3e+4,5x"),
                    Value::Int(FILTER_SANITIZE_NUMBER_FLOAT),
                    Value::Int(FILTER_FLAG_ALLOW_FRACTION | FILTER_FLAG_ALLOW_SCIENTIFIC),
                ],
            ),
            string("12.3e+45")
        );
    }

    #[test]
    fn filter_input_array_uses_request_snapshot() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let mut input = PhpArray::new();
        input.insert(string_key("id"), string("42"));
        context.set_filter_input_array(1, input);

        let has_var = ENTRIES
            .iter()
            .find(|entry| entry.name() == "filter_has_var")
            .expect("filter_has_var entry")
            .function()(
            &mut context,
            vec![Value::Int(1), string("id")],
            RuntimeSourceSpan::default(),
        )
        .expect("filter_has_var succeeds");
        assert_eq!(has_var, Value::Bool(true));

        let mut specs = PhpArray::new();
        specs.insert(string_key("id"), Value::Int(FILTER_VALIDATE_INT));
        let Value::Array(result) = ENTRIES
            .iter()
            .find(|entry| entry.name() == "filter_input_array")
            .expect("filter_input_array entry")
            .function()(
            &mut context,
            vec![Value::Int(1), Value::Array(specs)],
            RuntimeSourceSpan::default(),
        )
        .expect("filter_input_array succeeds") else {
            panic!("filter_input_array should return an array");
        };
        assert_eq!(result.get(&string_key("id")), Some(&Value::Int(42)));
    }

    #[test]
    fn filter_input_array_returns_null_for_empty_request_source() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        context.set_filter_input_array(1, PhpArray::new());

        let mut specs = PhpArray::new();
        let mut spec = PhpArray::new();
        spec.insert(string_key("flags"), Value::Int(FILTER_NULL_ON_FAILURE));
        specs.insert(string_key("c"), Value::Array(spec));

        let result = ENTRIES
            .iter()
            .find(|entry| entry.name() == "filter_input_array")
            .expect("filter_input_array entry")
            .function()(
            &mut context,
            vec![Value::Int(1), Value::Array(specs), Value::Bool(true)],
            RuntimeSourceSpan::default(),
        )
        .expect("filter_input_array succeeds");
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn filter_input_missing_value_respects_null_on_failure() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let result = ENTRIES
            .iter()
            .find(|entry| entry.name() == "filter_input")
            .expect("filter_input entry")
            .function()(
            &mut context,
            vec![
                Value::Int(1),
                string("missing"),
                Value::Int(FILTER_VALIDATE_INT),
                Value::Int(FILTER_NULL_ON_FAILURE),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("filter_input succeeds");
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn filter_input_missing_value_uses_default_option() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let mut payload = PhpArray::new();
        payload.insert(string_key("default"), Value::Int(23));
        let mut options = PhpArray::new();
        options.insert(string_key("flags"), Value::Int(FILTER_REQUIRE_SCALAR));
        options.insert(string_key("options"), Value::Array(payload));

        let result = ENTRIES
            .iter()
            .find(|entry| entry.name() == "filter_input")
            .expect("filter_input entry")
            .function()(
            &mut context,
            vec![
                Value::Int(INPUT_GET),
                string("foo"),
                Value::Int(FILTER_VALIDATE_INT),
                Value::Array(options),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("filter_input succeeds");
        assert_eq!(result, Value::Int(23));
    }

    #[test]
    fn filter_has_var_rejects_invalid_input_source() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let error = ENTRIES
            .iter()
            .find(|entry| entry.name() == "filter_has_var")
            .expect("filter_has_var entry")
            .function()(
            &mut context,
            vec![Value::Int(-1), string("missing")],
            RuntimeSourceSpan::default(),
        )
        .expect_err("filter_has_var should reject invalid input source");
        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_BUILTIN_VALUE");
        assert_eq!(
            error.message(),
            "filter_has_var(): Argument #1 ($input_type) must be an INPUT_* constant"
        );
    }

    #[test]
    fn filter_validate_domain_does_not_emit_unknown_filter_warning() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let result = ENTRIES
            .iter()
            .find(|entry| entry.name() == "filter_var")
            .expect("filter_var entry")
            .function()(
            &mut context,
            vec![
                string("example.com"),
                Value::Int(FILTER_VALIDATE_DOMAIN),
                Value::Int(FILTER_NULL_ON_FAILURE),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("filter_var succeeds");
        assert_eq!(result, string("example.com"));
        assert!(context.take_diagnostics().is_empty());
    }

    #[test]
    fn filter_validate_boolean_null_respects_null_on_failure() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let result = ENTRIES
            .iter()
            .find(|entry| entry.name() == "filter_var")
            .expect("filter_var entry")
            .function()(
            &mut context,
            vec![
                Value::Null,
                Value::Int(FILTER_VALIDATE_BOOL),
                Value::Int(FILTER_NULL_ON_FAILURE),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("filter_var succeeds");
        assert_eq!(result, Value::Null);
    }
}
