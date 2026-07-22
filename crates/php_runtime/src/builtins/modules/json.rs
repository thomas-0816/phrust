//! Json builtin registry slice.

use super::super::context::{
    JSON_BIGINT_AS_STRING, JSON_ERROR_CTRL_CHAR, JSON_ERROR_DEPTH,
    JSON_ERROR_INVALID_PROPERTY_NAME, JSON_ERROR_NONE, JSON_ERROR_STATE_MISMATCH,
    JSON_ERROR_SYNTAX, JSON_ERROR_UTF8, JSON_ERROR_UTF16, JSON_INVALID_UTF8_IGNORE,
    JSON_INVALID_UTF8_SUBSTITUTE, JSON_OBJECT_AS_ARRAY, JSON_PRETTY_PRINT, JSON_THROW_ON_ERROR,
    JsonBuiltinServices, json_error_message,
};
use super::core::*;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{ArrayKey, PhpString, Value, to_bool};
use serde_json::Value as JsonValue;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("json_decode", exact_json_decode, BuiltinCompatibility::Php),
    BuiltinEntry::new("json_encode", exact_json_encode, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "json_last_error",
        exact_json_last_error,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "json_last_error_msg",
        exact_json_last_error_msg,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "json_validate",
        exact_json_validate,
        BuiltinCompatibility::Php,
    ),
];

macro_rules! exact_json_builtin {
    ($entry:ident => $implementation:ident) => {
        #[doc(hidden)]
        pub fn $entry(
            context: &mut BuiltinContext<'_>,
            args: Vec<Value>,
            span: RuntimeSourceSpan,
        ) -> BuiltinResult {
            let mut services = context.json_services();
            $implementation(&mut services, args, span)
        }
    };
}

exact_json_builtin!(exact_json_encode => json_encode);
exact_json_builtin!(exact_json_decode => json_decode);
exact_json_builtin!(exact_json_validate => json_validate);
exact_json_builtin!(exact_json_last_error => json_last_error);
exact_json_builtin!(exact_json_last_error_msg => json_last_error_msg);

fn json_encode(
    context: &mut JsonBuiltinServices<'_>,
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
    let depth = args
        .get(2)
        .map(|value| int_arg("json_encode", value))
        .transpose()?
        .unwrap_or(512);
    if depth < 0 {
        return Err(argument_value_error(
            "json_encode",
            "#3 ($depth)",
            "must be greater than or equal to 0",
        ));
    }
    if depth > i32::MAX as i64 {
        return Err(argument_value_error(
            "json_encode",
            "#3 ($depth)",
            &format!("must be less than {}", i32::MAX),
        ));
    }
    match php_value_to_json_checked(&args[0], flags, depth as usize) {
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
fn json_decode(
    context: &mut JsonBuiltinServices<'_>,
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
        return Err(argument_value_error(
            "json_decode",
            "#3 ($depth)",
            "must be greater than 0",
        ));
    }
    if depth > i32::MAX as i64 {
        return Err(argument_value_error(
            "json_decode",
            "#3 ($depth)",
            &format!("must be less than {}", i32::MAX),
        ));
    }
    let input = match json_decode_input(input.as_bytes(), flags) {
        Ok(input) => input,
        Err(code) => return json_decode_failure(context, flags, code),
    };
    match serde_json::from_str::<JsonValue>(&input) {
        Ok(json) => {
            if json_depth(&json) > depth as usize {
                return json_decode_failure(context, flags, JSON_ERROR_DEPTH);
            }
            if !associative
                && flags & JSON_OBJECT_AS_ARRAY == 0
                && json_has_invalid_property_name(&json)
            {
                return json_decode_failure(context, flags, JSON_ERROR_INVALID_PROPERTY_NAME);
            }
            context.set_json_last_error(JSON_ERROR_NONE);
            Ok(json_to_php_value_with_flags(
                normalize_decoded_json_strings(json, flags),
                associative || flags & JSON_OBJECT_AS_ARRAY != 0,
                flags,
            ))
        }
        Err(error) => {
            json_decode_failure(context, flags, classify_json_decode_error(&input, &error))
        }
    }
}

fn json_decode_failure(
    context: &mut JsonBuiltinServices<'_>,
    flags: i64,
    code: i64,
) -> BuiltinResult {
    if flags & JSON_THROW_ON_ERROR != 0 {
        Err(
            BuiltinError::new("E_PHP_RUNTIME_JSON_EXCEPTION", json_error_message(code))
                .with_json_error_code(code),
        )
    } else {
        context.set_json_last_error(code);
        Ok(Value::Null)
    }
}

/// Temporary typed parse tree returned by the exact associative JSON decoder.
/// It contains JSON data only and is consumed immediately into authoritative
/// native slots; it is not a second PHP value representation.
#[derive(Debug)]
pub enum NativeJsonDecodedValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Vec<u8>),
    Array(Vec<Self>),
    Object(Vec<(Vec<u8>, Self)>),
}

