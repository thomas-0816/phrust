//! Bounded iconv conversion MVP.

use super::core::{
    argument_value_error, arity_error, deref_value, int_arg, string_arg, type_error,
};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult, RuntimeSourceSpan,
};
use crate::{ArrayKey, PhpArray, PhpString, RuntimeDiagnostic, RuntimeSeverity, Value};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use encoding_rs::{EUC_JP, Encoding, ISO_2022_JP, ISO_8859_2, SHIFT_JIS, WINDOWS_1252};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("iconv", builtin_iconv, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "iconv_get_encoding",
        builtin_iconv_get_encoding,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "iconv_set_encoding",
        builtin_iconv_set_encoding,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "iconv_strlen",
        builtin_iconv_strlen,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "iconv_strpos",
        builtin_iconv_strpos,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "iconv_strrpos",
        builtin_iconv_strrpos,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "iconv_substr",
        builtin_iconv_substr,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "iconv_mime_encode",
        builtin_iconv_mime_encode,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "iconv_mime_decode",
        builtin_iconv_mime_decode,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "iconv_mime_decode_headers",
        builtin_iconv_mime_decode_headers,
        BuiltinCompatibility::Php,
    ),
];

fn builtin_iconv_get_encoding(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("iconv_get_encoding", "zero or one argument"));
    }
    let kind = args
        .first()
        .map(|value| string_arg("iconv_get_encoding", value))
        .transpose()?;
    let input_encoding = effective_iconv_encoding(context, "input_encoding");
    let output_encoding = effective_iconv_encoding(context, "output_encoding");
    let internal_encoding = effective_iconv_encoding(context, "internal_encoding");
    match kind
        .as_ref()
        .map(|value| value.to_string_lossy())
        .as_deref()
    {
        None | Some("all") => {
            let mut array = PhpArray::new();
            array.insert(
                ArrayKey::String(PhpString::from_test_str("input_encoding")),
                Value::string(input_encoding.as_bytes().to_vec()),
            );
            array.insert(
                ArrayKey::String(PhpString::from_test_str("output_encoding")),
                Value::string(output_encoding.as_bytes().to_vec()),
            );
            array.insert(
                ArrayKey::String(PhpString::from_test_str("internal_encoding")),
                Value::string(internal_encoding.as_bytes().to_vec()),
            );
            Ok(Value::Array(array))
        }
        Some("input_encoding") => Ok(Value::string(input_encoding.into_bytes())),
        Some("output_encoding") => Ok(Value::string(output_encoding.into_bytes())),
        Some("internal_encoding") => Ok(Value::string(internal_encoding.into_bytes())),
        Some(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_iconv_set_encoding(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("iconv_set_encoding", "two arguments"));
    }
    let kind = string_arg("iconv_set_encoding", &args[0])?.to_string_lossy();
    let Some(encoding) = checked_encoding_arg(context, "iconv_set_encoding", &args[1], &span)?
    else {
        return Ok(Value::Bool(false));
    };
    let updated = context.iconv_state().set(&kind, encoding);
    if updated {
        context.ini_set(&kind, encoding);
        context.ini_set(&format!("iconv.{kind}"), encoding);
    }
    Ok(Value::Bool(updated))
}

fn builtin_iconv(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("iconv", "three argument(s)"));
    }
    let from_raw = raw_encoding_arg("iconv", &args[0])?;
    let to_raw = raw_encoding_arg("iconv", &args[1])?;
    if iconv_encoding_too_long(&from_raw) || iconv_encoding_too_long(&to_raw) {
        warn_iconv_encoding_too_long(context, "iconv", &span);
        return Ok(Value::Bool(false));
    }
    let Some(from) = canonical_encoding(&from_raw) else {
        warn_iconv_wrong_conversion(context, "iconv", &from_raw, conversion_base(&to_raw), &span);
        return Ok(Value::Bool(false));
    };
    let Some(to) = parse_conversion_target(&to_raw) else {
        warn_iconv_wrong_conversion(context, "iconv", &from_raw, conversion_base(&to_raw), &span);
        return Ok(Value::Bool(false));
    };
    let input = string_arg("iconv", &args[2])?;
    convert_encoding("iconv", input.as_bytes(), from, to)
}

