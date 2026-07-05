//! Bounded filter extension MVP for common validation and sanitization.

use super::core::{arity_error, conversion_error, deref_value, float_arg, int_arg, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinRegistry,
    BuiltinResult, RuntimeSourceSpan,
};
use crate::{ArrayKey, PhpArray, PhpString, Value, to_bool, to_string};
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
const FILTER_FLAG_ALLOW_FRACTION: i64 = 4_096;
const FILTER_FLAG_ALLOW_THOUSAND: i64 = 8_192;
const FILTER_FLAG_ALLOW_SCIENTIFIC: i64 = 16_384;
const FILTER_FLAG_IPV4: i64 = 1_048_576;
const FILTER_FLAG_IPV6: i64 = 2_097_152;
const FILTER_FLAG_PATH_REQUIRED: i64 = 262_144;
const FILTER_FLAG_QUERY_REQUIRED: i64 = 524_288;

const FILTER_NAMES: &[(&str, i64)] = &[
    ("int", FILTER_VALIDATE_INT),
    ("boolean", FILTER_VALIDATE_BOOL),
    ("float", FILTER_VALIDATE_FLOAT),
    ("validate_regexp", FILTER_VALIDATE_REGEXP),
    ("validate_url", FILTER_VALIDATE_URL),
    ("validate_email", FILTER_VALIDATE_EMAIL),
    ("validate_ip", FILTER_VALIDATE_IP),
    ("validate_mac", FILTER_VALIDATE_MAC),
    ("validate_domain", FILTER_VALIDATE_DOMAIN),
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
            return Ok(filter_failure(options.flags));
        }
        let mut output = PhpArray::new();
        for (key, value) in array.iter() {
            output.insert(
                key.clone(),
                apply_filter_scalar(context, name, value, filter, options, span.clone())?,
            );
        }
        return Ok(Value::Array(output));
    }

    if options.flags & FILTER_REQUIRE_ARRAY != 0 {
        return Ok(filter_failure(options.flags));
    }

    let filtered = apply_filter_scalar(context, name, value, filter, options, span)?;
    if options.flags & FILTER_FORCE_ARRAY != 0 {
        return Ok(Value::Array(PhpArray::from_packed(vec![filtered])));
    }
    Ok(filtered)
}

fn apply_filter_scalar(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: &Value,
    filter: i64,
    options: &FilterOptions,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    let failure = if options.flags & FILTER_NULL_ON_FAILURE != 0 {
        Value::Null
    } else {
        Value::Bool(false)
    };
    match filter {
        FILTER_DEFAULT => Ok(value.clone()),
        FILTER_VALIDATE_EMAIL => validate_email(name, value, failure),
        FILTER_VALIDATE_INT => validate_int(name, value, options, failure),
        FILTER_VALIDATE_FLOAT => validate_float(name, value, options, failure),
        FILTER_VALIDATE_URL => validate_url(name, value, options.flags, failure),
        FILTER_VALIDATE_IP => validate_ip(name, value, options.flags, failure),
        FILTER_VALIDATE_BOOL => validate_bool(name, value, options.flags, failure),
        FILTER_SANITIZE_EMAIL => sanitize(name, value, is_email_sanitize_byte),
        FILTER_SANITIZE_URL => sanitize(name, value, is_url_sanitize_byte),
        FILTER_SANITIZE_NUMBER_INT => sanitize(name, value, |byte| {
            byte.is_ascii_digit() || byte == b'+' || byte == b'-'
        }),
        FILTER_SANITIZE_NUMBER_FLOAT => sanitize_number_float(name, value, options.flags),
        FILTER_CALLBACK => apply_callback_filter(context, name, value, options, span),
        _ => Ok(failure),
    }
}

fn filter_failure(flags: i64) -> Value {
    if flags & FILTER_NULL_ON_FAILURE != 0 {
        Value::Null
    } else {
        Value::Bool(false)
    }
}

