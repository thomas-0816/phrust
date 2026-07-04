//! Default-flags `json_encode` fast path for scalar/array shapes.
//!
//! Encodes packed and record/string-key arrays, ints, bools, null, and
//! UTF-8 strings directly into one output buffer, matching the generic
//! serde-tree pipeline byte for byte (including the PHP-default `\/` and
//! `\uXXXX` non-ASCII escapes applied by `normalize_json_encoded`).
//! Anything outside those shapes returns the fallback reason so the caller
//! can take the generic path, which owns floats, objects, references,
//! non-default flags, and error/diagnostic behavior.

use std::fmt::Write as _;

use super::super::context::JSON_ERROR_NONE;
use crate::{ArrayKey, PhpArray, Value};

/// `json_last_error` value to install after a successful fast-path encode,
/// mirroring the generic builtin's success path.
pub const JSON_ENCODE_NO_ERROR: i64 = JSON_ERROR_NONE;

/// Nesting bound for the recursive fast path; deeper values take the
/// generic path so the fast encoder never risks exhausting the stack.
const MAX_FAST_DEPTH: usize = 128;

/// Encodes `value` with PHP-default `json_encode` flags, or names the
/// fallback reason when the value needs the generic pipeline.
pub fn json_encode_default_flags(value: &Value) -> Result<String, &'static str> {
    let mut output = String::with_capacity(32);
    encode_value(value, &mut output, 0)?;
    Ok(output)
}

fn encode_value(value: &Value, output: &mut String, depth: usize) -> Result<(), &'static str> {
    match value {
        Value::Null | Value::Uninitialized => output.push_str("null"),
        Value::Bool(true) => output.push_str("true"),
        Value::Bool(false) => output.push_str("false"),
        Value::Int(value) => {
            // fmt::Write to String is infallible.
            let _ = write!(output, "{value}");
        }
        Value::Float(_) => return Err("float"),
        Value::String(value) => encode_string(value.as_bytes(), output)?,
        Value::Array(array) => encode_array(array, output, depth)?,
        Value::Object(_) => return Err("object"),
        Value::Reference(_) => return Err("reference"),
        Value::Resource(_) | Value::Fiber(_) | Value::Generator(_) | Value::Callable(_) => {
            return Err("unsupported_value");
        }
    }
    Ok(())
}

fn encode_array(array: &PhpArray, output: &mut String, depth: usize) -> Result<(), &'static str> {
    if depth >= MAX_FAST_DEPTH {
        return Err("depth");
    }
    if let Some(values) = array.packed_values_fast() {
        output.push('[');
        for (index, value) in values.enumerate() {
            if index > 0 {
                output.push(',');
            }
            encode_value(value, output, depth + 1)?;
        }
        output.push(']');
        return Ok(());
    }
    if let Some(elements) = array.packed_elements() {
        output.push('[');
        for (index, value) in elements.into_iter().enumerate() {
            if index > 0 {
                output.push(',');
            }
            encode_value(value, output, depth + 1)?;
        }
        output.push(']');
        return Ok(());
    }
    output.push('{');
    for (index, (key, value)) in array.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        match key {
            ArrayKey::Int(key) => {
                output.push('"');
                // fmt::Write to String is infallible.
                let _ = write!(output, "{key}");
                output.push('"');
            }
            ArrayKey::String(key) => encode_string(key.as_bytes(), output)?,
        }
        output.push(':');
        encode_value(value, output, depth + 1)?;
    }
    output.push('}');
    Ok(())
}