fn builtin_iconv_mime_encode(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("iconv_mime_encode", "two or three argument(s)"));
    }
    let field_name = string_arg("iconv_mime_encode", &args[0])?.to_string_lossy();
    let field_value = string_arg("iconv_mime_encode", &args[1])?;
    let options_value = args.get(2).map(deref_value);
    let options = match options_value.as_ref() {
        None => None,
        Some(Value::Array(array)) => Some(array),
        Some(_) => return Err(type_error("iconv_mime_encode", "array", &args[2])),
    };
    let default_input_charset = effective_iconv_encoding(context, "input_encoding");
    let default_output_charset = effective_iconv_encoding(context, "output_encoding");
    let input_charset = mime_option(options, "input-charset", &default_input_charset)?;
    let output_charset = mime_option(options, "output-charset", &default_output_charset)?;
    let scheme = mime_option(options, "scheme", "B")?;
    if iconv_encoding_too_long(&input_charset) || iconv_encoding_too_long(&output_charset) {
        warn_iconv_encoding_too_long(context, "iconv_mime_encode", &span);
        return Ok(Value::Bool(false));
    }
    let Some(input_encoding) = canonical_encoding(&input_charset) else {
        warn_iconv_wrong_conversion(
            context,
            "iconv_mime_encode",
            &input_charset,
            &output_charset,
            &span,
        );
        return Ok(Value::Bool(false));
    };
    let Some(output_encoding) = canonical_encoding(&output_charset) else {
        warn_iconv_wrong_conversion(
            context,
            "iconv_mime_encode",
            &input_charset,
            &output_charset,
            &span,
        );
        return Ok(Value::Bool(false));
    };
    let text = string_for_encoding(
        "iconv_mime_encode",
        field_value.as_bytes(),
        input_encoding,
        "#2 ($field_value)",
    )?;
    let Some(bytes) = bytes_for_encoding(&text, output_encoding) else {
        return Ok(Value::Bool(false));
    };
    let scheme_upper = scheme.to_ascii_uppercase();
    let encoded = match scheme_upper.as_str() {
        "B" => BASE64_STANDARD.encode(bytes),
        "Q" => mime_q_encode(&bytes),
        _ => {
            return Err(argument_value_error(
                "iconv_mime_encode",
                "scheme",
                "must be B or Q",
            ));
        }
    };
    Ok(Value::string(
        format!("{field_name}: =?{output_charset}?{scheme_upper}?{encoded}?=").into_bytes(),
    ))
}

const ICONV_MIME_DECODE_STRICT: i64 = 1;
const ICONV_MIME_DECODE_CONTINUE_ON_ERROR: i64 = 2;

fn builtin_iconv_mime_decode(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error("iconv_mime_decode", "one to three argument(s)"));
    }
    let input = string_arg("iconv_mime_decode", &args[0])?.to_string_lossy();
    let mode = args
        .get(1)
        .map(|value| int_arg("iconv_mime_decode", value))
        .transpose()?
        .unwrap_or(0);
    let output_charset = args
        .get(2)
        .map(|value| checked_encoding_arg(context, "iconv_mime_decode", value, &span))
        .transpose()?
        .flatten()
        .unwrap_or("UTF-8");
    if args.get(2).is_some() && output_charset == "UTF-8" {
        let raw_output_charset = raw_encoding_arg("iconv_mime_decode", &args[2])?;
        if canonical_encoding(&raw_output_charset).is_none()
            || iconv_encoding_too_long(&raw_output_charset)
        {
            return Ok(Value::Bool(false));
        }
    }
    let decoded = match decode_mime_words(&input, MimeDecodeMode::from_flags(mode))? {
        MimeDecodeResult::Decoded(decoded) => decoded,
        MimeDecodeResult::Malformed => {
            context.record_diagnostic(RuntimeDiagnostic::new(
                "E_PHP_RUNTIME_ICONV_MIME_DECODE_MALFORMED",
                RuntimeSeverity::Warning,
                "iconv_mime_decode(): Malformed string",
                span,
                Vec::new(),
                Some(crate::PhpReferenceClassification::Warning),
            ));
            String::new()
        }
    };
    let Some(bytes) = bytes_for_encoding(&decoded, output_charset) else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::string(bytes))
}

fn builtin_iconv_mime_decode_headers(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error(
            "iconv_mime_decode_headers",
            "one to three argument(s)",
        ));
    }
    let input = string_arg("iconv_mime_decode_headers", &args[0])?.to_string_lossy();
    let mode = args
        .get(1)
        .map(|value| int_arg("iconv_mime_decode_headers", value))
        .transpose()?
        .unwrap_or(0);
    let output_charset = args
        .get(2)
        .map(|value| checked_encoding_arg(context, "iconv_mime_decode_headers", value, &span))
        .transpose()?
        .flatten()
        .unwrap_or("UTF-8");
    if args.get(2).is_some() && output_charset == "UTF-8" {
        let raw_output_charset = raw_encoding_arg("iconv_mime_decode_headers", &args[2])?;
        if canonical_encoding(&raw_output_charset).is_none()
            || iconv_encoding_too_long(&raw_output_charset)
        {
            return Ok(Value::Bool(false));
        }
    }
    let mut headers = PhpArray::new();
    let mut current_name: Option<String> = None;
    let mut current_value = String::new();
    for raw_line in input.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            if !current_value.is_empty() {
                current_value.push(' ');
            }
            current_value.push_str(line.trim());
            continue;
        }
        flush_decoded_header(
            &mut headers,
            current_name.take(),
            std::mem::take(&mut current_value),
            mode,
            output_charset,
        )?;
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        current_name = Some(name.trim().to_owned());
        current_value.push_str(value.trim());
    }
    flush_decoded_header(
        &mut headers,
        current_name,
        current_value,
        mode,
        output_charset,
    )?;
    Ok(Value::Array(headers))
}

fn builtin_iconv_strlen(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("iconv_strlen", "one or two argument(s)"));
    }
    let input = string_arg("iconv_strlen", &args[0])?;
    let Some(encoding) = encoding_arg_or_default(context, "iconv_strlen", args.get(1), &span)?
    else {
        return Ok(Value::Bool(false));
    };
    let chars = chars_for_encoding("iconv_strlen", input.as_bytes(), encoding, "#1 ($string)")?;
    Ok(Value::Int(chars.len() as i64))
}

