//! Strings builtin registry slice.

use super::core::*;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{ArrayKey, PhpArray, PhpString, RuntimeIniOptions, Value, to_bool};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use md5::{Digest, Md5};
use php_lexer::{LexerConfig, SymbolKind, TokenKind, TokenName, lex_all};
use sha1::Sha1;

fn pack_u16_bytes(code: u8, value: i64) -> [u8; 2] {
    match code {
        b'n' => (value as u16).to_be_bytes(),
        b'v' => (value as u16).to_le_bytes(),
        _ => unreachable!("checked pack format"),
    }
}

fn unpack_u16_value(code: u8, bytes: &[u8]) -> i64 {
    let [first, second] = bytes else {
        unreachable!("checked unpack width");
    };
    let bytes = [*first, *second];
    match code {
        b'n' => i64::from(u16::from_be_bytes(bytes)),
        b'v' => i64::from(u16::from_le_bytes(bytes)),
        _ => unreachable!("checked unpack format"),
    }
}

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "base64_decode",
        builtin_base64_decode,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "base64_encode",
        builtin_base64_encode,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("bin2hex", builtin_bin2hex, BuiltinCompatibility::Php),
    BuiltinEntry::new("chr", builtin_chr, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "addcslashes",
        builtin_addcslashes,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("addslashes", builtin_addslashes, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "convert_uudecode",
        builtin_convert_uudecode,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "convert_uuencode",
        builtin_convert_uuencode,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("crc32", builtin_crc32, BuiltinCompatibility::Php),
    BuiltinEntry::new("explode", builtin_explode, BuiltinCompatibility::Php),
    BuiltinEntry::new("hex2bin", builtin_hex2bin, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "quoted_printable_decode",
        builtin_quoted_printable_decode,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "highlight_string",
        builtin_highlight_string,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "html_entity_decode",
        builtin_html_entity_decode,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "get_html_translation_table",
        builtin_get_html_translation_table,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "htmlentities",
        builtin_htmlentities,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "htmlspecialchars",
        builtin_htmlspecialchars,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "htmlspecialchars_decode",
        builtin_htmlspecialchars_decode,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "http_build_query",
        builtin_http_build_query,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("implode", builtin_implode, BuiltinCompatibility::Php),
    BuiltinEntry::new("join", builtin_implode, BuiltinCompatibility::Php),
    BuiltinEntry::new("lcfirst", builtin_lcfirst, BuiltinCompatibility::Php),
    BuiltinEntry::new("ltrim", builtin_ltrim, BuiltinCompatibility::Php),
    BuiltinEntry::new("md5", builtin_md5, BuiltinCompatibility::Php),
    BuiltinEntry::new("ord", builtin_ord, BuiltinCompatibility::Php),
    BuiltinEntry::new("pack", builtin_pack, BuiltinCompatibility::Php),
    BuiltinEntry::new("parse_str", builtin_parse_str, BuiltinCompatibility::Php),
    BuiltinEntry::new("parse_url", builtin_parse_url, BuiltinCompatibility::Php),
    BuiltinEntry::new("printf", exact_printf, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "rawurldecode",
        builtin_rawurldecode,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "rawurlencode",
        builtin_rawurlencode,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("rtrim", builtin_rtrim, BuiltinCompatibility::Php),
    BuiltinEntry::new("sha1", builtin_sha1, BuiltinCompatibility::Php),
    BuiltinEntry::new("sprintf", exact_sprintf, BuiltinCompatibility::Php),
    BuiltinEntry::new("substr", builtin_substr, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "str_contains",
        builtin_str_contains,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "str_ends_with",
        builtin_str_ends_with,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("str_pad", builtin_str_pad, BuiltinCompatibility::Php),
    BuiltinEntry::new("str_repeat", builtin_str_repeat, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "str_replace",
        builtin_str_replace,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("str_split", builtin_str_split, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "str_starts_with",
        builtin_str_starts_with,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("strcasecmp", builtin_strcasecmp, BuiltinCompatibility::Php),
    BuiltinEntry::new("strcmp", builtin_strcmp, BuiltinCompatibility::Php),
    BuiltinEntry::new("strcspn", builtin_strcspn, BuiltinCompatibility::Php),
    BuiltinEntry::new("stripos", builtin_stripos, BuiltinCompatibility::Php),
    BuiltinEntry::new("strip_tags", builtin_strip_tags, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "stripcslashes",
        builtin_stripcslashes,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stripslashes",
        builtin_stripslashes,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("stristr", builtin_stristr, BuiltinCompatibility::Php),
    BuiltinEntry::new("strlen", builtin_strlen, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "strnatcasecmp",
        builtin_strnatcasecmp,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("strnatcmp", builtin_strnatcmp, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "strncasecmp",
        builtin_strncasecmp,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("strncmp", builtin_strncmp, BuiltinCompatibility::Php),
    BuiltinEntry::new("quotemeta", builtin_quotemeta, BuiltinCompatibility::Php),
    BuiltinEntry::new("strpbrk", builtin_strpbrk, BuiltinCompatibility::Php),
    BuiltinEntry::new("strpos", builtin_strpos, BuiltinCompatibility::Php),
    BuiltinEntry::new("strrev", builtin_strrev, BuiltinCompatibility::Php),
    BuiltinEntry::new("strrchr", builtin_strrchr, BuiltinCompatibility::Php),
    BuiltinEntry::new("strripos", builtin_strripos, BuiltinCompatibility::Php),
    BuiltinEntry::new("strrpos", builtin_strrpos, BuiltinCompatibility::Php),
    BuiltinEntry::new("strspn", builtin_strspn, BuiltinCompatibility::Php),
    BuiltinEntry::new("strstr", builtin_strstr, BuiltinCompatibility::Php),
    BuiltinEntry::new("strtok", builtin_strtok, BuiltinCompatibility::Php),
    BuiltinEntry::new("strtolower", builtin_strtolower, BuiltinCompatibility::Php),
    BuiltinEntry::new("strtoupper", builtin_strtoupper, BuiltinCompatibility::Php),
    BuiltinEntry::new("strtr", builtin_strtr, BuiltinCompatibility::Php),
    BuiltinEntry::new("strval", builtin_strval, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "substr_compare",
        builtin_substr_compare,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "substr_count",
        builtin_substr_count,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "substr_replace",
        builtin_substr_replace,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("trim", builtin_trim, BuiltinCompatibility::Php),
    BuiltinEntry::new("ucfirst", builtin_ucfirst, BuiltinCompatibility::Php),
    BuiltinEntry::new("ucwords", builtin_ucwords, BuiltinCompatibility::Php),
    BuiltinEntry::new("unpack", builtin_unpack, BuiltinCompatibility::Php),
    BuiltinEntry::new("urldecode", builtin_urldecode, BuiltinCompatibility::Php),
    BuiltinEntry::new("urlencode", builtin_urlencode, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "version_compare",
        builtin_version_compare,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("vprintf", exact_vprintf, BuiltinCompatibility::Php),
    BuiltinEntry::new("vsprintf", exact_vsprintf, BuiltinCompatibility::Php),
    BuiltinEntry::new("wordwrap", builtin_wordwrap, BuiltinCompatibility::Php),
];

pub(in crate::builtins::modules) fn builtin_strlen(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("strlen", &args, 1)?;
    let value = string_arg("strlen", &args[0])?;
    Ok(Value::Int(value.len() as i64))
}

pub(in crate::builtins::modules) fn builtin_highlight_string(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("highlight_string", "one or two argument(s)"));
    }
    let source = string_arg("highlight_string", &args[0])?.to_string_lossy();
    let should_return = args
        .get(1)
        .map_or(Ok(false), to_bool)
        .map_err(|message| conversion_error("highlight_string", message))?;
    let rendered = highlight_php_source(context, &source);
    if should_return {
        Ok(Value::string(rendered))
    } else {
        context.output().write_bytes(rendered.as_bytes());
        Ok(Value::Bool(true))
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum HighlightClass {
    Html,
    Default,
    Keyword,
    String,
    Comment,
}

struct HighlightColors {
    html: String,
    default: String,
    keyword: String,
    string: String,
    comment: String,
}

fn highlight_php_source(context: &BuiltinContext<'_>, source: &str) -> String {
    let colors = HighlightColors {
        html: highlight_color(context, "highlight.html", "#000000"),
        default: highlight_color(context, "highlight.default", "#0000BB"),
        keyword: highlight_color(context, "highlight.keyword", "#007700"),
        string: highlight_color(context, "highlight.string", "#DD0000"),
        comment: highlight_color(context, "highlight.comment", "#FF9900"),
    };
    let lexed = lex_all(source, LexerConfig::default());
    let mut output = String::new();
    output.push_str("<pre><code style=\"color: ");
    output.push_str(&colors.html);
    output.push_str("\">");

    let mut active_class: Option<HighlightClass> = None;
    let mut in_encapsed_string = false;
    for (index, token) in lexed.tokens.iter().enumerate() {
        let Some(text) = token.text(source) else {
            continue;
        };
        let next_kind = lexed.tokens.get(index + 1).map(|next| next.kind);
        let mut class = highlight_class_for_token(token.kind, text, active_class, next_kind);
        if is_double_quote_symbol(token.kind, text)
            && (in_encapsed_string || starts_encapsed_string(next_kind))
        {
            class = HighlightClass::String;
            in_encapsed_string = !in_encapsed_string;
        }
        append_highlighted_text(&mut output, &mut active_class, &colors, class, text);
    }

    close_highlight_span(&mut output, &mut active_class);
    output.push_str("</code></pre>");
    output
}

fn highlight_color(context: &BuiltinContext<'_>, name: &str, default: &str) -> String {
    context.ini_get(name).unwrap_or(default).to_owned()
}

fn highlight_class_for_token(
    kind: TokenKind,
    text: &str,
    active_class: Option<HighlightClass>,
    next_kind: Option<TokenKind>,
) -> HighlightClass {
    match kind {
        TokenKind::Named(TokenName::InlineHtml) => HighlightClass::Html,
        TokenKind::Named(TokenName::OpenTag | TokenName::OpenTagWithEcho | TokenName::CloseTag) => {
            HighlightClass::Default
        }
        TokenKind::Named(TokenName::Variable) => HighlightClass::Default,
        TokenKind::Named(TokenName::ConstantEncapsedString | TokenName::EncapsedAndWhitespace) => {
            HighlightClass::String
        }
        TokenKind::Named(TokenName::Comment | TokenName::DocComment) => HighlightClass::Comment,
        TokenKind::Named(TokenName::Whitespace) => active_class
            .filter(|class| *class != HighlightClass::Html)
            .unwrap_or(HighlightClass::Keyword),
        _ if is_double_quote_symbol(kind, text) && starts_encapsed_string(next_kind) => {
            HighlightClass::String
        }
        _ => HighlightClass::Keyword,
    }
}

fn append_highlighted_text(
    output: &mut String,
    active_class: &mut Option<HighlightClass>,
    colors: &HighlightColors,
    class: HighlightClass,
    text: &str,
) {
    if class == HighlightClass::Html {
        close_highlight_span(output, active_class);
    } else if *active_class != Some(class) {
        close_highlight_span(output, active_class);
        output.push_str("<span style=\"color: ");
        output.push_str(color_for_class(colors, class));
        output.push_str("\">");
        *active_class = Some(class);
    }
    push_highlight_escaped(output, text);
}

fn close_highlight_span(output: &mut String, active_class: &mut Option<HighlightClass>) {
    if active_class.take().is_some() {
        output.push_str("</span>");
    }
}

fn color_for_class(colors: &HighlightColors, class: HighlightClass) -> &str {
    match class {
        HighlightClass::Html => &colors.html,
        HighlightClass::Default => &colors.default,
        HighlightClass::Keyword => &colors.keyword,
        HighlightClass::String => &colors.string,
        HighlightClass::Comment => &colors.comment,
    }
}

fn is_double_quote_symbol(kind: TokenKind, text: &str) -> bool {
    kind == TokenKind::Symbol(SymbolKind::Char(b'"')) && text == "\""
}

fn starts_encapsed_string(kind: Option<TokenKind>) -> bool {
    matches!(
        kind,
        Some(TokenKind::Named(
            TokenName::EncapsedAndWhitespace | TokenName::Variable
        ))
    )
}

fn push_highlight_escaped(output: &mut String, text: &str) {
    for byte in text.bytes() {
        match byte {
            b'&' => output.push_str("&amp;"),
            b'<' => output.push_str("&lt;"),
            b'>' => output.push_str("&gt;"),
            _ => output.push(char::from(byte)),
        }
    }
}

pub(in crate::builtins::modules) fn builtin_strtoupper(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("strtoupper", &args, 1)?;
    Ok(Value::String(super::string_intrinsics::strtoupper_ascii(
        &string_arg("strtoupper", &args[0])?,
    )))
}

pub(in crate::builtins::modules) fn builtin_trim(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    trim_builtin(context, "trim", args, true, true, span)
}

pub(in crate::builtins::modules) fn builtin_ltrim(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    trim_builtin(context, "ltrim", args, true, false, span)
}

pub(in crate::builtins::modules) fn builtin_rtrim(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    trim_builtin(context, "rtrim", args, false, true, span)
}

pub(in crate::builtins::modules) fn builtin_explode(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin explode expects two or three argument(s)",
        ));
    }
    let separator = string_arg("explode", &args[0])?;
    if separator.is_empty() {
        return Err(argument_value_error(
            "explode",
            "#1 ($separator)",
            "must not be empty",
        ));
    }
    let string = string_arg("explode", &args[1])?;
    if args.len() == 2 && separator.len() == 1 {
        return Ok(Value::Array(super::string_intrinsics::explode_single_byte(
            separator.as_bytes()[0],
            &string,
        )));
    }
    let limit = args
        .get(2)
        .map(|value| int_arg("explode", value))
        .transpose()?;
    let mut parts = split_bytes(string.as_bytes(), separator.as_bytes());
    match limit {
        Some(0) => parts.truncate(1),
        Some(limit) if limit > 0 => {
            parts = split_bytes_limited(string.as_bytes(), separator.as_bytes(), limit as usize)
        }
        Some(limit) if limit < 0 => {
            let drop = limit.unsigned_abs() as usize;
            if drop >= parts.len() {
                parts.clear();
            } else {
                parts.truncate(parts.len() - drop);
            }
        }
        _ => {}
    }
    Ok(Value::Array(crate::PhpArray::from_packed(
        parts.into_iter().map(Value::string).collect(),
    )))
}

pub(in crate::builtins::modules) fn builtin_implode(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin implode expects one or two argument(s)",
        ));
    }
    let (separator, array) = if args.len() == 1 || matches!(deref_value(&args[0]), Value::Array(_))
    {
        (
            crate::PhpString::from_bytes(Vec::new()),
            array_value_arg("implode", &args[0])?,
        )
    } else {
        (
            string_arg("implode", &args[0])?,
            array_value_arg("implode", &args[1])?,
        )
    };
    let mut output = Vec::new();
    for (index, (_, value)) in array.iter().enumerate() {
        if index > 0 {
            output.extend_from_slice(separator.as_bytes());
        }
        let string = string_cast_value(context, value, span.clone()).map_err(|message| {
            BuiltinError::new(
                "E_PHP_RUNTIME_BUILTIN_TYPE",
                format!("builtin implode expects string-compatible value: {message}"),
            )
        })?;
        output.extend_from_slice(string.as_bytes());
    }
    Ok(Value::string(output))
}

pub(in crate::builtins::modules) fn builtin_str_replace(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=4).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin str_replace expects three or four argument(s)",
        ));
    }
    let search = string_list_arg("str_replace", &args[0])?;
    let replace = string_list_arg("str_replace", &args[1])?;
    let repeat_single_replacement = !matches!(deref_value(&args[1]), Value::Array(_));
    let mut count = 0_i64;
    let result = replace_subject(
        &args[2],
        &search,
        &replace,
        repeat_single_replacement,
        &mut count,
    )?;
    if let Some(Value::Reference(cell)) = args.get(3) {
        cell.set(Value::Int(count));
    }
    Ok(result)
}

pub(in crate::builtins::modules) fn builtin_strtr(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() == 2 {
        let mut subject = string_arg("strtr", &args[0])?.into_bytes();
        let Value::Array(map) = deref_value(&args[1]) else {
            return Err(strtr_argument_type_error("#2 ($from)", "array", &args[1]));
        };
        let mut replacements = Vec::new();
        for (key, value) in map.iter() {
            let key = match key {
                ArrayKey::Int(index) => index.to_string().into_bytes(),
                ArrayKey::String(key) => key.as_bytes().to_vec(),
            };
            if key.is_empty() {
                if !subject.is_empty() {
                    context.php_warning(
                        "E_PHP_RUNTIME_STRTR_EMPTY_SEARCH",
                        "strtr(): Ignoring replacement of empty string",
                        span.clone(),
                    );
                }
                continue;
            }
            replacements.push((key, string_arg("strtr", value)?.into_bytes()));
        }
        replacements.sort_by_key(|(key, _)| std::cmp::Reverse(key.len()));
        subject = native_strtr_map(&subject, &replacements);
        return Ok(Value::string(subject));
    }
    expect_arity("strtr", &args, 3)?;
    let subject = string_arg("strtr", &args[0])?;
    let from = strtr_string_arg(
        context,
        &args[1],
        "#2 ($from)",
        "array|string",
        span.clone(),
    )?;
    let to = strtr_string_arg(context, &args[2], "#3 ($to)", "string", span)?;
    Ok(Value::string(baseline_strtr(
        subject.as_bytes(),
        from.as_bytes(),
        to.as_bytes(),
    )))
}

pub fn native_strtr_into(subject: &[u8], from: &[u8], to: &[u8], output: &mut [u8]) -> bool {
    if output.len() != subject.len() {
        return false;
    }
    for (byte, output) in subject.iter().copied().zip(output.iter_mut()) {
        *output = if let Some(index) = from.iter().take(to.len()).rposition(|from| *from == byte)
            && let Some(replacement) = to.get(index)
        {
            *replacement
        } else {
            byte
        };
    }
    true
}

fn baseline_strtr(subject: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut output = vec![0; subject.len()];
    debug_assert!(native_strtr_into(subject, from, to, &mut output));
    output
}

/// Applies PHP's longest-key-first replacement-map form of `strtr` to
/// authoritative native byte pairs.
pub fn native_strtr_map<K, V>(subject: &[u8], replacements: &[(K, V)]) -> Vec<u8>
where
    K: AsRef<[u8]>,
    V: AsRef<[u8]>,
{
    replace_map(subject, replacements)
}

pub(in crate::builtins::modules) fn builtin_strip_tags(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error("strip_tags", "one or two argument(s)"));
    }
    let input = string_arg("strip_tags", &args[0])?;
    let allowed = args.get(1).map(allowed_strip_tags_arg).transpose()?;
    Ok(Value::string(baseline_strip_tags(
        input.as_bytes(),
        allowed.as_deref(),
    )))
}

