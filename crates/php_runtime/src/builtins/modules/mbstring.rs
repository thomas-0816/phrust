//! Bounded mbstring builtins over PHP strings.

use crate::Value;
use crate::builtins::modules::core::{
    argument_type_error, argument_value_error, arity_error, deref_value, int_arg, string_arg,
};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    MbSubstituteCharacter, RuntimeSourceSpan,
};
use encoding_rs::{EUC_JP, Encoding, ISO_2022_JP, SHIFT_JIS, WINDOWS_1252};

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
    BuiltinEntry::new(
        "mb_list_encodings",
        builtin_mb_list_encodings,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mb_encoding_aliases",
        builtin_mb_encoding_aliases,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mb_substitute_character",
        builtin_mb_substitute_character,
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
    BuiltinEntry::new("mb_stripos", builtin_mb_stripos, BuiltinCompatibility::Php),
    BuiltinEntry::new("mb_strpos", builtin_mb_strpos, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "mb_strripos",
        builtin_mb_strripos,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("mb_strrpos", builtin_mb_strrpos, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "mb_substr_count",
        builtin_mb_substr_count,
        BuiltinCompatibility::Php,
    ),
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
    context: &mut BuiltinContext<'_>,
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
        .unwrap_or_else(|| context.mb_internal_encoding().to_owned());
    let Some(to_canonical) = canonical_encoding(&to_encoding) else {
        return Err(unsupported_encoding_error(
            "mb_convert_encoding",
            "#2 ($to_encoding)",
            &to_encoding,
        ));
    };
    let Some(from_canonical) = canonical_encoding(&from_encoding) else {
        return Err(unsupported_encoding_error(
            "mb_convert_encoding",
            "#3 ($from_encoding)",
            &from_encoding,
        ));
    };
    let text = decode_bytes("mb_convert_encoding", string.as_bytes(), from_canonical)?;
    let output = encode_text("mb_convert_encoding", &text, to_canonical)?;
    Ok(Value::string(output))
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
    context.set_mb_internal_encoding(canonical);
    Ok(Value::Bool(true))
}

fn builtin_mb_list_encodings(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !args.is_empty() {
        return Err(arity_error("mb_list_encodings", "zero arguments"));
    }
    Ok(Value::packed_array(
        SUPPORTED_ENCODINGS
            .iter()
            .copied()
            .map(Value::string)
            .collect(),
    ))
}

fn builtin_mb_encoding_aliases(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("mb_encoding_aliases", "exactly one argument"));
    }
    let encoding = encoding_arg("mb_encoding_aliases", &args[0])?;
    let Some(canonical) = canonical_encoding(&encoding) else {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_VALUE",
            format!(
                "mb_encoding_aliases(): Argument #1 ($encoding) must be a valid encoding, {encoding:?} given"
            ),
        ));
    };
    Ok(Value::packed_array(
        encoding_aliases(canonical)
            .iter()
            .copied()
            .map(Value::string)
            .collect(),
    ))
}

fn builtin_mb_substitute_character(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error(
            "mb_substitute_character",
            "zero or one argument",
        ));
    }
    let Some(value) = args.first() else {
        return Ok(substitute_character_value(
            context.mb_substitute_character(),
        ));
    };
    let substitute = parse_substitute_character(value)?;
    context.set_mb_substitute_character(substitute);
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
    let Some(canonical) = canonical_encoding(&encoding) else {
        return Err(unsupported_encoding_error(
            "mb_strlen",
            "#2 ($encoding)",
            &encoding,
        ));
    };
    if canonical == "8BIT" {
        return Ok(Value::Int(string.len() as i64));
    }
    Ok(Value::Int(
        decode_bytes("mb_strlen", string.as_bytes(), canonical)?
            .chars()
            .count() as i64,
    ))
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
        return Err(unsupported_encoding_error(
            "mb_substr",
            "#4 ($encoding)",
            &encoding,
        ));
    };
    if canonical == "8BIT" {
        return Ok(Value::string(byte_substring(
            string.as_bytes(),
            start,
            length,
        )));
    }
    let text = decode_bytes("mb_substr", string.as_bytes(), canonical)?;
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
    let output = encode_text("mb_substr", &text[byte_start..byte_end], canonical)?;
    Ok(Value::string(output))
}