fn native_json_decoded_value(value: JsonValue) -> NativeJsonDecodedValue {
    match value {
        JsonValue::Null => NativeJsonDecodedValue::Null,
        JsonValue::Bool(value) => NativeJsonDecodedValue::Bool(value),
        JsonValue::Number(value) => value.as_i64().map_or_else(
            || {
                NativeJsonDecodedValue::Float(
                    value
                        .as_f64()
                        .or_else(|| value.to_string().parse().ok())
                        .unwrap_or(0.0),
                )
            },
            NativeJsonDecodedValue::Int,
        ),
        JsonValue::String(value) => NativeJsonDecodedValue::String(value.into_bytes()),
        JsonValue::Array(values) => NativeJsonDecodedValue::Array(
            values.into_iter().map(native_json_decoded_value).collect(),
        ),
        JsonValue::Object(values) => NativeJsonDecodedValue::Object(
            values
                .into_iter()
                .map(|(key, value)| (key.into_bytes(), native_json_decoded_value(value)))
                .collect(),
        ),
    }
}

/// Parses the exact `json_decode($bytes, true, $depth, 0)` capability without
/// constructing `PhpArray`, `ObjectRef`, or PHP `Value` instances.
#[doc(hidden)]
pub fn decode_native_json_associative(
    state: &mut crate::builtins::JsonRequestState,
    input: &[u8],
    depth: i64,
) -> Result<NativeJsonDecodedValue, BuiltinError> {
    if depth <= 0 {
        return Err(argument_value_error(
            "json_decode",
            "#3 ($depth)",
            "must be greater than 0",
        ));
    }
    if depth > i32::MAX as i64 {
        return Err(argument_value_error(
            "json_decode",
            "#3 ($depth)",
            &format!("must be less than {}", i32::MAX),
        ));
    }
    let input = match json_decode_input(input, 0) {
        Ok(input) => input,
        Err(code) => {
            state.set(code);
            return Ok(NativeJsonDecodedValue::Null);
        }
    };
    match serde_json::from_str::<JsonValue>(&input) {
        Ok(json) if json_depth(&json) <= depth as usize => {
            state.set(JSON_ERROR_NONE);
            Ok(native_json_decoded_value(json))
        }
        Ok(_) => {
            state.set(JSON_ERROR_DEPTH);
            Ok(NativeJsonDecodedValue::Null)
        }
        Err(error) => {
            state.set(classify_json_decode_error(&input, &error));
            Ok(NativeJsonDecodedValue::Null)
        }
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
                    ArrayKey::from_php_string(PhpString::from_test_str(&key)),
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
    if has_unpaired_utf16_escape(input) {
        return JSON_ERROR_UTF16;
    }
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

fn json_decode_input(bytes: &[u8], flags: i64) -> Result<String, i64> {
    match std::str::from_utf8(bytes) {
        Ok(input) => Ok(input.to_string()),
        Err(_) if flags & JSON_INVALID_UTF8_IGNORE != 0 => Ok(utf8_ignore_invalid(bytes)),
        Err(_) if flags & JSON_INVALID_UTF8_SUBSTITUTE != 0 => {
            Ok(String::from_utf8_lossy(bytes).into_owned())
        }
        Err(_) => Err(JSON_ERROR_UTF8),
    }
}

fn normalize_decoded_json_strings(value: JsonValue, flags: i64) -> JsonValue {
    match value {
        JsonValue::String(value) if flags & JSON_INVALID_UTF8_IGNORE != 0 => {
            JsonValue::String(value)
        }
        JsonValue::Array(values) => JsonValue::Array(
            values
                .into_iter()
                .map(|value| normalize_decoded_json_strings(value, flags))
                .collect(),
        ),
        JsonValue::Object(values) => JsonValue::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, normalize_decoded_json_strings(value, flags)))
                .collect(),
        ),
        value => value,
    }
}