fn validate_int(
    name: &str,
    value: &Value,
    options: &FilterOptions,
    failure: Value,
) -> BuiltinResult {
    let input = string_arg(name, value)?;
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
    let input = string_arg(name, value)?;
    let text = input.to_string_lossy();
    let trimmed = text.trim();
    match trimmed.parse::<f64>() {
        Ok(number)
            if number.is_finite()
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

fn builtin_filter_input(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=5).contains(&args.len()) {
        return Err(arity_error("filter_input", "two to five argument(s)"));
    }
    let source = int_arg("filter_input", &args[0])?;
    let name = string_arg("filter_input", &args[1])?.to_string_lossy();
    let Some(value) = context.filter_input_value(source, &name) else {
        return Ok(Value::Null);
    };
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
    apply_filter(context, "filter_input", &value, filter, &options, span)
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
    let Some(array) = context.filter_input_array(source) else {
        return Ok(Value::Null);
    };
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
        Some(other) => {
            filter_array_with_single_filter(context, name, input, int_arg(name, &other)?, span)
        }
    }
}

fn filter_array_with_single_filter(
    context: &mut BuiltinContext<'_>,
    name: &str,
    input: &PhpArray,
    filter: i64,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    let options = FilterOptions::default();
    let mut output = PhpArray::new();
    for (key, value) in input.iter() {
        output.insert(
            key.clone(),
            apply_filter(context, name, value, filter, &options, span.clone())?,
        );
    }
    Ok(Value::Array(output))
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
        match input.get(&key) {
            Some(value) => {
                let filter = filter_spec_filter(name, spec)?;
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

fn validate_email(name: &str, value: &Value, failure: Value) -> BuiltinResult {
    let input = string_arg(name, value)?;
    if input.as_bytes().len() > 320 {
        return Ok(failure);
    }
    let string = input.to_string_lossy();
    let mut parts = string.split('@');
    let Some(local) = parts.next() else {
        return Ok(failure);
    };
    let Some(domain) = parts.next() else {
        return Ok(failure);
    };
    if parts.next().is_none()
        && !local.is_empty()
        && local.len() <= 64
        && domain.contains('.')
        && domain
            .split('.')
            .all(|label| !label.is_empty() && label.len() <= 63)
        && !php_source::byte_kernel::contains_ascii_whitespace(input.as_bytes())
    {
        Ok(Value::String(input))
    } else {
        Ok(failure)
    }
}

fn validate_url(name: &str, value: &Value, flags: i64, failure: Value) -> BuiltinResult {
    let input = string_arg(name, value)?;
    let string = input.to_string_lossy();
    let has_scheme = string.starts_with("http://") || string.starts_with("https://");
    let after_scheme = string.split_once("://").map(|(_, tail)| tail).unwrap_or("");
    let has_host = !after_scheme.is_empty()
        && !after_scheme.starts_with('/')
        && !php_source::byte_kernel::contains_ascii_whitespace(after_scheme.as_bytes());
    let path_ok = flags & FILTER_FLAG_PATH_REQUIRED == 0 || after_scheme.contains('/');
    let query_ok = flags & FILTER_FLAG_QUERY_REQUIRED == 0 || after_scheme.contains('?');
    if has_scheme && has_host && path_ok && query_ok {
        Ok(Value::String(input))
    } else {
        Ok(failure)
    }
}

fn validate_ip(name: &str, value: &Value, flags: i64, failure: Value) -> BuiltinResult {
    let input = string_arg(name, value)?;
    let string = input.to_string_lossy();
    match string.parse::<IpAddr>() {
        Ok(IpAddr::V4(_)) if flags & FILTER_FLAG_IPV6 == 0 => Ok(Value::String(input)),
        Ok(IpAddr::V6(_)) if flags & FILTER_FLAG_IPV4 == 0 => Ok(Value::String(input)),
        Ok(_) => Ok(failure),
        Err(_) => Ok(failure),
    }
}

fn validate_bool(_name: &str, value: &Value, flags: i64, failure: Value) -> BuiltinResult {
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

fn is_email_sanitize_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-=?^_`{|}~@.[]".contains(&byte)
}

fn is_url_sanitize_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"$-_.+!*'(),{}|\\^~[]`<>#%\";/?:@&=".contains(&byte)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputBuffer;

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

    fn string(value: &str) -> Value {
        Value::String(PhpString::from_test_str(value))
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
}
