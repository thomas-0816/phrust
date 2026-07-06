//! Exact-case fast paths for hot app-flow string builtins.
//!
//! Each helper is the single semantic implementation for its exact common
//! case: the generic registry builtins delegate here (or share the same
//! byte kernels), and the VM builtin-intrinsic hook calls the same
//! functions, so the fast and generic paths cannot diverge.

use super::core::{
    HTML_ESCAPE_DEFAULT_FLAGS, find_bytes_from, html_escape_with_options, normalize_offset,
    replace_all,
};
use crate::{PhpArray, PhpString, Value};
use php_source::byte_kernel;

/// `strtoupper($string)`: PHP 8 case conversion is byte-based ASCII.
///
/// Shares the input storage when no byte needs conversion.
pub fn strtoupper_ascii(input: &PhpString) -> PhpString {
    let bytes = input.as_bytes();
    if !byte_kernel::contains_ascii_lowercase(bytes) {
        return input.clone();
    }
    PhpString::from_bytes(byte_kernel::ascii_uppercase_copy(bytes))
}

/// `strtolower($string)`: PHP 8 case conversion is byte-based ASCII.
///
/// Shares the input storage when no byte needs conversion.
pub fn strtolower_ascii(input: &PhpString) -> PhpString {
    let bytes = input.as_bytes();
    if !byte_kernel::contains_ascii_uppercase(bytes) {
        return input.clone();
    }
    PhpString::from_bytes(byte_kernel::ascii_lowercase_copy(bytes))
}

/// `str_replace($search, $replace, $subject)` with scalar string arguments.
///
/// Shares the subject storage when the needle does not occur (or is empty,
/// which the generic path treats as no replacement).
pub fn str_replace_scalar(
    search: &PhpString,
    replace: &PhpString,
    subject: &PhpString,
) -> PhpString {
    let needle = search.as_bytes();
    if needle.is_empty() || find_bytes_from(subject.as_bytes(), needle, 0, false).is_none() {
        return subject.clone();
    }
    let mut count = 0_i64;
    PhpString::from_bytes(replace_all(
        subject.as_bytes(),
        needle,
        replace.as_bytes(),
        &mut count,
    ))
}

/// `htmlspecialchars($string)` with default flags and `double_encode`.
///
/// Shares the input storage when no byte belongs to the escape set.
pub fn htmlspecialchars_default(input: &PhpString) -> PhpString {
    let bytes = input.as_bytes();
    if byte_kernel::find_html_escape_byte(bytes).is_none() {
        return input.clone();
    }
    PhpString::from_bytes(html_escape_with_options(
        bytes,
        HTML_ESCAPE_DEFAULT_FLAGS,
        true,
    ))
}

/// `explode($separator, $string)` with a single-byte separator and no limit.
///
/// The parts vector is sized exactly from a separator pre-count.
pub fn explode_single_byte(separator: u8, subject: &PhpString) -> PhpArray {
    let bytes = subject.as_bytes();
    let mut parts = Vec::with_capacity(byte_kernel::count_byte(bytes, separator) + 1);
    let mut start = 0;
    while let Some(offset) = byte_kernel::find_byte(&bytes[start..], separator) {
        parts.push(Value::string(bytes[start..start + offset].to_vec()));
        start += offset + 1;
    }
    parts.push(Value::string(bytes[start..].to_vec()));
    PhpArray::from_packed(parts)
}

/// `substr($string, $offset, $length)` over byte offsets, sharing the
/// generic builtin's normalization: negative offsets count from the end,
/// negative lengths trim from the end, and out-of-range slices are empty.
#[must_use]
pub fn substr_bytes(string: &PhpString, offset: i64, length: Option<i64>) -> PhpString {
    let bytes = string.as_bytes();
    let start = normalize_offset(bytes.len(), offset);
    let end = match length {
        None => bytes.len(),
        Some(length) if length >= 0 => start.saturating_add(length as usize).min(bytes.len()),
        Some(length) => bytes.len().saturating_sub(length.unsigned_abs() as usize),
    };
    if start >= bytes.len() || end < start {
        return PhpString::from(&b""[..]);
    }
    if start == 0 && end == bytes.len() {
        return string.clone();
    }
    PhpString::from(&bytes[start..end])
}

/// `trim($string)` with the default character mask, sharing the generic
/// builtin's mask table; an untrimmed input shares its storage.
#[must_use]
pub fn trim_ascii_default(string: &PhpString) -> PhpString {
    let bytes = string.as_bytes();
    let (start, end) = byte_kernel::trim_default_bounds(bytes);
    if start == 0 && end == bytes.len() {
        return string.clone();
    }
    PhpString::from(&bytes[start..end])
}