fn builtin_iconv_substr(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(arity_error("iconv_substr", "two to four argument(s)"));
    }
    let input = string_arg("iconv_substr", &args[0])?;
    let offset = int_arg("iconv_substr", &args[1])?;
    let length = args
        .get(2)
        .map(|value| int_arg("iconv_substr", value))
        .transpose()?;
    let Some(encoding) = encoding_arg_or_default(context, "iconv_substr", args.get(3), &span)?
    else {
        return Ok(Value::Bool(false));
    };
    let chars = chars_for_encoding("iconv_substr", input.as_bytes(), encoding, "#1 ($string)")?;
    let start = normalize_offset(chars.len(), offset).min(chars.len());
    let end = length
        .map_or(chars.len(), |value| {
            if value < 0 {
                chars.len().saturating_sub(value.unsigned_abs() as usize)
            } else {
                start.saturating_add(value as usize).min(chars.len())
            }
        })
        .min(chars.len());
    if end < start {
        return Ok(Value::string(Vec::new()));
    }
    Ok(Value::string(chars[start..end].iter().collect::<String>()))
}

fn builtin_iconv_strpos(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(arity_error("iconv_strpos", "two to four argument(s)"));
    }
    let haystack = string_arg("iconv_strpos", &args[0])?;
    let needle = string_arg("iconv_strpos", &args[1])?;
    let offset = args
        .get(2)
        .map(|value| int_arg("iconv_strpos", value))
        .transpose()?
        .unwrap_or(0);
    let Some(encoding) = encoding_arg_or_default(context, "iconv_strpos", args.get(3), &span)?
    else {
        return Ok(Value::Bool(false));
    };
    let haystack_chars = chars_for_encoding(
        "iconv_strpos",
        haystack.as_bytes(),
        encoding,
        "#1 ($haystack)",
    )?;
    let needle_string =
        chars_for_encoding("iconv_strpos", needle.as_bytes(), encoding, "#2 ($needle)")?
            .iter()
            .collect::<String>();
    let start = normalize_offset(haystack_chars.len(), offset);
    let tail = haystack_chars[start..].iter().collect::<String>();
    Ok(tail
        .find(&needle_string)
        .map_or(Value::Bool(false), |byte_offset| {
            Value::Int((start + tail[..byte_offset].chars().count()) as i64)
        }))
}

fn builtin_iconv_strrpos(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("iconv_strrpos", "two or three argument(s)"));
    }
    let haystack = string_arg("iconv_strrpos", &args[0])?;
    let needle = string_arg("iconv_strrpos", &args[1])?;
    let Some(encoding) = encoding_arg_or_default(context, "iconv_strrpos", args.get(2), &span)?
    else {
        return Ok(Value::Bool(false));
    };
    let haystack_string = string_for_encoding(
        "iconv_strrpos",
        haystack.as_bytes(),
        encoding,
        "#1 ($haystack)",
    )?;
    let needle_string =
        string_for_encoding("iconv_strrpos", needle.as_bytes(), encoding, "#2 ($needle)")?;
    if needle_string.is_empty() {
        return Ok(Value::Bool(false));
    }
    Ok(haystack_string
        .rfind(&needle_string)
        .map_or(Value::Bool(false), |byte_offset| {
            Value::Int(haystack_string[..byte_offset].chars().count() as i64)
        }))
}

fn flush_decoded_header(
    headers: &mut PhpArray,
    name: Option<String>,
    value: String,
    mode: i64,
    output_charset: &str,
) -> Result<(), crate::builtins::BuiltinError> {
    let Some(name) = name else {
        return Ok(());
    };
    let decoded = match decode_mime_words(&value, MimeDecodeMode::from_flags(mode))? {
        MimeDecodeResult::Decoded(decoded) => decoded,
        MimeDecodeResult::Malformed => String::new(),
    };
    let Some(bytes) = bytes_for_encoding(&decoded, output_charset) else {
        return Ok(());
    };
    let key = ArrayKey::String(PhpString::from(name.as_bytes().to_vec()));
    let new_value = Value::string(bytes);
    match headers.get(&key).cloned() {
        None => {
            headers.insert(key, new_value);
        }
        Some(Value::Array(mut values)) => {
            values.append(new_value);
            headers.insert(key, Value::Array(values));
        }
        Some(previous) => {
            headers.insert(
                key,
                Value::Array(PhpArray::from_packed(vec![previous, new_value])),
            );
        }
    }
    Ok(())
}

fn mime_option(
    options: Option<&PhpArray>,
    key: &str,
    default: &str,
) -> Result<String, crate::builtins::BuiltinError> {
    let Some(options) = options else {
        return Ok(default.to_owned());
    };
    let key = ArrayKey::String(PhpString::from_test_str(key));
    let Some(value) = options.get(&key) else {
        return Ok(default.to_owned());
    };
    Ok(string_arg("iconv_mime_encode", value)?.to_string_lossy())
}

fn mime_q_encode(bytes: &[u8]) -> String {
    let mut encoded = String::new();
    for byte in bytes {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => encoded.push(char::from(*byte)),
            _ => encoded.push_str(&format!("={byte:02X}")),
        }
    }
    encoded
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MimeDecodeMode {
    strict: bool,
    continue_on_error: bool,
}

