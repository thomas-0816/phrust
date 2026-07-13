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
use unicode_width::UnicodeWidthChar;

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
    BuiltinEntry::new("mb_strcut", builtin_mb_strcut, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "mb_strwidth",
        builtin_mb_strwidth,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mb_strimwidth",
        builtin_mb_strimwidth,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mb_convert_case",
        builtin_mb_convert_case,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("mb_ucfirst", builtin_mb_ucfirst, BuiltinCompatibility::Php),
    BuiltinEntry::new("mb_lcfirst", builtin_mb_lcfirst, BuiltinCompatibility::Php),
    BuiltinEntry::new("mb_ord", builtin_mb_ord, BuiltinCompatibility::Php),
    BuiltinEntry::new("mb_chr", builtin_mb_chr, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "mb_parse_str",
        builtin_mb_parse_str,
        BuiltinCompatibility::Php,
    ),
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
            return Err(detect_invalid_encoding_error(
                "mb_detect_encoding",
                &encoding,
            ));
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
        return Err(unsupported_encoding_error(
            "mb_check_encoding",
            "#2 ($encoding)",
            &encoding,
        ));
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
        return Err(unsupported_encoding_error(
            "mb_internal_encoding",
            "#1 ($encoding)",
            &encoding,
        ));
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
        ENCODING_REGISTRY
            .iter()
            .map(|spec| spec.list_name)
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

fn builtin_mb_strcut(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(arity_error("mb_strcut", "two to four argument(s)"));
    }
    let string = string_arg("mb_strcut", &args[0])?;
    let start = int_arg("mb_strcut", &args[1])?;
    let length = match args.get(2).map(deref_value) {
        Some(Value::Null) | None => None,
        Some(value) => Some(int_arg("mb_strcut", &value)?),
    };
    let encoding = args
        .get(3)
        .map(|value| encoding_arg("mb_strcut", value))
        .transpose()?
        .unwrap_or_else(|| context.mb_internal_encoding().to_owned());
    let Some(canonical) = canonical_encoding(&encoding) else {
        return Err(unsupported_encoding_error(
            "mb_strcut",
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
    let text = decode_bytes("mb_strcut", string.as_bytes(), canonical)?;
    let total = string.len();
    let byte_start = normalize_character_offset(total, start);
    let byte_end = length
        .map(|length| {
            if length < 0 {
                total.saturating_sub(length.unsigned_abs() as usize)
            } else {
                byte_start.saturating_add(length as usize).min(total)
            }
        })
        .unwrap_or(total)
        .min(total);
    let mut output = String::new();
    let mut cursor = 0usize;
    for character in text.chars() {
        let encoded = encode_text("mb_strcut", &character.to_string(), canonical)?;
        let next = cursor + encoded.len();
        if cursor >= byte_start && next <= byte_end {
            output.push(character);
        }
        cursor = next;
    }
    Ok(Value::string(encode_text("mb_strcut", &output, canonical)?))
}

fn builtin_mb_strwidth(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("mb_strwidth", "one or two argument(s)"));
    }
    let string = string_arg("mb_strwidth", &args[0])?;
    let encoding = args
        .get(1)
        .map(|value| encoding_arg("mb_strwidth", value))
        .transpose()?
        .unwrap_or_else(|| context.mb_internal_encoding().to_owned());
    let Some(canonical) = canonical_encoding(&encoding) else {
        return Err(unsupported_encoding_error(
            "mb_strwidth",
            "#2 ($encoding)",
            &encoding,
        ));
    };
    let text = decode_bytes("mb_strwidth", string.as_bytes(), canonical)?;
    Ok(Value::Int(display_width(&text) as i64))
}

fn builtin_mb_strimwidth(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=5).contains(&args.len()) {
        return Err(arity_error("mb_strimwidth", "three to five argument(s)"));
    }
    let string = string_arg("mb_strimwidth", &args[0])?;
    let start = int_arg("mb_strimwidth", &args[1])?;
    let width = int_arg("mb_strimwidth", &args[2])?;
    let trim_marker = args
        .get(3)
        .map(|value| string_arg("mb_strimwidth", value))
        .transpose()?
        .unwrap_or_else(|| "".into());
    let encoding = args
        .get(4)
        .map(|value| encoding_arg("mb_strimwidth", value))
        .transpose()?
        .unwrap_or_else(|| context.mb_internal_encoding().to_owned());
    let Some(canonical) = canonical_encoding(&encoding) else {
        return Err(unsupported_encoding_error(
            "mb_strimwidth",
            "#5 ($encoding)",
            &encoding,
        ));
    };
    if width < 0 {
        return Err(argument_value_error(
            "mb_strimwidth",
            "#3 ($width)",
            "must be greater than or equal to 0",
        ));
    }
    let text = decode_bytes("mb_strimwidth", string.as_bytes(), canonical)?;
    let marker = decode_bytes("mb_strimwidth", trim_marker.as_bytes(), canonical)?;
    let output = trim_to_display_width(&text, start, width as usize, &marker);
    Ok(Value::string(encode_text(
        "mb_strimwidth",
        &output,
        canonical,
    )?))
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

const MB_CASE_UPPER: i64 = 0;
const MB_CASE_LOWER: i64 = 1;
const MB_CASE_TITLE: i64 = 2;
const MB_CASE_FOLD: i64 = 3;
const MB_CASE_UPPER_SIMPLE: i64 = 4;
const MB_CASE_LOWER_SIMPLE: i64 = 5;
const MB_CASE_TITLE_SIMPLE: i64 = 6;
const MB_CASE_FOLD_SIMPLE: i64 = 7;

fn builtin_mb_convert_case(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("mb_convert_case", "two or three argument(s)"));
    }
    let string = string_arg("mb_convert_case", &args[0])?;
    let mode = int_arg("mb_convert_case", &args[1])?;
    let encoding = args
        .get(2)
        .map(|value| encoding_arg("mb_convert_case", value))
        .transpose()?
        .unwrap_or_else(|| context.mb_internal_encoding().to_owned());
    let Some(canonical) = canonical_encoding(&encoding) else {
        return Err(unsupported_encoding_error(
            "mb_convert_case",
            "#3 ($encoding)",
            &encoding,
        ));
    };
    let text = decode_bytes("mb_convert_case", string.as_bytes(), canonical)?;
    let output = match mode {
        MB_CASE_UPPER | MB_CASE_UPPER_SIMPLE => text
            .chars()
            .flat_map(char::to_uppercase)
            .collect::<String>(),
        MB_CASE_LOWER | MB_CASE_LOWER_SIMPLE | MB_CASE_FOLD | MB_CASE_FOLD_SIMPLE => {
            lowercase(&text)
        }
        MB_CASE_TITLE | MB_CASE_TITLE_SIMPLE => titlecase(&text),
        _ => {
            return Err(argument_value_error(
                "mb_convert_case",
                "#2 ($mode)",
                "must be one of the MB_CASE_* constants",
            ));
        }
    };
    Ok(Value::string(encode_text(
        "mb_convert_case",
        &output,
        canonical,
    )?))
}