/// Joins all-string array parts with an exact-capacity byte buffer. Returns
/// `None` when any element is not already a string, letting the caller fall
/// back to the generic conversion-aware implode.
#[must_use]
pub fn implode_string_parts(separator: &PhpString, parts: &PhpArray) -> Option<PhpString> {
    let mut total = 0usize;
    let mut count = 0usize;
    for (_, value) in parts.iter() {
        let Value::String(part) = value else {
            return None;
        };
        total += part.len();
        count += 1;
    }
    if count == 0 {
        return Some(PhpString::from_bytes(Vec::new()));
    }
    total += separator.len() * (count - 1);
    let mut joined = Vec::with_capacity(total);
    for (index, (_, value)) in parts.iter().enumerate() {
        let Value::String(part) = value else {
            return None;
        };
        if index > 0 {
            joined.extend_from_slice(separator.as_bytes());
        }
        joined.extend_from_slice(part.as_bytes());
    }
    Some(PhpString::from_bytes(joined))
}

#[cfg(test)]
mod tests {
    use super::super::core::split_bytes;
    use super::*;

    fn string(bytes: &[u8]) -> PhpString {
        PhpString::from_bytes(bytes.to_vec())
    }

    #[test]
    fn strtoupper_matches_generic_byte_map() {
        for input in [
            &b""[..],
            b"already UPPER 123",
            b"MiXeD case",
            b"binary \x00\xff\x80 tail",
            b"umlaut \xc3\xa4",
        ] {
            let expected: Vec<u8> = input.iter().map(u8::to_ascii_uppercase).collect();
            assert_eq!(strtoupper_ascii(&string(input)).as_bytes(), &expected[..]);
        }
    }

    #[test]
    fn strtolower_matches_generic_byte_map() {
        for input in [
            &b""[..],
            b"already lower 123",
            b"MiXeD case",
            b"binary \x00\xff\x80 tail",
            b"umlaut \xc3\x84",
        ] {
            let expected: Vec<u8> = input.iter().map(u8::to_ascii_lowercase).collect();
            assert_eq!(strtolower_ascii(&string(input)).as_bytes(), &expected[..]);
        }
    }

    #[test]
    fn str_replace_scalar_matches_replace_all() {
        for (search, replace, subject) in [
            (&b","[..], &b";"[..], &b"a,b,c"[..]),
            (b"aa", b"b", b"aaa"),
            (b"x", b"", b"xxhelloxx"),
            (b"missing", b"y", b"subject"),
            (b"", b"y", b"subject"),
            (b"\x00", b"\xff", b"a\x00b\x00"),
            (b"ab", b"longer text", b"ab ab ab"),
        ] {
            let mut count = 0_i64;
            let expected = if search.is_empty() {
                subject.to_vec()
            } else {
                replace_all(subject, search, replace, &mut count)
            };
            let actual = str_replace_scalar(&string(search), &string(replace), &string(subject));
            assert_eq!(actual.as_bytes(), &expected[..]);
        }
    }

    #[test]
    fn htmlspecialchars_default_matches_generic_escape() {
        for input in [
            &b""[..],
            b"plain text",
            b"a < b && c > d",
            b"quotes \" and ' mixed",
            b"&amp; is double encoded",
            b"umlaut \xc3\xa4 <tag>",
        ] {
            let expected = html_escape_with_options(input, HTML_ESCAPE_DEFAULT_FLAGS, true);
            assert_eq!(
                htmlspecialchars_default(&string(input)).as_bytes(),
                &expected[..]
            );
        }
    }

    #[test]
    fn explode_single_byte_matches_split_bytes() {
        for (separator, subject) in [
            (b',', &b""[..]),
            (b',', b"a"),
            (b',', b",a,"),
            (b',', b"a,,b"),
            (b',', b"trailing,"),
            (b'\x00', b"a\x00b\x00c"),
        ] {
            let expected: Vec<Vec<u8>> = split_bytes(subject, &[separator]);
            let actual = explode_single_byte(separator, &string(subject));
            let actual_parts: Vec<Vec<u8>> = actual
                .iter()
                .map(|(_, value)| match value {
                    Value::String(part) => part.as_bytes().to_vec(),
                    other => panic!("expected string part, got {other:?}"),
                })
                .collect();
            assert_eq!(actual_parts, expected);
        }
    }
}