fn builtin_mb_strpos(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    string_position_builtin(context, "mb_strpos", args, false)
}

fn builtin_mb_stripos(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    string_position_builtin(context, "mb_stripos", args, true)
}

fn builtin_mb_strrpos(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    reverse_string_position_builtin(context, "mb_strrpos", args, false)
}

fn builtin_mb_strripos(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    reverse_string_position_builtin(context, "mb_strripos", args, true)
}

fn builtin_mb_substr_count(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("mb_substr_count", "two or three argument(s)"));
    }
    let haystack = string_arg("mb_substr_count", &args[0])?;
    let needle = string_arg("mb_substr_count", &args[1])?;
    if needle.is_empty() {
        return Err(argument_value_error(
            "mb_substr_count",
            "#2 ($needle)",
            "must not be empty",
        ));
    }
    let encoding = args
        .get(2)
        .map(|value| encoding_arg("mb_substr_count", value))
        .transpose()?
        .unwrap_or_else(|| context.mb_internal_encoding().to_owned());
    let Some(canonical) = canonical_encoding(&encoding) else {
        return Err(unsupported_encoding_error(
            "mb_substr_count",
            "#3 ($encoding)",
            &encoding,
        ));
    };
    if canonical == "8BIT" {
        return Ok(Value::Int(
            non_overlapping_byte_count(haystack.as_bytes(), needle.as_bytes()) as i64,
        ));
    }
    let haystack = decode_bytes("mb_substr_count", haystack.as_bytes(), canonical)?;
    let needle = decode_bytes("mb_substr_count", needle.as_bytes(), canonical)?;
    Ok(Value::Int(
        non_overlapping_text_count(&haystack, &needle) as i64
    ))
}

fn string_position_builtin(
    context: &mut BuiltinContext<'_>,
    name: &'static str,
    args: Vec<Value>,
    case_insensitive: bool,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(arity_error(name, "two to four argument(s)"));
    }
    let haystack = string_arg(name, &args[0])?;
    let needle = string_arg(name, &args[1])?;
    let offset = args
        .get(2)
        .map(|value| int_arg(name, value))
        .transpose()?
        .unwrap_or(0);
    let encoding = args
        .get(3)
        .map(|value| encoding_arg(name, value))
        .transpose()?
        .unwrap_or_else(|| context.mb_internal_encoding().to_owned());
    let Some(canonical) = canonical_encoding(&encoding) else {
        return Err(unsupported_encoding_error(
            name,
            "#4 ($encoding)",
            &encoding,
        ));
    };
    if canonical == "8BIT" {
        return byte_position(
            haystack.as_bytes(),
            needle.as_bytes(),
            offset,
            case_insensitive,
            name,
        );
    }
    let haystack_chars = decode_bytes(name, haystack.as_bytes(), canonical)?
        .chars()
        .collect::<Vec<_>>();
    let needle_string = decode_bytes(name, needle.as_bytes(), canonical)?;
    let start = validate_position_offset(haystack_chars.len(), offset, name)?;
    let tail = haystack_chars[start..].iter().collect::<String>();
    let (tail, needle_string) = if case_insensitive {
        (lowercase(&tail), lowercase(&needle_string))
    } else {
        (tail, needle_string)
    };
    Ok(tail
        .find(&needle_string)
        .map_or(Value::Bool(false), |byte_offset| {
            Value::Int((start + tail[..byte_offset].chars().count()) as i64)
        }))
}

