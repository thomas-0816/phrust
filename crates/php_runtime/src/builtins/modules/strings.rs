//! Strings builtin registry slice.

use super::core::*;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{ArrayKey, PhpArray, PhpString, Value, to_bool};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use md5::{Digest, Md5};
use php_lexer::{LexerConfig, SymbolKind, TokenKind, TokenName, lex_all};
use sha1::Sha1;

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
    BuiltinEntry::new("hash", builtin_hash, BuiltinCompatibility::Php),
    BuiltinEntry::new("hash_hmac", builtin_hash_hmac, BuiltinCompatibility::Php),
    BuiltinEntry::new("hex2bin", builtin_hex2bin, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "highlight_string",
        builtin_highlight_string,
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
    BuiltinEntry::new("lcfirst", builtin_lcfirst, BuiltinCompatibility::Php),
    BuiltinEntry::new("ltrim", builtin_ltrim, BuiltinCompatibility::Php),
    BuiltinEntry::new("md5", builtin_md5, BuiltinCompatibility::Php),
    BuiltinEntry::new("ord", builtin_ord, BuiltinCompatibility::Php),
    BuiltinEntry::new("pack", builtin_pack, BuiltinCompatibility::Php),
    BuiltinEntry::new("parse_url", builtin_parse_url, BuiltinCompatibility::Php),
    BuiltinEntry::new("printf", builtin_printf, BuiltinCompatibility::Php),
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
    BuiltinEntry::new("sprintf", builtin_sprintf, BuiltinCompatibility::Php),
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
    BuiltinEntry::new("vprintf", builtin_vprintf, BuiltinCompatibility::Php),
    BuiltinEntry::new("vsprintf", builtin_vsprintf, BuiltinCompatibility::Php),
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
    Ok(Value::string(
        string_arg("strtoupper", &args[0])?
            .as_bytes()
            .iter()
            .map(u8::to_ascii_uppercase)
            .collect::<Vec<_>>(),
    ))
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
    let mut count = 0_i64;
    let result = replace_subject(&args[2], &search, &replace, &mut count)?;
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
        subject = replace_map(&subject, &replacements);
        return Ok(Value::string(subject));
    }
    expect_arity("strtr", &args, 3)?;
    let mut subject = string_arg("strtr", &args[0])?.into_bytes();
    let from = strtr_string_arg(
        context,
        &args[1],
        "#2 ($from)",
        "array|string",
        span.clone(),
    )?;
    let to = strtr_string_arg(context, &args[2], "#3 ($to)", "string", span)?;
    let to_bytes = to.as_bytes();
    for byte in &mut subject {
        if let Some(index) = from
            .as_bytes()
            .iter()
            .take(to_bytes.len())
            .rposition(|from| from == byte)
            && let Some(replacement) = to_bytes.get(index)
        {
            *byte = *replacement;
        }
    }
    Ok(Value::string(subject))
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
    Ok(Value::string(strip_tags_bytes(
        input.as_bytes(),
        allowed.as_deref(),
    )))
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
    Ok(Value::string(
        string_arg("strtolower", &args[0])?
            .as_bytes()
            .iter()
            .map(u8::to_ascii_lowercase)
            .collect::<Vec<_>>(),
    ))
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
    let mut bytes = string_arg("ucwords", &args[0])?.into_bytes();
    let delimiters = args
        .get(1)
        .map(|value| string_arg("ucwords", value))
        .transpose()?;
    let delimiters = delimiters
        .as_ref()
        .map_or(b" \t\r\n\x0c\x0b".as_slice(), crate::PhpString::as_bytes);
    let mut at_word_start = true;
    for byte in &mut bytes {
        if delimiters.contains(byte) {
            at_word_start = true;
        } else if at_word_start {
            *byte = byte.to_ascii_uppercase();
            at_word_start = false;
        }
    }
    Ok(Value::string(bytes))
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
    let target = length as usize;
    if input.len() >= target {
        return Ok(Value::String(input));
    }
    let needed = target - input.len();
    let (left, right) = match pad_type {
        0 => (needed, 0),
        2 => (needed / 2, needed - (needed / 2)),
        _ => (0, needed),
    };
    let mut output = repeat_pad(pad.as_bytes(), left);
    output.extend_from_slice(input.as_bytes());
    output.extend_from_slice(&repeat_pad(pad.as_bytes(), right));
    Ok(Value::string(output))
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
    let input = string_arg("quotemeta", &args[0])?.into_bytes();
    let mut out = Vec::with_capacity(input.len());
    for &byte in &input {
        if matches!(
            byte,
            b'.' | b'\\' | b'+' | b'*' | b'?' | b'[' | b'^' | b']' | b'$' | b'(' | b')'
        ) {
            out.push(b'\\');
        }
        out.push(byte);
    }
    Ok(Value::string(out))
}

