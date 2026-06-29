//! Literal text, heredoc, and interpolation parsing for IR lowering.

pub(crate) fn quoted_literal_body(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    let quote_start = if matches!(bytes, [b'b' | b'B', b'\'' | b'"', ..]) {
        1
    } else {
        0
    };
    let quote = *bytes.get(quote_start)?;
    if bytes.len() < quote_start + 2
        || (quote != b'\'' && quote != b'"')
        || bytes.last().copied() != Some(quote)
    {
        return None;
    }
    let body = &bytes[quote_start + 1..bytes.len() - 1];
    Some(if quote == b'\'' {
        unescape_single_quoted_php_string(body)
    } else {
        unescape_double_quoted_php_string(body)
    })
}

pub(crate) fn heredoc_literal_body(text: &str) -> Option<Vec<u8>> {
    let info = heredoc_body_info(text)?;
    if info.nowdoc {
        Some(info.body.to_vec())
    } else {
        Some(unescape_heredoc_php_string(info.body))
    }
}

#[derive(Clone, Copy, Debug)]
struct HeredocBodyInfo<'a> {
    body: &'a [u8],
    nowdoc: bool,
}

fn heredoc_body_info(text: &str) -> Option<HeredocBodyInfo<'_>> {
    let bytes = text.as_bytes();
    if !bytes.starts_with(b"<<<") {
        return None;
    }
    let first_newline = bytes.iter().position(|byte| *byte == b'\n')?;
    let header = std::str::from_utf8(&bytes[..first_newline]).ok()?.trim();
    let marker = header.strip_prefix("<<<")?.trim();
    if marker.is_empty() {
        return None;
    }
    let nowdoc = marker.starts_with('\'') && marker.ends_with('\'') && marker.len() >= 2;
    let body_start = first_newline + 1;
    let body_and_end = &bytes[body_start..];
    let end_line_start = body_and_end
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(body_start, |offset| body_start + offset + 1);
    if end_line_start < body_start {
        return None;
    }
    let mut body_end = end_line_start.saturating_sub(usize::from(end_line_start > body_start));
    if body_end > body_start && bytes.get(body_end - 1).copied() == Some(b'\r') {
        body_end -= 1;
    }
    Some(HeredocBodyInfo {
        body: &bytes[body_start..body_end],
        nowdoc,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum InterpolatedPart {
    Bytes(Vec<u8>),
    Variable {
        name: String,
        dim: Option<InterpolatedDim>,
        deprecated_dollar_brace: bool,
    },
    MethodCall {
        receiver: String,
        method: String,
    },
    Property {
        receiver: String,
        property: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum InterpolatedDim {
    Variable(String),
    Int(i64),
    String(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedInterpolatedVariable {
    name: String,
    dim: Option<InterpolatedDim>,
    end: usize,
    deprecated_dollar_brace: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedInterpolatedMethodCall {
    receiver: String,
    method: String,
    end: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedInterpolatedProperty {
    receiver: String,
    property: String,
    end: usize,
}

pub(crate) fn interpolated_literal_parts(text: &str) -> Option<Vec<InterpolatedPart>> {
    let trimmed = text.trim();
    let bytes = trimmed.as_bytes();
    let (body, decode_escaped_quote) =
        if bytes.first().copied() == Some(b'"') && bytes.last().copied() == Some(b'"') {
            (&bytes[1..bytes.len() - 1], true)
        } else {
            let heredoc = heredoc_body_info(trimmed)?;
            if heredoc.nowdoc {
                return None;
            }
            (heredoc.body, false)
        };
    parse_interpolated_double_quoted_body(body, decode_escaped_quote)
}

fn parse_interpolated_double_quoted_body(
    body: &[u8],
    decode_escaped_quote: bool,
) -> Option<Vec<InterpolatedPart>> {
    let mut parts = Vec::new();
    let mut chunk_start = 0;
    let mut index = 0;
    while index < body.len() {
        if body[index] == b'\\' {
            index += usize::from(index + 1 < body.len()) + 1;
            continue;
        }
        if body[index] == b'{'
            && body.get(index + 1).copied() == Some(b'$')
            && let Some(parsed) = parse_braced_interpolated_method_call(body, index)
        {
            parts.push(InterpolatedPart::Bytes(
                unescape_double_quoted_php_string_with_quote_mode(
                    &body[chunk_start..index],
                    decode_escaped_quote,
                ),
            ));
            parts.push(InterpolatedPart::MethodCall {
                receiver: parsed.receiver,
                method: parsed.method,
            });
            index = parsed.end;
            chunk_start = parsed.end;
            continue;
        }
        if body[index] == b'{'
            && body.get(index + 1).copied() == Some(b'$')
            && let Some(parsed) = parse_braced_interpolated_property(body, index)
        {
            parts.push(InterpolatedPart::Bytes(
                unescape_double_quoted_php_string_with_quote_mode(
                    &body[chunk_start..index],
                    decode_escaped_quote,
                ),
            ));
            parts.push(InterpolatedPart::Property {
                receiver: parsed.receiver,
                property: parsed.property,
            });
            index = parsed.end;
            chunk_start = parsed.end;
            continue;
        }
        if body[index] == b'$'
            && let Some(parsed) = parse_simple_interpolated_property(body, index)
        {
            parts.push(InterpolatedPart::Bytes(
                unescape_double_quoted_php_string_with_quote_mode(
                    &body[chunk_start..index],
                    decode_escaped_quote,
                ),
            ));
            parts.push(InterpolatedPart::Property {
                receiver: parsed.receiver,
                property: parsed.property,
            });
            index = parsed.end;
            chunk_start = parsed.end;
            continue;
        }
        let parsed = if body[index] == b'$' {
            parse_deprecated_dollar_brace_interpolated_variable(body, index).or_else(|| {
                parse_simple_interpolated_variable(body, index).map(|mut parsed| {
                    parsed.deprecated_dollar_brace = false;
                    parsed
                })
            })
        } else if body[index] == b'{' && body.get(index + 1).copied() == Some(b'$') {
            parse_braced_interpolated_variable(body, index)
        } else {
            None
        };
        let Some(parsed) = parsed else {
            index += 1;
            continue;
        };
        parts.push(InterpolatedPart::Bytes(
            unescape_double_quoted_php_string_with_quote_mode(
                &body[chunk_start..index],
                decode_escaped_quote,
            ),
        ));
        parts.push(InterpolatedPart::Variable {
            name: parsed.name,
            dim: parsed.dim,
            deprecated_dollar_brace: parsed.deprecated_dollar_brace,
        });
        index = parsed.end;
        chunk_start = parsed.end;
    }
    if parts.is_empty() {
        return None;
    }
    parts.push(InterpolatedPart::Bytes(
        unescape_double_quoted_php_string_with_quote_mode(
            &body[chunk_start..],
            decode_escaped_quote,
        ),
    ));
    Some(parts)
}

fn parse_simple_interpolated_variable(
    bytes: &[u8],
    start: usize,
) -> Option<ParsedInterpolatedVariable> {
    let mut index = start + 1;
    if !is_php_variable_start(bytes.get(index).copied()?) {
        return None;
    }
    index += 1;
    while bytes
        .get(index)
        .copied()
        .is_some_and(is_php_variable_continue)
    {
        index += 1;
    }
    let name = std::str::from_utf8(&bytes[start + 1..index])
        .ok()?
        .to_string();
    let (dim, end) = parse_interpolated_dim(bytes, index)
        .map(|(dim, end)| (Some(dim), end))
        .unwrap_or((None, index));
    Some(ParsedInterpolatedVariable {
        name,
        dim,
        end,
        deprecated_dollar_brace: false,
    })
}

fn parse_braced_interpolated_variable(
    bytes: &[u8],
    start: usize,
) -> Option<ParsedInterpolatedVariable> {
    let mut parsed = parse_simple_interpolated_variable(bytes, start + 1)?;
    if bytes.get(parsed.end).copied() != Some(b'}') {
        return None;
    }
    parsed.end += 1;
    Some(parsed)
}

fn parse_braced_interpolated_method_call(
    bytes: &[u8],
    start: usize,
) -> Option<ParsedInterpolatedMethodCall> {
    if bytes.get(start).copied() != Some(b'{') || bytes.get(start + 1).copied() != Some(b'$') {
        return None;
    }
    let mut index = start + 2;
    if !is_php_variable_start(bytes.get(index).copied()?) {
        return None;
    }
    index += 1;
    while bytes
        .get(index)
        .copied()
        .is_some_and(is_php_variable_continue)
    {
        index += 1;
    }
    let receiver = std::str::from_utf8(&bytes[start + 2..index])
        .ok()?
        .to_string();
    if bytes.get(index).copied() != Some(b'-') || bytes.get(index + 1).copied() != Some(b'>') {
        return None;
    }
    index += 2;
    let method_start = index;
    if !is_php_variable_start(bytes.get(index).copied()?) {
        return None;
    }
    index += 1;
    while bytes
        .get(index)
        .copied()
        .is_some_and(is_php_variable_continue)
    {
        index += 1;
    }
    let method = std::str::from_utf8(&bytes[method_start..index])
        .ok()?
        .to_string();
    if bytes.get(index).copied() != Some(b'(')
        || bytes.get(index + 1).copied() != Some(b')')
        || bytes.get(index + 2).copied() != Some(b'}')
    {
        return None;
    }
    Some(ParsedInterpolatedMethodCall {
        receiver,
        method,
        end: index + 3,
    })
}

fn parse_simple_interpolated_property(
    bytes: &[u8],
    start: usize,
) -> Option<ParsedInterpolatedProperty> {
    if bytes.get(start).copied() != Some(b'$') {
        return None;
    }
    let mut index = start + 1;
    if !is_php_variable_start(bytes.get(index).copied()?) {
        return None;
    }
    index += 1;
    while bytes
        .get(index)
        .copied()
        .is_some_and(is_php_variable_continue)
    {
        index += 1;
    }
    let receiver = std::str::from_utf8(&bytes[start + 1..index])
        .ok()?
        .to_string();
    if bytes.get(index).copied() != Some(b'-') || bytes.get(index + 1).copied() != Some(b'>') {
        return None;
    }
    index += 2;
    let property_start = index;
    if !is_php_variable_start(bytes.get(index).copied()?) {
        return None;
    }
    index += 1;
    while bytes
        .get(index)
        .copied()
        .is_some_and(is_php_variable_continue)
    {
        index += 1;
    }
    Some(ParsedInterpolatedProperty {
        receiver,
        property: std::str::from_utf8(&bytes[property_start..index])
            .ok()?
            .to_string(),
        end: index,
    })
}

fn parse_braced_interpolated_property(
    bytes: &[u8],
    start: usize,
) -> Option<ParsedInterpolatedProperty> {
    if bytes.get(start).copied() != Some(b'{') || bytes.get(start + 1).copied() != Some(b'$') {
        return None;
    }
    let mut parsed = parse_simple_interpolated_property(bytes, start + 1)?;
    if bytes.get(parsed.end).copied() != Some(b'}') {
        return None;
    }
    parsed.end += 1;
    Some(parsed)
}

fn parse_deprecated_dollar_brace_interpolated_variable(
    bytes: &[u8],
    start: usize,
) -> Option<ParsedInterpolatedVariable> {
    if bytes.get(start).copied() != Some(b'$') || bytes.get(start + 1).copied() != Some(b'{') {
        return None;
    }
    let mut index = start + 2;
    if !is_php_variable_start(bytes.get(index).copied()?) {
        return None;
    }
    index += 1;
    while bytes
        .get(index)
        .copied()
        .is_some_and(is_php_variable_continue)
    {
        index += 1;
    }
    if bytes.get(index).copied() != Some(b'}') {
        return None;
    }
    Some(ParsedInterpolatedVariable {
        name: std::str::from_utf8(&bytes[start + 2..index])
            .ok()?
            .to_string(),
        dim: None,
        end: index + 1,
        deprecated_dollar_brace: true,
    })
}

fn parse_interpolated_dim(bytes: &[u8], start: usize) -> Option<(InterpolatedDim, usize)> {
    if bytes.get(start).copied() != Some(b'[') {
        return None;
    }
    let end = bytes[start + 1..]
        .iter()
        .position(|byte| *byte == b']')
        .map(|offset| start + 1 + offset)?;
    let inner = &bytes[start + 1..end];
    if inner.is_empty() {
        return None;
    }
    let dim = if inner.first().copied() == Some(b'$') {
        let parsed = parse_simple_interpolated_variable(inner, 0)?;
        if parsed.end != inner.len() || parsed.dim.is_some() {
            return None;
        }
        InterpolatedDim::Variable(parsed.name)
    } else if inner.iter().all(u8::is_ascii_digit) {
        InterpolatedDim::Int(std::str::from_utf8(inner).ok()?.parse().ok()?)
    } else if is_quoted_interpolated_dim(inner) {
        InterpolatedDim::String(
            std::str::from_utf8(&inner[1..inner.len() - 1])
                .ok()?
                .to_string(),
        )
    } else if inner.first().copied().is_some_and(is_php_variable_start)
        && inner.iter().skip(1).copied().all(is_php_variable_continue)
    {
        InterpolatedDim::String(std::str::from_utf8(inner).ok()?.to_string())
    } else {
        return None;
    };
    Some((dim, end + 1))
}

fn is_quoted_interpolated_dim(inner: &[u8]) -> bool {
    inner.len() >= 2
        && matches!(
            (inner.first().copied(), inner.last().copied()),
            (Some(b'\''), Some(b'\'')) | (Some(b'"'), Some(b'"'))
        )
}

fn is_php_variable_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic() || byte >= 0x80
}

fn is_php_variable_continue(byte: u8) -> bool {
    is_php_variable_start(byte) || byte.is_ascii_digit()
}

fn unescape_single_quoted_php_string(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len());
    let mut index = 0;
    while index < body.len() {
        let byte = body[index];
        if byte == b'\\' {
            match body.get(index + 1).copied() {
                Some(b'\\') => {
                    out.push(b'\\');
                    index += 2;
                }
                Some(b'\'') => {
                    out.push(b'\'');
                    index += 2;
                }
                Some(next) => {
                    out.push(b'\\');
                    out.push(next);
                    index += 2;
                }
                None => {
                    out.push(b'\\');
                    index += 1;
                }
            }
        } else {
            out.push(byte);
            index += 1;
        }
    }
    out
}

fn unescape_double_quoted_php_string(body: &[u8]) -> Vec<u8> {
    unescape_double_quoted_php_string_with_quote_mode(body, true)
}

fn unescape_heredoc_php_string(body: &[u8]) -> Vec<u8> {
    unescape_double_quoted_php_string_with_quote_mode(body, false)
}

fn unescape_double_quoted_php_string_with_quote_mode(
    body: &[u8],
    decode_escaped_quote: bool,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len());
    let mut index = 0;
    while index < body.len() {
        let byte = body[index];
        if byte != b'\\' {
            out.push(byte);
            index += 1;
            continue;
        }
        let Some(next) = body.get(index + 1).copied() else {
            out.push(b'\\');
            index += 1;
            continue;
        };
        match next {
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'v' => out.push(0x0b),
            b'e' => out.push(0x1b),
            b'f' => out.push(0x0c),
            b'\\' => out.push(b'\\'),
            b'$' => out.push(b'$'),
            b'"' if decode_escaped_quote => out.push(b'"'),
            b'"' => {
                out.push(b'\\');
                out.push(b'"');
            }
            b'x' | b'X' => {
                let (value, consumed) = decode_hex_escape(&body[index + 2..]);
                if consumed == 0 {
                    out.push(b'\\');
                    out.push(next);
                    index += 2;
                    continue;
                }
                out.push(value);
                index += 2 + consumed;
                continue;
            }
            b'u' if body.get(index + 2).copied() == Some(b'{') => {
                if let Some((bytes, consumed)) = decode_unicode_escape(&body[index + 3..]) {
                    out.extend_from_slice(&bytes);
                    index += 3 + consumed;
                    continue;
                }
                out.push(b'\\');
                out.push(next);
            }
            b'0'..=b'7' => {
                let (value, consumed) = decode_octal_escape(&body[index + 1..]);
                out.push(value);
                index += 1 + consumed;
                continue;
            }
            _ => {
                out.push(b'\\');
                out.push(next);
            }
        }
        index += 2;
    }
    out
}

fn decode_hex_escape(bytes: &[u8]) -> (u8, usize) {
    let mut value = 0u8;
    let mut consumed = 0;
    for byte in bytes.iter().take(2).copied() {
        let Some(nibble) = hex_nibble(byte) else {
            break;
        };
        value = (value << 4) | nibble;
        consumed += 1;
    }
    (value, consumed)
}

fn decode_octal_escape(bytes: &[u8]) -> (u8, usize) {
    let mut value = 0u16;
    let mut consumed = 0;
    for byte in bytes.iter().take(3).copied() {
        if !(b'0'..=b'7').contains(&byte) {
            break;
        }
        value = (value << 3) | u16::from(byte - b'0');
        consumed += 1;
    }
    (value as u8, consumed)
}

fn decode_unicode_escape(bytes: &[u8]) -> Option<(Vec<u8>, usize)> {
    let mut value = 0u32;
    for (consumed, byte) in bytes.iter().copied().enumerate() {
        if byte == b'}' {
            if consumed == 0 {
                return None;
            }
            let ch = char::from_u32(value)?;
            let mut encoded = [0; 4];
            return Some((
                ch.encode_utf8(&mut encoded).as_bytes().to_vec(),
                consumed + 1,
            ));
        }
        let nibble = hex_nibble(byte)?;
        value = value.checked_mul(16)?.checked_add(u32::from(nibble))?;
    }
    None
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
