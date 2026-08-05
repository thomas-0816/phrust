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
use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};
use serde_json::Value as JsonValue;
use std::fmt;

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

/// Exact structured-output sink implemented by the authoritative native value
/// plane. Composite publication consumes all child owners and must roll them
/// back itself when it cannot publish the container.
#[doc(hidden)]
pub trait NativeStructuredValuePublisher {
    type Output;

    fn publish_null(&mut self) -> Option<Self::Output>;
    fn publish_bool(&mut self, value: bool) -> Option<Self::Output>;
    fn publish_int(&mut self, value: i64) -> Option<Self::Output>;
    fn publish_float(&mut self, value: f64) -> Option<Self::Output>;
    fn publish_string(&mut self, value: &[u8]) -> Option<Self::Output>;
    fn rollback(&mut self, value: Self::Output);
    fn publish_array_stream<E>(
        &mut self,
        build: impl FnOnce(
            &mut Self,
            &mut dyn FnMut(&mut Self, Self::Output) -> Option<()>,
        ) -> Result<(), E>,
    ) -> Result<Option<Self::Output>, E>
    where
        Self: Sized;
    fn publish_object_stream<E>(
        &mut self,
        build: impl FnOnce(
            &mut Self,
            &mut dyn FnMut(&mut Self, &[u8], Self::Output) -> Option<()>,
        ) -> Result<(), E>,
    ) -> Result<Option<Self::Output>, E>
    where
        Self: Sized;
    fn publish_array_with(
        &mut self,
        length: usize,
        build: impl FnMut(&mut Self, usize) -> Option<Self::Output>,
    ) -> Option<Self::Output>
    where
        Self: Sized;
}

struct NativeJsonSeed<'a, P> {
    publisher: &'a mut P,
    depth: usize,
    maximum_depth: usize,
    flags: i64,
    depth_exceeded: &'a mut bool,
    publisher_failed: &'a mut bool,
}

struct NativeJsonVisitor<'a, P> {
    publisher: &'a mut P,
    depth: usize,
    maximum_depth: usize,
    flags: i64,
    depth_exceeded: &'a mut bool,
    publisher_failed: &'a mut bool,
}

struct NativeJsonValidationSeed<'a> {
    depth: usize,
    maximum_depth: usize,
    depth_exceeded: &'a mut bool,
}

struct NativeJsonValidationVisitor<'a> {
    depth: usize,
    maximum_depth: usize,
    depth_exceeded: &'a mut bool,
}

impl<'de> DeserializeSeed<'de> for NativeJsonValidationSeed<'_> {
    type Value = ();

    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(NativeJsonValidationVisitor {
            depth: self.depth,
            maximum_depth: self.maximum_depth,
            depth_exceeded: self.depth_exceeded,
        })
    }
}

impl NativeJsonValidationVisitor<'_> {
    fn check_container_depth<E: serde::de::Error>(&mut self) -> Result<(), E> {
        if self.depth >= self.maximum_depth {
            *self.depth_exceeded = true;
            return Err(E::custom("native JSON maximum depth exceeded"));
        }
        Ok(())
    }
}

impl<'de> Visitor<'de> for NativeJsonValidationVisitor<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_i128<E>(self, _value: i128) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_u128<E>(self, _value: u128) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_borrowed_str<E>(self, _value: &'de str) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_string<E>(self, _value: String) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(mut self, mut sequence: A) -> Result<Self::Value, A::Error> {
        self.check_container_depth()?;
        while sequence
            .next_element_seed(NativeJsonValidationSeed {
                depth: self.depth + 1,
                maximum_depth: self.maximum_depth,
                depth_exceeded: self.depth_exceeded,
            })?
            .is_some()
        {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(mut self, mut map: A) -> Result<Self::Value, A::Error> {
        let first_key = map.next_key::<String>()?;
        if first_key.as_deref() == Some("$serde_json::private::Number") {
            let _ = map.next_value::<String>()?;
            return Ok(());
        }
        self.check_container_depth()?;
        if first_key.is_some() {
            map.next_value_seed(NativeJsonValidationSeed {
                depth: self.depth + 1,
                maximum_depth: self.maximum_depth,
                depth_exceeded: self.depth_exceeded,
            })?;
        }
        while map.next_key::<serde::de::IgnoredAny>()?.is_some() {
            map.next_value_seed(NativeJsonValidationSeed {
                depth: self.depth + 1,
                maximum_depth: self.maximum_depth,
                depth_exceeded: self.depth_exceeded,
            })?;
        }
        Ok(())
    }
}

impl<'de, P: NativeStructuredValuePublisher> DeserializeSeed<'de> for NativeJsonSeed<'_, P> {
    type Value = P::Output;

    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(NativeJsonVisitor {
            publisher: self.publisher,
            depth: self.depth,
            maximum_depth: self.maximum_depth,
            flags: self.flags,
            depth_exceeded: self.depth_exceeded,
            publisher_failed: self.publisher_failed,
        })
    }
}