fn builtin_mb_ucfirst(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    first_char_case_builtin(context, "mb_ucfirst", args, true)
}

fn builtin_mb_lcfirst(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    first_char_case_builtin(context, "mb_lcfirst", args, false)
}

fn builtin_mb_ord(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("mb_ord", "one or two argument(s)"));
    }
    let string = string_arg("mb_ord", &args[0])?;
    let encoding = args
        .get(1)
        .map(|value| encoding_arg("mb_ord", value))
        .transpose()?
        .unwrap_or_else(|| context.mb_internal_encoding().to_owned());
    let Some(canonical) = canonical_encoding(&encoding) else {
        return Err(unsupported_encoding_error(
            "mb_ord",
            "#2 ($encoding)",
            &encoding,
        ));
    };
    let text = decode_bytes("mb_ord", string.as_bytes(), canonical)?;
    Ok(text.chars().next().map_or(Value::Bool(false), |character| {
        Value::Int(character as u32 as i64)
    }))
}

fn builtin_mb_chr(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("mb_chr", "one or two argument(s)"));
    }
    let codepoint = int_arg("mb_chr", &args[0])?;
    let Some(character) = char::from_u32(codepoint as u32) else {
        return Err(argument_value_error(
            "mb_chr",
            "#1 ($codepoint)",
            "must be a valid codepoint",
        ));
    };
    let encoding = args
        .get(1)
        .map(|value| encoding_arg("mb_chr", value))
        .transpose()?
        .unwrap_or_else(|| context.mb_internal_encoding().to_owned());
    let Some(canonical) = canonical_encoding(&encoding) else {
        return Err(unsupported_encoding_error(
            "mb_chr",
            "#2 ($encoding)",
            &encoding,
        ));
    };
    Ok(Value::string(encode_text(
        "mb_chr",
        &character.to_string(),
        canonical,
    )?))
}

fn builtin_mb_parse_str(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    super::strings::builtin_parse_str(context, args, span).map(|_| Value::Bool(true))
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

fn first_char_case_builtin(
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
    let text = decode_bytes(name, string.as_bytes(), canonical)?;
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return Ok(Value::string(Vec::new()));
    };
    let mut output = String::new();
    if uppercase {
        output.extend(first.to_uppercase());
    } else {
        output.extend(first.to_lowercase());
    }
    output.push_str(chars.as_str());
    Ok(Value::string(encode_text(name, &output, canonical)?))
}