/// Removes PHP/HTML tags from native bytes. Allowed-tag syntax is normalized
/// to ASCII lowercase before the shared scanner consumes it.
fn baseline_strip_tags(input: &[u8], allowed: Option<&[u8]>) -> Vec<u8> {
    let allowed = allowed.map(lower_ascii_bytes);
    strip_tags_bytes(input, allowed.as_deref())
}

pub(in crate::builtins::modules) fn builtin_strtok(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("strtok", "one or two argument(s)"));
    }
    if args.len() == 1 {
        let Some(state) = context.strtok_state() else {
            return Ok(Value::Bool(false));
        };
        if state.requires_input() {
            context.php_warning(
                "E_PHP_RUNTIME_STRTOK_MISSING_INPUT",
                "strtok(): Both arguments must be provided when starting tokenization",
                span,
            );
            return Ok(Value::Bool(false));
        }
    }
    let Some(state) = context.strtok_state() else {
        return Ok(Value::Bool(false));
    };
    let delimiters = if args.len() == 2 {
        let input = string_arg("strtok", &args[0])?;
        state.reset(input.into_bytes());
        string_arg("strtok", &args[1])?
    } else {
        string_arg("strtok", &args[0])?
    };
    Ok(state
        .next_token(delimiters.as_bytes())
        .map_or(Value::Bool(false), Value::string))
}

pub(in crate::builtins::modules) fn builtin_strtolower(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("strtolower", &args, 1)?;
    Ok(Value::String(super::string_intrinsics::strtolower_ascii(
        &string_arg("strtolower", &args[0])?,
    )))
}

pub(in crate::builtins::modules) fn builtin_ucfirst(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("ucfirst", &args, 1)?;
    Ok(Value::string(change_first_ascii(
        string_arg("ucfirst", &args[0])?,
        true,
    )))
}

pub(in crate::builtins::modules) fn builtin_lcfirst(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("lcfirst", &args, 1)?;
    Ok(Value::string(change_first_ascii(
        string_arg("lcfirst", &args[0])?,
        false,
    )))
}

pub(in crate::builtins::modules) fn builtin_ucwords(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin ucwords expects one or two argument(s)",
        ));
    }
    let input = string_arg("ucwords", &args[0])?;
    let delimiters = args
        .get(1)
        .map(|value| string_arg("ucwords", value))
        .transpose()?;
    Ok(Value::string(baseline_ucwords(
        input.as_bytes(),
        delimiters.as_ref().map(crate::PhpString::as_bytes),
    )))
}

pub fn native_ucwords_into(input: &[u8], delimiters: Option<&[u8]>, output: &mut [u8]) -> bool {
    if output.len() != input.len() {
        return false;
    }
    let delimiters = delimiters.unwrap_or(b" \t\r\n\x0c\x0b");
    let mut at_word_start = true;
    for (byte, output) in input.iter().copied().zip(output.iter_mut()) {
        if delimiters.contains(&byte) {
            *output = byte;
            at_word_start = true;
        } else if at_word_start {
            *output = byte.to_ascii_uppercase();
            at_word_start = false;
        } else {
            *output = byte;
        }
    }
    true
}

fn baseline_ucwords(input: &[u8], delimiters: Option<&[u8]>) -> Vec<u8> {
    let mut output = vec![0; input.len()];
    debug_assert!(native_ucwords_into(input, delimiters, &mut output));
    output
}

pub(in crate::builtins::modules) fn builtin_str_repeat(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("str_repeat", &args, 2)?;
    let string = string_arg("str_repeat", &args[0])?;
    let count = int_arg("str_repeat", &args[1])?;
    if count < 0 {
        return Err(value_error(
            "str_repeat",
            "count must be greater than or equal to 0",
        ));
    }
    Ok(Value::string(string.as_bytes().repeat(count as usize)))
}

pub(in crate::builtins::modules) fn builtin_str_split(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error("str_split", "one or two argument(s)"));
    }
    let string = string_arg("str_split", &args[0])?;
    let length = args
        .get(1)
        .map(|value| int_arg("str_split", value))
        .transpose()?
        .unwrap_or(1);
    if length <= 0 {
        return Err(argument_value_error(
            "str_split",
            "#2 ($length)",
            "must be greater than 0",
        ));
    }
    Ok(Value::Array(PhpArray::from_packed(
        string
            .as_bytes()
            .chunks(length as usize)
            .map(Value::string)
            .collect(),
    )))
}

pub(in crate::builtins::modules) fn builtin_str_pad(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin str_pad expects two to four argument(s)",
        ));
    }
    let input = string_arg("str_pad", &args[0])?;
    let length = int_arg("str_pad", &args[1])?;
    if length < 0 {
        return Err(value_error(
            "str_pad",
            "length must be greater than or equal to 0",
        ));
    }
    let pad = args
        .get(2)
        .map(|value| string_arg("str_pad", value))
        .transpose()?
        .unwrap_or_else(|| crate::PhpString::from_test_str(" "));
    if pad.is_empty() {
        return Err(value_error("str_pad", "pad string cannot be empty"));
    }
    let pad_type = args
        .get(3)
        .map(|value| int_arg("str_pad", value))
        .transpose()?
        .unwrap_or(1);
    Ok(Value::string(baseline_str_pad(
        input.as_bytes(),
        length as usize,
        pad.as_bytes(),
        pad_type,
    )))
}

pub fn native_str_pad_output_length(input: &[u8], target: usize, pad: &[u8]) -> Option<usize> {
    if pad.is_empty() {
        return None;
    }
    Some(input.len().max(target))
}

pub fn native_str_pad_into(
    input: &[u8],
    target: usize,
    pad: &[u8],
    pad_type: i64,
    output: &mut [u8],
) -> bool {
    if native_str_pad_output_length(input, target, pad) != Some(output.len()) {
        return false;
    }
    if input.len() >= target {
        output.copy_from_slice(input);
        return true;
    }
    let needed = target - input.len();
    let (left, right) = match pad_type {
        0 => (needed, 0),
        2 => (needed / 2, needed - (needed / 2)),
        _ => (0, needed),
    };
    for (index, slot) in output[..left].iter_mut().enumerate() {
        *slot = pad[index % pad.len()];
    }
    output[left..left + input.len()].copy_from_slice(input);
    for (index, slot) in output[left + input.len()..left + input.len() + right]
        .iter_mut()
        .enumerate()
    {
        *slot = pad[index % pad.len()];
    }
    true
}

fn baseline_str_pad(input: &[u8], target: usize, pad: &[u8], pad_type: i64) -> Vec<u8> {
    let Some(length) = native_str_pad_output_length(input, target, pad) else {
        return Vec::new();
    };
    let mut output = vec![0; length];
    debug_assert!(native_str_pad_into(
        input,
        target,
        pad,
        pad_type,
        &mut output
    ));
    output
}

pub(in crate::builtins::modules) fn builtin_strrev(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("strrev", &args, 1)?;
    let mut bytes = string_arg("strrev", &args[0])?.into_bytes();
    bytes.reverse();
    Ok(Value::string(bytes))
}

pub(in crate::builtins::modules) fn builtin_quotemeta(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("quotemeta", &args, 1)?;
    let input = string_arg("quotemeta", &args[0])?;
    Ok(Value::string(baseline_quotemeta(input.as_bytes())))
}

fn is_quotemeta_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'.' | b'\\' | b'+' | b'*' | b'?' | b'[' | b'^' | b']' | b'$' | b'(' | b')'
    )
}

/// Exact output length for a `quotemeta()` native string.
pub fn native_quotemeta_output_length(input: &[u8]) -> Option<usize> {
    input.len().checked_add(
        input
            .iter()
            .filter(|byte| is_quotemeta_byte(**byte))
            .count(),
    )
}

/// Writes `quotemeta()` bytes directly into an already reserved native range.
pub fn native_quotemeta_into(input: &[u8], output: &mut [u8]) -> bool {
    if native_quotemeta_output_length(input) != Some(output.len()) {
        return false;
    }
    let mut cursor = 0;
    for &byte in input {
        if is_quotemeta_byte(byte) {
            output[cursor] = b'\\';
            cursor += 1;
        }
        output[cursor] = byte;
        cursor += 1;
    }
    true
}

fn baseline_quotemeta(input: &[u8]) -> Vec<u8> {
    let Some(length) = native_quotemeta_output_length(input) else {
        return Vec::new();
    };
    let mut output = vec![0; length];
    debug_assert!(native_quotemeta_into(input, &mut output));
    output
}