impl<P: NativeStructuredValuePublisher> NativeJsonVisitor<'_, P> {
    fn published<E: serde::de::Error>(&mut self, value: Option<P::Output>) -> Result<P::Output, E> {
        match value {
            Some(value) => Ok(value),
            None => {
                *self.publisher_failed = true;
                Err(E::custom("native JSON publication failed"))
            }
        }
    }

    fn check_container_depth<E: serde::de::Error>(&mut self) -> Result<(), E> {
        if self.depth >= self.maximum_depth {
            *self.depth_exceeded = true;
            return Err(E::custom("native JSON maximum depth exceeded"));
        }
        Ok(())
    }
}

impl<'de, P: NativeStructuredValuePublisher> Visitor<'de> for NativeJsonVisitor<'_, P> {
    type Value = P::Output;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_unit<E: serde::de::Error>(mut self) -> Result<Self::Value, E> {
        let value = self.publisher.publish_null();
        self.published(value)
    }

    fn visit_bool<E: serde::de::Error>(mut self, value: bool) -> Result<Self::Value, E> {
        let value = self.publisher.publish_bool(value);
        self.published(value)
    }

    fn visit_i64<E: serde::de::Error>(mut self, value: i64) -> Result<Self::Value, E> {
        let value = self.publisher.publish_int(value);
        self.published(value)
    }

    fn visit_u64<E: serde::de::Error>(mut self, value: u64) -> Result<Self::Value, E> {
        let published = match i64::try_from(value) {
            Ok(value) => self.publisher.publish_int(value),
            Err(_) if self.flags & JSON_BIGINT_AS_STRING != 0 => {
                self.publisher.publish_string(value.to_string().as_bytes())
            }
            Err(_) => self.publisher.publish_float(value as f64),
        };
        self.published(published)
    }

    fn visit_i128<E: serde::de::Error>(mut self, value: i128) -> Result<Self::Value, E> {
        let published = match i64::try_from(value) {
            Ok(value) => self.publisher.publish_int(value),
            Err(_) if self.flags & JSON_BIGINT_AS_STRING != 0 => {
                self.publisher.publish_string(value.to_string().as_bytes())
            }
            Err(_) => self.publisher.publish_float(value as f64),
        };
        self.published(published)
    }

    fn visit_u128<E: serde::de::Error>(mut self, value: u128) -> Result<Self::Value, E> {
        let published = match i64::try_from(value) {
            Ok(value) => self.publisher.publish_int(value),
            Err(_) if self.flags & JSON_BIGINT_AS_STRING != 0 => {
                self.publisher.publish_string(value.to_string().as_bytes())
            }
            Err(_) => self.publisher.publish_float(value as f64),
        };
        self.published(published)
    }

    fn visit_f64<E: serde::de::Error>(mut self, value: f64) -> Result<Self::Value, E> {
        let value = self.publisher.publish_float(value);
        self.published(value)
    }

    fn visit_str<E: serde::de::Error>(mut self, value: &str) -> Result<Self::Value, E> {
        let value = self.publisher.publish_string(value.as_bytes());
        self.published(value)
    }

    fn visit_borrowed_str<E: serde::de::Error>(
        mut self,
        value: &'de str,
    ) -> Result<Self::Value, E> {
        let value = self.publisher.publish_string(value.as_bytes());
        self.published(value)
    }

    fn visit_string<E: serde::de::Error>(mut self, value: String) -> Result<Self::Value, E> {
        let value = self.publisher.publish_string(value.as_bytes());
        self.published(value)
    }

    fn visit_seq<A: SeqAccess<'de>>(mut self, mut sequence: A) -> Result<Self::Value, A::Error> {
        self.check_container_depth()?;
        let depth = self.depth;
        let maximum_depth = self.maximum_depth;
        let depth_exceeded = self.depth_exceeded;
        let publisher_failed = self.publisher_failed;
        let published = self.publisher.publish_array_stream(|publisher, push| {
            loop {
                let Some(value) = sequence.next_element_seed(NativeJsonSeed {
                    publisher,
                    depth: depth + 1,
                    maximum_depth,
                    flags: self.flags,
                    depth_exceeded,
                    publisher_failed,
                })?
                else {
                    break;
                };
                if push(publisher, value).is_none() {
                    *publisher_failed = true;
                    return Err(A::Error::custom("native JSON array insertion failed"));
                }
            }
            Ok(())
        })?;
        match published {
            Some(value) => Ok(value),
            None => {
                *publisher_failed = true;
                Err(A::Error::custom("native JSON array publication failed"))
            }
        }
    }

    fn visit_map<A: MapAccess<'de>>(mut self, mut map: A) -> Result<Self::Value, A::Error> {
        let first_key = map.next_key::<String>()?;
        if first_key.as_deref() == Some("$serde_json::private::Number") {
            let number = map.next_value::<String>()?;
            if self.flags & JSON_BIGINT_AS_STRING != 0
                && number
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b'-')
            {
                let published = self.publisher.publish_string(number.as_bytes());
                return self.published(published);
            }
            let value = number
                .parse::<f64>()
                .map_err(|_| A::Error::custom("invalid arbitrary-precision JSON number"))?;
            let published = self.publisher.publish_float(value);
            return self.published(published);
        }
        self.check_container_depth()?;
        let depth = self.depth;
        let maximum_depth = self.maximum_depth;
        let depth_exceeded = self.depth_exceeded;
        let publisher_failed = self.publisher_failed;
        let published = self.publisher.publish_object_stream(|publisher, push| {
            if let Some(key) = first_key {
                let value = map.next_value_seed(NativeJsonSeed {
                    publisher,
                    depth: depth + 1,
                    maximum_depth,
                    flags: self.flags,
                    depth_exceeded,
                    publisher_failed,
                })?;
                if push(publisher, key.as_bytes(), value).is_none() {
                    *publisher_failed = true;
                    return Err(A::Error::custom("native JSON object insertion failed"));
                }
            }
            loop {
                let Some(key) = map.next_key::<String>()? else {
                    break;
                };
                let value = map.next_value_seed(NativeJsonSeed {
                    publisher,
                    depth: depth + 1,
                    maximum_depth,
                    flags: self.flags,
                    depth_exceeded,
                    publisher_failed,
                })?;
                if push(publisher, key.as_bytes(), value).is_none() {
                    *publisher_failed = true;
                    return Err(A::Error::custom("native JSON object insertion failed"));
                }
            }
            Ok(())
        })?;
        match published {
            Some(value) => Ok(value),
            None => {
                *publisher_failed = true;
                Err(A::Error::custom("native JSON object publication failed"))
            }
        }
    }
}

