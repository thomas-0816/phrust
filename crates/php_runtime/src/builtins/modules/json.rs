//! Json builtin registry slice.

use super::super::context::{
    JSON_BIGINT_AS_STRING, JSON_ERROR_CTRL_CHAR, JSON_ERROR_DEPTH, JSON_ERROR_NONE,
    JSON_ERROR_STATE_MISMATCH, JSON_ERROR_SYNTAX, JSON_ERROR_UTF8, JSON_OBJECT_AS_ARRAY,
    JSON_PRETTY_PRINT, JSON_THROW_ON_ERROR, json_error_message,
};
use super::core::*;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{Value, to_bool};
use serde_json::Value as JsonValue;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "json_decode",
        builtin_json_decode,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "json_encode",
        builtin_json_encode,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "json_last_error",
        builtin_json_last_error,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "json_last_error_msg",
        builtin_json_last_error_msg,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "json_validate",
        builtin_json_validate,
        BuiltinCompatibility::Php,
    ),
];

pub(in crate::builtins::modules) fn builtin_json_encode(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error("json_encode", "one to three argument(s)"));
    }
    let flags = args
        .get(1)
        .map(|value| int_arg("json_encode", value))
        .transpose()?
        .unwrap_or(0);
    match php_value_to_json_checked(&args[0], flags) {
        Ok((json, encode_error)) => {
            let encoded = if flags & JSON_PRETTY_PRINT != 0 {
                serde_json::to_string_pretty(&json)
            } else {
                serde_json::to_string(&json)
            };
            match encoded {
                Ok(encoded) => {
                    context.set_json_last_error(encode_error.unwrap_or(JSON_ERROR_NONE));
                    Ok(Value::string(normalize_json_encoded(encoded, flags)))
                }
                Err(_) => json_failure(context, flags, JSON_ERROR_SYNTAX),
            }
        }
        Err(code) => json_failure(context, flags, code),
    }
}
pub(in crate::builtins::modules) fn builtin_json_decode(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 4 {
        return Err(arity_error("json_decode", "one to four argument(s)"));
    }
    let input = string_arg("json_decode", &args[0])?;
    let associative = args
        .get(1)
        .map(|value| {
            if matches!(deref_value(value), Value::Null) {
                Ok(false)
            } else {
                to_bool(value)
                    .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_TYPE_ERROR", message))
            }
        })
        .transpose()?
        .unwrap_or(false);
    let depth = args
        .get(2)
        .map(|value| int_arg("json_decode", value))
        .transpose()?
        .unwrap_or(512);
    let flags = args
        .get(3)
        .map(|value| int_arg("json_decode", value))
        .transpose()?
        .unwrap_or(0);
    if depth <= 0 {
        return json_decode_failure(context, flags, JSON_ERROR_DEPTH);
    }
    let Ok(input) = std::str::from_utf8(input.as_bytes()) else {
        return json_decode_failure(context, flags, JSON_ERROR_UTF8);
    };
    match serde_json::from_str::<JsonValue>(input) {
        Ok(json) => {
            if json_depth(&json) > depth as usize {
                return json_decode_failure(context, flags, JSON_ERROR_DEPTH);
            }
            context.set_json_last_error(JSON_ERROR_NONE);
            Ok(json_to_php_value_with_flags(
                json,
                associative || flags & JSON_OBJECT_AS_ARRAY != 0,
                flags,
            ))
        }
        Err(error) => {
            json_decode_failure(context, flags, classify_json_decode_error(input, &error))
        }
    }
}

fn json_decode_failure(context: &mut BuiltinContext<'_>, flags: i64, code: i64) -> BuiltinResult {
    context.set_json_last_error(code);
    if flags & JSON_THROW_ON_ERROR != 0 {
        Err(BuiltinError::new(
            "E_PHP_RUNTIME_JSON_EXCEPTION",
            json_error_message(code),
        ))
    } else {
        Ok(Value::Null)
    }
}