pub(in crate::builtins::modules) fn builtin_bin2hex(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("bin2hex", &args, 1)?;
    Ok(Value::string(baseline_bin2hex(
        string_arg("bin2hex", &args[0])?.as_bytes(),
    )))
}

/// Exact output length for hexadecimal encoding.
pub fn native_bin2hex_output_length(input: &[u8]) -> Option<usize> {
    input.len().checked_mul(2)
}

/// Writes lower-case hexadecimal bytes directly into a native string range.
pub fn native_bin2hex_into(input: &[u8], output: &mut [u8]) -> bool {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    if native_bin2hex_output_length(input) != Some(output.len()) {
        return false;
    }
    for (byte, pair) in input.iter().zip(output.chunks_exact_mut(2)) {
        pair[0] = HEX[(byte >> 4) as usize];
        pair[1] = HEX[(byte & 0x0f) as usize];
    }
    true
}

fn baseline_bin2hex(input: &[u8]) -> Vec<u8> {
    let Some(length) = native_bin2hex_output_length(input) else {
        return Vec::new();
    };
    let mut output = vec![0; length];
    debug_assert!(native_bin2hex_into(input, &mut output));
    output
}

pub(in crate::builtins::modules) fn builtin_hex2bin(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("hex2bin", &args, 1)?;
    let input = string_arg("hex2bin", &args[0])?;
    if !input.as_bytes().len().is_multiple_of(2) {
        context.php_warning(
            "E_PHP_RUNTIME_HEX2BIN_ODD_LENGTH",
            "hex2bin(): Hexadecimal input string must have an even length",
            span,
        );
        return Ok(Value::Bool(false));
    }
    if input
        .as_bytes()
        .iter()
        .any(|byte| hex_nibble(*byte).is_none())
    {
        context.php_warning(
            "E_PHP_RUNTIME_HEX2BIN_INVALID_HEX",
            "hex2bin(): Input string must be hexadecimal string",
            span,
        );
        return Ok(Value::Bool(false));
    }
    baseline_hex2bin(input.as_bytes())
        .map_or(Ok(Value::Bool(false)), |bytes| Ok(Value::string(bytes)))
}

/// Validates hexadecimal input and returns its exact decoded length.
pub fn native_hex2bin_output_length(input: &[u8]) -> Option<usize> {
    if !input.len().is_multiple_of(2) || input.iter().any(|byte| hex_nibble(*byte).is_none()) {
        return None;
    }
    Some(input.len() / 2)
}

/// Decodes validated hexadecimal input directly into a native string range.
pub fn native_hex2bin_into(input: &[u8], output: &mut [u8]) -> bool {
    if native_hex2bin_output_length(input) != Some(output.len()) {
        return false;
    }
    for (pair, byte) in input.chunks_exact(2).zip(output) {
        let (Some(high), Some(low)) = (hex_nibble(pair[0]), hex_nibble(pair[1])) else {
            return false;
        };
        *byte = (high << 4) | low;
    }
    true
}

fn baseline_hex2bin(input: &[u8]) -> Option<Vec<u8>> {
    let length = native_hex2bin_output_length(input)?;
    let mut output = vec![0; length];
    native_hex2bin_into(input, &mut output).then_some(output)
}

pub(in crate::builtins::modules) fn builtin_quoted_printable_decode(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("quoted_printable_decode", &args, 1)?;
    let input = string_arg("quoted_printable_decode", &args[0])?;
    Ok(Value::string(baseline_quoted_printable_decode(
        input.as_bytes(),
    )))
}

fn visit_quoted_printable_decoded(bytes: &[u8], mut emit: impl FnMut(u8)) -> usize {
    let mut length = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'=' {
            if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                index += 2;
                continue;
            }
            if index + 2 < bytes.len() && bytes[index + 1] == b'\r' && bytes[index + 2] == b'\n' {
                index += 3;
                continue;
            }
            if index + 2 < bytes.len()
                && let (Some(high), Some(low)) =
                    (hex_nibble(bytes[index + 1]), hex_nibble(bytes[index + 2]))
            {
                emit((high << 4) | low);
                length += 1;
                index += 3;
                continue;
            }
        }
        emit(bytes[index]);
        length += 1;
        index += 1;
    }
    length
}

/// Exact decoded length for `quoted_printable_decode()`.
pub fn native_quoted_printable_decode_output_length(bytes: &[u8]) -> usize {
    visit_quoted_printable_decoded(bytes, |_| {})
}

/// Decodes quoted-printable bytes directly into a native string range.
pub fn native_quoted_printable_decode_into(bytes: &[u8], output: &mut [u8]) -> bool {
    if native_quoted_printable_decode_output_length(bytes) != output.len() {
        return false;
    }
    let mut cursor = 0;
    let length = visit_quoted_printable_decoded(bytes, |byte| {
        output[cursor] = byte;
        cursor += 1;
    });
    length == output.len()
}

fn baseline_quoted_printable_decode(bytes: &[u8]) -> Vec<u8> {
    let mut output = vec![0; native_quoted_printable_decode_output_length(bytes)];
    debug_assert!(native_quoted_printable_decode_into(bytes, &mut output));
    output
}

pub(in crate::builtins::modules) fn builtin_ord(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("ord", &args, 1)?;
    let input = string_arg("ord", &args[0])?;
    input
        .as_bytes()
        .first()
        .copied()
        .map(|byte| Value::Int(i64::from(byte)))
        .ok_or_else(|| value_error("ord", "string must not be empty"))
}

pub(in crate::builtins::modules) fn builtin_chr(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("chr", &args, 1)?;
    let value = int_arg("chr", &args[0])?.rem_euclid(256) as u8;
    Ok(Value::string(vec![value]))
}

/// Scalar payload requested by the exact native `pack()` format visitor.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NativePackArgument<'a> {
    Int(i64),
    String(&'a [u8]),
}

/// Visits one fully validated `pack()` result without constructing a runtime
/// `Value` or an aggregate output vector. Callers run a validation/count pass
/// before reserving visible storage, then a second pass that writes directly.
#[doc(hidden)]
pub fn visit_native_pack<'a>(
    format: &[u8],
    argument_count: usize,
    mut argument: impl FnMut(usize) -> Option<NativePackArgument<'a>>,
    mut emit: impl FnMut(&[u8]) -> Option<()>,
) -> Option<usize> {
    let specs = parse_pack_format(format, false).ok()?;
    let mut argument_index = 0usize;
    let mut output_length = 0usize;
    for spec in specs {
        match spec.code {
            b'n' | b'v' => {
                let count = if spec.count_all {
                    argument_count.checked_sub(argument_index)?
                } else {
                    spec.count.unwrap_or(1)
                };
                for _ in 0..count {
                    let NativePackArgument::Int(value) = argument(argument_index)? else {
                        return None;
                    };
                    argument_index = argument_index.checked_add(1)?;
                    let bytes = pack_u16_bytes(spec.code, value);
                    emit(&bytes)?;
                    output_length = output_length.checked_add(bytes.len())?;
                }
            }
            b'l' | b'I' | b'V' => {
                let count = if spec.count_all {
                    argument_count.checked_sub(argument_index)?
                } else {
                    spec.count.unwrap_or(1)
                };
                for _ in 0..count {
                    let NativePackArgument::Int(value) = argument(argument_index)? else {
                        return None;
                    };
                    argument_index = argument_index.checked_add(1)?;
                    let bytes = pack_u32_bytes(spec.code, value);
                    emit(&bytes)?;
                    output_length = output_length.checked_add(bytes.len())?;
                }
            }
            b'h' | b'H' => {
                let NativePackArgument::String(input) = argument(argument_index)? else {
                    return None;
                };
                argument_index = argument_index.checked_add(1)?;
                let count = if spec.count_all {
                    input.len()
                } else {
                    spec.count.unwrap_or(1)
                };
                for index in (0..count).step_by(2) {
                    let first = input.get(index).map_or(0, |byte| hex_pack_nibble(*byte));
                    let second = if index + 1 < count {
                        input
                            .get(index + 1)
                            .map_or(0, |byte| hex_pack_nibble(*byte))
                    } else {
                        0
                    };
                    let byte = if spec.code == b'H' {
                        (first << 4) | second
                    } else {
                        (second << 4) | first
                    };
                    emit(std::slice::from_ref(&byte))?;
                    output_length = output_length.checked_add(1)?;
                }
            }
            _ => return None,
        }
    }
    Some(output_length)
}

/// Key description emitted by the exact native `unpack()` visitor.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeUnpackKey<'a> {
    Int(i64),
    String(&'a [u8]),
    IndexedString(&'a [u8], usize),
}

/// Value description emitted by the exact native `unpack()` visitor.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeUnpackValue<'a> {
    Int(i64),
    Hex {
        code: u8,
        input: &'a [u8],
        count: usize,
    },
}

fn native_unpack_key<'a>(
    spec: &'a PackFormatSpec,
    index: usize,
    next_numeric_key: &mut i64,
    hex: bool,
) -> Option<NativeUnpackKey<'a>> {
    match spec.label.as_deref() {
        Some(label) if !label.is_empty() && (hex || spec.count.unwrap_or(1) == 1) => {
            Some(NativeUnpackKey::String(label))
        }
        Some(label) if !label.is_empty() => {
            Some(NativeUnpackKey::IndexedString(label, index.checked_add(1)?))
        }
        _ => {
            let key = *next_numeric_key;
            *next_numeric_key = next_numeric_key.checked_add(1)?;
            Some(NativeUnpackKey::Int(key))
        }
    }
}

/// Visits a validated `unpack()` result as native key/value descriptions.
/// No result array, string `Value`, or compatibility identity is created.
#[doc(hidden)]
pub fn visit_native_unpack<'a>(
    format: &[u8],
    data: &'a [u8],
    offset: usize,
    mut emit: impl FnMut(NativeUnpackKey<'_>, NativeUnpackValue<'a>) -> Option<()>,
) -> Option<usize> {
    if offset > data.len() {
        return None;
    }
    let specs = parse_pack_format(format, true).ok()?;
    let base = offset;
    let mut cursor = base;
    let mut next_numeric_key = 1_i64;
    let mut entry_count = 0usize;
    for spec in &specs {
        match spec.code {
            b'n' | b'v' => {
                let count = if spec.count_all {
                    data.len().saturating_sub(cursor) / 2
                } else {
                    spec.count.unwrap_or(1)
                };
                for index in 0..count {
                    let end = cursor.checked_add(2)?;
                    let bytes = data.get(cursor..end)?;
                    cursor = end;
                    emit(
                        native_unpack_key(spec, index, &mut next_numeric_key, false)?,
                        NativeUnpackValue::Int(unpack_u16_value(spec.code, bytes)),
                    )?;
                    entry_count = entry_count.checked_add(1)?;
                }
            }
            b'l' | b'I' | b'V' => {
                let count = if spec.count_all {
                    data.len().saturating_sub(cursor) / 4
                } else {
                    spec.count.unwrap_or(1)
                };
                for index in 0..count {
                    let end = cursor.checked_add(4)?;
                    let bytes = data.get(cursor..end)?;
                    cursor = end;
                    emit(
                        native_unpack_key(spec, index, &mut next_numeric_key, false)?,
                        NativeUnpackValue::Int(unpack_u32_value(spec.code, bytes)),
                    )?;
                    entry_count = entry_count.checked_add(1)?;
                }
            }
            b'h' | b'H' => {
                let count = if spec.count_all {
                    data.len().saturating_sub(cursor).checked_mul(2)?
                } else {
                    spec.count.unwrap_or(1)
                };
                let width = count.div_ceil(2);
                let end = cursor.checked_add(width)?;
                let input = data.get(cursor..end)?;
                cursor = end;
                emit(
                    native_unpack_key(spec, 0, &mut next_numeric_key, true)?,
                    NativeUnpackValue::Hex {
                        code: spec.code,
                        input,
                        count,
                    },
                )?;
                entry_count = entry_count.checked_add(1)?;
            }
            b'@' => {
                cursor = base.checked_add(spec.count.unwrap_or(0))?;
                if cursor > data.len() {
                    return None;
                }
            }
            b'X' => {
                cursor = cursor.checked_sub(spec.count.unwrap_or(1))?;
            }
            _ => return None,
        }
    }
    Some(entry_count)
}

/// Writes one hexadecimal `unpack()` result into an exact native string.
#[doc(hidden)]
pub fn native_unpack_hex_into(code: u8, input: &[u8], count: usize, output: &mut [u8]) -> bool {
    if !matches!(code, b'h' | b'H') || output.len() != count || input.len() < count.div_ceil(2) {
        return false;
    }
    let mut cursor = 0usize;
    for byte in input {
        let high = byte >> 4;
        let low = byte & 0x0f;
        for nibble in if code == b'H' {
            [high, low]
        } else {
            [low, high]
        } {
            if cursor == count {
                return true;
            }
            output[cursor] = hex_digit(nibble);
            cursor += 1;
        }
    }
    cursor == count
}

pub(in crate::builtins::modules) fn builtin_pack(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() {
        return Err(arity_error("pack", "at least one argument"));
    }
    let format = string_arg("pack", &args[0])?;
    let specs = parse_pack_format(format.as_bytes(), false)?;
    let mut values = args.iter().skip(1);
    let mut output = Vec::new();

    for spec in specs {
        match spec.code {
            b'n' | b'v' => {
                let count = if spec.count_all {
                    values.len()
                } else {
                    spec.count.unwrap_or(1)
                };
                for _ in 0..count {
                    let value = values
                        .next()
                        .ok_or_else(|| value_error("pack", "not enough arguments"))?;
                    output.extend_from_slice(&pack_u16_bytes(spec.code, int_arg("pack", value)?));
                }
            }
            b'l' | b'I' | b'V' => {
                let count = if spec.count_all {
                    values.len()
                } else {
                    spec.count.unwrap_or(1)
                };
                for _ in 0..count {
                    let value = values
                        .next()
                        .ok_or_else(|| value_error("pack", "not enough arguments"))?;
                    let number = int_arg("pack", value)?;
                    output.extend_from_slice(&pack_u32_bytes(spec.code, number));
                }
            }
            b'h' | b'H' => {
                let value = values
                    .next()
                    .ok_or_else(|| value_error("pack", "not enough arguments"))?;
                let input = string_arg("pack", value)?;
                let count = if spec.count_all {
                    input.as_bytes().len()
                } else {
                    spec.count.unwrap_or(1)
                };
                output.extend_from_slice(&pack_hex_nibbles(spec.code, input.as_bytes(), count));
            }
            code => return Err(invalid_pack_format("pack", code)),
        }
    }

    Ok(Value::string(output))
}