fn json_has_invalid_property_name(value: &JsonValue) -> bool {
    match value {
        JsonValue::Object(values) => values
            .iter()
            .any(|(key, value)| key.contains('\0') || json_has_invalid_property_name(value)),
        JsonValue::Array(values) => values.iter().any(json_has_invalid_property_name),
        _ => false,
    }
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

fn has_unpaired_utf16_escape(input: &str) -> bool {
    let bytes = input.as_bytes();
    let mut index = 0;
    while index + 6 <= bytes.len() {
        if bytes[index] == b'\\' && bytes[index + 1] == b'u' {
            if let Some(code) = parse_json_hex4(&bytes[index + 2..index + 6]) {
                if (0xD800..=0xDBFF).contains(&code) {
                    let paired = index + 12 <= bytes.len()
                        && bytes[index + 6] == b'\\'
                        && bytes[index + 7] == b'u'
                        && parse_json_hex4(&bytes[index + 8..index + 12])
                            .is_some_and(|low| (0xDC00..=0xDFFF).contains(&low));
                    if !paired {
                        return true;
                    }
                    index += 12;
                    continue;
                }
                if (0xDC00..=0xDFFF).contains(&code) {
                    return true;
                }
            }
            index += 6;
        } else {
            index += 1;
        }
    }
    false
}

fn parse_json_hex4(bytes: &[u8]) -> Option<u16> {
    if bytes.len() != 4 {
        return None;
    }
    let mut value = 0u16;
    for byte in bytes {
        value = value.checked_mul(16)?;
        value = value.checked_add(match byte {
            b'0'..=b'9' => (byte - b'0') as u16,
            b'a'..=b'f' => (byte - b'a' + 10) as u16,
            b'A'..=b'F' => (byte - b'A' + 10) as u16,
            _ => return None,
        })?;
    }
    Some(value)
}

fn json_validate(
    context: &mut JsonBuiltinServices<'_>,
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
    validate_native_json(context.request_state(), input.as_bytes(), depth, flags).map(Value::Bool)
}

/// Exact JSON validation over native string bytes and the dedicated
/// request-local JSON capability. No PHP `Value` or generic builtin context
/// crosses this boundary.
#[doc(hidden)]
pub fn validate_native_json(
    state: &mut crate::builtins::JsonRequestState,
    input: &[u8],
    depth: i64,
    flags: i64,
) -> Result<bool, BuiltinError> {
    if input.is_empty() {
        state.set(JSON_ERROR_SYNTAX);
        return Ok(false);
    }
    if depth <= 0 {
        return Err(argument_value_error(
            "json_validate",
            "#2 ($depth)",
            "must be greater than 0",
        ));
    }
    if depth > i32::MAX as i64 {
        return Err(argument_value_error(
            "json_validate",
            "#2 ($depth)",
            &format!("must be less than {}", i32::MAX),
        ));
    }
    if flags & !JSON_INVALID_UTF8_IGNORE != 0 {
        return Err(argument_value_error(
            "json_validate",
            "#3 ($flags)",
            "must be a valid flag (allowed flags: JSON_INVALID_UTF8_IGNORE)",
        ));
    }
    let input = match std::str::from_utf8(input) {
        Ok(input) => input.to_string(),
        Err(_) if flags & JSON_INVALID_UTF8_IGNORE != 0 => utf8_ignore_invalid(input),
        Err(_) => {
            state.set(JSON_ERROR_UTF8);
            return Ok(false);
        }
    };
    match serde_json::from_str::<JsonValue>(&input) {
        Ok(json) if json_depth(&json) <= depth as usize => {
            state.set(JSON_ERROR_NONE);
            Ok(true)
        }
        Ok(_) => {
            state.set(JSON_ERROR_DEPTH);
            Ok(false)
        }
        Err(_) if flags & JSON_THROW_ON_ERROR != 0 => Err(BuiltinError::new(
            "E_PHP_RUNTIME_JSON_EXCEPTION",
            json_error_message(JSON_ERROR_SYNTAX),
        )),
        Err(_) => {
            state.set(JSON_ERROR_SYNTAX);
            Ok(false)
        }
    }
}
#[inline(always)]
fn json_last_error(
    context: &mut JsonBuiltinServices<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("json_last_error", &args, 0)?;
    Ok(Value::Int(context.json_last_error().0))
}
#[inline(always)]
fn json_last_error_msg(
    context: &mut JsonBuiltinServices<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("json_last_error_msg", &args, 0)?;
    Ok(Value::string(context.json_last_error().1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OutputBuffer, builtins::BuiltinContext};

    fn call(name: &str, args: Vec<Value>) -> Value {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        ENTRIES
            .iter()
            .find(|entry| entry.name() == name)
            .expect("entry")
            .function()(&mut context, args, RuntimeSourceSpan::default())
        .expect("builtin succeeds")
    }

    #[test]
    fn json_decode_associative_normalizes_numeric_object_keys() {
        let decoded = call(
            "json_decode",
            vec![
                Value::string(r#"{"123":{"456":{"abc":{"789":"def","012":"keep"}}}}"#),
                Value::Bool(true),
            ],
        );

        let Value::Array(root) = decoded else {
            panic!("expected array");
        };
        let Some(Value::Array(nested)) = root.get(&ArrayKey::Int(123)) else {
            panic!("expected integer key 123");
        };
        let Some(Value::Array(inner)) = nested.get(&ArrayKey::Int(456)) else {
            panic!("expected integer key 456");
        };
        let Some(Value::Array(values)) =
            inner.get(&ArrayKey::String(PhpString::from_test_str("abc")))
        else {
            panic!("expected string key abc");
        };
        assert_eq!(values.get(&ArrayKey::Int(789)), Some(&Value::string("def")));
        assert_eq!(
            values.get(&ArrayKey::String(PhpString::from_test_str("012"))),
            Some(&Value::string("keep"))
        );
    }
}
