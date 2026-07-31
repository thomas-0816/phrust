//! Byte-level JSON primitives consumed by authoritative native value walkers.

pub const NATIVE_JSON_HEX_TAG: i64 = 1;
pub const NATIVE_JSON_HEX_AMP: i64 = 2;
pub const NATIVE_JSON_HEX_APOS: i64 = 4;
pub const NATIVE_JSON_HEX_QUOT: i64 = 8;
pub const NATIVE_JSON_FORCE_OBJECT: i64 = 16;
pub const NATIVE_JSON_NUMERIC_CHECK: i64 = 32;
pub const NATIVE_JSON_UNESCAPED_SLASHES: i64 = 64;
pub const NATIVE_JSON_PRETTY_PRINT: i64 = 128;
pub const NATIVE_JSON_UNESCAPED_UNICODE: i64 = 256;
pub const NATIVE_JSON_PARTIAL_OUTPUT_ON_ERROR: i64 = 512;
pub const NATIVE_JSON_PRESERVE_ZERO_FRACTION: i64 = 1024;
pub const NATIVE_JSON_UNESCAPED_LINE_TERMINATORS: i64 = 2048;
pub const NATIVE_JSON_INVALID_UTF8_IGNORE: i64 = 1_048_576;
pub const NATIVE_JSON_INVALID_UTF8_SUBSTITUTE: i64 = 2_097_152;
pub const NATIVE_JSON_THROW_ON_ERROR: i64 = 4_194_304;

pub const NATIVE_JSON_DIRECT_ENCODE_FLAGS: i64 = NATIVE_JSON_HEX_TAG
    | NATIVE_JSON_HEX_AMP
    | NATIVE_JSON_HEX_APOS
    | NATIVE_JSON_HEX_QUOT
    | NATIVE_JSON_FORCE_OBJECT
    | NATIVE_JSON_NUMERIC_CHECK
    | NATIVE_JSON_UNESCAPED_SLASHES
    | NATIVE_JSON_PRETTY_PRINT
    | NATIVE_JSON_UNESCAPED_UNICODE
    | NATIVE_JSON_PRESERVE_ZERO_FRACTION
    | NATIVE_JSON_UNESCAPED_LINE_TERMINATORS
    | NATIVE_JSON_INVALID_UTF8_IGNORE
    | NATIVE_JSON_INVALID_UTF8_SUBSTITUTE;

/// Escapes like serde_json plus the PHP-default post passes: `/` becomes
/// `\/` and every non-ASCII scalar becomes lowercase `\uXXXX` (surrogate
/// pairs above the BMP). Invalid UTF-8 defers to the generic path.
/// Appends one PHP-default JSON string directly from bytes.
///
/// Native exact handlers use this narrow primitive while walking their own
/// authoritative value slots. It deliberately owns no `Value` conversion or
/// request state; unsupported byte sequences are reported before publication.
pub fn append_json_default_string(bytes: &[u8], output: &mut String) -> Result<(), &'static str> {
    append_json_string_with_flags(bytes, output, 0)
}

/// Appends a native UTF-8 string with PHP's representation-preserving JSON
/// flags. Flags that change type interpretation or error control stay on the
/// exact baseline continuation and are therefore rejected by the caller
/// before this byte primitive is entered.
pub fn append_json_string_with_flags(
    bytes: &[u8],
    output: &mut String,
    flags: i64,
) -> Result<(), &'static str> {
    visit_json_string_with_flags(bytes, flags, |encoded| {
        output.push_str(std::str::from_utf8(encoded).map_err(|_| "invalid_utf8")?);
        Ok(())
    })
}

/// Visits one PHP JSON string as encoded byte ranges without allocating.
///
/// The callback receives only UTF-8/ASCII output ranges and is invoked in
/// final wire order, including the surrounding quotes.
pub fn visit_json_string_with_flags(
    bytes: &[u8],
    flags: i64,
    mut emit: impl FnMut(&[u8]) -> Result<(), &'static str>,
) -> Result<(), &'static str> {
    emit(b"\"")?;
    let mut rest = bytes;
    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(valid) => {
                visit_valid_json_text(valid, flags, &mut emit)?;
                rest = &[];
            }
            Err(error) => {
                let valid_up_to = error.valid_up_to();
                if valid_up_to != 0 {
                    let valid =
                        std::str::from_utf8(&rest[..valid_up_to]).map_err(|_| "invalid_utf8")?;
                    visit_valid_json_text(valid, flags, &mut emit)?;
                }
                if flags & NATIVE_JSON_INVALID_UTF8_IGNORE == 0 {
                    if flags & NATIVE_JSON_INVALID_UTF8_SUBSTITUTE == 0 {
                        return Err("invalid_utf8");
                    }
                    visit_json_char_with_flags('\u{fffd}', flags, &mut emit)?;
                }
                let skip = error.error_len().unwrap_or(1);
                rest = &rest[valid_up_to.saturating_add(skip)..];
            }
        }
    }
    emit(b"\"")
}

fn visit_valid_json_text(
    text: &str,
    flags: i64,
    emit: &mut impl FnMut(&[u8]) -> Result<(), &'static str>,
) -> Result<(), &'static str> {
    for ch in text.chars() {
        visit_json_char_with_flags(ch, flags, emit)?;
    }
    Ok(())
}