pub(in crate::builtins::modules) fn builtin_unpack(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("unpack", "two or three argument(s)"));
    }
    let format = string_arg("unpack", &args[0])?;
    let data = string_arg("unpack", &args[1])?;
    let offset = args
        .get(2)
        .map(|value| int_arg("unpack", value))
        .transpose()?
        .unwrap_or(0);
    if offset < 0 || offset as usize > data.len() {
        return Err(unpack_offset_error());
    }

    let specs = parse_pack_format(format.as_bytes(), true)?;
    let base = offset as usize;
    let mut cursor = base;
    let mut next_numeric_key = 1_i64;
    let mut output = PhpArray::new();

    for spec in specs {
        match spec.code {
            b'n' | b'v' => {
                let count = if spec.count_all {
                    (data.len().saturating_sub(cursor)) / 2
                } else {
                    spec.count.unwrap_or(1)
                };
                for index in 0..count {
                    let end = cursor.checked_add(2).ok_or_else(|| {
                        value_error("unpack", "Type value overflows internal cursor")
                    })?;
                    if end > data.len() {
                        return Err(BuiltinError::new(
                            "E_PHP_RUNTIME_BUILTIN_VALUE",
                            "Type value overflows input data string",
                        ));
                    }
                    let value = unpack_u16_value(spec.code, &data.as_bytes()[cursor..end]);
                    cursor = end;
                    let key = unpack_result_key(&spec, index, &mut next_numeric_key);
                    output.insert(key, Value::Int(value));
                }
            }
            b'l' | b'I' | b'V' => {
                let count = if spec.count_all {
                    (data.len().saturating_sub(cursor)) / 4
                } else {
                    spec.count.unwrap_or(1)
                };
                for index in 0..count {
                    let end = cursor.checked_add(4).ok_or_else(|| {
                        value_error("unpack", "Type value overflows internal cursor")
                    })?;
                    if end > data.len() {
                        return Err(BuiltinError::new(
                            "E_PHP_RUNTIME_BUILTIN_VALUE",
                            "Type value overflows input data string",
                        ));
                    }
                    let value = unpack_u32_value(spec.code, &data.as_bytes()[cursor..end]);
                    cursor = end;
                    let key = unpack_result_key(&spec, index, &mut next_numeric_key);
                    output.insert(key, Value::Int(value));
                }
            }
            b'h' | b'H' => {
                let count = if spec.count_all {
                    (data.len().saturating_sub(cursor)) * 2
                } else {
                    spec.count.unwrap_or(1)
                };
                let width = count.div_ceil(2);
                let end = cursor
                    .checked_add(width)
                    .ok_or_else(|| value_error("unpack", "cursor is out of range"))?;
                if end > data.len() {
                    return Err(BuiltinError::new(
                        "E_PHP_RUNTIME_BUILTIN_VALUE",
                        "Type value overflows input data string",
                    ));
                }
                let value = unpack_hex_nibbles(spec.code, &data.as_bytes()[cursor..end], count);
                cursor = end;
                let key = unpack_hex_result_key(&spec, &mut next_numeric_key);
                output.insert(key, Value::string(value));
            }
            b'@' => {
                cursor = base
                    .checked_add(spec.count.unwrap_or(0))
                    .ok_or_else(|| value_error("unpack", "cursor is out of range"))?;
                if cursor > data.len() {
                    return Err(value_error("unpack", "cursor is out of range"));
                }
            }
            b'X' => {
                let count = spec.count.unwrap_or(1);
                cursor = cursor
                    .checked_sub(count)
                    .ok_or_else(|| value_error("unpack", "cursor is out of range"))?;
            }
            code => return Err(invalid_pack_format("unpack", code)),
        }
    }

    Ok(Value::Array(output))
}

fn pack_hex_nibbles(code: u8, input: &[u8], count: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(count.div_ceil(2));
    for index in (0..count).step_by(2) {
        let first = input.get(index).map_or(0, |byte| hex_pack_nibble(*byte));
        let second = if index + 1 < count {
            input
                .get(index + 1)
                .map_or(0, |byte| hex_pack_nibble(*byte))
        } else {
            0
        };
        let byte = if code == b'H' {
            (first << 4) | second
        } else {
            (second << 4) | first
        };
        output.push(byte);
    }
    output
}

fn hex_pack_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

fn unpack_hex_nibbles(code: u8, input: &[u8], count: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(count);
    for byte in input {
        let high = byte >> 4;
        let low = byte & 0x0f;
        if code == b'H' {
            output.push(hex_digit(high));
            output.push(hex_digit(low));
        } else {
            output.push(hex_digit(low));
            output.push(hex_digit(high));
        }
    }
    output.truncate(count);
    output
}

fn hex_digit(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0' + nibble,
        10..=15 => b'a' + (nibble - 10),
        _ => unreachable!("nibble is masked"),
    }
}

fn unpack_hex_result_key(spec: &PackFormatSpec, next_numeric_key: &mut i64) -> ArrayKey {
    match &spec.label {
        Some(label) if !label.is_empty() => ArrayKey::String(PhpString::from_bytes(label.clone())),
        _ => {
            let key = *next_numeric_key;
            *next_numeric_key += 1;
            ArrayKey::Int(key)
        }
    }
}

pub(in crate::builtins::modules) fn builtin_md5(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin md5 expects one or two argument(s)",
        ));
    }
    let input = string_arg("md5", &args[0])?;
    let raw = args
        .get(1)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("md5", message))?
        .unwrap_or(false);
    Ok(Value::string(baseline_md5(input.as_bytes(), raw)))
}

pub fn native_md5_output_length(raw: bool) -> usize {
    if raw { 16 } else { 32 }
}

pub fn native_md5_into(input: &[u8], raw: bool, output: &mut [u8]) -> bool {
    if output.len() != native_md5_output_length(raw) {
        return false;
    }
    let digest = Md5::digest(input);
    if raw {
        output.copy_from_slice(&digest);
    } else {
        write_hex_into(&digest, output);
    }
    true
}

fn baseline_md5(input: &[u8], raw: bool) -> Vec<u8> {
    let mut output = vec![0; native_md5_output_length(raw)];
    debug_assert!(native_md5_into(input, raw, &mut output));
    output
}

pub(in crate::builtins::modules) fn builtin_sha1(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin sha1 expects one or two argument(s)",
        ));
    }
    let input = string_arg("sha1", &args[0])?;
    let raw = args
        .get(1)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("sha1", message))?
        .unwrap_or(false);
    Ok(Value::string(baseline_sha1(input.as_bytes(), raw)))
}

pub fn native_sha1_output_length(raw: bool) -> usize {
    if raw { 20 } else { 40 }
}

pub fn native_sha1_into(input: &[u8], raw: bool, output: &mut [u8]) -> bool {
    if output.len() != native_sha1_output_length(raw) {
        return false;
    }
    let digest = Sha1::digest(input);
    if raw {
        output.copy_from_slice(&digest);
    } else {
        write_hex_into(&digest, output);
    }
    true
}

fn baseline_sha1(input: &[u8], raw: bool) -> Vec<u8> {
    let mut output = vec![0; native_sha1_output_length(raw)];
    debug_assert!(native_sha1_into(input, raw, &mut output));
    output
}

fn write_hex_into(input: &[u8], output: &mut [u8]) {
    debug_assert_eq!(output.len(), input.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for (byte, pair) in input.iter().zip(output.chunks_exact_mut(2)) {
        pair[0] = HEX[(byte >> 4) as usize];
        pair[1] = HEX[(byte & 0x0f) as usize];
    }
}

pub(in crate::builtins::modules) fn builtin_crc32(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("crc32", &args, 1)?;
    let input = string_arg("crc32", &args[0])?;
    Ok(Value::Int(native_crc32(input.as_bytes())))
}

/// Exact native CRC-32 result over the authoritative string bytes.
pub fn native_crc32(input: &[u8]) -> i64 {
    i64::from(crc32fast::hash(input))
}

pub fn native_hash_output_length(algorithm: &[u8], binary: bool) -> Option<usize> {
    direct_hash_output_length(algorithm, binary)
}

pub fn native_hash_into(algorithm: &[u8], input: &[u8], binary: bool, output: &mut [u8]) -> bool {
    direct_hash_into(algorithm, input, binary, output)
}

pub fn native_hash_hmac_output_length(algorithm: &[u8], binary: bool) -> Option<usize> {
    direct_hash_hmac_output_length(algorithm, binary)
}

pub fn native_hash_hmac_into(
    algorithm: &[u8],
    input: &[u8],
    key: &[u8],
    binary: bool,
    output: &mut [u8],
) -> bool {
    direct_hash_hmac_into(algorithm, input, key, binary, output)
}

pub(in crate::builtins::modules) fn builtin_hash(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(arity_error("hash", "two to four argument(s)"));
    }
    let algorithm = string_arg("hash", &args[0])?.to_string_lossy();
    let input = string_arg("hash", &args[1])?;
    let binary = args
        .get(2)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("hash", message))?
        .unwrap_or(false);
    let options = parse_hash_options(context, "hash", &algorithm, args.get(3), span)?;
    let digest = hash_digest_bytes_with_options("hash", &algorithm, input.as_bytes(), &options)?;
    Ok(if binary {
        Value::string(digest)
    } else {
        Value::string(hex_encode(&digest))
    })
}

pub(in crate::builtins::modules) fn builtin_hash_hmac(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=4).contains(&args.len()) {
        return Err(arity_error("hash_hmac", "three or four argument(s)"));
    }
    let algorithm = string_arg("hash_hmac", &args[0])?.to_string_lossy();
    let input = string_arg("hash_hmac", &args[1])?;
    let key = string_arg("hash_hmac", &args[2])?;
    let binary = args
        .get(3)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("hash_hmac", message))?
        .unwrap_or(false);
    let digest = hmac_digest_bytes("hash_hmac", &algorithm, key.as_bytes(), input.as_bytes())?;
    Ok(if binary {
        Value::string(digest)
    } else {
        Value::string(hex_encode(&digest))
    })
}

pub(in crate::builtins::modules) fn builtin_base64_encode(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("base64_encode", &args, 1)?;
    let input = string_arg("base64_encode", &args[0])?;
    Ok(Value::string(baseline_base64_encode(input.as_bytes())))
}

/// Exact output length for canonical padded Base64.
pub fn native_base64_encode_output_length(input: &[u8]) -> Option<usize> {
    base64::encoded_len(input.len(), true)
}

/// Encodes directly into one authoritative native string range.
pub fn native_base64_encode_into(input: &[u8], output: &mut [u8]) -> bool {
    native_base64_encode_output_length(input) == Some(output.len())
        && BASE64_STANDARD
            .encode_slice(input, output)
            .is_ok_and(|written| written == output.len())
}

fn baseline_base64_encode(input: &[u8]) -> Vec<u8> {
    let Some(length) = native_base64_encode_output_length(input) else {
        return Vec::new();
    };
    let mut output = vec![0; length];
    debug_assert!(native_base64_encode_into(input, &mut output));
    output
}

pub(in crate::builtins::modules) fn builtin_base64_decode(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin base64_decode expects one or two argument(s)",
        ));
    }
    let input = string_arg("base64_decode", &args[0])?;
    let strict = args
        .get(1)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("base64_decode", message))?
        .unwrap_or(false);
    match baseline_base64_decode(input.as_bytes(), strict) {
        Some(bytes) => Ok(Value::string(bytes)),
        None => Ok(Value::Bool(false)),
    }
}

fn base64_sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn is_php_base64_whitespace(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | b'\r' | b' ')
}

fn base64_decode_shape(input: &[u8], strict: bool) -> Option<(usize, usize)> {
    let mut symbol_count = 0_usize;
    let mut padding = 0_usize;
    for byte in input.iter().copied() {
        if base64_sextet(byte).is_some() {
            if strict && padding != 0 {
                return None;
            }
            symbol_count += 1;
        } else if byte == b'=' {
            padding += 1;
        } else if strict && !is_php_base64_whitespace(byte) {
            return None;
        }
    }

    if strict && symbol_count % 4 == 1 {
        return None;
    }
    if strict && padding != 0 && (padding > 2 || !(symbol_count + padding).is_multiple_of(4)) {
        return None;
    }
    let output_length = symbol_count.checked_mul(3)?.checked_div(4)?;
    Some((symbol_count, output_length))
}

/// Validates Base64 and returns its exact decoded native-string length.
pub fn native_base64_decode_output_length(input: &[u8], strict: bool) -> Option<usize> {
    base64_decode_shape(input, strict).map(|(_, output_length)| output_length)
}