pub(in crate::builtins::modules) fn builtin_bin2hex(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("bin2hex", &args, 1)?;
    Ok(Value::string(hex_encode(
        string_arg("bin2hex", &args[0])?.as_bytes(),
    )))
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
    hex_decode(input.as_bytes()).map_or(Ok(Value::Bool(false)), |bytes| Ok(Value::string(bytes)))
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
            b'l' | b'I' | b'V' => {
                let count = spec.count.unwrap_or(1);
                for _ in 0..count {
                    let value = values
                        .next()
                        .ok_or_else(|| value_error("pack", "not enough arguments"))?;
                    let number = int_arg("pack", value)?;
                    output.extend_from_slice(&pack_u32_bytes(spec.code, number));
                }
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
            b'l' | b'I' | b'V' => {
                let count = spec.count.unwrap_or(1);
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
    let digest = Md5::digest(input.as_bytes());
    Ok(if raw {
        Value::string(digest.to_vec())
    } else {
        Value::string(hex_encode(&digest))
    })
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
    let digest = Sha1::digest(input.as_bytes());
    Ok(if raw {
        Value::string(digest.to_vec())
    } else {
        Value::string(hex_encode(&digest))
    })
}

pub(in crate::builtins::modules) fn builtin_crc32(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("crc32", &args, 1)?;
    let input = string_arg("crc32", &args[0])?;
    Ok(Value::Int(i64::from(crc32fast::hash(input.as_bytes()))))
}

pub(in crate::builtins::modules) fn builtin_hash(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("hash", "two or three argument(s)"));
    }
    let algorithm = string_arg("hash", &args[0])?.to_string_lossy();
    let input = string_arg("hash", &args[1])?;
    let binary = args
        .get(2)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("hash", message))?
        .unwrap_or(false);
    let digest = hash_digest_bytes("hash", &algorithm, input.as_bytes())?;
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
    Ok(Value::string(
        BASE64_STANDARD
            .encode(string_arg("base64_encode", &args[0])?.as_bytes())
            .into_bytes(),
    ))
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
    let source = if strict {
        input.as_bytes().to_vec()
    } else {
        input
            .as_bytes()
            .iter()
            .copied()
            .filter(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
            .collect()
    };
    match BASE64_STANDARD.decode(source) {
        Ok(bytes) => Ok(Value::string(bytes)),
        Err(_) => Ok(Value::Bool(false)),
    }
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
    Ok(Value::string(html_escape(
        string_arg("htmlspecialchars", &args[0])?.as_bytes(),
    )))
}

pub(in crate::builtins::modules) fn builtin_htmlentities(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    builtin_htmlspecialchars(context, args, span)
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
    Ok(Value::string(html_decode(
        &string_arg("htmlspecialchars_decode", &args[0])?.to_string_lossy(),
    )))
}

