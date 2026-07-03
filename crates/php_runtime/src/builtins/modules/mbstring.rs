//! Bounded UTF-8 mbstring builtins.

use crate::Value;
use crate::builtins::modules::core::{
    argument_type_error, argument_value_error, arity_error, deref_value, int_arg, string_arg,
};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "mb_detect_encoding",
        builtin_mb_detect_encoding,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mb_check_encoding",
        builtin_mb_check_encoding,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mb_convert_encoding",
        builtin_mb_convert_encoding,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mb_internal_encoding",
        builtin_mb_internal_encoding,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("mb_strlen", builtin_mb_strlen, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "mb_strtolower",
        builtin_mb_strtolower,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mb_strtoupper",
        builtin_mb_strtoupper,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("mb_strpos", builtin_mb_strpos, BuiltinCompatibility::Php),
    BuiltinEntry::new("mb_substr", builtin_mb_substr, BuiltinCompatibility::Php),
];

fn builtin_mb_detect_encoding(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error(
            "mb_detect_encoding",
            "one to three argument(s)",
        ));
    }
    let string = string_arg("mb_detect_encoding", &args[0])?;
    let encodings = args
        .get(1)
        .map(|value| encoding_candidates("mb_detect_encoding", value))
        .transpose()?
        .unwrap_or_else(|| vec![context.mb_internal_encoding().to_owned()]);
    let _strict = args.get(2);
    for encoding in encodings {
        let Some(canonical) = canonical_encoding(&encoding) else {
            continue;
        };
        if bytes_match_encoding(string.as_bytes(), canonical) {
            return Ok(Value::string(canonical));
        }
    }
    Ok(Value::Bool(false))
}

fn builtin_mb_check_encoding(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 2 {
        return Err(arity_error("mb_check_encoding", "zero to two argument(s)"));
    }
    let encoding = args
        .get(1)
        .map(|value| encoding_arg("mb_check_encoding", value))
        .transpose()?
        .unwrap_or_else(|| context.mb_internal_encoding().to_owned());
    let Some(canonical) = canonical_encoding(&encoding) else {
        return Ok(Value::Bool(false));
    };
    let Some(value) = args.first() else {
        return Ok(Value::Bool(true));
    };
    Ok(Value::Bool(value_matches_encoding(value, canonical)?))
}

fn builtin_mb_convert_encoding(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error(
            "mb_convert_encoding",
            "two or three argument(s)",
        ));
    }
    let string = string_arg("mb_convert_encoding", &args[0])?;
    let to_encoding = encoding_arg("mb_convert_encoding", &args[1])?;
    let from_encoding = args
        .get(2)
        .map(|value| encoding_arg("mb_convert_encoding", value))
        .transpose()?
        .unwrap_or_else(|| "UTF-8".to_owned());
    let Some(to_canonical) = canonical_encoding(&to_encoding) else {
        return Err(unsupported_encoding_error(
            "mb_convert_encoding",
            &to_encoding,
        ));
    };
    let Some(from_canonical) = canonical_encoding(&from_encoding) else {
        return Err(unsupported_encoding_error(
            "mb_convert_encoding",
            &from_encoding,
        ));
    };
    if to_canonical != "UTF-8" || from_canonical != "UTF-8" {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_UNSUPPORTED_MBSTRING_ENCODING",
            "mb_convert_encoding(): only UTF-8 to UTF-8 conversion is implemented",
        ));
    }
    validate_utf8("mb_convert_encoding", string.as_bytes())?;
    Ok(Value::String(string))
}

fn builtin_mb_internal_encoding(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("mb_internal_encoding", "zero or one argument"));
    }
    let Some(value) = args.first() else {
        return Ok(Value::string(context.mb_internal_encoding().to_owned()));
    };
    let encoding = encoding_arg("mb_internal_encoding", value)?;
    let Some(canonical) = canonical_encoding(&encoding) else {
        return Ok(Value::Bool(false));
    };
    if canonical != "UTF-8" && canonical != "ASCII" {
        return Ok(Value::Bool(false));
    }
    context.set_mb_internal_encoding(canonical);
    Ok(Value::Bool(true))
}