/// Decodes Base64 directly into one authoritative native string range.
pub fn native_base64_decode_into(input: &[u8], strict: bool, output: &mut [u8]) -> bool {
    let Some((_, output_length)) = base64_decode_shape(input, strict) else {
        return false;
    };
    if output_length != output.len() {
        return false;
    }

    let mut quartet = [0_u8; 4];
    let mut quartet_length = 0_usize;
    let mut cursor = 0;
    for byte in input.iter().copied() {
        let Some(sextet) = base64_sextet(byte) else {
            continue;
        };
        quartet[quartet_length] = sextet;
        quartet_length += 1;
        if quartet_length != 4 {
            continue;
        }
        output[cursor] = (quartet[0] << 2) | (quartet[1] >> 4);
        output[cursor + 1] = (quartet[1] << 4) | (quartet[2] >> 2);
        output[cursor + 2] = (quartet[2] << 6) | quartet[3];
        cursor += 3;
        quartet_length = 0;
    }
    if quartet_length >= 2 {
        output[cursor] = (quartet[0] << 2) | (quartet[1] >> 4);
        cursor += 1;
    }
    if quartet_length == 3 {
        output[cursor] = (quartet[1] << 4) | (quartet[2] >> 2);
        cursor += 1;
    }
    cursor == output.len()
}

fn baseline_base64_decode(input: &[u8], strict: bool) -> Option<Vec<u8>> {
    let length = native_base64_decode_output_length(input, strict)?;
    let mut output = vec![0; length];
    native_base64_decode_into(input, strict, &mut output).then_some(output)
}

pub(in crate::builtins::modules) fn builtin_htmlspecialchars(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=4).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin htmlspecialchars expects one to four argument(s)",
        ));
    }
    let flags = args.get(1).map_or(Ok(HTML_ESCAPE_DEFAULT_FLAGS), |value| {
        int_arg("htmlspecialchars", value)
    })?;
    let double_encode = args.get(3).map_or(Ok(true), to_bool).map_err(|message| {
        BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!("builtin htmlspecialchars expects bool-compatible double_encode: {message}"),
        )
    })?;
    let input = string_arg("htmlspecialchars", &args[0])?;
    Ok(Value::string(baseline_html_escape(
        input.as_bytes(),
        flags,
        double_encode,
        false,
    )))
}

pub const NATIVE_HTML_ESCAPE_DEFAULT_FLAGS: i64 = HTML_ESCAPE_DEFAULT_FLAGS;

/// Escapes one authoritative native byte string with PHP's HTML-special-char
/// flag semantics.
pub fn native_html_escape_output_length(
    input: &[u8],
    flags: i64,
    double_encode: bool,
    all_entities: bool,
) -> Option<usize> {
    direct_html_escape_output_length(input, flags, double_encode, all_entities)
}

pub fn native_html_escape_into(
    input: &[u8],
    flags: i64,
    double_encode: bool,
    all_entities: bool,
    output: &mut [u8],
) -> bool {
    direct_html_escape_into(input, flags, double_encode, all_entities, output)
}

fn baseline_html_escape(
    input: &[u8],
    flags: i64,
    double_encode: bool,
    all_entities: bool,
) -> Vec<u8> {
    if all_entities {
        htmlentities_escape_with_options(input, flags, double_encode)
    } else {
        html_escape_with_options(input, flags, double_encode)
    }
}

pub(in crate::builtins::modules) fn builtin_htmlentities(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=4).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin htmlentities expects one to four argument(s)",
        ));
    }
    let flags = args.get(1).map_or(Ok(HTML_ESCAPE_DEFAULT_FLAGS), |value| {
        int_arg("htmlentities", value)
    })?;
    let double_encode = args.get(3).map_or(Ok(true), to_bool).map_err(|message| {
        BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!("builtin htmlentities expects bool-compatible double_encode: {message}"),
        )
    })?;
    let input = string_arg("htmlentities", &args[0])?;
    Ok(Value::string(baseline_html_escape(
        input.as_bytes(),
        flags,
        double_encode,
        true,
    )))
}

pub(in crate::builtins::modules) fn builtin_html_entity_decode(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=3).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin html_entity_decode expects one to three argument(s)",
        ));
    }
    let flags = args.get(1).map_or(Ok(HTML_ESCAPE_DEFAULT_FLAGS), |value| {
        int_arg("html_entity_decode", value)
    })?;
    let input = string_arg("html_entity_decode", &args[0])?;
    Ok(Value::string(baseline_html_entity_decode(
        input.as_bytes(),
        flags,
        false,
    )))
}

pub fn native_html_entity_decode_output_length(
    input: &[u8],
    flags: i64,
    special_only: bool,
) -> Option<usize> {
    direct_html_entity_decode_output_length(input, flags, special_only)
}

pub fn native_html_entity_decode_into(
    input: &[u8],
    flags: i64,
    special_only: bool,
    output: &mut [u8],
) -> bool {
    direct_html_entity_decode_into(input, flags, special_only, output)
}

fn baseline_html_entity_decode(input: &[u8], flags: i64, special_only: bool) -> Vec<u8> {
    let input = String::from_utf8_lossy(input);
    if special_only {
        htmlspecialchars_decode_with_flags(&input, flags)
    } else {
        html_entity_decode_with_flags(&input, flags)
    }
}

pub(in crate::builtins::modules) fn builtin_get_html_translation_table(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 3 {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin get_html_translation_table expects at most three argument(s)",
        ));
    }
    let table = args
        .first()
        .map_or(Ok(0), |value| int_arg("get_html_translation_table", value))?;
    let flags = args.get(1).map_or(Ok(HTML_ESCAPE_DEFAULT_FLAGS), |value| {
        int_arg("get_html_translation_table", value)
    })?;
    let encoding = args
        .get(2)
        .map(|value| string_arg("get_html_translation_table", value))
        .transpose()?;
    Ok(Value::Array(html_translation_table(
        table,
        flags,
        encoding.as_ref(),
    )))
}

pub(in crate::builtins::modules) fn builtin_htmlspecialchars_decode(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin htmlspecialchars_decode expects one or two argument(s)",
        ));
    }
    let flags = args.get(1).map_or(Ok(HTML_ESCAPE_DEFAULT_FLAGS), |value| {
        int_arg("htmlspecialchars_decode", value)
    })?;
    let input = string_arg("htmlspecialchars_decode", &args[0])?;
    Ok(Value::string(baseline_html_entity_decode(
        input.as_bytes(),
        flags,
        true,
    )))
}

pub(in crate::builtins::modules) fn builtin_urlencode(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("urlencode", &args, 1)?;
    Ok(Value::string(baseline_url_encode(
        string_arg("urlencode", &args[0])?.as_bytes(),
        false,
    )))
}

pub(in crate::builtins::modules) fn builtin_rawurlencode(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("rawurlencode", &args, 1)?;
    Ok(Value::string(baseline_url_encode(
        string_arg("rawurlencode", &args[0])?.as_bytes(),
        true,
    )))
}

fn url_byte_is_unescaped(byte: u8, raw: bool) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(byte, b'-' | b'_')
        || (!raw && byte == b'.')
        || (raw && matches!(byte, b'.' | b'~'))
}

/// Exact output length for PHP URL encoding.
pub fn native_url_encode_output_length(input: &[u8], raw: bool) -> Option<usize> {
    let escaped = input
        .iter()
        .filter(|byte| !url_byte_is_unescaped(**byte, raw) && (raw || **byte != b' '))
        .count();
    input.len().checked_add(escaped.checked_mul(2)?)
}

/// URL-encodes bytes directly into a native string range.
pub fn native_url_encode_into(input: &[u8], raw: bool, output: &mut [u8]) -> bool {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    if native_url_encode_output_length(input, raw) != Some(output.len()) {
        return false;
    }
    let mut cursor = 0;
    for &byte in input {
        if url_byte_is_unescaped(byte, raw) {
            output[cursor] = byte;
            cursor += 1;
        } else if !raw && byte == b' ' {
            output[cursor] = b'+';
            cursor += 1;
        } else {
            output[cursor] = b'%';
            output[cursor + 1] = HEX[(byte >> 4) as usize];
            output[cursor + 2] = HEX[(byte & 0x0f) as usize];
            cursor += 3;
        }
    }
    true
}

fn baseline_url_encode(input: &[u8], raw: bool) -> Vec<u8> {
    let Some(length) = native_url_encode_output_length(input, raw) else {
        return Vec::new();
    };
    let mut output = vec![0; length];
    debug_assert!(native_url_encode_into(input, raw, &mut output));
    output
}

pub(in crate::builtins::modules) fn builtin_urldecode(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("urldecode", &args, 1)?;
    Ok(Value::string(baseline_url_decode(
        string_arg("urldecode", &args[0])?.as_bytes(),
        false,
    )))
}

pub(in crate::builtins::modules) fn builtin_rawurldecode(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("rawurldecode", &args, 1)?;
    Ok(Value::string(baseline_url_decode(
        string_arg("rawurldecode", &args[0])?.as_bytes(),
        true,
    )))
}

fn visit_url_decoded(input: &[u8], raw: bool, mut emit: impl FnMut(u8)) -> usize {
    let mut index = 0;
    let mut length = 0;
    while index < input.len() {
        if input[index] == b'%'
            && index + 2 < input.len()
            && let (Some(high), Some(low)) =
                (hex_nibble(input[index + 1]), hex_nibble(input[index + 2]))
        {
            emit((high << 4) | low);
            index += 3;
        } else {
            emit(if !raw && input[index] == b'+' {
                b' '
            } else {
                input[index]
            });
            index += 1;
        }
        length += 1;
    }
    length
}

/// Exact output length for PHP URL decoding.
pub fn native_url_decode_output_length(input: &[u8], raw: bool) -> usize {
    visit_url_decoded(input, raw, |_| {})
}

/// URL-decodes bytes directly into a native string range.
pub fn native_url_decode_into(input: &[u8], raw: bool, output: &mut [u8]) -> bool {
    if native_url_decode_output_length(input, raw) != output.len() {
        return false;
    }
    let mut cursor = 0;
    let length = visit_url_decoded(input, raw, |byte| {
        output[cursor] = byte;
        cursor += 1;
    });
    length == output.len()
}

fn baseline_url_decode(input: &[u8], raw: bool) -> Vec<u8> {
    let mut output = vec![0; native_url_decode_output_length(input, raw)];
    debug_assert!(native_url_decode_into(input, raw, &mut output));
    output
}

pub(in crate::builtins::modules) fn builtin_parse_url(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin parse_url expects one or two argument(s)",
        ));
    }
    let url = string_arg("parse_url", &args[0])?;
    let Some(parsed) = parse_php_url(url.as_bytes()) else {
        return Ok(Value::Bool(false));
    };

    if let Some(component) = args.get(1) {
        let component = int_arg("parse_url", component)?;
        if component >= 0 {
            return parse_url_component(&parsed, component);
        }
    }

    let mut array = PhpArray::new();
    insert_url_component(&mut array, "scheme", parsed.scheme);
    insert_url_component(&mut array, "host", parsed.host);
    if let Some(port) = parsed.port {
        array.insert(string_array_key("port"), Value::Int(port));
    }
    insert_url_component(&mut array, "user", parsed.user);
    insert_url_component(&mut array, "pass", parsed.pass);
    insert_url_component(&mut array, "path", parsed.path);
    insert_url_component(&mut array, "query", parsed.query);
    insert_url_component(&mut array, "fragment", parsed.fragment);
    Ok(Value::Array(array))
}

/// Parses a URL and publishes its exact scalar/array result directly into the
/// supplied authoritative native sink.
///
/// The returned pair is `(parsed, value)`: an unparsed URL is PHP `false`,
/// while `parsed && value.is_none()` means native publication capacity was
/// unavailable and the exact handler must take its baseline continuation.
pub fn native_parse_url_into<P: super::json::NativeStructuredValuePublisher>(
    input: &[u8],
    component: Option<i64>,
    publisher: &mut P,
) -> Result<(bool, Option<P::Output>), i64> {
    let Some(parsed) = parse_php_url(input) else {
        return Ok((false, None));
    };
    if let Some(component) = component
        && component >= 0
    {
        fn optional_string<P: super::json::NativeStructuredValuePublisher>(
            publisher: &mut P,
            value: Option<Vec<u8>>,
        ) -> Option<P::Output> {
            match value {
                Some(value) => publisher.publish_string(&value),
                None => publisher.publish_null(),
            }
        }
        let value = match component {
            0 => optional_string(publisher, parsed.scheme),
            1 => optional_string(publisher, parsed.host),
            2 => match parsed.port {
                Some(value) => publisher.publish_int(value),
                None => publisher.publish_null(),
            },
            3 => optional_string(publisher, parsed.user),
            4 => optional_string(publisher, parsed.pass),
            5 => optional_string(publisher, parsed.path),
            6 => optional_string(publisher, parsed.query),
            7 => optional_string(publisher, parsed.fragment),
            other => return Err(other),
        };
        return Ok((true, value));
    }

    enum NativeParsedUrlField {
        String(Vec<u8>),
        Int(i64),
    }
    let fields = [
        parsed
            .scheme
            .map(|value| (b"scheme".as_slice(), NativeParsedUrlField::String(value))),
        parsed
            .host
            .map(|value| (b"host".as_slice(), NativeParsedUrlField::String(value))),
        parsed
            .port
            .map(|value| (b"port".as_slice(), NativeParsedUrlField::Int(value))),
        parsed
            .user
            .map(|value| (b"user".as_slice(), NativeParsedUrlField::String(value))),
        parsed
            .pass
            .map(|value| (b"pass".as_slice(), NativeParsedUrlField::String(value))),
        parsed
            .path
            .map(|value| (b"path".as_slice(), NativeParsedUrlField::String(value))),
        parsed
            .query
            .map(|value| (b"query".as_slice(), NativeParsedUrlField::String(value))),
        parsed
            .fragment
            .map(|value| (b"fragment".as_slice(), NativeParsedUrlField::String(value))),
    ];
    let published = publisher.publish_object_stream::<()>(|publisher, push| {
        for (key, value) in fields.into_iter().flatten() {
            let value = match value {
                NativeParsedUrlField::String(value) => publisher.publish_string(&value),
                NativeParsedUrlField::Int(value) => publisher.publish_int(value),
            }
            .ok_or(())?;
            push(publisher, key, value).ok_or(())?;
        }
        Ok(())
    });
    Ok((true, published.unwrap_or(None)))
}