fn reverse_string_position_builtin(
    context: &mut BuiltinContext<'_>,
    name: &'static str,
    args: Vec<Value>,
    case_insensitive: bool,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(arity_error(name, "two to four argument(s)"));
    }
    let haystack = string_arg(name, &args[0])?;
    let needle = string_arg(name, &args[1])?;
    let offset = args
        .get(2)
        .map(|value| int_arg(name, value))
        .transpose()?
        .unwrap_or(0);
    let encoding = args
        .get(3)
        .map(|value| encoding_arg(name, value))
        .transpose()?
        .unwrap_or_else(|| context.mb_internal_encoding().to_owned());
    let Some(canonical) = canonical_encoding(&encoding) else {
        return Err(unsupported_encoding_error(
            name,
            "#4 ($encoding)",
            &encoding,
        ));
    };
    if canonical == "8BIT" {
        return byte_reverse_position(
            haystack.as_bytes(),
            needle.as_bytes(),
            offset,
            case_insensitive,
            name,
        );
    }
    let haystack = decode_bytes(name, haystack.as_bytes(), canonical)?;
    let needle = decode_bytes(name, needle.as_bytes(), canonical)?;
    reverse_text_position(&haystack, &needle, offset, case_insensitive, name)
}

fn reverse_text_position(
    haystack: &str,
    needle: &str,
    offset: i64,
    case_insensitive: bool,
    name: &str,
) -> BuiltinResult {
    let total = haystack.chars().count();
    let limit = validate_reverse_offset(total, offset, name)?;
    if needle.is_empty() {
        return Ok(Value::Int(if offset < 0 { limit } else { total } as i64));
    }
    let (haystack, needle) = if case_insensitive {
        (lowercase(haystack), lowercase(needle))
    } else {
        (haystack.to_owned(), needle.to_owned())
    };
    Ok(haystack
        .match_indices(&needle)
        .filter_map(|(byte_offset, _)| {
            let position = haystack[..byte_offset].chars().count();
            if offset >= 0 {
                (position >= limit).then_some(position)
            } else {
                (position <= limit).then_some(position)
            }
        })
        .last()
        .map_or(Value::Bool(false), |position| Value::Int(position as i64)))
}

fn lowercase(value: &str) -> String {
    value.chars().flat_map(char::to_lowercase).collect()
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
        return Err(unsupported_encoding_error(
            name,
            "#2 ($encoding)",
            &encoding,
        ));
    };
    if canonical == "8BIT" {
        return Ok(Value::String(string));
    }
    let text = decode_bytes(name, string.as_bytes(), canonical)?;
    let mut output = String::new();
    for character in text.chars() {
        if uppercase {
            output.extend(character.to_uppercase());
        } else {
            output.extend(character.to_lowercase());
        }
    }
    Ok(Value::string(encode_text(name, &output, canonical)?))
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
        "iso88591" | "latin1" => Some("ISO-8859-1"),
        "windows1252" | "cp1252" => Some("Windows-1252"),
        "sjis" | "shiftjis" | "shiftjisx0213" | "sjiswin" | "cp932" => Some("SJIS"),
        "eucjp" | "ujis" => Some("EUC-JP"),
        "iso2022jp" | "jis" => Some("ISO-2022-JP"),
        _ => None,
    }
}

const SUPPORTED_ENCODINGS: &[&str] = &[
    "UTF-8",
    "ASCII",
    "8bit",
    "ISO-8859-1",
    "Windows-1252",
    "SJIS",
    "EUC-JP",
    "ISO-2022-JP",
];

fn encoding_aliases(encoding: &str) -> &'static [&'static str] {
    match encoding {
        "UTF-8" => &["utf8"],
        "ASCII" => &[
            "ANSI_X3.4-1968",
            "iso-ir-6",
            "ANSI_X3.4-1986",
            "ISO_646.irv:1991",
            "US-ASCII",
            "ISO646-US",
            "us",
            "IBM367",
            "IBM-367",
            "cp367",
            "csASCII",
        ],
        "8BIT" => &["binary"],
        "ISO-8859-1" => &["ISO8859-1", "latin1"],
        "Windows-1252" => &["cp1252"],
        "SJIS" => &["x-sjis", "SHIFT-JIS"],
        "EUC-JP" => &["EUC", "EUC_JP", "eucJP", "x-euc-jp"],
        "ISO-2022-JP" => &[],
        _ => &[],
    }
}