pub(in crate::builtins::modules) fn builtin_urlencode(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("urlencode", &args, 1)?;
    Ok(Value::string(url_encode(
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
    Ok(Value::string(url_encode(
        string_arg("rawurlencode", &args[0])?.as_bytes(),
        true,
    )))
}

pub(in crate::builtins::modules) fn builtin_urldecode(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("urldecode", &args, 1)?;
    Ok(Value::string(url_decode(
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
    Ok(Value::string(url_decode(
        string_arg("rawurldecode", &args[0])?.as_bytes(),
        true,
    )))
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
        return parse_url_component(&parsed, int_arg("parse_url", component)?);
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

pub(in crate::builtins::modules) fn builtin_http_build_query(
    _context: &mut BuiltinContext<'_>,
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
    let mut pairs = Vec::new();
    build_query_pairs(None, &Value::Array(array), &mut pairs)?;
    Ok(Value::string(pairs.join("&").into_bytes()))
}

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
    let bytes = string.as_bytes();
    let start = normalize_offset(bytes.len(), offset);
    let end = match length {
        None => bytes.len(),
        Some(length) if length >= 0 => start.saturating_add(length as usize).min(bytes.len()),
        Some(length) => bytes.len().saturating_sub(length.unsigned_abs() as usize),
    };
    if start >= bytes.len() || end < start {
        return Ok(Value::string(Vec::new()));
    }
    Ok(Value::string(bytes[start..end].to_vec()))
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
    Ok(haystack
        .as_bytes()
        .iter()
        .rposition(|byte| *byte == needle)
        .map_or(Value::Bool(false), |index| {
            if before_needle {
                Value::string(haystack.as_bytes()[..index].to_vec())
            } else {
                Value::string(haystack.as_bytes()[index..].to_vec())
            }
        }))
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
    Ok(haystack
        .as_bytes()
        .iter()
        .position(|byte| chars.as_bytes().contains(byte))
        .map_or(Value::Bool(false), |index| {
            Value::string(haystack.as_bytes()[index..].to_vec())
        }))
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
    let start = substr_compare_offset(main.len(), offset)?;
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
    let compare_len = byte_substring_length("substr_compare", main.len(), start, length)?;
    let mut left = main.as_bytes()[start..start + compare_len].to_vec();
    let mut right = other.as_bytes().to_vec();
    if let Some(length) = length
        && length >= 0
    {
        right.truncate(length as usize);
    }
    if case_insensitive {
        left.iter_mut()
            .for_each(|byte| *byte = byte.to_ascii_lowercase());
        right
            .iter_mut()
            .for_each(|byte| *byte = byte.to_ascii_lowercase());
    }
    Ok(Value::Int(match left.cmp(&right) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }))
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

pub(in crate::builtins::modules) fn builtin_stripslashes(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("stripslashes", &args, 1)?;
    let input = string_arg("stripslashes", &args[0])?;
    Ok(Value::string(stripslashes_bytes(input.as_bytes())))
}

pub(in crate::builtins::modules) fn builtin_stripcslashes(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("stripcslashes", &args, 1)?;
    let input = string_arg("stripcslashes", &args[0])?;
    Ok(Value::string(stripcslashes_bytes(input.as_bytes())))
}

pub(in crate::builtins::modules) fn builtin_strnatcmp(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("strnatcmp", &args, 2)?;
    natural_compare_builtin("strnatcmp", &args, false)
}

pub(in crate::builtins::modules) fn builtin_strnatcasecmp(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("strnatcasecmp", &args, 2)?;
    natural_compare_builtin("strnatcasecmp", &args, true)
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

pub(in crate::builtins::modules) fn builtin_convert_uuencode(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("convert_uuencode", &args, 1)?;
    let input = string_arg("convert_uuencode", &args[0])?;
    Ok(Value::string(uuencode_bytes(input.as_bytes())))
}

pub(in crate::builtins::modules) fn builtin_convert_uudecode(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("convert_uudecode", &args, 1)?;
    let input = string_arg("convert_uudecode", &args[0])?;
    Ok(uudecode_bytes(input.as_bytes()).map_or_else(
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
    if part.as_bytes().iter().all(u8::is_ascii_digit) {
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

pub(in crate::builtins::modules) fn builtin_printf(
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

pub(in crate::builtins::modules) fn builtin_sprintf(
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

pub(in crate::builtins::modules) fn builtin_vprintf(
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

pub(in crate::builtins::modules) fn builtin_vsprintf(
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