pub(in crate::builtins::modules) fn builtin_parse_str(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("parse_str", &args, 2)?;
    let query = string_arg("parse_str", &args[0])?.to_string_lossy();
    let ini = input_ini_options(context);
    let pairs = crate::parse_query_string_with_separators(&query, &ini.arg_separator_input);
    let array = crate::context::input_pairs_array(&pairs, &ini);
    assign_reference_arg(args.get(1), Value::Array(array));
    Ok(Value::Null)
}

/// Streams one query string into an exact native sink while preserving PHP
/// array-key, nesting, filtering, and input-limit semantics. No aggregate
/// parsed-value tree is constructed.
pub fn native_parse_str_into<E>(
    input: &[u8],
    ini: &RuntimeIniOptions,
    insert: impl FnMut(&[crate::context::NativeInputSegment], &[u8]) -> Result<(), E>,
) -> Result<(), E> {
    crate::context::native_input_bytes_into(input, ini, insert)
}

fn input_ini_options(context: &BuiltinContext<'_>) -> RuntimeIniOptions {
    let mut ini = RuntimeIniOptions::default();
    if let Some(value) = context.ini_get("arg_separator.input") {
        ini.arg_separator_input = value.to_string();
    }
    if let Some(value) = context.ini_get("max_input_vars")
        && let Ok(limit) = value.parse::<usize>()
    {
        ini.max_input_vars = limit;
    }
    if let Some(value) = context.ini_get("max_input_nesting_level")
        && let Ok(limit) = value.parse::<usize>()
    {
        ini.max_input_nesting_level = limit;
    }
    if let Some(value) = context.ini_get("filter.default")
        && let Some(filter) = crate::RuntimeInputFilter::from_ini_value(value)
    {
        ini.default_input_filter = filter;
    }
    if let Some(value) = context.ini_get("filter.default_flags")
        && let Ok(flags) = value.parse::<i64>()
    {
        ini.default_input_filter_flags = flags;
    }
    ini
}

pub(in crate::builtins::modules) fn builtin_http_build_query(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=4).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin http_build_query expects one to four argument(s)",
        ));
    }
    let Value::Array(array) = deref_value(&args[0]) else {
        return Err(type_error("http_build_query", "array", &args[0]));
    };
    let numeric_prefix = args
        .get(1)
        .map(|value| string_arg("http_build_query", value))
        .transpose()?;
    let arg_separator = match args.get(2) {
        Some(Value::Null) | None => context
            .ini_get("arg_separator.output")
            .unwrap_or("&")
            .to_owned(),
        Some(value) => string_arg("http_build_query", value)?.to_string_lossy(),
    };
    let raw_encoding = args
        .get(3)
        .map(|value| int_arg("http_build_query", value))
        .transpose()?
        == Some(PHP_QUERY_RFC3986);
    let numeric_prefix_text = numeric_prefix.as_ref().map(|value| value.to_string_lossy());
    let mut pairs = Vec::new();
    build_query_pairs(
        None,
        numeric_prefix_text.as_deref(),
        raw_encoding,
        &Value::Array(array),
        &mut pairs,
    )?;
    Ok(Value::string(pairs.join(&arg_separator).into_bytes()))
}

pub const NATIVE_PHP_QUERY_RFC3986: i64 = PHP_QUERY_RFC3986;

pub(in crate::builtins::modules) fn builtin_substr(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin substr expects two or three argument(s)",
        ));
    }
    let string = string_arg("substr", &args[0])?;
    let offset = int_arg("substr", &args[1])?;
    let length = match args.get(2).map(deref_value) {
        Some(Value::Null) | None => None,
        Some(value) => Some(int_arg("substr", &value)?),
    };
    Ok(Value::String(super::string_intrinsics::substr_bytes(
        &string, offset, length,
    )))
}

pub(in crate::builtins::modules) fn builtin_strpos(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    string_position(context, "strpos", args, false, false, span)
}

pub(in crate::builtins::modules) fn builtin_stripos(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    string_position(context, "stripos", args, true, false, span)
}

pub(in crate::builtins::modules) fn builtin_strrpos(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    string_position(context, "strrpos", args, false, true, span)
}

pub(in crate::builtins::modules) fn builtin_strripos(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    string_position(context, "strripos", args, true, true, span)
}

pub(in crate::builtins::modules) fn builtin_strrchr(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("strrchr", "two or three argument(s)"));
    }
    let haystack = string_arg("strrchr", &args[0])?;
    let needle = string_arg("strrchr", &args[1])?;
    let before_needle = args
        .get(2)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("strrchr", message))?
        .unwrap_or(false);
    let needle = needle.as_bytes().first().copied().unwrap_or(0);
    Ok(native_strrchr(haystack.as_bytes(), needle, before_needle)
        .map_or(Value::Bool(false), |bytes| Value::string(bytes.to_vec())))
}

/// Returns the byte slice selected by PHP's `strrchr`, if the byte exists.
pub fn native_strrchr(haystack: &[u8], needle: u8, before_needle: bool) -> Option<&[u8]> {
    let index = find_last_byte(haystack, needle)?;
    Some(if before_needle {
        &haystack[..index]
    } else {
        &haystack[index..]
    })
}

pub(in crate::builtins::modules) fn builtin_strstr(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    string_search_slice(context, "strstr", args, false, span)
}

pub(in crate::builtins::modules) fn builtin_stristr(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    string_search_slice(context, "stristr", args, true, span)
}

/// Selects the prefix or suffix around the first native byte-string match.
pub fn native_string_search_slice<'a>(
    haystack: &'a [u8],
    needle: &[u8],
    case_insensitive: bool,
    before_needle: bool,
) -> Option<&'a [u8]> {
    if needle.is_empty() {
        return Some(if before_needle {
            &haystack[..0]
        } else {
            haystack
        });
    }
    let index = find_bytes_from(haystack, needle, 0, case_insensitive)?;
    Some(if before_needle {
        &haystack[..index]
    } else {
        &haystack[index..]
    })
}

pub(in crate::builtins::modules) fn builtin_strpbrk(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("strpbrk", &args, 2)?;
    let haystack = string_arg("strpbrk", &args[0])?;
    let chars = string_arg("strpbrk", &args[1])?;
    if chars.is_empty() {
        return Err(argument_value_error(
            "strpbrk",
            "#2 ($characters)",
            "must be a non-empty string",
        ));
    }
    Ok(native_strpbrk(haystack.as_bytes(), chars.as_bytes())
        .map_or(Value::Bool(false), |bytes| Value::string(bytes.to_vec())))
}

/// Returns the suffix beginning with the first byte from `characters`.
///
/// Empty character sets are rejected by the PHP-facing caller before this
/// byte-only operation is entered.
pub fn native_strpbrk<'a>(haystack: &'a [u8], characters: &[u8]) -> Option<&'a [u8]> {
    let index = find_first_of(haystack, 0, characters);
    (index != haystack.len()).then(|| &haystack[index..])
}

pub(in crate::builtins::modules) fn builtin_strspn(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    string_span("strspn", args, true)
}

pub(in crate::builtins::modules) fn builtin_strcspn(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    string_span("strcspn", args, false)
}

pub(in crate::builtins::modules) fn builtin_substr_count(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(arity_error("substr_count", "two to four argument(s)"));
    }
    let haystack = string_arg("substr_count", &args[0])?;
    let needle = string_arg("substr_count", &args[1])?;
    if needle.is_empty() {
        return Err(substr_count_argument_error(
            "#2 ($needle) must not be empty",
        ));
    }
    let offset = args
        .get(2)
        .map(|value| int_arg("substr_count", value))
        .transpose()?
        .unwrap_or(0);
    let start = checked_search_offset("substr_count", haystack.len(), offset).map_err(|_| {
        substr_count_argument_error("#3 ($offset) must be contained in argument #1 ($haystack)")
    })?;
    let length = match args.get(3) {
        Some(Value::Null) | None => None,
        Some(value) => Some(int_arg("substr_count", value)?),
    };
    let count_len = substr_count_length(haystack.len(), start, length)?;
    let end = start + count_len;
    let bytes = &haystack.as_bytes()[start..end];
    let mut count = 0i64;
    let mut search = 0usize;
    while let Some(index) = find_bytes_from(bytes, needle.as_bytes(), search, false) {
        count += 1;
        search = index + needle.len();
    }
    Ok(Value::Int(count))
}

pub(in crate::builtins::modules) fn substr_count_argument_error(message: &str) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_VALUE",
        format!("substr_count(): Argument {message}"),
    )
}

pub(in crate::builtins::modules) fn substr_count_length(
    total: usize,
    start: usize,
    length: Option<i64>,
) -> Result<usize, BuiltinError> {
    let remaining = total.saturating_sub(start);
    match length {
        None => Ok(remaining),
        Some(length) if length >= 0 && length as usize <= remaining => Ok(length as usize),
        Some(length) if length < 0 && length.unsigned_abs() as usize <= remaining => {
            Ok(remaining - length.unsigned_abs() as usize)
        }
        Some(_) => Err(substr_count_argument_error(
            "#4 ($length) must be contained in argument #1 ($haystack)",
        )),
    }
}

pub(in crate::builtins::modules) fn builtin_substr_compare(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=5).contains(&args.len()) {
        return Err(arity_error("substr_compare", "three to five argument(s)"));
    }
    let main = string_arg("substr_compare", &args[0])?;
    let other = string_arg("substr_compare", &args[1])?;
    let offset = int_arg("substr_compare", &args[2])?;
    substr_compare_offset(main.len(), offset)?;
    let length = match args.get(3) {
        Some(Value::Null) | None => None,
        Some(value) => {
            let length = int_arg("substr_compare", value)?;
            if length < 0 {
                return Err(argument_value_error(
                    "substr_compare",
                    "#4 ($length)",
                    "must be greater than or equal to 0",
                ));
            }
            Some(length)
        }
    };
    let case_insensitive = args
        .get(4)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("substr_compare", message))?
        .unwrap_or(false);
    Ok(Value::Int(
        native_substr_compare(
            main.as_bytes(),
            other.as_bytes(),
            offset,
            length.map(|length| length as usize),
            case_insensitive,
        )
        .expect("validated substr_compare offset"),
    ))
}

/// Compares one native byte substring using PHP's normalized offset rules.
///
/// `None` means the positive offset is outside the source string and must
/// resume at the PHP-visible ValueError boundary.
pub fn native_substr_compare(
    main: &[u8],
    other: &[u8],
    offset: i64,
    length: Option<usize>,
    case_insensitive: bool,
) -> Option<i64> {
    if offset > main.len() as i64 {
        return None;
    }
    let start = normalize_offset(main.len(), offset);
    let compare_len = length
        .unwrap_or_else(|| main.len().saturating_sub(start))
        .min(main.len().saturating_sub(start));
    let left = &main[start..start + compare_len];
    let right = &other[..length.map_or(other.len(), |length| length.min(other.len()))];
    let ordering = if case_insensitive {
        left.iter()
            .map(|byte| byte.to_ascii_lowercase())
            .cmp(right.iter().map(|byte| byte.to_ascii_lowercase()))
    } else {
        left.cmp(right)
    };
    Some(ordering_to_i64(ordering))
}

pub(in crate::builtins::modules) fn substr_compare_offset(
    len: usize,
    offset: i64,
) -> Result<usize, BuiltinError> {
    if offset > len as i64 {
        return Err(value_error("substr_compare", "offset is out of range"));
    }
    Ok(normalize_offset(len, offset))
}

pub(in crate::builtins::modules) fn builtin_str_contains(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("str_contains", &args, 2)?;
    let haystack = string_arg("str_contains", &args[0])?;
    let needle = string_arg("str_contains", &args[1])?;
    Ok(Value::Bool(
        find_bytes(haystack.as_bytes(), needle.as_bytes()).is_some(),
    ))
}

pub(in crate::builtins::modules) fn builtin_str_starts_with(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("str_starts_with", &args, 2)?;
    let haystack = string_arg("str_starts_with", &args[0])?;
    let needle = string_arg("str_starts_with", &args[1])?;
    Ok(Value::Bool(
        haystack.as_bytes().starts_with(needle.as_bytes()),
    ))
}

pub(in crate::builtins::modules) fn builtin_str_ends_with(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("str_ends_with", &args, 2)?;
    let haystack = string_arg("str_ends_with", &args[0])?;
    let needle = string_arg("str_ends_with", &args[1])?;
    Ok(Value::Bool(
        haystack.as_bytes().ends_with(needle.as_bytes()),
    ))
}

pub(in crate::builtins::modules) fn builtin_strcmp(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("strcmp", &args, 2)?;
    compare_strings("strcmp", &args, false, None)
}

pub(in crate::builtins::modules) fn builtin_strncmp(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("strncmp", &args, 3)?;
    let length = int_arg("strncmp", &args[2])?;
    if length < 0 {
        return Err(argument_value_error(
            "strncmp",
            "#3 ($length)",
            "must be greater than or equal to 0",
        ));
    }
    compare_strings("strncmp", &args, false, Some(length as usize))
}

pub(in crate::builtins::modules) fn builtin_strcasecmp(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("strcasecmp", &args, 2)?;
    compare_strings("strcasecmp", &args, true, None)
}

pub(in crate::builtins::modules) fn builtin_strncasecmp(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("strncasecmp", &args, 3)?;
    let length = int_arg("strncasecmp", &args[2])?;
    if length < 0 {
        return Err(argument_value_error(
            "strncasecmp",
            "#3 ($length)",
            "must be greater than or equal to 0",
        ));
    }
    compare_strings("strncasecmp", &args, true, Some(length as usize))
}