fn bytes_match_encoding(bytes: &[u8], encoding: &str) -> bool {
    decode_bytes("mb_detect_encoding", bytes, encoding).is_ok()
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

fn decode_bytes(name: &str, bytes: &[u8], encoding: &str) -> Result<String, BuiltinError> {
    match encoding {
        "8BIT" => Ok(bytes.iter().map(|byte| char::from(*byte)).collect()),
        "ASCII" if bytes.is_ascii() => Ok(bytes.iter().map(|byte| char::from(*byte)).collect()),
        "ASCII" => Err(invalid_encoding_value_error(name, encoding)),
        "UTF-8" => validate_utf8(name, bytes).map(ToOwned::to_owned),
        "ISO-8859-1" => Ok(bytes.iter().map(|byte| char::from(*byte)).collect()),
        "Windows-1252" | "SJIS" | "EUC-JP" | "ISO-2022-JP" => {
            let encoding = encoding_rs_encoding(encoding);
            let (text, had_errors) = encoding.decode_without_bom_handling(bytes);
            if had_errors {
                Err(invalid_encoding_value_error(name, encoding.name()))
            } else {
                Ok(text.into_owned())
            }
        }
        _ => Err(unsupported_encoding_error(name, "encoding", encoding)),
    }
}

fn encode_text(name: &str, text: &str, encoding: &str) -> Result<Vec<u8>, BuiltinError> {
    match encoding {
        "8BIT" | "UTF-8" => Ok(text.as_bytes().to_vec()),
        "ASCII" => {
            if text.is_ascii() {
                Ok(text.as_bytes().to_vec())
            } else {
                Err(invalid_encoding_value_error(name, encoding))
            }
        }
        "ISO-8859-1" => encode_latin1(name, text),
        "Windows-1252" | "SJIS" | "EUC-JP" | "ISO-2022-JP" => {
            let encoding = encoding_rs_encoding(encoding);
            let (bytes, _encoding, had_errors) = encoding.encode(text);
            if had_errors {
                Err(invalid_encoding_value_error(name, encoding.name()))
            } else {
                Ok(bytes.into_owned())
            }
        }
        _ => Err(unsupported_encoding_error(name, "encoding", encoding)),
    }
}

fn encode_latin1(name: &str, text: &str) -> Result<Vec<u8>, BuiltinError> {
    let mut output = Vec::with_capacity(text.len());
    for character in text.chars() {
        let value = character as u32;
        if value > 0xff {
            return Err(invalid_encoding_value_error(name, "ISO-8859-1"));
        }
        output.push(value as u8);
    }
    Ok(output)
}

fn encoding_rs_encoding(encoding: &str) -> &'static Encoding {
    match encoding {
        "Windows-1252" => WINDOWS_1252,
        "SJIS" => SHIFT_JIS,
        "EUC-JP" => EUC_JP,
        "ISO-2022-JP" => ISO_2022_JP,
        _ => Encoding::for_label(encoding.as_bytes()).unwrap_or(encoding_rs::UTF_8),
    }
}

fn invalid_encoding_value_error(name: &str, encoding: &str) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_VALUE",
        format!("{name}(): Argument #1 ($string) is not valid for encoding {encoding}"),
    )
}

fn parse_substitute_character(value: &Value) -> Result<MbSubstituteCharacter, BuiltinError> {
    match deref_value(value) {
        Value::Int(codepoint) if valid_unicode_codepoint(codepoint) => {
            Ok(MbSubstituteCharacter::Codepoint(codepoint))
        }
        Value::String(_) => {
            let mode = encoding_arg("mb_substitute_character", value)?;
            match mode.to_ascii_lowercase().as_str() {
                "none" => Ok(MbSubstituteCharacter::Mode("none")),
                "long" => Ok(MbSubstituteCharacter::Mode("long")),
                "entity" => Ok(MbSubstituteCharacter::Mode("entity")),
                _ => Err(invalid_substitute_character_error()),
            }
        }
        _ => Err(invalid_substitute_character_error()),
    }
}