fn titlecase(value: &str) -> String {
    let mut output = String::new();
    let mut word_start = true;
    for character in value.chars() {
        if character.is_alphanumeric() {
            if word_start {
                output.extend(character.to_uppercase());
            } else {
                output.extend(character.to_lowercase());
            }
            word_start = false;
        } else {
            output.push(character);
            word_start = true;
        }
    }
    output
}

fn display_width(value: &str) -> usize {
    value
        .chars()
        .map(|character| UnicodeWidthChar::width(character).unwrap_or(0))
        .sum()
}

fn trim_to_display_width(value: &str, start: i64, width: usize, marker: &str) -> String {
    let total_width = display_width(value);
    let start = if start < 0 {
        total_width.saturating_sub(start.unsigned_abs() as usize)
    } else {
        start as usize
    };
    let mut skipped = 0usize;
    let mut tail = String::new();
    for character in value.chars() {
        let char_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if skipped + char_width <= start {
            skipped += char_width;
            continue;
        }
        tail.push(character);
    }
    if display_width(&tail) <= width {
        return tail;
    }
    let marker_width = display_width(marker);
    let body_width = width.saturating_sub(marker_width);
    let mut output = String::new();
    let mut used = 0usize;
    for character in tail.chars() {
        let char_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + char_width > body_width {
            break;
        }
        output.push(character);
        used += char_width;
    }
    output.push_str(marker);
    output
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
    encoding_spec(encoding).map(|spec| spec.canonical)
}

#[derive(Clone, Copy)]
enum EncodingBackend {
    Utf8,
    Binary,
    Ascii,
    Latin1,
    EncodingRs(&'static Encoding),
}

struct EncodingSpec {
    canonical: &'static str,
    list_name: &'static str,
    aliases: &'static [&'static str],
    backend: EncodingBackend,
    supports_encode: bool,
    supports_decode: bool,
    supports_detect: bool,
}

const ENCODING_REGISTRY: &[EncodingSpec] = &[
    EncodingSpec {
        canonical: "UTF-8",
        list_name: "UTF-8",
        aliases: &["utf8"],
        backend: EncodingBackend::Utf8,
        supports_encode: true,
        supports_decode: true,
        supports_detect: true,
    },
    EncodingSpec {
        canonical: "8BIT",
        list_name: "8bit",
        aliases: &["binary"],
        backend: EncodingBackend::Binary,
        supports_encode: true,
        supports_decode: true,
        supports_detect: false,
    },
    EncodingSpec {
        canonical: "ASCII",
        list_name: "ASCII",
        aliases: &[
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
        backend: EncodingBackend::Ascii,
        supports_encode: true,
        supports_decode: true,
        supports_detect: true,
    },
    EncodingSpec {
        canonical: "7bit",
        list_name: "7bit",
        aliases: &[],
        backend: EncodingBackend::Ascii,
        supports_encode: true,
        supports_decode: true,
        supports_detect: false,
    },
    EncodingSpec {
        canonical: "HTML-ENTITIES",
        list_name: "HTML-ENTITIES",
        aliases: &["HTML", "html"],
        backend: EncodingBackend::Utf8,
        supports_encode: false,
        supports_decode: false,
        supports_detect: false,
    },
    EncodingSpec {
        canonical: "ISO-8859-1",
        list_name: "ISO-8859-1",
        aliases: &["ISO8859-1", "latin1"],
        backend: EncodingBackend::Latin1,
        supports_encode: true,
        supports_decode: true,
        supports_detect: true,
    },
    EncodingSpec {
        canonical: "Windows-1252",
        list_name: "Windows-1252",
        aliases: &["cp1252"],
        backend: EncodingBackend::EncodingRs(WINDOWS_1252),
        supports_encode: true,
        supports_decode: true,
        supports_detect: true,
    },
    EncodingSpec {
        canonical: "SJIS",
        list_name: "SJIS",
        aliases: &["x-sjis", "SHIFT-JIS"],
        backend: EncodingBackend::EncodingRs(SHIFT_JIS),
        supports_encode: true,
        supports_decode: true,
        supports_detect: true,
    },
    EncodingSpec {
        canonical: "EUC-JP",
        list_name: "EUC-JP",
        aliases: &["EUC", "EUC_JP", "eucJP", "x-euc-jp"],
        backend: EncodingBackend::EncodingRs(EUC_JP),
        supports_encode: true,
        supports_decode: true,
        supports_detect: true,
    },
    EncodingSpec {
        canonical: "ISO-2022-JP",
        list_name: "ISO-2022-JP",
        aliases: &[],
        backend: EncodingBackend::EncodingRs(ISO_2022_JP),
        supports_encode: true,
        supports_decode: true,
        supports_detect: true,
    },
];

fn encoding_spec(encoding: &str) -> Option<&'static EncodingSpec> {
    let normalized = encoding
        .trim()
        .chars()
        .filter(|character| *character != '-' && *character != '_')
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if normalized.is_empty() {
        return encoding_by_canonical("UTF-8");
    }
    ENCODING_REGISTRY.iter().find(|spec| {
        normalized_encoding_key(spec.canonical) == normalized
            || normalized_encoding_key(spec.list_name) == normalized
            || spec
                .aliases
                .iter()
                .any(|alias| normalized_encoding_key(alias) == normalized)
            || legacy_encoding_alias_matches(&normalized, spec.canonical)
    })
}