/// Parses the exact `json_decode($bytes, true, $depth, 0)` capability without
/// constructing `PhpArray`, `ObjectRef`, PHP `Value`, or a second decoded
/// value tree. Parsed children move directly into the supplied native sink.
#[doc(hidden)]
pub fn decode_native_json_associative_into<P: NativeStructuredValuePublisher>(
    state: &mut crate::builtins::JsonRequestState,
    input: &[u8],
    depth: i64,
    publisher: &mut P,
) -> Result<Option<P::Output>, BuiltinError> {
    decode_native_json_into(state, input, depth, 0, publisher)
}

/// Parses JSON directly into the supplied native value publisher while
/// honoring decode flags that do not require a second PHP value graph.
#[doc(hidden)]
pub fn decode_native_json_into<P: NativeStructuredValuePublisher>(
    state: &mut crate::builtins::JsonRequestState,
    input: &[u8],
    depth: i64,
    flags: i64,
    publisher: &mut P,
) -> Result<Option<P::Output>, BuiltinError> {
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
    let input = match json_decode_input(input, flags) {
        Ok(input) => input,
        Err(code) => {
            state.set(code);
            return Ok(publisher.publish_null());
        }
    };
    let mut depth_exceeded = false;
    let mut publisher_failed = false;
    let mut deserializer = serde_json::Deserializer::from_str(&input);
    let parsed = NativeJsonSeed {
        publisher,
        depth: 0,
        maximum_depth: depth as usize,
        flags,
        depth_exceeded: &mut depth_exceeded,
        publisher_failed: &mut publisher_failed,
    }
    .deserialize(&mut deserializer);
    match parsed {
        Ok(value) => match deserializer.end() {
            Ok(()) => {
                state.set(JSON_ERROR_NONE);
                Ok(Some(value))
            }
            Err(error) => {
                publisher.rollback(value);
                state.set(classify_json_decode_error(&input, &error));
                Ok(publisher.publish_null())
            }
        },
        Err(_) if publisher_failed => Ok(None),
        Err(_) if depth_exceeded => {
            state.set(JSON_ERROR_DEPTH);
            Ok(publisher.publish_null())
        }
        Err(error) => {
            state.set(classify_json_decode_error(&input, &error));
            Ok(publisher.publish_null())
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
    let mut depth_exceeded = false;
    let mut deserializer = serde_json::Deserializer::from_str(&input);
    let parsed = NativeJsonValidationSeed {
        depth: 0,
        maximum_depth: depth as usize,
        depth_exceeded: &mut depth_exceeded,
    }
    .deserialize(&mut deserializer)
    .and_then(|()| deserializer.end());
    match parsed {
        Ok(()) => {
            state.set(JSON_ERROR_NONE);
            Ok(true)
        }
        Err(_) if depth_exceeded => {
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
