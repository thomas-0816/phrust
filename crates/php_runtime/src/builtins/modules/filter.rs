//! Bounded filter extension MVP for common validation and sanitization.

use super::core::{arity_error, conversion_error, deref_value, float_arg, int_arg, string_arg};
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
const FILTER_FLAG_STRIP_BACKTICK: i64 = 512;
const FILTER_FLAG_ALLOW_FRACTION: i64 = 4_096;
const FILTER_FLAG_ALLOW_THOUSAND: i64 = 8_192;
const FILTER_FLAG_ALLOW_SCIENTIFIC: i64 = 16_384;
const FILTER_FLAG_IPV4: i64 = 1_048_576;
const FILTER_FLAG_IPV6: i64 = 2_097_152;
const FILTER_FLAG_HOSTNAME: i64 = 1_048_576;
const FILTER_FLAG_PATH_REQUIRED: i64 = 262_144;
const FILTER_FLAG_QUERY_REQUIRED: i64 = 524_288;

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
    decimal: Option<String>,
    separator: Option<String>,
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
            decimal: None,
            separator: None,
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
            return Ok(filter_failure(options.flags));
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
        return Ok(filter_failure(options.flags));
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
    let failure = if options.flags & FILTER_NULL_ON_FAILURE != 0 {
        Value::Null
    } else {
        Value::Bool(false)
    };
    match filter {
        FILTER_DEFAULT => unsafe_raw(name, value, options.flags),
        FILTER_VALIDATE_EMAIL => validate_email(name, value, failure),
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
        FILTER_SANITIZE_SPECIAL_CHARS | FILTER_SANITIZE_FULL_SPECIAL_CHARS => {
            sanitize_special_chars(name, value, options.flags)
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
    let normalized = normalize_filter_float_decimal(name, trimmed, options.decimal.as_deref())?;
    match normalized.parse::<f64>() {
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
        return Ok(if options.flags & FILTER_NULL_ON_FAILURE != 0 {
            Value::Bool(false)
        } else {
            Value::Null
        });
    };
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
    let compiled = match context.pcre_cache().compile(regexp) {
        Ok(compiled) => compiled,
        Err(error) => {
            context.set_preg_last_error(error.code(), pcre::preg_error_message(error.code()));
            return Ok(failure);
        }
    };
    match compiled.is_match(input.as_bytes()) {
        Ok(true) => Ok(Value::String(input)),
        Ok(false) => Ok(failure),
        Err(error) => {
            context.set_preg_last_error(error.code(), pcre::preg_error_message(error.code()));
            Ok(failure)
        }
    }
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

fn validate_mac(
    name: &str,
    value: &Value,
    options: &FilterOptions,
    failure: Value,
) -> BuiltinResult {
    let input = string_arg(name, value)?;
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
    let input = string_arg(name, value)?;
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
    Ok(Value::string(
        encoded
            .into_iter()
            .filter(|byte| {
                !(strip_low && is_filter_low(*byte))
                    && !(strip_high && is_filter_high(*byte))
                    && !(strip_backtick && *byte == b'`')
            })
            .collect::<Vec<_>>(),
    ))
}

fn sanitize_encoded(name: &str, value: &Value, flags: i64) -> BuiltinResult {
    let input = string_arg(name, value)?;
    let strip_low = flags & FILTER_FLAG_STRIP_LOW != 0;
    let strip_high = flags & FILTER_FLAG_STRIP_HIGH != 0;
    let stripped: Vec<u8> = input
        .as_bytes()
        .iter()
        .copied()
        .filter(|byte| {
            !(strip_low && is_filter_low(*byte)) && !(strip_high && is_filter_high(*byte))
        })
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
    Ok(Value::string(output))
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
            !(strip_low && is_filter_low(*byte))
                && !(strip_high && is_filter_high(*byte))
                && !(strip_backtick && *byte == b'`')
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
        if matches!(*byte, b'<' | b'>' | b'&')
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
                    string("<data&sons>"),
                    Value::Int(FILTER_SANITIZE_SPECIAL_CHARS),
                ],
            ),
            string("&#60;data&#38;sons&#62;")
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
}