fn encoding_by_canonical(canonical: &str) -> Option<&'static EncodingSpec> {
    ENCODING_REGISTRY
        .iter()
        .find(|spec| spec.canonical == canonical)
}

fn normalized_encoding_key(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|character| *character != '-' && *character != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

fn legacy_encoding_alias_matches(normalized: &str, canonical: &str) -> bool {
    matches!(
        (normalized, canonical),
        ("shiftjisx0213", "SJIS")
            | ("sjiswin", "SJIS")
            | ("cp932", "SJIS")
            | ("ujis", "EUC-JP")
            | ("jis", "ISO-2022-JP")
    )
}

fn encoding_aliases(encoding: &str) -> &'static [&'static str] {
    encoding_by_canonical(encoding).map_or(&[], |spec| spec.aliases)
}

fn bytes_match_encoding(bytes: &[u8], encoding: &str) -> bool {
    let Some(spec) = encoding_by_canonical(encoding) else {
        return false;
    };
    if spec.canonical == "ASCII" && bytes.contains(&0x1b) {
        return false;
    }
    spec.supports_detect && decode_bytes("mb_detect_encoding", bytes, encoding).is_ok()
}

fn bytes_are_valid_for_encoding(bytes: &[u8], encoding: &str) -> bool {
    decode_bytes("mb_check_encoding", bytes, encoding).is_ok()
}

fn value_matches_encoding(value: &Value, encoding: &str) -> Result<bool, BuiltinError> {
    match deref_value(value) {
        Value::String(string) => Ok(bytes_are_valid_for_encoding(string.as_bytes(), encoding)),
        Value::Array(array) => {
            for (_, value) in array.iter() {
                if !value_matches_encoding(value, encoding)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Value::Null => Ok(true),
        other => Ok(bytes_are_valid_for_encoding(
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
    let Some(spec) = encoding_by_canonical(encoding) else {
        return Err(unsupported_encoding_error(name, "encoding", encoding));
    };
    if !spec.supports_decode {
        return Err(unsupported_encoding_error(name, "encoding", encoding));
    }
    match spec.backend {
        EncodingBackend::Binary => Ok(bytes.iter().map(|byte| char::from(*byte)).collect()),
        EncodingBackend::Ascii if bytes.is_ascii() => {
            Ok(bytes.iter().map(|byte| char::from(*byte)).collect())
        }
        EncodingBackend::Ascii => Err(invalid_encoding_value_error(name, encoding)),
        EncodingBackend::Utf8 => validate_utf8(name, bytes).map(ToOwned::to_owned),
        EncodingBackend::Latin1 => Ok(bytes.iter().map(|byte| char::from(*byte)).collect()),
        EncodingBackend::EncodingRs(encoding) => {
            let (text, had_errors) = encoding.decode_without_bom_handling(bytes);
            if had_errors {
                Err(invalid_encoding_value_error(name, encoding.name()))
            } else {
                Ok(text.into_owned())
            }
        }
    }
}

fn encode_text(name: &str, text: &str, encoding: &str) -> Result<Vec<u8>, BuiltinError> {
    let Some(spec) = encoding_by_canonical(encoding) else {
        return Err(unsupported_encoding_error(name, "encoding", encoding));
    };
    if !spec.supports_encode {
        return Err(unsupported_encoding_error(name, "encoding", encoding));
    }
    match spec.backend {
        EncodingBackend::Utf8 => Ok(text.as_bytes().to_vec()),
        EncodingBackend::Binary => encode_latin1(name, text),
        EncodingBackend::Ascii => {
            if text.is_ascii() {
                Ok(text.as_bytes().to_vec())
            } else {
                Err(invalid_encoding_value_error(name, encoding))
            }
        }
        EncodingBackend::Latin1 => encode_latin1(name, text),
        EncodingBackend::EncodingRs(encoding) => {
            let (bytes, _encoding, had_errors) = encoding.encode(text);
            if had_errors {
                Err(invalid_encoding_value_error(name, encoding.name()))
            } else {
                Ok(bytes.into_owned())
            }
        }
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
        .next_back()
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

fn detect_invalid_encoding_error(name: &str, encoding: &str) -> BuiltinError {
    argument_value_error(
        name,
        "#2 ($encodings)",
        &format!("contains invalid encoding {encoding:?}"),
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