pub(in crate::builtins::modules) fn builtin_version_compare(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("version_compare", "2 or 3 argument(s)"));
    }

    let left = string_arg("version_compare", &args[0])?.to_string_lossy();
    let right = string_arg("version_compare", &args[1])?.to_string_lossy();
    let comparison = compare_versions(&left, &right);
    if let Some(operator) = args.get(2) {
        let operator = string_arg("version_compare", operator)?.to_string_lossy();
        return Ok(Value::Bool(version_operator_matches(
            &operator, comparison,
        )?));
    }
    Ok(Value::Int(comparison))
}

pub(in crate::builtins::modules) fn builtin_addslashes(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("addslashes", &args, 1)?;
    let input = string_arg("addslashes", &args[0])?;
    let mut output = Vec::with_capacity(input.len());
    for byte in input.as_bytes() {
        match *byte {
            b'\0' => output.extend_from_slice(b"\\0"),
            b'\'' | b'"' | b'\\' => {
                output.push(b'\\');
                output.push(*byte);
            }
            byte => output.push(byte),
        }
    }
    Ok(Value::string(output))
}

pub(in crate::builtins::modules) fn builtin_addcslashes(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("addcslashes", &args, 2)?;
    let input = string_arg("addcslashes", &args[0])?;
    let charlist = string_arg("addcslashes", &args[1])?;
    Ok(Value::string(baseline_addcslashes(
        input.as_bytes(),
        charlist.as_bytes(),
    )))
}

pub fn native_addcslashes_output_length(input: &[u8], charlist: &[u8]) -> Option<usize> {
    let escaped = addcslashes_charlist(charlist);
    let mut length = 0_usize;
    for byte in input {
        let escaped_length = if !escaped[usize::from(*byte)] {
            1
        } else if matches!(
            *byte,
            b'\n' | b'\r' | b'\t' | 0x0b | 0x0c | 0x07 | 0x08 | 0x20..=0x7e
        ) {
            2
        } else {
            4
        };
        length = length.checked_add(escaped_length)?;
    }
    Some(length)
}

pub fn native_addcslashes_into(input: &[u8], charlist: &[u8], output: &mut [u8]) -> bool {
    if native_addcslashes_output_length(input, charlist) != Some(output.len()) {
        return false;
    }
    let escaped = addcslashes_charlist(charlist);
    let mut cursor = 0;
    for byte in input.iter().copied() {
        if !escaped[usize::from(byte)] {
            output[cursor] = byte;
            cursor += 1;
            continue;
        }
        let encoded: &[u8] = match byte {
            b'\n' => b"\\n",
            b'\r' => b"\\r",
            b'\t' => b"\\t",
            0x0b => b"\\v",
            0x0c => b"\\f",
            0x07 => b"\\a",
            0x08 => b"\\b",
            0x20..=0x7e => {
                output[cursor] = b'\\';
                output[cursor + 1] = byte;
                cursor += 2;
                continue;
            }
            _ => {
                output[cursor] = b'\\';
                output[cursor + 1] = b'0' + ((byte >> 6) & 0x07);
                output[cursor + 2] = b'0' + ((byte >> 3) & 0x07);
                output[cursor + 3] = b'0' + (byte & 0x07);
                cursor += 4;
                continue;
            }
        };
        output[cursor..cursor + 2].copy_from_slice(encoded);
        cursor += 2;
    }
    cursor == output.len()
}

fn baseline_addcslashes(input: &[u8], charlist: &[u8]) -> Vec<u8> {
    let Some(length) = native_addcslashes_output_length(input, charlist) else {
        return Vec::new();
    };
    let mut output = vec![0; length];
    debug_assert!(native_addcslashes_into(input, charlist, &mut output));
    output
}

fn addcslashes_charlist(charlist: &[u8]) -> [bool; 256] {
    let mut escaped = [false; 256];
    let mut index = 0;
    while index < charlist.len() {
        let Some((start, consumed)) = addcslashes_charlist_atom(&charlist[index..]) else {
            break;
        };
        let after_start = index + consumed;
        if charlist.get(after_start) == Some(&b'.')
            && charlist.get(after_start + 1) == Some(&b'.')
            && let Some((end, end_consumed)) =
                addcslashes_charlist_atom(&charlist[after_start + 2..])
        {
            if start <= end {
                for byte in start..=end {
                    escaped[usize::from(byte)] = true;
                }
            } else {
                escaped[usize::from(start)] = true;
                escaped[usize::from(end)] = true;
            }
            index = after_start + 2 + end_consumed;
        } else {
            escaped[usize::from(start)] = true;
            index = after_start;
        }
    }
    escaped
}

fn addcslashes_charlist_atom(input: &[u8]) -> Option<(u8, usize)> {
    let first = *input.first()?;
    if first != b'\\' {
        return Some((first, 1));
    }
    let Some(next) = input.get(1).copied() else {
        return Some((b'\\', 1));
    };
    match next {
        b'n' => Some((b'\n', 2)),
        b'r' => Some((b'\r', 2)),
        b't' => Some((b'\t', 2)),
        b'v' => Some((0x0b, 2)),
        b'f' => Some((0x0c, 2)),
        b'a' => Some((0x07, 2)),
        b'b' => Some((0x08, 2)),
        b'\\' | b'\'' | b'"' => Some((next, 2)),
        b'x' | b'X' => {
            let (decoded, consumed) = decode_c_hex_escape(&input[2..]);
            (consumed > 0).then_some((decoded, consumed + 2))
        }
        b'0'..=b'7' => {
            let (decoded, consumed) = decode_c_octal_escape(&input[1..]);
            Some((decoded, consumed + 1))
        }
        _ => Some((b'\\', 1)),
    }
}

pub(in crate::builtins::modules) fn builtin_stripslashes(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("stripslashes", &args, 1)?;
    let input = string_arg("stripslashes", &args[0])?;
    Ok(Value::string(baseline_stripslashes(input.as_bytes())))
}

fn visit_stripslashes(input: &[u8], mut emit: impl FnMut(u8)) -> usize {
    let mut index = 0;
    let mut length = 0;
    while index < input.len() {
        if input[index] != b'\\' {
            emit(input[index]);
            index += 1;
        } else if let Some(next) = input.get(index + 1).copied() {
            emit(if next == b'0' { b'\0' } else { next });
            index += 2;
        } else {
            index += 1;
            continue;
        }
        length += 1;
    }
    length
}

/// Exact output length for `stripslashes()`.
pub fn native_stripslashes_output_length(input: &[u8]) -> usize {
    visit_stripslashes(input, |_| {})
}

/// Removes addslashes-style escapes directly into a native string range.
pub fn native_stripslashes_into(input: &[u8], output: &mut [u8]) -> bool {
    if native_stripslashes_output_length(input) != output.len() {
        return false;
    }
    let mut cursor = 0;
    let length = visit_stripslashes(input, |byte| {
        output[cursor] = byte;
        cursor += 1;
    });
    length == output.len()
}

fn baseline_stripslashes(input: &[u8]) -> Vec<u8> {
    let mut output = vec![0; native_stripslashes_output_length(input)];
    debug_assert!(native_stripslashes_into(input, &mut output));
    output
}

pub(in crate::builtins::modules) fn builtin_stripcslashes(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("stripcslashes", &args, 1)?;
    let input = string_arg("stripcslashes", &args[0])?;
    Ok(Value::string(baseline_stripcslashes(input.as_bytes())))
}

fn visit_stripcslashes(input: &[u8], mut emit: impl FnMut(u8)) -> usize {
    let mut index = 0;
    let mut length = 0;
    while index < input.len() {
        if input[index] != b'\\' {
            emit(input[index]);
            index += 1;
            length += 1;
            continue;
        }
        index += 1;
        let Some(next) = input.get(index).copied() else {
            emit(b'\\');
            length += 1;
            break;
        };
        match next {
            b'n' => emit(b'\n'),
            b'r' => emit(b'\r'),
            b't' => emit(b'\t'),
            b'v' => emit(0x0b),
            b'f' => emit(0x0c),
            b'a' => emit(0x07),
            b'b' => emit(0x08),
            b'\\' | b'\'' | b'"' => emit(next),
            b'x' | b'X' => {
                let (decoded, consumed) = decode_c_hex_escape(&input[index + 1..]);
                if consumed == 0 {
                    emit(next);
                } else {
                    emit(decoded);
                    index += consumed;
                }
            }
            b'0'..=b'7' => {
                let (decoded, consumed) = decode_c_octal_escape(&input[index..]);
                emit(decoded);
                index += consumed.saturating_sub(1);
            }
            byte => emit(byte),
        }
        index += 1;
        length += 1;
    }
    length
}

/// Exact output length for `stripcslashes()`.
pub fn native_stripcslashes_output_length(input: &[u8]) -> usize {
    visit_stripcslashes(input, |_| {})
}

/// Decodes C-style escapes directly into a native string range.
pub fn native_stripcslashes_into(input: &[u8], output: &mut [u8]) -> bool {
    if native_stripcslashes_output_length(input) != output.len() {
        return false;
    }
    let mut cursor = 0;
    let length = visit_stripcslashes(input, |byte| {
        output[cursor] = byte;
        cursor += 1;
    });
    length == output.len()
}

fn baseline_stripcslashes(input: &[u8]) -> Vec<u8> {
    let mut output = vec![0; native_stripcslashes_output_length(input)];
    debug_assert!(native_stripcslashes_into(input, &mut output));
    output
}

pub(in crate::builtins::modules) fn builtin_strnatcmp(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("strnatcmp", &args, 2)?;
    let left = string_arg("strnatcmp", &args[0])?;
    let right = string_arg("strnatcmp", &args[1])?;
    Ok(Value::Int(native_natural_compare(
        left.as_bytes(),
        right.as_bytes(),
        false,
    )))
}

pub(in crate::builtins::modules) fn builtin_strnatcasecmp(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("strnatcasecmp", &args, 2)?;
    let left = string_arg("strnatcasecmp", &args[0])?;
    let right = string_arg("strnatcasecmp", &args[1])?;
    Ok(Value::Int(native_natural_compare(
        left.as_bytes(),
        right.as_bytes(),
        true,
    )))
}

/// Performs PHP's natural byte ordering without constructing string Values.
pub fn native_natural_compare(left: &[u8], right: &[u8], case_insensitive: bool) -> i64 {
    ordering_to_i64(natural_compare_bytes(left, right, case_insensitive))
}

pub(in crate::builtins::modules) fn builtin_wordwrap(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=4).contains(&args.len()) {
        return Err(arity_error("wordwrap", "one to four argument(s)"));
    }
    let input = string_arg("wordwrap", &args[0])?;
    let width = args
        .get(1)
        .map(|value| int_arg("wordwrap", value))
        .transpose()?
        .unwrap_or(75);
    let break_string = args
        .get(2)
        .map(|value| string_arg("wordwrap", value))
        .transpose()?
        .unwrap_or_else(|| PhpString::from("\n"));
    let cut = args
        .get(3)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("wordwrap", message))?
        .unwrap_or(false);
    if break_string.is_empty() {
        return Err(argument_value_error(
            "wordwrap",
            "#3 ($break)",
            "must not be empty",
        ));
    }
    if width == 0 && cut {
        return Err(argument_value_error(
            "wordwrap",
            "#4 ($cut_long_words)",
            "cannot be true when argument #2 ($width) is 0",
        ));
    }
    if width < 0 && cut {
        return Ok(Value::string(wordwrap_negative_cut_bytes(
            input.as_bytes(),
            break_string.as_bytes(),
        )));
    }
    if width == 0 {
        return Ok(Value::string(wordwrap_zero_width_bytes(
            input.as_bytes(),
            break_string.as_bytes(),
        )));
    }
    let width = if width <= 0 { 1 } else { width as usize };
    wordwrap_check_memory_limit(
        context,
        input.as_bytes(),
        width,
        break_string.as_bytes(),
        &span,
    )?;
    Ok(Value::string(wordwrap_bytes(
        input.as_bytes(),
        width,
        break_string.as_bytes(),
        cut,
    )))
}

pub(in crate::builtins::modules) fn builtin_substr_replace(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=4).contains(&args.len()) {
        return Err(arity_error("substr_replace", "three or four argument(s)"));
    }
    match deref_value(&args[0]) {
        Value::Array(array) => {
            let mut result = PhpArray::new();
            for (index, (key, value)) in array.iter().enumerate() {
                let replacement = substr_replace_indexed_string_arg(&args[1], index)?;
                let offset = substr_replace_indexed_int_arg(&args[2], index)?.unwrap_or(0);
                let length = args
                    .get(3)
                    .map(|value| substr_replace_indexed_int_arg(value, index))
                    .transpose()?
                    .flatten();
                let replaced =
                    substr_replace_one("substr_replace", value, &replacement, offset, length)?;
                result.insert(key.clone(), replaced);
            }
            Ok(Value::Array(result))
        }
        subject => {
            if matches!(deref_value(&args[2]), Value::Array(_)) {
                return Err(BuiltinError::new(
                    "E_PHP_RUNTIME_BUILTIN_TYPE",
                    "substr_replace(): Argument #3 ($offset) cannot be an array when working on a single string",
                ));
            }
            if args
                .get(3)
                .is_some_and(|value| matches!(deref_value(value), Value::Array(_)))
            {
                return Err(BuiltinError::new(
                    "E_PHP_RUNTIME_BUILTIN_TYPE",
                    "substr_replace(): Argument #4 ($length) cannot be an array when working on a single string",
                ));
            }
            let replacement = substr_replace_indexed_string_arg(&args[1], 0)?;
            let offset = int_arg("substr_replace", &args[2])?;
            let length = args
                .get(3)
                .map(|value| int_arg("substr_replace", value))
                .transpose()?;
            substr_replace_one("substr_replace", &subject, &replacement, offset, length)
        }
    }
}