impl MimeDecodeMode {
    fn from_flags(flags: i64) -> Self {
        Self {
            strict: flags & ICONV_MIME_DECODE_STRICT != 0,
            continue_on_error: flags & ICONV_MIME_DECODE_CONTINUE_ON_ERROR != 0,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum MimeDecodeResult {
    Decoded(String),
    Malformed,
}

fn decode_mime_words(
    input: &str,
    mode: MimeDecodeMode,
) -> Result<MimeDecodeResult, crate::builtins::BuiltinError> {
    let mut output = String::new();
    let mut rest = input;
    let mut previous_was_encoded = false;
    while let Some(start) = rest.find("=?") {
        let prefix = &rest[..start];
        if mode.strict && previous_was_encoded && prefix.is_empty() {
            return Ok(MimeDecodeResult::Decoded(input.to_owned()));
        }
        if !(previous_was_encoded && prefix.chars().all(char::is_whitespace)) {
            output.push_str(prefix);
        }
        let candidate = &rest[start + 2..];
        let Some(charset_end) = candidate.find('?') else {
            return Ok(malformed_mime_result(&mut output, rest, start, mode));
        };
        let charset = &candidate[..charset_end];
        let after_charset = &candidate[charset_end + 1..];
        let Some(scheme_end) = after_charset.find('?') else {
            return Ok(malformed_mime_result(&mut output, rest, start, mode));
        };
        let scheme = &after_charset[..scheme_end];
        let after_scheme = &after_charset[scheme_end + 1..];
        let Some(payload_end) = after_scheme.find("?=") else {
            return Ok(malformed_mime_result(&mut output, rest, start, mode));
        };
        let payload = &after_scheme[..payload_end];
        if payload.contains("=?") {
            return Ok(malformed_mime_result(&mut output, rest, start, mode));
        }
        let encoding = canonical_encoding(charset.split('*').next().unwrap_or(charset))
            .ok_or_else(|| {
                argument_value_error(
                    "iconv_mime_decode",
                    "charset",
                    "must be a supported encoding",
                )
            })?;
        let bytes = match scheme.to_ascii_uppercase().as_str() {
            "B" => match BASE64_STANDARD.decode(payload.as_bytes()) {
                Ok(bytes) => bytes,
                Err(_) => return Ok(malformed_mime_result(&mut output, rest, start, mode)),
            },
            "Q" => mime_q_decode(payload)?,
            _ => {
                return Ok(malformed_mime_result(&mut output, rest, start, mode));
            }
        };
        output.push_str(&string_for_encoding(
            "iconv_mime_decode",
            &bytes,
            encoding,
            "#1 ($string)",
        )?);
        previous_was_encoded = true;
        rest = &after_scheme[payload_end + 2..];
    }
    output.push_str(rest);
    Ok(MimeDecodeResult::Decoded(output))
}

fn malformed_mime_result(
    output: &mut String,
    rest: &str,
    start: usize,
    mode: MimeDecodeMode,
) -> MimeDecodeResult {
    if mode.continue_on_error {
        output.push_str(&rest[start..]);
        MimeDecodeResult::Decoded(output.clone())
    } else {
        MimeDecodeResult::Malformed
    }
}

fn mime_q_decode(payload: &str) -> Result<Vec<u8>, crate::builtins::BuiltinError> {
    let mut output = Vec::with_capacity(payload.len());
    let bytes = payload.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'_' => {
                output.push(b' ');
                index += 1;
            }
            b'=' if index + 2 < bytes.len() => {
                let high = hex_value(bytes[index + 1]);
                let low = hex_value(bytes[index + 2]);
                if let (Some(high), Some(low)) = (high, low) {
                    output.push((high << 4) | low);
                    index += 3;
                } else {
                    output.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    Ok(output)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

const ICONV_ENCODING_MAX_LEN: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConversionTarget {
    encoding: &'static str,
    ignore: bool,
    transliterate: bool,
}

fn raw_encoding_arg(name: &str, value: &Value) -> Result<String, crate::builtins::BuiltinError> {
    Ok(string_arg(name, value)?.to_string_lossy())
}

fn checked_encoding_arg(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: &Value,
    span: &RuntimeSourceSpan,
) -> Result<Option<&'static str>, crate::builtins::BuiltinError> {
    let raw = raw_encoding_arg(name, value)?;
    if iconv_encoding_too_long(&raw) {
        warn_iconv_encoding_too_long(context, name, span);
        return Ok(None);
    }
    let Some(encoding) = canonical_encoding(&raw) else {
        warn_iconv_wrong_conversion(context, name, &raw, "UCS-4LE", span);
        return Ok(None);
    };
    Ok(Some(encoding))
}

fn encoding_arg_or_default(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: Option<&Value>,
    span: &RuntimeSourceSpan,
) -> Result<Option<&'static str>, crate::builtins::BuiltinError> {
    if let Some(value) = value {
        return checked_encoding_arg(context, name, value, span);
    }
    let raw = default_iconv_internal_encoding(context);
    let Some(encoding) = canonical_encoding(&raw) else {
        warn_iconv_wrong_conversion(context, name, &raw, "UCS-4LE", span);
        return Ok(None);
    };
    Ok(Some(encoding))
}

fn default_iconv_internal_encoding(context: &mut BuiltinContext<'_>) -> String {
    effective_iconv_encoding(context, "internal_encoding")
}

fn effective_iconv_encoding(context: &mut BuiltinContext<'_>, kind: &str) -> String {
    let state_value = match kind {
        "input_encoding" => context.iconv_state().input_encoding().to_owned(),
        "output_encoding" => context.iconv_state().output_encoding().to_owned(),
        "internal_encoding" => context.iconv_state().internal_encoding().to_owned(),
        _ => "UTF-8".to_owned(),
    };
    if !state_value.eq_ignore_ascii_case("UTF-8") {
        return state_value;
    }
    let legacy = context.ini_get(kind).unwrap_or_default();
    if !legacy.is_empty() {
        return legacy.to_owned();
    }
    let iconv_name = format!("iconv.{kind}");
    let iconv_value = context.ini_get(&iconv_name).unwrap_or_default();
    if !iconv_value.is_empty() {
        return iconv_value.to_owned();
    }
    context
        .ini_get("default_charset")
        .unwrap_or("UTF-8")
        .to_owned()
}

fn iconv_encoding_too_long(encoding: &str) -> bool {
    encoding.len() > ICONV_ENCODING_MAX_LEN
}

fn warn_iconv_encoding_too_long(
    context: &mut BuiltinContext<'_>,
    name: &str,
    span: &RuntimeSourceSpan,
) {
    context.php_warning(
        "E_PHP_RUNTIME_ICONV_ENCODING_TOO_LONG",
        format!(
            "{name}(): Encoding parameter exceeds the maximum allowed length of {ICONV_ENCODING_MAX_LEN} characters"
        ),
        span.clone(),
    );
}

fn warn_iconv_wrong_conversion(
    context: &mut BuiltinContext<'_>,
    name: &str,
    from: &str,
    to: &str,
    span: &RuntimeSourceSpan,
) {
    context.php_warning(
        "E_PHP_RUNTIME_ICONV_WRONG_ENCODING",
        format!("{name}(): Wrong encoding, conversion from \"{from}\" to \"{to}\" is not allowed"),
        span.clone(),
    );
}

fn conversion_base(encoding: &str) -> &str {
    encoding.split("//").next().unwrap_or(encoding)
}

fn parse_conversion_target(encoding: &str) -> Option<ConversionTarget> {
    let mut parts = encoding.split("//");
    let base = canonical_encoding(parts.next().unwrap_or_default())?;
    let mut target = ConversionTarget {
        encoding: base,
        ignore: false,
        transliterate: false,
    };
    for option in parts {
        match option.trim().to_ascii_uppercase().as_str() {
            "IGNORE" => target.ignore = true,
            "TRANSLIT" => target.transliterate = true,
            "" => {}
            _ => return None,
        }
    }
    Some(target)
}

fn canonical_encoding(encoding: &str) -> Option<&'static str> {
    let base = encoding.split("//").next().unwrap_or(encoding);
    match base.trim().to_ascii_uppercase().replace('_', "-").as_str() {
        "UTF-8" | "UTF8" => Some("UTF-8"),
        "ASCII" | "US-ASCII" | "ANSI-X3.4-1968" | "ANSI-X3.4-1986" | "ISO-IR-6" | "ISO646-US"
        | "US" | "IBM367" | "CP367" | "CSASCII" => Some("ASCII"),
        "ISO-8859-1" | "ISO8859-1" | "ISO-8859-1:1987" | "ISO-IR-100" | "LATIN1" | "LATIN-1"
        | "L1" | "IBM819" | "CP819" | "CSISOLATIN1" => Some("ISO-8859-1"),
        "ISO-8859-2" | "ISO8859-2" | "ISO-8859-2:1987" | "ISO-IR-101" | "LATIN2" | "LATIN-2"
        | "L2" | "CSISOLATIN2" => Some("ISO-8859-2"),
        "WINDOWS-1252" | "CP1252" => Some("Windows-1252"),
        "SJIS" | "SHIFT-JIS" | "SHIFT-JISX0213" | "SJIS-WIN" | "CP932" => Some("SJIS"),
        "EUC-JP" | "EUCJP" | "UJIS" => Some("EUC-JP"),
        "ISO-2022-JP" | "ISO2022-JP" | "JIS" => Some("ISO-2022-JP"),
        _ => None,
    }
}

fn convert_encoding(name: &str, input: &[u8], from: &str, to: ConversionTarget) -> BuiltinResult {
    let text = string_for_encoding(name, input, from, "#3 ($string)")?;
    match bytes_for_encoding(&text, to.encoding)
        .or_else(|| bytes_for_encoding_with_options(&text, to))
    {
        Some(bytes) => Ok(Value::string(bytes)),
        None => Ok(Value::Bool(false)),
    }
}

fn chars_for_encoding(
    name: &str,
    input: &[u8],
    encoding: &str,
    argument: &'static str,
) -> Result<Vec<char>, crate::builtins::BuiltinError> {
    Ok(string_for_encoding(name, input, encoding, argument)?
        .chars()
        .collect())
}

fn string_for_encoding(
    name: &str,
    input: &[u8],
    encoding: &str,
    argument: &'static str,
) -> Result<String, crate::builtins::BuiltinError> {
    match encoding {
        "UTF-8" => std::str::from_utf8(input)
            .map(str::to_owned)
            .map_err(|_| argument_value_error(name, argument, "must be valid UTF-8")),
        "ASCII" => {
            if input.is_ascii() {
                Ok(input.iter().map(|byte| char::from(*byte)).collect())
            } else {
                Err(argument_value_error(name, argument, "must be ASCII"))
            }
        }
        "ISO-8859-1" => Ok(input.iter().map(|byte| char::from(*byte)).collect()),
        "ISO-8859-2" | "Windows-1252" | "SJIS" | "EUC-JP" | "ISO-2022-JP" => {
            let encoding = encoding_rs_backend(encoding).expect("canonical encoding has backend");
            let (text, _encoding_used, had_errors) = encoding.decode(input);
            if had_errors {
                Err(argument_value_error(
                    name,
                    argument,
                    "must be valid for the selected encoding",
                ))
            } else {
                Ok(text.into_owned())
            }
        }
        _ => Err(argument_value_error(
            name,
            "encoding",
            "must be a supported encoding",
        )),
    }
}

fn bytes_for_encoding(text: &str, encoding: &str) -> Option<Vec<u8>> {
    match encoding {
        "UTF-8" => Some(text.as_bytes().to_vec()),
        "ASCII" => {
            if text.is_ascii() {
                Some(text.as_bytes().to_vec())
            } else {
                None
            }
        }
        "ISO-8859-1" => {
            let mut output = Vec::with_capacity(text.len());
            for ch in text.chars() {
                let code = ch as u32;
                if code > 0xff {
                    return None;
                }
                output.push(code as u8);
            }
            Some(output)
        }
        "ISO-8859-2" | "Windows-1252" | "SJIS" | "EUC-JP" | "ISO-2022-JP" => {
            let encoding = encoding_rs_backend(encoding).expect("canonical encoding has backend");
            let (bytes, _encoding_used, had_errors) = encoding.encode(text);
            if had_errors {
                None
            } else {
                Some(bytes.into_owned())
            }
        }
        _ => None,
    }
}

fn bytes_for_encoding_with_options(text: &str, target: ConversionTarget) -> Option<Vec<u8>> {
    if !target.ignore && !target.transliterate {
        return None;
    }
    let mut output = Vec::new();
    for ch in text.chars() {
        if let Some(bytes) = bytes_for_encoding(&ch.to_string(), target.encoding) {
            output.extend(bytes);
            continue;
        }
        if target.transliterate
            && let Some(replacement) = transliterate_char(ch)
            && let Some(bytes) = bytes_for_encoding(replacement, target.encoding)
        {
            output.extend(bytes);
            continue;
        }
        if target.ignore {
            continue;
        }
        return None;
    }
    Some(output)
}

fn transliterate_char(ch: char) -> Option<&'static str> {
    match ch as u32 {
        0x00a3 => Some("GBP"),
        0x00a5 => Some("YEN"),
        0x00a9 => Some("(C)"),
        0x00ae => Some("(R)"),
        0x00c0..=0x00c5 => Some("A"),
        0x00c6 => Some("AE"),
        0x00c7 => Some("C"),
        0x00c8..=0x00cb => Some("E"),
        0x00cc..=0x00cf => Some("I"),
        0x00d1 => Some("N"),
        0x00d2..=0x00d6 => Some("O"),
        0x00d8 => Some("O"),
        0x00d9..=0x00dc => Some("U"),
        0x00dd => Some("Y"),
        0x00df => Some("ss"),
        0x00e0..=0x00e5 => Some("a"),
        0x00e6 => Some("ae"),
        0x00e7 => Some("c"),
        0x00e8..=0x00eb => Some("e"),
        0x00ec..=0x00ef => Some("i"),
        0x00f1 => Some("n"),
        0x00f2..=0x00f6 => Some("o"),
        0x00f8 => Some("o"),
        0x00f9..=0x00fb => Some("u"),
        0x00fc => Some("\"u"),
        0x00fd | 0x00ff => Some("y"),
        0x0152 => Some("OE"),
        0x0153 => Some("oe"),
        0x20ac => Some("EUR"),
        0x2122 => Some("TM"),
        _ => None,
    }
}

fn encoding_rs_backend(encoding: &str) -> Option<&'static Encoding> {
    match encoding {
        "ISO-8859-2" => Some(ISO_8859_2),
        "Windows-1252" => Some(WINDOWS_1252),
        "SJIS" => Some(SHIFT_JIS),
        "EUC-JP" => Some(EUC_JP),
        "ISO-2022-JP" => Some(ISO_2022_JP),
        _ => None,
    }
}

fn normalize_offset(len: usize, offset: i64) -> usize {
    if offset < 0 {
        len.saturating_sub(offset.unsigned_abs() as usize)
    } else {
        (offset as usize).min(len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OutputBuffer, PhpString};

    fn call(name: &str, args: Vec<Value>) -> Value {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        ENTRIES
            .iter()
            .find(|entry| entry.name() == name)
            .expect("iconv entry")
            .function()(&mut context, args, RuntimeSourceSpan::default())
        .expect("iconv succeeds")
    }

    fn string(value: &str) -> Value {
        Value::String(PhpString::from_test_str(value))
    }

    fn bytes(value: &[u8]) -> Value {
        Value::string(value.to_vec())
    }

    fn options(pairs: &[(&str, &str)]) -> Value {
        let mut array = PhpArray::new();
        for (key, value) in pairs {
            array.insert(
                ArrayKey::String(PhpString::from_test_str(key)),
                Value::String(PhpString::from_test_str(value)),
            );
        }
        Value::Array(array)
    }

    #[test]
    fn common_ascii_and_latin1_aliases_are_canonicalized() {
        assert_eq!(
            call(
                "iconv",
                vec![string("CP819"), string("UTF-8"), Value::string(vec![0xE4]),],
            ),
            bytes(b"\xC3\xA4")
        );
        assert_eq!(
            call(
                "iconv",
                vec![string("ANSI_X3.4-1968"), string("UTF-8"), string("abc"),],
            ),
            string("abc")
        );
        assert_eq!(
            call("iconv_strlen", vec![string("abc"), string("CP367")]),
            Value::Int(3)
        );
    }

    #[test]
    fn substr_returns_empty_string_when_length_ends_before_start() {
        assert_eq!(
            call(
                "iconv_substr",
                vec![
                    string("foo"),
                    Value::Int(2),
                    Value::Int(-2),
                    string("UTF-8"),
                ],
            ),
            Value::string(Vec::new())
        );
    }

    #[test]
    fn strrpos_reports_last_character_position() {
        assert_eq!(
            call(
                "iconv_strrpos",
                vec![string("abecdbcdabcdef"), string("bcd")]
            ),
            Value::Int(9)
        );
        assert_eq!(
            call("iconv_strrpos", vec![string("string"), string("")]),
            Value::Bool(false)
        );
        assert_eq!(
            call("iconv_strrpos", vec![string(""), string("string")]),
            Value::Bool(false)
        );
    }

    #[test]
    fn selected_japanese_encodings_convert_and_search() {
        let euc_jp = Value::string(vec![0xC6, 0xFC, 0xCB, 0xDC, 0xC6, 0xFC, 0xCB, 0xDC]);
        assert_eq!(
            call(
                "iconv_strlen",
                vec![
                    euc_jp.clone(),
                    Value::String(PhpString::from_test_str("EUC-JP"))
                ],
            ),
            Value::Int(4)
        );
        assert_eq!(
            call(
                "iconv_strrpos",
                vec![
                    euc_jp.clone(),
                    Value::string(vec![0xCB, 0xDC]),
                    Value::String(PhpString::from_test_str("EUC-JP")),
                ],
            ),
            Value::Int(3)
        );
        let iso_2022_jp = call(
            "iconv",
            vec![
                Value::String(PhpString::from_test_str("EUC-JP")),
                Value::String(PhpString::from_test_str("ISO-2022-JP")),
                euc_jp,
            ],
        );
        assert_eq!(
            call(
                "iconv_strlen",
                vec![
                    iso_2022_jp,
                    Value::String(PhpString::from_test_str("ISO-2022-JP")),
                ],
            ),
            Value::Int(4)
        );
    }

    #[test]
    fn conversion_options_ignore_or_transliterate_unencodable_chars() {
        assert_eq!(
            call(
                "iconv",
                vec![
                    string("UTF-8"),
                    string("ASCII//IGNORE"),
                    bytes(b"Pr\xC3\xBCfung \xE2\x82\xAC"),
                ],
            ),
            bytes(b"Prfung ")
        );
        assert_eq!(
            call(
                "iconv",
                vec![
                    string("UTF-8"),
                    string("ASCII//TRANSLIT"),
                    bytes(b"Pr\xC3\xBCfung \xE2\x82\xAC"),
                ],
            ),
            bytes(b"Pr\"ufung EUR")
        );
        assert_eq!(
            call(
                "iconv",
                vec![
                    string("UTF-8"),
                    string("ISO-8859-1//IGNORE"),
                    bytes(b"Price \xE2\x82\xAC"),
                ],
            ),
            bytes(b"Price ")
        );
        assert_eq!(
            call(
                "iconv",
                vec![
                    string("UTF-8"),
                    string("ISO-8859-1//TRANSLIT"),
                    bytes(b"Price \xE2\x82\xAC"),
                ],
            ),
            bytes(b"Price EUR")
        );
    }

    #[test]
    fn mime_encode_supports_basic_b_and_q_words() {
        assert_eq!(
            call(
                "iconv_mime_encode",
                vec![string("Subject"), string("hello")]
            ),
            string("Subject: =?UTF-8?B?aGVsbG8=?=")
        );
        assert_eq!(
            call(
                "iconv_mime_encode",
                vec![
                    string("Subject"),
                    bytes(b"Pr\xC3\xBCfung"),
                    options(&[
                        ("input-charset", "UTF-8"),
                        ("output-charset", "UTF-8"),
                        ("scheme", "B"),
                    ]),
                ],
            ),
            string("Subject: =?UTF-8?B?UHLDvGZ1bmc=?=")
        );
        assert_eq!(
            call(
                "iconv_mime_encode",
                vec![
                    string("Subject"),
                    bytes(b"Pr\xC3\xBCfung"),
                    options(&[
                        ("input-charset", "UTF-8"),
                        ("output-charset", "UTF-8"),
                        ("scheme", "Q"),
                    ]),
                ],
            ),
            string("Subject: =?UTF-8?Q?Pr=C3=BCfung?=")
        );
    }

    #[test]
    fn mime_decode_replaces_selected_encoded_words() {
        assert_eq!(
            call(
                "iconv_mime_decode",
                vec![string("=?utf-8?B?UHLDvGZ1bmc=?=")]
            ),
            bytes(b"Pr\xC3\xBCfung")
        );
        assert_eq!(
            call(
                "iconv_mime_decode",
                vec![
                    string("Subject: =?utf-8?Q?Pr=C3=BCfung?="),
                    Value::Int(0),
                    string("UTF-8"),
                ],
            ),
            bytes(b"Subject: Pr\xC3\xBCfung")
        );
        assert_eq!(
            call("iconv_mime_decode", vec![string("=?UTF-8?Q?hello_world?=")]),
            string("hello world")
        );
    }

    #[test]
    fn mime_decode_handles_language_suffixes_and_encoded_word_whitespace() {
        assert_eq!(
            call(
                "iconv_mime_decode",
                vec![
                    string(
                        "Subject: =?ISO-8859-1?Q?Pr=FCfung?=\n    =?ISO-8859-1*de_DE?Q?Pr=FCfung?=\t\n     =?ISO-8859-2?Q?k=F9=D4=F1=D3let?="
                    ),
                    Value::Int(0),
                    string("UTF-8"),
                ],
            ),
            bytes(b"Subject: Pr\xC3\xBCfungPr\xC3\xBCfungk\xC5\xAF\xC3\x94\xC5\x84\xC3\x93let")
        );
        assert_eq!(
            call(
                "iconv_mime_decode",
                vec![
                    string(
                        "Subject: =?ISO-8859-1?Q?Pr=FCfung?= =?ISO-8859-1*de_DE?Q?=20Pr=FCfung?= \t  =?ISO-8859-2?Q?k=F9=D4=F1=D3let?="
                    ),
                    Value::Int(0),
                    string("UTF-8"),
                ],
            ),
            bytes(b"Subject: Pr\xC3\xBCfung Pr\xC3\xBCfungk\xC5\xAF\xC3\x94\xC5\x84\xC3\x93let")
        );
        assert_eq!(
            call(
                "iconv_mime_decode",
                vec![
                    string(
                        "Subject: =?ISO-8859-1?Q?Pr=FCfung?==?ISO-8859-1*de_DE?Q?Pr=FCfung?==?ISO-8859-2?Q?k=F9=D4=F1=D3let?="
                    ),
                    Value::Int(ICONV_MIME_DECODE_STRICT),
                    string("UTF-8"),
                ],
            ),
            string(
                "Subject: =?ISO-8859-1?Q?Pr=FCfung?==?ISO-8859-1*de_DE?Q?Pr=FCfung?==?ISO-8859-2?Q?k=F9=D4=F1=D3let?="
            )
        );
        assert_eq!(
            call(
                "iconv_mime_decode",
                vec![
                    string("Subject: =?ISO-8859-1?Q?Pr=FCfung?= =?ISO-8859-1*de_DE?Q?Pr=FCfung??   =?ISO-8859-2?X?k=F9=D4=F1=D3let?="),
                    Value::Int(ICONV_MIME_DECODE_CONTINUE_ON_ERROR),
                    string("UTF-8"),
                ],
            ),
            bytes(b"Subject: Pr\xC3\xBCfung=?ISO-8859-1*de_DE?Q?Pr=FCfung??   =?ISO-8859-2?X?k=F9=D4=F1=D3let?=")
        );
    }

    #[test]
    fn mime_decode_headers_decodes_values_and_accumulates_duplicates() {
        let result = call(
            "iconv_mime_decode_headers",
            vec![
                string(
                    "Subject: =?utf-8?B?UHLDvGZ1bmc=?=\r\nFrom: Alice <a@example.com>\r\nX-Test: =?UTF-8?Q?hello_world?=\r\n",
                ),
                Value::Int(0),
                string("UTF-8"),
            ],
        );
        let Value::Array(headers) = result else {
            panic!("headers should be array");
        };
        assert_eq!(
            headers.get(&ArrayKey::String(PhpString::from_test_str("Subject"))),
            Some(&bytes(b"Pr\xC3\xBCfung"))
        );
        assert_eq!(
            headers.get(&ArrayKey::String(PhpString::from_test_str("From"))),
            Some(&string("Alice <a@example.com>"))
        );
        assert_eq!(
            headers.get(&ArrayKey::String(PhpString::from_test_str("X-Test"))),
            Some(&string("hello world"))
        );

        let result = call(
            "iconv_mime_decode_headers",
            vec![
                string("Subject: one\r\nSubject: two\r\n"),
                Value::Int(0),
                string("UTF-8"),
            ],
        );
        let Value::Array(headers) = result else {
            panic!("headers should be array");
        };
        assert_eq!(
            headers.get(&ArrayKey::String(PhpString::from_test_str("Subject"))),
            Some(&Value::Array(PhpArray::from_packed(vec![
                string("one"),
                string("two"),
            ])))
        );
    }
}
