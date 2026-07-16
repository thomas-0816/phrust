pub(super) fn escape_json(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch <= '\u{1f}' => {
                use std::fmt::Write as _;
                write!(out, "\\u{:04x}", ch as u32).expect("writing to a String cannot fail");
            }
            ch => out.push(ch),
        }
    }
    out
}

fn parse_json_string_contents(value: &str, field: &str) -> Result<(String, usize), String> {
    let mut out = String::new();
    let mut chars = value.char_indices();
    while let Some((offset, ch)) = chars.next() {
        match ch {
            '"' => return Ok((out, offset + ch.len_utf8())),
            '\\' => {
                let (_, escape) = chars
                    .next()
                    .ok_or_else(|| format!("unterminated escape in {field}"))?;
                match escape {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'b' => out.push('\u{08}'),
                    'f' => out.push('\u{0c}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        let mut codepoint = 0_u32;
                        for _ in 0..4 {
                            let (_, digit) = chars
                                .next()
                                .ok_or_else(|| format!("unterminated unicode escape in {field}"))?;
                            codepoint = (codepoint << 4)
                                | digit
                                    .to_digit(16)
                                    .ok_or_else(|| format!("invalid unicode escape in {field}"))?;
                        }
                        let decoded = char::from_u32(codepoint)
                            .ok_or_else(|| format!("invalid unicode scalar in {field}"))?;
                        out.push(decoded);
                    }
                    _ => return Err(format!("unsupported escape in {field}")),
                }
            }
            ch if ch <= '\u{1f}' => {
                return Err(format!("unescaped control character in {field}"));
            }
            ch => out.push(ch),
        }
    }
    Err(format!("unterminated string in {field}"))
}

pub(super) fn extract_json_string(line: &str, key: &str) -> Result<String, String> {
    let needle = format!("\"{key}\":\"");
    let start = line
        .find(&needle)
        .ok_or_else(|| format!("missing string field `{key}`"))?
        + needle.len();
    parse_json_string_contents(&line[start..], &format!("field `{key}`")).map(|(value, _)| value)
}

pub(super) fn extract_optional_json_string(
    line: &str,
    key: &str,
) -> Result<Option<String>, String> {
    let needle = format!("\"{key}\":\"");
    if !line.contains(&needle) {
        return Ok(None);
    }
    extract_json_string(line, key).map(Some)
}

pub(super) fn extract_json_bool(line: &str, key: &str) -> Result<bool, String> {
    let needle = format!("\"{key}\":");
    let start = line
        .find(&needle)
        .ok_or_else(|| format!("missing bool field `{key}`"))?
        + needle.len();
    if line[start..].starts_with("true") {
        Ok(true)
    } else if line[start..].starts_with("false") {
        Ok(false)
    } else {
        Err(format!("invalid bool field `{key}`"))
    }
}

pub(super) fn extract_json_string_array(line: &str, key: &str) -> Result<Vec<String>, String> {
    let needle = format!("\"{key}\":[");
    let start = line
        .find(&needle)
        .ok_or_else(|| format!("missing array field `{key}`"))?
        + needle.len();
    let mut values = Vec::new();
    let mut index = start;
    loop {
        let rest = &line[index..];
        if rest.starts_with(']') {
            return Ok(values);
        }
        if !rest.starts_with('"') {
            return Err(format!("invalid array field `{key}`"));
        }
        index += 1;
        let (value, consumed) =
            parse_json_string_contents(&line[index..], &format!("array field `{key}`"))?;
        index += consumed;
        values.push(value);
        let rest = &line[index..];
        if rest.starts_with(',') {
            index += 1;
        } else if rest.starts_with(']') {
            return Ok(values);
        } else {
            return Err(format!("unterminated array field `{key}`"));
        }
    }
}

pub(super) fn extract_json_u64(line: &str, key: &str) -> Result<u64, String> {
    let needle = format!("\"{key}\":");
    let start = line
        .find(&needle)
        .ok_or_else(|| format!("missing numeric field `{key}`"))?
        + needle.len();
    let digits = line[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return Err(format!("empty numeric field `{key}`"));
    }
    digits
        .parse()
        .map_err(|error| format!("invalid numeric field `{key}`: {error}"))
}

pub(super) fn extract_json_usize(line: &str, key: &str) -> Result<usize, String> {
    extract_json_u64(line, key).and_then(|value| {
        usize::try_from(value)
            .map_err(|error| format!("numeric field `{key}` is too large: {error}"))
    })
}