/// Replaces one normalized native byte substring. `None` preserves the
/// PHP-visible negative-length range error at the baseline continuation.
pub fn native_substr_replace_output_length(
    subject: &[u8],
    replacement: &[u8],
    offset: i64,
    length: Option<i64>,
) -> Option<usize> {
    let start = normalize_offset(subject.len(), offset);
    let replace_len = byte_substring_length("substr_replace", subject.len(), start, length).ok()?;
    subject
        .len()
        .checked_sub(replace_len)?
        .checked_add(replacement.len())
}

pub fn native_substr_replace_into(
    subject: &[u8],
    replacement: &[u8],
    offset: i64,
    length: Option<i64>,
    output: &mut [u8],
) -> bool {
    if native_substr_replace_output_length(subject, replacement, offset, length)
        != Some(output.len())
    {
        return false;
    }
    let start = normalize_offset(subject.len(), offset);
    let Ok(replace_len) = byte_substring_length("substr_replace", subject.len(), start, length)
    else {
        return false;
    };
    let end = start + replace_len;
    let replacement_end = start + replacement.len();
    output[..start].copy_from_slice(&subject[..start]);
    output[start..replacement_end].copy_from_slice(replacement);
    output[replacement_end..].copy_from_slice(&subject[end..]);
    true
}

pub(in crate::builtins::modules) fn baseline_substr_replace(
    subject: &[u8],
    replacement: &[u8],
    offset: i64,
    length: Option<i64>,
) -> Option<Vec<u8>> {
    let output_length = native_substr_replace_output_length(subject, replacement, offset, length)?;
    let mut output = vec![0; output_length];
    native_substr_replace_into(subject, replacement, offset, length, &mut output).then_some(output)
}

pub(in crate::builtins::modules) fn builtin_convert_uuencode(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("convert_uuencode", &args, 1)?;
    let input = string_arg("convert_uuencode", &args[0])?;
    Ok(Value::string(baseline_convert_uuencode(input.as_bytes())))
}

fn uuencode_sixbit(value: u8) -> u8 {
    let encoded = (value & 0x3f) + 0x20;
    if encoded == 0x20 { b'`' } else { encoded }
}

/// Exact output length for `convert_uuencode()`.
pub fn native_convert_uuencode_output_length(input: &[u8]) -> Option<usize> {
    let mut length = 2_usize;
    for chunk in input.chunks(45) {
        length = length
            .checked_add(2)?
            .checked_add(chunk.len().div_ceil(3).checked_mul(4)?)?;
    }
    Some(length)
}

/// Encodes uuencoded data directly into one authoritative native string range.
pub fn native_convert_uuencode_into(input: &[u8], output: &mut [u8]) -> bool {
    if native_convert_uuencode_output_length(input) != Some(output.len()) {
        return false;
    }
    let mut cursor = 0;
    for chunk in input.chunks(45) {
        output[cursor] = uuencode_sixbit(chunk.len() as u8);
        cursor += 1;
        for triple in chunk.chunks(3) {
            let a = triple.first().copied().unwrap_or(0);
            let b = triple.get(1).copied().unwrap_or(0);
            let c = triple.get(2).copied().unwrap_or(0);
            output[cursor] = uuencode_sixbit(a >> 2);
            output[cursor + 1] = uuencode_sixbit(((a << 4) | (b >> 4)) & 0x3f);
            output[cursor + 2] = uuencode_sixbit(((b << 2) | (c >> 6)) & 0x3f);
            output[cursor + 3] = uuencode_sixbit(c & 0x3f);
            cursor += 4;
        }
        output[cursor] = b'\n';
        cursor += 1;
    }
    output[cursor..].copy_from_slice(b"`\n");
    cursor + 2 == output.len()
}

fn baseline_convert_uuencode(input: &[u8]) -> Vec<u8> {
    let Some(length) = native_convert_uuencode_output_length(input) else {
        return Vec::new();
    };
    let mut output = vec![0; length];
    debug_assert!(native_convert_uuencode_into(input, &mut output));
    output
}

pub(in crate::builtins::modules) fn builtin_convert_uudecode(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("convert_uudecode", &args, 1)?;
    let input = string_arg("convert_uudecode", &args[0])?;
    Ok(baseline_convert_uudecode(input.as_bytes()).map_or_else(
        || {
            context.php_warning(
                "E_PHP_RUNTIME_INVALID_UUENCODED_STRING",
                "convert_uudecode(): Argument #1 ($data) is not a valid uuencoded string",
                span,
            );
            Value::Bool(false)
        },
        Value::string,
    ))
}

fn uudecode_sixbit(value: u8) -> u8 {
    if value == b'`' {
        0
    } else {
        value.wrapping_sub(0x20) & 0x3f
    }
}

/// Validates uuencoded data and returns its exact decoded native-string length.
pub fn native_convert_uudecode_output_length(input: &[u8]) -> Option<usize> {
    if input.is_empty() {
        return None;
    }
    let mut output_length = 0_usize;
    for raw_line in input.split(|byte| *byte == b'\n') {
        let line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
        if line.is_empty() {
            continue;
        }
        let length = uudecode_sixbit(*line.first()?) as usize;
        if length == 0 {
            return Some(output_length);
        }
        let encoded_length = length.div_ceil(3).checked_mul(4)?;
        if line.len().saturating_sub(1) < encoded_length {
            return None;
        }
        output_length = output_length.checked_add(length)?;
    }
    Some(output_length)
}

/// Decodes uuencoded data directly into one authoritative native string range.
pub fn native_convert_uudecode_into(input: &[u8], output: &mut [u8]) -> bool {
    if native_convert_uudecode_output_length(input) != Some(output.len()) {
        return false;
    }
    let mut cursor = 0;
    for raw_line in input.split(|byte| *byte == b'\n') {
        let line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
        if line.is_empty() {
            continue;
        }
        let length = uudecode_sixbit(line[0]) as usize;
        if length == 0 {
            return cursor == output.len();
        }
        let encoded_length = length.div_ceil(3) * 4;
        let line_output_end = cursor + length;
        for group in line[1..1 + encoded_length].chunks(4) {
            let a = uudecode_sixbit(group[0]);
            let b = uudecode_sixbit(group[1]);
            let c = uudecode_sixbit(group[2]);
            let d = uudecode_sixbit(group[3]);
            for byte in [(a << 2) | (b >> 4), (b << 4) | (c >> 2), (c << 6) | d] {
                if cursor == line_output_end {
                    break;
                }
                output[cursor] = byte;
                cursor += 1;
            }
        }
    }
    cursor == output.len()
}

fn baseline_convert_uudecode(input: &[u8]) -> Option<Vec<u8>> {
    let length = native_convert_uudecode_output_length(input)?;
    let mut output = vec![0; length];
    native_convert_uudecode_into(input, &mut output).then_some(output)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VersionPart {
    Number(i64),
    Label(i8),
}

fn compare_versions(left: &str, right: &str) -> i64 {
    let left = version_parts(left);
    let right = version_parts(right);
    let len = left.len().max(right.len());
    for index in 0..len {
        let ordering = compare_version_part(left.get(index), right.get(index));
        if ordering != 0 {
            return ordering;
        }
    }
    0
}

/// Compares two PHP version byte strings without constructing runtime values.
///
/// `version_compare()` historically coerces its string arguments through
/// lossy UTF-8 display before tokenization; the exact native builtin retains
/// that behavior while keeping the authoritative inputs in native storage.
#[must_use]
pub fn native_version_compare(left: &[u8], right: &[u8]) -> i64 {
    compare_versions(
        String::from_utf8_lossy(left).as_ref(),
        String::from_utf8_lossy(right).as_ref(),
    )
}

fn version_parts(version: &str) -> Vec<VersionPart> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut current_is_digit: Option<bool> = None;

    for ch in version.chars() {
        if ch.is_ascii_alphanumeric() {
            let is_digit = ch.is_ascii_digit();
            if current_is_digit.is_some_and(|was_digit| was_digit != is_digit) {
                push_version_part(&mut parts, &current);
                current.clear();
            }
            current.push(ch);
            current_is_digit = Some(is_digit);
        } else if matches!(ch, '.' | '-' | '_' | '+') {
            if !current.is_empty() {
                push_version_part(&mut parts, &current);
                current.clear();
            }
            current_is_digit = None;
        } else if !current.is_empty() {
            push_version_part(&mut parts, &current);
            current.clear();
            current_is_digit = None;
        }
    }

    if !current.is_empty() {
        push_version_part(&mut parts, &current);
    }

    while matches!(parts.last(), Some(VersionPart::Number(0))) {
        parts.pop();
    }
    parts
}

fn push_version_part(parts: &mut Vec<VersionPart>, part: &str) {
    if php_source::byte_kernel::all_ascii_digits(part.as_bytes()) {
        parts.push(VersionPart::Number(part.parse::<i64>().unwrap_or(i64::MAX)));
    } else {
        parts.push(VersionPart::Label(version_label_rank(part)));
    }
}

pub(in crate::builtins::modules) fn version_label_rank(label: &str) -> i8 {
    match label.to_ascii_lowercase().as_str() {
        "dev" => -6,
        "alpha" | "a" => -5,
        "beta" | "b" => -4,
        "rc" => -3,
        "pl" | "p" => 1,
        _ => -2,
    }
}

fn compare_version_part(left: Option<&VersionPart>, right: Option<&VersionPart>) -> i64 {
    match (left, right) {
        (None, None) => 0,
        (Some(part), None) => compare_part_to_release(*part),
        (None, Some(part)) => -compare_part_to_release(*part),
        (Some(VersionPart::Number(left)), Some(VersionPart::Number(right))) => {
            ordering_to_i64(left.cmp(right))
        }
        (Some(left), Some(right)) => {
            ordering_to_i64(version_part_rank(*left).cmp(&version_part_rank(*right)))
        }
    }
}

fn compare_part_to_release(part: VersionPart) -> i64 {
    match part {
        VersionPart::Number(0) => 0,
        VersionPart::Number(_) => 1,
        VersionPart::Label(rank) => ordering_to_i64(rank.cmp(&0)),
    }
}

fn version_part_rank(part: VersionPart) -> i16 {
    match part {
        VersionPart::Number(0) => 0,
        VersionPart::Number(value) => 10 + value.min(1_000) as i16,
        VersionPart::Label(rank) => i16::from(rank),
    }
}

pub(in crate::builtins::modules) fn version_operator_matches(
    operator: &str,
    comparison: i64,
) -> Result<bool, BuiltinError> {
    match operator.to_ascii_lowercase().as_str() {
        "<" | "lt" => Ok(comparison < 0),
        "<=" | "le" => Ok(comparison <= 0),
        ">" | "gt" => Ok(comparison > 0),
        ">=" | "ge" => Ok(comparison >= 0),
        "==" | "=" | "eq" => Ok(comparison == 0),
        "!=" | "<>" | "ne" => Ok(comparison != 0),
        _ => Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_VALUE",
            format!("builtin version_compare received unsupported operator {operator}"),
        )),
    }
}

/// Applies a validated `version_compare()` operator to an already computed
/// three-way comparison. `None` denotes the PHP value-error shape, which the
/// exact handler sends to its single baseline continuation for diagnostics.
#[must_use]
pub fn native_version_operator_matches(operator: &[u8], comparison: i64) -> Option<bool> {
    let operator = String::from_utf8_lossy(operator);
    match operator.to_ascii_lowercase().as_str() {
        "<" | "lt" => Some(comparison < 0),
        "<=" | "le" => Some(comparison <= 0),
        ">" | "gt" => Some(comparison > 0),
        ">=" | "ge" => Some(comparison >= 0),
        "==" | "=" | "eq" => Some(comparison == 0),
        "!=" | "<>" | "ne" => Some(comparison != 0),
        _ => None,
    }
}

#[doc(hidden)]
pub fn exact_printf(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() {
        return Err(arity_error("printf", "one or more argument(s)"));
    }
    let format = string_needle_arg("printf", "#1 ($format)", &args[0])?;
    let rendered = php_format("printf", format.as_bytes(), &args[1..], context, span)?;
    let length = rendered.len() as i64;
    context.output().write_bytes(rendered);
    Ok(Value::Int(length))
}

#[doc(hidden)]
pub fn exact_sprintf(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() {
        return Err(arity_error("sprintf", "one or more argument(s)"));
    }
    let format = string_needle_arg("sprintf", "#1 ($format)", &args[0])?;
    Ok(Value::string(php_format(
        "sprintf",
        format.as_bytes(),
        &args[1..],
        context,
        span,
    )?))
}

#[doc(hidden)]
pub fn exact_vprintf(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("vprintf", &args, 2)?;
    let format = string_needle_arg("vprintf", "#1 ($format)", &args[0])?;
    let values = format_array_values("vprintf", "#2 ($values)", &args[1])?;
    let rendered = php_format("vprintf", format.as_bytes(), &values, context, span)?;
    let length = rendered.len() as i64;
    context.output().write_bytes(rendered);
    Ok(Value::Int(length))
}

#[doc(hidden)]
pub fn exact_vsprintf(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("vsprintf", &args, 2)?;
    let format = string_needle_arg("vsprintf", "#1 ($format)", &args[0])?;
    let values = format_array_values("vsprintf", "#2 ($values)", &args[1])?;
    Ok(Value::string(php_format(
        "vsprintf",
        format.as_bytes(),
        &values,
        context,
        span,
    )?))
}

pub(in crate::builtins::modules) fn builtin_strval(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("strval", &args, 1)?;
    let value = args.into_iter().next().expect("checked arity");
    string_cast_value(context, &value, span)
        .map(Value::String)
        .map_err(|message| conversion_error("strval", message))
}