fn json_to_php_value_with_flags(value: JsonValue, associative: bool, flags: i64) -> Value {
    match value {
        JsonValue::Number(number)
            if flags & JSON_BIGINT_AS_STRING != 0
                && number.as_i64().is_none()
                && number.as_u64().is_none() =>
        {
            let text = number.to_string();
            if text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'-')
            {
                Value::string(text)
            } else {
                json_to_php_value(JsonValue::Number(number), associative)
            }
        }
        JsonValue::Array(values) => Value::packed_array(
            values
                .into_iter()
                .map(|value| json_to_php_value_with_flags(value, associative, flags))
                .collect(),
        ),
        JsonValue::Object(values) if associative => {
            let mut array = crate::PhpArray::new();
            for (key, value) in values {
                array.insert(
                    crate::ArrayKey::String(crate::PhpString::from_test_str(&key)),
                    json_to_php_value_with_flags(value, associative, flags),
                );
            }
            Value::Array(array)
        }
        JsonValue::Object(values) => {
            let object = crate::ObjectRef::new_with_display_name(&json_std_class(), "stdClass");
            for (key, value) in values {
                object.set_property(key, json_to_php_value_with_flags(value, associative, flags));
            }
            Value::Object(object)
        }
        value => json_to_php_value(value, associative),
    }
}

fn json_depth(value: &JsonValue) -> usize {
    match value {
        JsonValue::Array(values) => 1 + values.iter().map(json_depth).max().unwrap_or(0),
        JsonValue::Object(values) => 1 + values.values().map(json_depth).max().unwrap_or(0),
        _ => 1,
    }
}

fn classify_json_decode_error(input: &str, error: &serde_json::Error) -> i64 {
    if input
        .bytes()
        .any(|byte| byte < 0x20 && !matches!(byte, b'\t' | b'\n' | b'\r' | b' '))
    {
        return JSON_ERROR_CTRL_CHAR;
    }
    if matches!(error.classify(), serde_json::error::Category::Syntax)
        && has_mismatched_json_closer(input)
    {
        return JSON_ERROR_STATE_MISMATCH;
    }
    JSON_ERROR_SYNTAX
}

fn has_mismatched_json_closer(input: &str) -> bool {
    let mut stack = Vec::new();
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    while let Some(ch) = chars.next() {
        if in_string {
            match ch {
                '\\' => {
                    let _ = chars.next();
                }
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '[' => stack.push(']'),
            '{' => stack.push('}'),
            ']' | '}' if stack.pop() != Some(ch) => return true,
            _ => {}
        }
    }
    false
}

pub(in crate::builtins::modules) fn builtin_json_validate(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error("json_validate", "one to three argument(s)"));
    }
    let input = string_arg("json_validate", &args[0])?;
    let depth = args
        .get(1)
        .map(|value| int_arg("json_validate", value))
        .transpose()?
        .unwrap_or(512);
    let flags = args
        .get(2)
        .map(|value| int_arg("json_validate", value))
        .transpose()?
        .unwrap_or(0);
    if depth <= 0 {
        context.set_json_last_error(JSON_ERROR_DEPTH);
        return Ok(Value::Bool(false));
    }
    let Ok(input) = std::str::from_utf8(input.as_bytes()) else {
        context.set_json_last_error(JSON_ERROR_UTF8);
        return Ok(Value::Bool(false));
    };
    match serde_json::from_str::<JsonValue>(input) {
        Ok(_) => {
            context.set_json_last_error(JSON_ERROR_NONE);
            Ok(Value::Bool(true))
        }
        Err(_) if flags & JSON_THROW_ON_ERROR != 0 => Err(BuiltinError::new(
            "E_PHP_RUNTIME_JSON_EXCEPTION",
            json_error_message(JSON_ERROR_SYNTAX),
        )),
        Err(_) => {
            context.set_json_last_error(JSON_ERROR_SYNTAX);
            Ok(Value::Bool(false))
        }
    }
}
pub(in crate::builtins::modules) fn builtin_json_last_error(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("json_last_error", &args, 0)?;
    Ok(Value::Int(context.json_last_error().0))
}
pub(in crate::builtins::modules) fn builtin_json_last_error_msg(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("json_last_error_msg", &args, 0)?;
    Ok(Value::string(context.json_last_error().1))
}