fn substitute_character_value(substitute: &MbSubstituteCharacter) -> Value {
    match substitute {
        MbSubstituteCharacter::Codepoint(codepoint) => Value::Int(*codepoint),
        MbSubstituteCharacter::Mode(mode) => Value::string(*mode),
    }
}

fn valid_unicode_codepoint(codepoint: i64) -> bool {
    matches!(codepoint, 0..=0xd7ff | 0xe000..=0x10ffff)
}

fn invalid_substitute_character_error() -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_VALUE",
        "mb_substitute_character(): Argument #1 ($substitute_character) must be \"none\", \"long\", \"entity\" or a valid codepoint",
    )
}

fn byte_substring(bytes: &[u8], start: i64, length: Option<i64>) -> Vec<u8> {
    let total = bytes.len();
    let start = normalize_character_offset(total, start);
    if start >= total {
        return Vec::new();
    }
    let len = match length {
        None => total - start,
        Some(length) if length >= 0 => (length as usize).min(total - start),
        Some(length) => (total - start).saturating_sub(length.unsigned_abs() as usize),
    };
    bytes[start..start + len].to_vec()
}

fn byte_position(
    haystack: &[u8],
    needle: &[u8],
    offset: i64,
    case_insensitive: bool,
    name: &str,
) -> BuiltinResult {
    let start = validate_position_offset(haystack.len(), offset, name)?;
    if needle.is_empty() {
        return Ok(Value::Int(start as i64));
    }
    if start > haystack.len() {
        return Ok(Value::Bool(false));
    }
    let mut haystack_tail = haystack[start..].to_vec();
    let mut needle = needle.to_vec();
    if case_insensitive {
        haystack_tail.make_ascii_lowercase();
        needle.make_ascii_lowercase();
    }
    Ok(haystack_tail
        .windows(needle.len())
        .position(|window| window == needle.as_slice())
        .map_or(Value::Bool(false), |offset| {
            Value::Int((start + offset) as i64)
        }))
}

fn byte_reverse_position(
    haystack: &[u8],
    needle: &[u8],
    offset: i64,
    case_insensitive: bool,
    name: &str,
) -> BuiltinResult {
    let total = haystack.len();
    let limit = validate_reverse_offset(total, offset, name)?;
    if needle.is_empty() {
        return Ok(Value::Int(if offset < 0 { limit } else { total } as i64));
    }
    let mut haystack = haystack.to_vec();
    let mut needle = needle.to_vec();
    if case_insensitive {
        haystack.make_ascii_lowercase();
        needle.make_ascii_lowercase();
    }
    Ok(haystack
        .windows(needle.len())
        .enumerate()
        .filter_map(|(position, window)| {
            if window != needle.as_slice() {
                return None;
            }
            if offset >= 0 {
                (position >= limit).then_some(position)
            } else {
                (position <= limit).then_some(position)
            }
        })
        .last()
        .map_or(Value::Bool(false), |position| Value::Int(position as i64)))
}

fn non_overlapping_byte_count(haystack: &[u8], needle: &[u8]) -> usize {
    let mut count = 0;
    let mut tail = haystack;
    while let Some(offset) = tail
        .windows(needle.len())
        .position(|window| window == needle)
    {
        count += 1;
        tail = &tail[offset + needle.len()..];
    }
    count
}

fn non_overlapping_text_count(haystack: &str, needle: &str) -> usize {
    let mut count = 0;
    let mut tail = haystack;
    while let Some(offset) = tail.find(needle) {
        count += 1;
        tail = &tail[offset + needle.len()..];
    }
    count
}

fn unsupported_encoding_error(name: &str, argument: &str, encoding: &str) -> BuiltinError {
    argument_value_error(
        name,
        argument,
        &format!("must be a valid encoding, {encoding:?} given"),
    )
}

fn normalize_character_offset(total: usize, offset: i64) -> usize {
    if offset >= 0 {
        (offset as usize).min(total)
    } else {
        total.saturating_sub(offset.unsigned_abs() as usize)
    }
}

