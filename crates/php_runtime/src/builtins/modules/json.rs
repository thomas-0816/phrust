//! Json builtin registry slice.

use super::super::context::{
    JSON_ERROR_DEPTH, JSON_ERROR_NONE, JSON_ERROR_SYNTAX, JSON_ERROR_UTF8, JSON_OBJECT_AS_ARRAY,
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
    match php_value_to_json(&args[0], flags) {
        Ok(json) => {
            let encoded = if flags & JSON_PRETTY_PRINT != 0 {
                serde_json::to_string_pretty(&json)
            } else {
                serde_json::to_string(&json)
            };
            match encoded {
                Ok(encoded) => {
                    context.set_json_last_error(JSON_ERROR_NONE);
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
            context.set_json_last_error(JSON_ERROR_NONE);
            Ok(json_to_php_value(
                json,
                associative || flags & JSON_OBJECT_AS_ARRAY != 0,
            ))
        }
        Err(_) => json_decode_failure(context, flags, JSON_ERROR_SYNTAX),
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