/// Escapes like serde_json plus the PHP-default post passes: `/` becomes
/// `\/` and every non-ASCII scalar becomes lowercase `\uXXXX` (surrogate
/// pairs above the BMP). Invalid UTF-8 defers to the generic path.
fn encode_string(bytes: &[u8], output: &mut String) -> Result<(), &'static str> {
    let text = std::str::from_utf8(bytes).map_err(|_| "invalid_utf8")?;
    output.reserve(text.len() + 2);
    output.push('"');
    let mut run_start = 0;
    for (index, ch) in text.char_indices() {
        let escape: Option<&str> = match ch {
            '"' => Some("\\\""),
            '\\' => Some("\\\\"),
            '/' => Some("\\/"),
            '\u{8}' => Some("\\b"),
            '\t' => Some("\\t"),
            '\n' => Some("\\n"),
            '\u{c}' => Some("\\f"),
            '\r' => Some("\\r"),
            ch if (ch as u32) < 0x20 || !ch.is_ascii() => None,
            _ => continue,
        };
        output.push_str(&text[run_start..index]);
        run_start = index + ch.len_utf8();
        if let Some(escape) = escape {
            output.push_str(escape);
            continue;
        }
        let code = ch as u32;
        if code <= 0xFFFF {
            let _ = write!(output, "\\u{code:04x}");
        } else {
            let code = code - 0x1_0000;
            let high = 0xD800 + ((code >> 10) & 0x3FF);
            let low = 0xDC00 + (code & 0x3FF);
            let _ = write!(output, "\\u{high:04x}\\u{low:04x}");
        }
    }
    output.push_str(&text[run_start..]);
    output.push('"');
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::core::{normalize_json_encoded, php_value_to_json_checked};
    use super::*;
    use crate::PhpString;

    fn generic_encode(value: &Value) -> String {
        let (json, error) =
            php_value_to_json_checked(value, 0, 512).expect("generic encode succeeds");
        assert_eq!(error, None);
        normalize_json_encoded(
            serde_json::to_string(&json).expect("serde encode succeeds"),
            0,
        )
    }

    fn assert_parity(value: &Value) {
        let fast = json_encode_default_flags(value).expect("fast path handles value");
        assert_eq!(fast, generic_encode(value));
    }

    #[test]
    fn scalars_match_generic_pipeline() {
        assert_parity(&Value::Null);
        assert_parity(&Value::Bool(true));
        assert_parity(&Value::Bool(false));
        assert_parity(&Value::Int(0));
        assert_parity(&Value::Int(i64::MIN));
        assert_parity(&Value::Int(i64::MAX));
        assert_parity(&Value::string("plain"));
        assert_parity(&Value::string(""));
    }

    #[test]
    fn every_ascii_char_escapes_like_generic_pipeline() {
        for byte in 0_u8..=0x7F {
            let value = Value::string(vec![b'a', byte, b'z']);
            assert_parity(&value);
        }
    }

    #[test]
    fn non_ascii_strings_escape_like_generic_pipeline() {
        for text in [
            "uml \u{e4}\u{f6}\u{fc}",
            "euro \u{20ac}",
            "astral \u{1F600} pair",
            "mix / \\ \" \u{7f} \u{80} \u{ffff}",
        ] {
            assert_parity(&Value::string(text));
        }
    }

    #[test]
    fn packed_record_and_mixed_arrays_match_generic_pipeline() {
        let packed = Value::packed_array(vec![
            Value::Int(1),
            Value::string("two"),
            Value::Null,
            Value::Bool(false),
        ]);
        assert_parity(&packed);

        let mut record = PhpArray::new();
        record.insert(
            ArrayKey::String(PhpString::from_test_str("id")),
            Value::Int(7),
        );
        record.insert(
            ArrayKey::String(PhpString::from_test_str("name")),
            Value::string("Ada / \"L\""),
        );
        record.insert(
            ArrayKey::String(PhpString::from_test_str("tags")),
            packed.clone(),
        );
        assert_parity(&Value::Array(record));

        let mut mixed = PhpArray::new();
        mixed.insert(ArrayKey::Int(5), Value::string("five"));
        mixed.insert(
            ArrayKey::String(PhpString::from_test_str("k")),
            Value::Int(-2),
        );
        assert_parity(&Value::Array(mixed));

        assert_parity(&Value::Array(PhpArray::new()));
    }

    #[test]
    fn unsupported_shapes_name_fallback_reasons() {
        assert_eq!(json_encode_default_flags(&Value::float(1.5)), Err("float"));
        assert_eq!(
            json_encode_default_flags(&Value::string(vec![0xFF, 0xFE])),
            Err("invalid_utf8")
        );
        let nested = Value::packed_array(vec![Value::float(0.5)]);
        assert_eq!(json_encode_default_flags(&nested), Err("float"));

        let mut deep = Value::packed_array(vec![Value::Int(1)]);
        for _ in 0..MAX_FAST_DEPTH {
            deep = Value::packed_array(vec![deep]);
        }
        assert_eq!(json_encode_default_flags(&deep), Err("depth"));
    }
}