fn validate_position_offset(total: usize, offset: i64, name: &str) -> Result<usize, BuiltinError> {
    if offset > total as i64 || offset < -(total as i64) {
        return Err(argument_value_error(
            name,
            "#3 ($offset)",
            "must be contained in argument #1 ($haystack)",
        ));
    }
    Ok(normalize_character_offset(total, offset))
}

fn validate_reverse_offset(total: usize, offset: i64, name: &str) -> Result<usize, BuiltinError> {
    if offset > total as i64 || offset < -(total as i64) {
        return Err(argument_value_error(
            name,
            "#3 ($offset)",
            "must be contained in argument #1 ($haystack)",
        ));
    }
    Ok(if offset < 0 {
        total - offset.unsigned_abs() as usize
    } else {
        offset as usize
    })
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

        let position = (registry.get("mb_strpos").unwrap().function())(
            &mut context,
            vec![Value::string("Aé日é"), Value::string("é"), Value::Int(2)],
            RuntimeSourceSpan::default(),
        )
        .expect("mb_strpos succeeds");
        assert_eq!(position, Value::Int(3));

        let insensitive_position = (registry.get("mb_stripos").unwrap().function())(
            &mut context,
            vec![Value::string("Aé日É"), Value::string("é")],
            RuntimeSourceSpan::default(),
        )
        .expect("mb_stripos succeeds");
        assert_eq!(insensitive_position, Value::Int(1));

        let invalid_position_offset = (registry.get("mb_strpos").unwrap().function())(
            &mut context,
            vec![Value::string("Aé日é"), Value::string("é"), Value::Int(5)],
            RuntimeSourceSpan::default(),
        )
        .expect_err("mb_strpos rejects out-of-range offset");
        assert_eq!(
            invalid_position_offset.message(),
            "mb_strpos(): Argument #3 ($offset) must be contained in argument #1 ($haystack)"
        );

        let reverse_position = (registry.get("mb_strrpos").unwrap().function())(
            &mut context,
            vec![Value::string("Aé日é"), Value::string("é")],
            RuntimeSourceSpan::default(),
        )
        .expect("mb_strrpos succeeds");
        assert_eq!(reverse_position, Value::Int(3));

        let reverse_negative_offset = (registry.get("mb_strrpos").unwrap().function())(
            &mut context,
            vec![Value::string("foo"), Value::string("foo"), Value::Int(-1)],
            RuntimeSourceSpan::default(),
        )
        .expect("mb_strrpos negative offset succeeds");
        assert_eq!(reverse_negative_offset, Value::Int(0));

        let reverse_insensitive_position = (registry.get("mb_strripos").unwrap().function())(
            &mut context,
            vec![Value::string("Aé日É"), Value::string("é")],
            RuntimeSourceSpan::default(),
        )
        .expect("mb_strripos succeeds");
        assert_eq!(reverse_insensitive_position, Value::Int(3));

        let count = (registry.get("mb_substr_count").unwrap().function())(
            &mut context,
            vec![Value::string("日本語日本語日本語"), Value::string("日本語")],
            RuntimeSourceSpan::default(),
        )
        .expect("mb_substr_count succeeds");
        assert_eq!(count, Value::Int(3));
    }

    #[test]
    fn mbstring_substr_count_matches_non_overlapping_and_error_edges() {
        let registry = BuiltinRegistry::new();
        let entry = registry
            .get("mb_substr_count")
            .expect("mb_substr_count exists");
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        let ascii = (entry.function())(
            &mut context,
            vec![Value::string("abcabcabc"), Value::string("abcabc")],
            RuntimeSourceSpan::default(),
        )
        .expect("ascii count succeeds");
        assert_eq!(ascii, Value::Int(1));

        let binary = (entry.function())(
            &mut context,
            vec![
                Value::string(vec![0xff, b'A', 0xff, b'A']),
                Value::string(vec![0xff, b'A']),
                Value::string("8bit"),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("8bit count succeeds");
        assert_eq!(binary, Value::Int(2));

        let empty_needle = (entry.function())(
            &mut context,
            vec![Value::string("abc"), Value::string("")],
            RuntimeSourceSpan::default(),
        )
        .expect_err("empty needle is rejected");
        assert_eq!(
            empty_needle.message(),
            "mb_substr_count(): Argument #2 ($needle) must not be empty"
        );

        let bad_encoding = (entry.function())(
            &mut context,
            vec![
                Value::string("abc"),
                Value::string("a"),
                Value::string("unknown-encoding"),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect_err("unknown encoding is rejected");
        assert_eq!(
            bad_encoding.message(),
            "mb_substr_count(): Argument #3 ($encoding) must be a valid encoding, \"unknown-encoding\" given"
        );
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

    #[test]
    fn mbstring_common_legacy_encodings_convert_and_count() {
        let registry = BuiltinRegistry::new();
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        let latin1 = Value::string(vec![b'R', 0xe9, b's', b'u', b'm', 0xe9]);
        let converted = (registry.get("mb_convert_encoding").unwrap().function())(
            &mut context,
            vec![
                latin1.clone(),
                Value::string("UTF-8"),
                Value::string("ISO-8859-1"),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("latin1 to utf8 succeeds");
        assert_eq!(converted, Value::string("Résumé"));

        let roundtrip = (registry.get("mb_convert_encoding").unwrap().function())(
            &mut context,
            vec![
                converted,
                Value::string("ISO-8859-1"),
                Value::string("UTF-8"),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("utf8 to latin1 succeeds");
        assert_eq!(roundtrip, latin1);

        let sjis = Value::string(vec![0x93, 0xfa, 0x96, 0x7b]);
        let len = (registry.get("mb_strlen").unwrap().function())(
            &mut context,
            vec![sjis.clone(), Value::string("SJIS")],
            RuntimeSourceSpan::default(),
        )
        .expect("sjis length succeeds");
        assert_eq!(len, Value::Int(2));

        let detected = (registry.get("mb_detect_encoding").unwrap().function())(
            &mut context,
            vec![
                sjis,
                Value::packed_array(vec![Value::string("ASCII"), Value::string("SJIS")]),
                Value::Bool(true),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("sjis detection succeeds");
        assert_eq!(detected, Value::string("SJIS"));
    }

    #[test]
    fn mbstring_registry_helpers_cover_supported_aliases_and_substitution_state() {
        let registry = BuiltinRegistry::new();
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        let encodings = (registry.get("mb_list_encodings").unwrap().function())(
            &mut context,
            vec![],
            RuntimeSourceSpan::default(),
        )
        .expect("mb_list_encodings succeeds");
        let Value::Array(encodings) = encodings else {
            panic!("expected encoding array");
        };
        assert!(
            encodings
                .iter()
                .any(|(_, value)| value == &Value::string("UTF-8"))
        );
        assert!(
            encodings
                .iter()
                .any(|(_, value)| value == &Value::string("SJIS"))
        );

        let aliases = (registry.get("mb_encoding_aliases").unwrap().function())(
            &mut context,
            vec![Value::string("SJIS")],
            RuntimeSourceSpan::default(),
        )
        .expect("aliases succeed");
        let Value::Array(aliases) = aliases else {
            panic!("expected alias array");
        };
        assert!(
            aliases
                .iter()
                .any(|(_, value)| value == &Value::string("SHIFT-JIS"))
        );

        let substitute = registry.get("mb_substitute_character").unwrap();
        let current = (substitute.function())(&mut context, vec![], RuntimeSourceSpan::default())
            .expect("default substitute");
        assert_eq!(current, Value::Int(63));
        let updated = (substitute.function())(
            &mut context,
            vec![Value::string("none")],
            RuntimeSourceSpan::default(),
        )
        .expect("set substitute");
        assert_eq!(updated, Value::Bool(true));
        let current = (substitute.function())(&mut context, vec![], RuntimeSourceSpan::default())
            .expect("updated substitute");
        assert_eq!(current, Value::string("none"));
    }
}
