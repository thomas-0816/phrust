//! Byte-level JSON primitives consumed by authoritative native value walkers.

use std::fmt::Write as _;

/// Escapes like serde_json plus the PHP-default post passes: `/` becomes
/// `\/` and every non-ASCII scalar becomes lowercase `\uXXXX` (surrogate
/// pairs above the BMP). Invalid UTF-8 defers to the generic path.
/// Appends one PHP-default JSON string directly from bytes.
///
/// Native exact handlers use this narrow primitive while walking their own
/// authoritative value slots. It deliberately owns no `Value` conversion or
/// request state; unsupported byte sequences are reported before publication.
pub fn append_json_default_string(bytes: &[u8], output: &mut String) -> Result<(), &'static str> {
    if php_source::byte_kernel::find_json_escape_byte(bytes).is_none() {
        let text = std::str::from_utf8(bytes).map_err(|_| "invalid_utf8")?;
        output.reserve(text.len() + 2);
        output.push('"');
        output.push_str(text);
        output.push('"');
        return Ok(());
    }
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
    use super::super::core::normalize_json_encoded;
    use super::*;

    fn assert_string_parity(bytes: &[u8]) {
        let text = std::str::from_utf8(bytes).expect("test string is valid UTF-8");
        let expected = normalize_json_encoded(
            serde_json::to_string(text).expect("serde string encode succeeds"),
            0,
        );
        let mut encoded = String::new();
        append_json_default_string(bytes, &mut encoded).expect("native byte escape succeeds");
        assert_eq!(encoded, expected);
    }

    #[test]
    fn every_ascii_char_escapes_like_generic_pipeline() {
        for byte in 0_u8..=0x7F {
            assert_string_parity(&[b'a', byte, b'z']);
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
            assert_string_parity(text.as_bytes());
        }
    }

    #[test]
    fn invalid_utf8_is_rejected_before_publication() {
        assert_eq!(
            append_json_default_string(&[0xFF, 0xFE], &mut String::new()),
            Err("invalid_utf8")
        );
    }
}