fn builtin_mb_strlen(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("mb_strlen", "one or two argument(s)"));
    }
    let string = string_arg("mb_strlen", &args[0])?;
    let encoding = args
        .get(1)
        .map(|value| encoding_arg("mb_strlen", value))
        .transpose()?
        .unwrap_or_else(|| context.mb_internal_encoding().to_owned());
    match canonical_encoding(&encoding) {
        Some("UTF-8") => Ok(Value::Int(
            validate_utf8("mb_strlen", string.as_bytes())?
                .chars()
                .count() as i64,
        )),
        Some("8BIT") => Ok(Value::Int(string.len() as i64)),
        Some("ASCII") => {
            if string.as_bytes().is_ascii() {
                Ok(Value::Int(string.len() as i64))
            } else {
                Err(argument_value_error(
                    "mb_strlen",
                    "#1 ($string)",
                    "is not valid for encoding ASCII",
                ))
            }
        }
        _ => Err(unsupported_encoding_error("mb_strlen", &encoding)),
    }
}

fn builtin_mb_strtolower(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    convert_case_builtin(context, "mb_strtolower", args, false)
}

fn builtin_mb_strtoupper(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    convert_case_builtin(context, "mb_strtoupper", args, true)
}

fn builtin_mb_substr(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(arity_error("mb_substr", "two to four argument(s)"));
    }
    let string = string_arg("mb_substr", &args[0])?;
    let start = int_arg("mb_substr", &args[1])?;
    let length = match args.get(2).map(deref_value) {
        Some(Value::Null) | None => None,
        Some(value) => Some(int_arg("mb_substr", &value)?),
    };
    let encoding = args
        .get(3)
        .map(|value| encoding_arg("mb_substr", value))
        .transpose()?
        .unwrap_or_else(|| context.mb_internal_encoding().to_owned());
    let Some(canonical) = canonical_encoding(&encoding) else {
        return Err(unsupported_encoding_error("mb_substr", &encoding));
    };
    if canonical != "UTF-8" {
        return Err(unsupported_encoding_error("mb_substr", &encoding));
    }
    let text = validate_utf8("mb_substr", string.as_bytes())?;
    let chars = text.char_indices().collect::<Vec<_>>();
    let total = chars.len();
    let start = normalize_character_offset(total, start);
    if start >= total {
        return Ok(Value::string(Vec::new()));
    }
    let char_len = match length {
        None => total - start,
        Some(length) if length >= 0 => (length as usize).min(total - start),
        Some(length) => (total - start).saturating_sub(length.unsigned_abs() as usize),
    };
    if char_len == 0 {
        return Ok(Value::string(Vec::new()));
    }
    let byte_start = chars[start].0;
    let end_char = start + char_len;
    let byte_end = chars
        .get(end_char)
        .map_or(text.len(), |(offset, _)| *offset);
    Ok(Value::string(
        text.as_bytes()[byte_start..byte_end].to_vec(),
    ))
}

fn builtin_mb_strpos(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(arity_error("mb_strpos", "two to four argument(s)"));
    }
    let haystack = string_arg("mb_strpos", &args[0])?;
    let needle = string_arg("mb_strpos", &args[1])?;
    let offset = args
        .get(2)
        .map(|value| int_arg("mb_strpos", value))
        .transpose()?
        .unwrap_or(0);
    let encoding = args
        .get(3)
        .map(|value| encoding_arg("mb_strpos", value))
        .transpose()?
        .unwrap_or_else(|| context.mb_internal_encoding().to_owned());
    let Some(canonical) = canonical_encoding(&encoding) else {
        return Err(unsupported_encoding_error("mb_strpos", &encoding));
    };
    let haystack_chars = encoded_chars("mb_strpos", haystack.as_bytes(), canonical)?;
    let needle_string = encoded_chars("mb_strpos", needle.as_bytes(), canonical)?
        .into_iter()
        .collect::<String>();
    let start = if offset < 0 {
        haystack_chars
            .len()
            .saturating_sub(offset.unsigned_abs() as usize)
    } else {
        (offset as usize).min(haystack_chars.len())
    };
    let tail = haystack_chars[start..].iter().collect::<String>();
    Ok(tail
        .find(&needle_string)
        .map_or(Value::Bool(false), |byte_offset| {
            Value::Int((start + tail[..byte_offset].chars().count()) as i64)
        }))
}