fn visit_json_char_with_flags(
    ch: char,
    flags: i64,
    emit: &mut impl FnMut(&[u8]) -> Result<(), &'static str>,
) -> Result<(), &'static str> {
    match ch {
        '"' if flags & NATIVE_JSON_HEX_QUOT != 0 => emit(b"\\u0022"),
        '"' => emit(b"\\\""),
        '\\' => emit(b"\\\\"),
        '/' if flags & NATIVE_JSON_UNESCAPED_SLASHES == 0 => emit(b"\\/"),
        '\u{8}' => emit(b"\\b"),
        '\t' => emit(b"\\t"),
        '\n' => emit(b"\\n"),
        '\u{c}' => emit(b"\\f"),
        '\r' => emit(b"\\r"),
        '<' if flags & NATIVE_JSON_HEX_TAG != 0 => emit(b"\\u003C"),
        '>' if flags & NATIVE_JSON_HEX_TAG != 0 => emit(b"\\u003E"),
        '&' if flags & NATIVE_JSON_HEX_AMP != 0 => emit(b"\\u0026"),
        '\'' if flags & NATIVE_JSON_HEX_APOS != 0 => emit(b"\\u0027"),
        '\u{2028}' | '\u{2029}' if flags & NATIVE_JSON_UNESCAPED_LINE_TERMINATORS == 0 => {
            emit_json_hex_escape(ch as u32, emit)
        }
        _ if (ch as u32) < 0x20 => emit_json_hex_escape(ch as u32, emit),
        _ if ch.is_ascii() || flags & NATIVE_JSON_UNESCAPED_UNICODE != 0 => {
            let mut encoded = [0_u8; 4];
            emit(ch.encode_utf8(&mut encoded).as_bytes())
        }
        _ => {
            let code = ch as u32;
            if code <= 0xFFFF {
                emit_json_hex_escape(code, emit)
            } else {
                let code = code - 0x1_0000;
                let high = 0xD800 + ((code >> 10) & 0x3FF);
                let low = 0xDC00 + (code & 0x3FF);
                emit_json_hex_escape(high, emit)?;
                emit_json_hex_escape(low, emit)
            }
        }
    }
}

fn emit_json_hex_escape(
    value: u32,
    emit: &mut impl FnMut(&[u8]) -> Result<(), &'static str>,
) -> Result<(), &'static str> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = *b"\\u0000";
    encoded[2] = HEX[((value >> 12) & 0xF) as usize];
    encoded[3] = HEX[((value >> 8) & 0xF) as usize];
    encoded[4] = HEX[((value >> 4) & 0xF) as usize];
    encoded[5] = HEX[(value & 0xF) as usize];
    emit(&encoded)
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

    #[test]
    fn invalid_utf8_flags_match_php_recovery_rules() {
        let bytes = b"a\xffb";
        let mut encoded = String::new();
        append_json_string_with_flags(bytes, &mut encoded, NATIVE_JSON_INVALID_UTF8_IGNORE)
            .expect("ignore flag accepts invalid bytes");
        assert_eq!(encoded, "\"ab\"");

        encoded.clear();
        append_json_string_with_flags(bytes, &mut encoded, NATIVE_JSON_INVALID_UTF8_SUBSTITUTE)
            .expect("substitute flag accepts invalid bytes");
        assert_eq!(encoded, "\"a\\ufffdb\"");

        encoded.clear();
        append_json_string_with_flags(
            bytes,
            &mut encoded,
            NATIVE_JSON_INVALID_UTF8_SUBSTITUTE | NATIVE_JSON_UNESCAPED_UNICODE,
        )
        .expect("unescaped substitute flag accepts invalid bytes");
        assert_eq!(encoded, "\"a\u{fffd}b\"");
    }

    #[test]
    fn representation_flags_match_the_generic_pipeline() {
        let text = "<tag attr='quoted'>& / \u{e4} \u{2028} \u{1f600}";
        for flags in [
            NATIVE_JSON_HEX_TAG | NATIVE_JSON_HEX_AMP | NATIVE_JSON_HEX_APOS | NATIVE_JSON_HEX_QUOT,
            NATIVE_JSON_UNESCAPED_SLASHES,
            NATIVE_JSON_UNESCAPED_UNICODE,
            NATIVE_JSON_UNESCAPED_UNICODE | NATIVE_JSON_UNESCAPED_LINE_TERMINATORS,
            NATIVE_JSON_HEX_TAG
                | NATIVE_JSON_HEX_AMP
                | NATIVE_JSON_HEX_APOS
                | NATIVE_JSON_HEX_QUOT
                | NATIVE_JSON_UNESCAPED_SLASHES
                | NATIVE_JSON_UNESCAPED_UNICODE
                | NATIVE_JSON_UNESCAPED_LINE_TERMINATORS,
        ] {
            let expected = normalize_json_encoded(
                serde_json::to_string(text).expect("serde string encode succeeds"),
                flags,
            );
            let mut encoded = String::new();
            append_json_string_with_flags(text.as_bytes(), &mut encoded, flags)
                .expect("native byte escape succeeds");
            assert_eq!(encoded, expected, "flags={flags}");
        }
    }
}