fn encoded_chars(function: &str, bytes: &[u8], encoding: &str) -> Result<Vec<char>, BuiltinError> {
    match encoding {
        "UTF-8" => Ok(validate_utf8(function, bytes)?.chars().collect()),
        "ASCII" if bytes.is_ascii() => Ok(bytes.iter().map(|byte| char::from(*byte)).collect()),
        "ASCII" => Err(argument_value_error(
            function,
            "#1 ($string)",
            "is not valid for encoding ASCII",
        )),
        _ => Err(unsupported_encoding_error(function, encoding)),
    }
}

fn convert_case_builtin(
    context: &mut BuiltinContext<'_>,
    name: &str,
    args: Vec<Value>,
    uppercase: bool,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error(name, "one or two argument(s)"));
    }
    let string = string_arg(name, &args[0])?;
    let encoding = args
        .get(1)
        .map(|value| encoding_arg(name, value))
        .transpose()?
        .unwrap_or_else(|| context.mb_internal_encoding().to_owned());
    let Some(canonical) = canonical_encoding(&encoding) else {
        return Err(unsupported_encoding_error(name, &encoding));
    };
    if canonical != "UTF-8" {
        return Err(unsupported_encoding_error(name, &encoding));
    }
    let text = validate_utf8(name, string.as_bytes())?;
    let mut output = String::new();
    for character in text.chars() {
        if uppercase {
            output.extend(character.to_uppercase());
        } else {
            output.extend(character.to_lowercase());
        }
    }
    Ok(Value::string(output))
}

fn encoding_arg(name: &str, value: &Value) -> Result<String, BuiltinError> {
    Ok(string_arg(name, value)?.to_string_lossy())
}

fn encoding_candidates(name: &str, value: &Value) -> Result<Vec<String>, BuiltinError> {
    match deref_value(value) {
        Value::Null => Ok(vec!["UTF-8".to_owned()]),
        Value::String(_) => Ok(encoding_arg(name, value)?
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect()),
        Value::Array(array) => array
            .iter()
            .map(|(_, value)| encoding_arg(name, value))
            .collect(),
        other => Err(argument_type_error(
            name,
            "#2 ($encodings)",
            "array|string|null",
            &other,
        )),
    }
}

fn canonical_encoding(encoding: &str) -> Option<&'static str> {
    let normalized = encoding
        .trim()
        .chars()
        .filter(|character| *character != '-' && *character != '_')
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match normalized.as_str() {
        "" | "utf8" => Some("UTF-8"),
        "ascii" | "usascii" => Some("ASCII"),
        "8bit" | "binary" => Some("8BIT"),
        _ => None,
    }
}

fn bytes_match_encoding(bytes: &[u8], encoding: &str) -> bool {
    match encoding {
        "ASCII" => bytes.is_ascii(),
        "8BIT" => true,
        "UTF-8" => std::str::from_utf8(bytes).is_ok(),
        _ => false,
    }
}

fn value_matches_encoding(value: &Value, encoding: &str) -> Result<bool, BuiltinError> {
    match deref_value(value) {
        Value::String(string) => Ok(bytes_match_encoding(string.as_bytes(), encoding)),
        Value::Array(array) => {
            for (_, value) in array.iter() {
                if !value_matches_encoding(value, encoding)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Value::Null => Ok(true),
        other => Ok(bytes_match_encoding(
            string_arg("mb_check_encoding", &other)?.as_bytes(),
            encoding,
        )),
    }
}

fn validate_utf8<'a>(name: &str, bytes: &'a [u8]) -> Result<&'a str, BuiltinError> {
    std::str::from_utf8(bytes)
        .map_err(|_| argument_value_error(name, "#1 ($string)", "is not valid for encoding UTF-8"))
}

fn unsupported_encoding_error(name: &str, encoding: &str) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_UNSUPPORTED_MBSTRING_ENCODING",
        format!("{name}(): encoding {encoding:?} is outside the bounded UTF-8 mbstring MVP"),
    )
}

fn normalize_character_offset(total: usize, offset: i64) -> usize {
    if offset >= 0 {
        (offset as usize).min(total)
    } else {
        total.saturating_sub(offset.unsigned_abs() as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputBuffer;
    use crate::builtins::BuiltinRegistry;

    #[test]
    fn mbstring_utf8_builtins_cover_prompt_2f_surface() {
        let registry = BuiltinRegistry::new();
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        let len = (registry.get("mb_strlen").unwrap().function())(
            &mut context,
            vec![Value::string("Aé日")],
            RuntimeSourceSpan::default(),
        )
        .expect("mb_strlen succeeds");
        assert_eq!(len, Value::Int(3));

        let substr = (registry.get("mb_substr").unwrap().function())(
            &mut context,
            vec![Value::string("Aé日"), Value::Int(1), Value::Int(2)],
            RuntimeSourceSpan::default(),
        )
        .expect("mb_substr succeeds");
        assert_eq!(substr, Value::string("é日"));

        let lower = (registry.get("mb_strtolower").unwrap().function())(
            &mut context,
            vec![Value::string("ÄÖÜ ABC")],
            RuntimeSourceSpan::default(),
        )
        .expect("mb_strtolower succeeds");
        assert_eq!(lower, Value::string("äöü abc"));

        let upper = (registry.get("mb_strtoupper").unwrap().function())(
            &mut context,
            vec![Value::string("äöü abc")],
            RuntimeSourceSpan::default(),
        )
        .expect("mb_strtoupper succeeds");
        assert_eq!(upper, Value::string("ÄÖÜ ABC"));
    }

    #[test]
    fn mbstring_internal_encoding_is_request_local() {
        let entry = BuiltinRegistry::new()
            .get("mb_internal_encoding")
            .expect("mb_internal_encoding exists");
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        let current = (entry.function())(&mut context, vec![], RuntimeSourceSpan::default())
            .expect("initial encoding");
        assert_eq!(current, Value::string("UTF-8"));

        let updated = (entry.function())(
            &mut context,
            vec![Value::string("ASCII")],
            RuntimeSourceSpan::default(),
        )
        .expect("set encoding");
        assert_eq!(updated, Value::Bool(true));

        let current = (entry.function())(&mut context, vec![], RuntimeSourceSpan::default())
            .expect("updated encoding");
        assert_eq!(current, Value::string("ASCII"));
    }

    #[test]
    fn mbstring_8bit_encoding_counts_and_accepts_raw_bytes() {
        let registry = BuiltinRegistry::new();
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let raw_bytes = Value::string(vec![0xff, b'A', 0xc3]);

        let len = (registry.get("mb_strlen").unwrap().function())(
            &mut context,
            vec![raw_bytes.clone(), Value::string("8bit")],
            RuntimeSourceSpan::default(),
        )
        .expect("mb_strlen 8bit succeeds");
        assert_eq!(len, Value::Int(3));

        let binary_len = (registry.get("mb_strlen").unwrap().function())(
            &mut context,
            vec![raw_bytes.clone(), Value::string("binary")],
            RuntimeSourceSpan::default(),
        )
        .expect("mb_strlen binary succeeds");
        assert_eq!(binary_len, Value::Int(3));

        let check = (registry.get("mb_check_encoding").unwrap().function())(
            &mut context,
            vec![raw_bytes, Value::string("8bit")],
            RuntimeSourceSpan::default(),
        )
        .expect("mb_check_encoding 8bit succeeds");
        assert_eq!(check, Value::Bool(true));
    }
}
