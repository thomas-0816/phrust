//! Safe byte-oriented scanning helpers.
//!
//! These helpers are intentionally small facades around scalar reference logic
//! and well-maintained optimized byte-search routines. Public APIs stay safe and
//! byte-oriented so source, lexer, and runtime string callers can share the same
//! behavior without introducing UTF-8 assumptions.

/// Returns the active byte-kernel backend selected for this process.
#[must_use]
pub fn active_backend_name() -> &'static str {
    arch::active_backend_name()
}

/// Finds the first occurrence of `needle`.
#[must_use]
pub fn find_byte(bytes: &[u8], needle: u8) -> Option<usize> {
    memchr::memchr(needle, bytes)
}

/// Finds the last occurrence of `needle`.
#[must_use]
pub fn rfind_byte(bytes: &[u8], needle: u8) -> Option<usize> {
    memchr::memrchr(needle, bytes)
}

/// Scalar reference implementation for [`find_byte`].
#[must_use]
pub fn find_byte_scalar(bytes: &[u8], needle: u8) -> Option<usize> {
    bytes.iter().position(|byte| *byte == needle)
}

/// Scalar reference implementation for [`rfind_byte`].
#[must_use]
pub fn rfind_byte_scalar(bytes: &[u8], needle: u8) -> Option<usize> {
    bytes.iter().rposition(|byte| *byte == needle)
}

/// Finds the first occurrence of either byte.
#[must_use]
pub fn find_any2(bytes: &[u8], first: u8, second: u8) -> Option<usize> {
    memchr::memchr2(first, second, bytes)
}

/// Scalar reference implementation for [`find_any2`].
#[must_use]
pub fn find_any2_scalar(bytes: &[u8], first: u8, second: u8) -> Option<usize> {
    bytes
        .iter()
        .position(|byte| *byte == first || *byte == second)
}

/// Finds the first occurrence of any of three bytes.
#[must_use]
pub fn find_any3(bytes: &[u8], first: u8, second: u8, third: u8) -> Option<usize> {
    memchr::memchr3(first, second, third, bytes)
}

/// Scalar reference implementation for [`find_any3`].
#[must_use]
pub fn find_any3_scalar(bytes: &[u8], first: u8, second: u8, third: u8) -> Option<usize> {
    bytes
        .iter()
        .position(|byte| *byte == first || *byte == second || *byte == third)
}

/// Finds the first occurrence of a byte substring.
#[must_use]
pub fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    find_bytes_from(haystack, needle, 0)
}

/// Finds the first occurrence of a byte substring at or after `start`.
#[must_use]
pub fn find_bytes_from(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(start.min(haystack.len()));
    }
    if start > haystack.len() || needle.len() > haystack.len().saturating_sub(start) {
        return None;
    }
    match needle {
        [single] => find_byte(&haystack[start..], *single).map(|offset| start + offset),
        _ => memchr::memmem::find(&haystack[start..], needle).map(|offset| start + offset),
    }
}

/// Finds the last occurrence of a byte substring at or before `end`.
#[must_use]
pub fn rfind_bytes_before(haystack: &[u8], needle: &[u8], end: usize) -> Option<usize> {
    let end = end.min(haystack.len());
    if needle.is_empty() {
        return Some(end);
    }
    if needle.len() > end {
        return None;
    }
    match needle {
        [single] => rfind_byte(&haystack[..end], *single),
        _ => memchr::memmem::rfind(&haystack[..end], needle),
    }
}

/// Scalar reference implementation for [`find_bytes_from`].
#[must_use]
pub fn find_bytes_from_scalar(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(start.min(haystack.len()));
    }
    if start > haystack.len() || needle.len() > haystack.len().saturating_sub(start) {
        return None;
    }
    haystack[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| start + offset)
}

/// Scalar reference implementation for [`rfind_bytes_before`].
#[must_use]
pub fn rfind_bytes_before_scalar(haystack: &[u8], needle: &[u8], end: usize) -> Option<usize> {
    let end = end.min(haystack.len());
    if needle.is_empty() {
        return Some(end);
    }
    if needle.len() > end {
        return None;
    }
    haystack[..end]
        .windows(needle.len())
        .rposition(|window| window == needle)
}

/// Finds an ASCII case-insensitive byte substring at or after `start`.
///
/// Non-ASCII bytes compare byte-for-byte, matching Rust/PHP ASCII-folding
/// behavior used by the runtime string builtins.
#[must_use]
pub fn find_bytes_ascii_case_insensitive_from(
    haystack: &[u8],
    needle: &[u8],
    start: usize,
) -> Option<usize> {
    if needle.is_empty() {
        return Some(start.min(haystack.len()));
    }
    if start > haystack.len() || needle.len() > haystack.len().saturating_sub(start) {
        return None;
    }
    if needle.len() == 1 {
        return find_ascii_folded_byte(&haystack[start..], needle[0]).map(|offset| start + offset);
    }

    let first = needle[0];
    let mut search = start;
    while search <= haystack.len().saturating_sub(needle.len()) {
        let index =
            find_ascii_folded_byte(&haystack[search..], first).map(|offset| search + offset)?;
        if index + needle.len() > haystack.len() {
            return None;
        }
        if haystack[index..index + needle.len()].eq_ignore_ascii_case(needle) {
            return Some(index);
        }
        search = index + 1;
    }
    None
}

/// Finds an ASCII case-insensitive byte substring whose start is at or before
/// `end - needle.len()`.
#[must_use]
pub fn rfind_bytes_ascii_case_insensitive_before(
    haystack: &[u8],
    needle: &[u8],
    end: usize,
) -> Option<usize> {
    let end = end.min(haystack.len());
    if needle.is_empty() {
        return Some(end);
    }
    if needle.len() > end {
        return None;
    }
    if needle.len() == 1 {
        return rfind_ascii_folded_byte(&haystack[..end], needle[0]);
    }

    let first = needle[0];
    let mut search_end = end - needle.len() + 1;
    while search_end > 0 {
        let index = rfind_ascii_folded_byte(&haystack[..search_end], first)?;
        if haystack[index..index + needle.len()].eq_ignore_ascii_case(needle) {
            return Some(index);
        }
        search_end = index;
    }
    None
}

/// Scalar reference implementation for [`find_bytes_ascii_case_insensitive_from`].
#[must_use]
pub fn find_bytes_ascii_case_insensitive_from_scalar(
    haystack: &[u8],
    needle: &[u8],
    start: usize,
) -> Option<usize> {
    if needle.is_empty() {
        return Some(start.min(haystack.len()));
    }
    if start > haystack.len() || needle.len() > haystack.len().saturating_sub(start) {
        return None;
    }
    haystack[start..]
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
        .map(|offset| start + offset)
}

/// Scalar reference implementation for [`rfind_bytes_ascii_case_insensitive_before`].
#[must_use]
pub fn rfind_bytes_ascii_case_insensitive_before_scalar(
    haystack: &[u8],
    needle: &[u8],
    end: usize,
) -> Option<usize> {
    let end = end.min(haystack.len());
    if needle.is_empty() {
        return Some(end);
    }
    if needle.len() > end {
        return None;
    }
    haystack[..end]
        .windows(needle.len())
        .rposition(|window| window.eq_ignore_ascii_case(needle))
}

/// Counts occurrences of `needle`.
#[must_use]
pub fn count_byte(bytes: &[u8], needle: u8) -> usize {
    memchr::memchr_iter(needle, bytes).count()
}

/// Scalar reference implementation for [`count_byte`].
#[must_use]
pub fn count_byte_scalar(bytes: &[u8], needle: u8) -> usize {
    bytes.iter().filter(|byte| **byte == needle).count()
}

/// Counts PHP source line breaks.
///
/// `\n`, `\r\n`, and standalone `\r` each count as one line break. This
/// matches [`crate::LineIndex`] and keeps byte offsets as the source of truth.
#[must_use]
pub fn count_newlines(bytes: &[u8]) -> usize {
    let mut count = 0;
    let mut offset = 0;

    while offset < bytes.len() {
        let Some(relative) = memchr::memchr2(b'\n', b'\r', &bytes[offset..]) else {
            break;
        };
        offset += relative;
        count += 1;

        if bytes[offset] == b'\r' && bytes.get(offset + 1) == Some(&b'\n') {
            offset += 2;
        } else {
            offset += 1;
        }
    }

    count
}

/// Scalar reference implementation for [`count_newlines`].
#[must_use]
pub fn count_newlines_scalar(bytes: &[u8]) -> usize {
    let mut count = 0;
    let mut offset = 0;

    while offset < bytes.len() {
        match bytes[offset] {
            b'\n' => {
                count += 1;
                offset += 1;
            }
            b'\r' if bytes.get(offset + 1) == Some(&b'\n') => {
                count += 1;
                offset += 2;
            }
            b'\r' => {
                count += 1;
                offset += 1;
            }
            _ => {
                offset += 1;
            }
        }
    }

    count
}

/// Returns true when every byte is ASCII.
#[must_use]
pub fn is_all_ascii(bytes: &[u8]) -> bool {
    bytes.is_ascii()
}

/// Scalar reference implementation for [`is_all_ascii`].
#[must_use]
pub fn is_all_ascii_scalar(bytes: &[u8]) -> bool {
    bytes.iter().all(u8::is_ascii)
}

/// Returns the first byte that needs PHP-default JSON escaping.
#[must_use]
pub fn find_json_escape_byte(bytes: &[u8]) -> Option<usize> {
    arch::find_json_escape_byte(bytes)
}

/// Scalar reference implementation for [`find_json_escape_byte`].
#[must_use]
pub fn find_json_escape_byte_scalar(bytes: &[u8]) -> Option<usize> {
    bytes
        .iter()
        .position(|byte| matches!(*byte, b'"' | b'\\' | b'/') || *byte < 0x20 || *byte >= 0x80)
}

/// Returns the first byte that needs default `htmlspecialchars` escaping.
#[must_use]
pub fn find_html_escape_byte(bytes: &[u8]) -> Option<usize> {
    arch::find_html_escape_byte(bytes)
}

/// Scalar reference implementation for [`find_html_escape_byte`].
#[must_use]
pub fn find_html_escape_byte_scalar(bytes: &[u8]) -> Option<usize> {
    bytes
        .iter()
        .position(|byte| matches!(*byte, b'&' | b'<' | b'>' | b'"' | b'\''))
}

/// Returns true for ASCII identifier-start bytes.
///
/// This is ASCII-only by design. PHP lexer call sites must keep handling
/// non-ASCII identifier bytes separately.
#[must_use]
pub const fn is_ascii_identifier_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

/// Returns true for ASCII identifier-continuation bytes.
///
/// This is ASCII-only by design. PHP lexer call sites must keep handling
/// non-ASCII identifier bytes separately.
#[must_use]
pub const fn is_ascii_identifier_continue(byte: u8) -> bool {
    is_ascii_identifier_start(byte) || byte.is_ascii_digit()
}

/// Returns the length of the initial ASCII identifier-continuation chunk.
#[must_use]
pub fn ascii_identifier_continue_chunk_len(bytes: &[u8]) -> usize {
    arch::ascii_identifier_continue_chunk_len(bytes)
}

/// Scalar reference implementation for [`ascii_identifier_continue_chunk_len`].
#[must_use]
pub fn ascii_identifier_continue_chunk_len_scalar(bytes: &[u8]) -> usize {
    let mut len = 0;
    while len < bytes.len() && is_ascii_identifier_continue(bytes[len]) {
        len += 1;
    }
    len
}

/// Returns the length of the initial ASCII digit chunk.
#[must_use]
pub fn ascii_digit_run_len(bytes: &[u8]) -> usize {
    arch::ascii_digit_run_len(bytes)
}

/// Scalar reference implementation for [`ascii_digit_run_len`].
#[must_use]
pub fn ascii_digit_run_len_scalar(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .unwrap_or(bytes.len())
}

/// Returns true when every byte is an ASCII decimal digit.
#[must_use]
pub fn all_ascii_digits(bytes: &[u8]) -> bool {
    ascii_digit_run_len(bytes) == bytes.len()
}

/// Scalar reference implementation for [`all_ascii_digits`].
#[must_use]
pub fn all_ascii_digits_scalar(bytes: &[u8]) -> bool {
    bytes.iter().all(u8::is_ascii_digit)
}

/// Finds the first byte that is not PHP ASCII whitespace.
#[must_use]
pub fn find_non_ascii_whitespace(bytes: &[u8]) -> Option<usize> {
    arch::find_non_ascii_whitespace(bytes)
}

/// Finds the first ASCII whitespace byte according to `u8::is_ascii_whitespace`.
#[must_use]
pub fn find_ascii_whitespace(bytes: &[u8]) -> Option<usize> {
    min_index(
        memchr::memchr3(b' ', b'\t', b'\n', bytes),
        memchr::memchr2(b'\r', 0x0c, bytes),
    )
}

/// Finds the last ASCII whitespace byte according to `u8::is_ascii_whitespace`.
#[must_use]
pub fn rfind_ascii_whitespace(bytes: &[u8]) -> Option<usize> {
    max_index(
        memchr::memrchr3(b' ', b'\t', b'\n', bytes),
        memchr::memrchr2(b'\r', 0x0c, bytes),
    )
}

/// Scalar reference implementation for [`find_non_ascii_whitespace`].
#[must_use]
pub fn find_non_ascii_whitespace_scalar(bytes: &[u8]) -> Option<usize> {
    bytes.iter().position(|byte| !byte.is_ascii_whitespace())
}

/// Scalar reference implementation for [`find_ascii_whitespace`].
#[must_use]
pub fn find_ascii_whitespace_scalar(bytes: &[u8]) -> Option<usize> {
    bytes.iter().position(u8::is_ascii_whitespace)
}

/// Scalar reference implementation for [`rfind_ascii_whitespace`].
#[must_use]
pub fn rfind_ascii_whitespace_scalar(bytes: &[u8]) -> Option<usize> {
    bytes.iter().rposition(u8::is_ascii_whitespace)
}

/// Returns true when any byte is ASCII whitespace.
#[must_use]
pub fn contains_ascii_whitespace(bytes: &[u8]) -> bool {
    find_ascii_whitespace(bytes).is_some()
}

/// Scalar reference implementation for [`contains_ascii_whitespace`].
#[must_use]
pub fn contains_ascii_whitespace_scalar(bytes: &[u8]) -> bool {
    bytes.iter().any(u8::is_ascii_whitespace)
}

/// Returns true when every byte is ASCII whitespace.
#[must_use]
pub fn all_ascii_whitespace(bytes: &[u8]) -> bool {
    find_non_ascii_whitespace(bytes).is_none()
}

/// Scalar reference implementation for [`all_ascii_whitespace`].
#[must_use]
pub fn all_ascii_whitespace_scalar(bytes: &[u8]) -> bool {
    bytes.iter().all(u8::is_ascii_whitespace)
}

/// Returns true when any byte is ASCII lowercase.
#[must_use]
pub fn contains_ascii_lowercase(bytes: &[u8]) -> bool {
    arch::contains_ascii_lowercase(bytes)
}

/// Scalar reference implementation for [`contains_ascii_lowercase`].
#[must_use]
pub fn contains_ascii_lowercase_scalar(bytes: &[u8]) -> bool {
    bytes.iter().any(u8::is_ascii_lowercase)
}

/// Returns true when any byte is ASCII uppercase.
#[must_use]
pub fn contains_ascii_uppercase(bytes: &[u8]) -> bool {
    arch::contains_ascii_uppercase(bytes)
}

/// Scalar reference implementation for [`contains_ascii_uppercase`].
#[must_use]
pub fn contains_ascii_uppercase_scalar(bytes: &[u8]) -> bool {
    bytes.iter().any(u8::is_ascii_uppercase)
}

/// Converts ASCII lowercase bytes to uppercase in place.
///
/// Non-ASCII and non-lowercase bytes are left unchanged.
pub fn ascii_uppercase_in_place(bytes: &mut [u8]) {
    arch::ascii_uppercase_in_place(bytes);
}

/// Converts ASCII uppercase bytes to lowercase in place.
///
/// Non-ASCII and non-uppercase bytes are left unchanged.
pub fn ascii_lowercase_in_place(bytes: &mut [u8]) {
    arch::ascii_lowercase_in_place(bytes);
}

/// Returns an ASCII-uppercase copy.
#[must_use]
pub fn ascii_uppercase_copy(bytes: &[u8]) -> Vec<u8> {
    let mut copy = bytes.to_vec();
    ascii_uppercase_in_place(&mut copy);
    copy
}

/// Returns an ASCII-lowercase copy.
#[must_use]
pub fn ascii_lowercase_copy(bytes: &[u8]) -> Vec<u8> {
    let mut copy = bytes.to_vec();
    ascii_lowercase_in_place(&mut copy);
    copy
}

/// Returns bounds after applying PHP's default trim mask.
#[must_use]
pub fn trim_default_bounds(bytes: &[u8]) -> (usize, usize) {
    arch::trim_default_bounds(bytes)
}

/// Scalar reference implementation for [`trim_default_bounds`].
#[must_use]
pub fn trim_default_bounds_scalar(bytes: &[u8]) -> (usize, usize) {
    let start = bytes
        .iter()
        .position(|byte| !is_default_trim_byte(*byte))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !is_default_trim_byte(*byte))
        .map_or(start, |index| index + 1);
    (start, end)
}

#[must_use]
fn is_default_trim_byte(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b'\0' | 0x0b)
}

#[must_use]
fn find_ascii_folded_byte(bytes: &[u8], needle: u8) -> Option<usize> {
    if needle.is_ascii_alphabetic() {
        memchr::memchr2(
            needle.to_ascii_lowercase(),
            needle.to_ascii_uppercase(),
            bytes,
        )
    } else {
        find_byte(bytes, needle)
    }
}

#[must_use]
fn rfind_ascii_folded_byte(bytes: &[u8], needle: u8) -> Option<usize> {
    if needle.is_ascii_alphabetic() {
        memchr::memrchr2(
            needle.to_ascii_lowercase(),
            needle.to_ascii_uppercase(),
            bytes,
        )
    } else {
        rfind_byte(bytes, needle)
    }
}

#[must_use]
const fn min_index(left: Option<usize>, right: Option<usize>) -> Option<usize> {
    match (left, right) {
        (Some(left), Some(right)) if left < right => Some(left),
        (Some(_), Some(right)) => Some(right),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

#[must_use]
const fn max_index(left: Option<usize>, right: Option<usize>) -> Option<usize> {
    match (left, right) {
        (Some(left), Some(right)) if left > right => Some(left),
        (Some(_), Some(right)) => Some(right),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

#[allow(dead_code)]
mod scalar {
    use super::{
        ascii_digit_run_len_scalar, ascii_identifier_continue_chunk_len_scalar,
        contains_ascii_lowercase_scalar, contains_ascii_uppercase_scalar,
        find_html_escape_byte_scalar, find_json_escape_byte_scalar,
        find_non_ascii_whitespace_scalar, trim_default_bounds_scalar,
    };

    pub(super) fn active_backend_name() -> &'static str {
        "scalar"
    }

    pub(super) fn find_json_escape_byte(bytes: &[u8]) -> Option<usize> {
        find_json_escape_byte_scalar(bytes)
    }

    pub(super) fn find_html_escape_byte(bytes: &[u8]) -> Option<usize> {
        find_html_escape_byte_scalar(bytes)
    }

    pub(super) fn ascii_identifier_continue_chunk_len(bytes: &[u8]) -> usize {
        ascii_identifier_continue_chunk_len_scalar(bytes)
    }

    pub(super) fn ascii_digit_run_len(bytes: &[u8]) -> usize {
        ascii_digit_run_len_scalar(bytes)
    }

    pub(super) fn find_non_ascii_whitespace(bytes: &[u8]) -> Option<usize> {
        find_non_ascii_whitespace_scalar(bytes)
    }

    pub(super) fn contains_ascii_lowercase(bytes: &[u8]) -> bool {
        contains_ascii_lowercase_scalar(bytes)
    }

    pub(super) fn contains_ascii_uppercase(bytes: &[u8]) -> bool {
        contains_ascii_uppercase_scalar(bytes)
    }

    pub(super) fn ascii_uppercase_in_place(bytes: &mut [u8]) {
        for byte in bytes {
            byte.make_ascii_uppercase();
        }
    }

    pub(super) fn ascii_lowercase_in_place(bytes: &mut [u8]) {
        for byte in bytes {
            byte.make_ascii_lowercase();
        }
    }

    pub(super) fn trim_default_bounds(bytes: &[u8]) -> (usize, usize) {
        trim_default_bounds_scalar(bytes)
    }
}

#[cfg(target_arch = "x86_64")]
mod arch {
    // SIMD intrinsics require `unsafe`; the hardening gate's `-D unsafe-code`
    // reaches workspace dependencies, so scope the allowance to this backend.
    #![allow(unsafe_code)]
    #![allow(unsafe_op_in_unsafe_fn)]

    use super::scalar;
    use core::arch::x86_64::*;

    pub(super) fn active_backend_name() -> &'static str {
        if std::is_x86_feature_detected!("avx2") {
            "amd64-avx2"
        } else {
            "amd64-sse2"
        }
    }

    pub(super) fn find_json_escape_byte(bytes: &[u8]) -> Option<usize> {
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support is checked immediately above.
            unsafe { find_json_escape_byte_avx2(bytes) }
        } else {
            // SAFETY: SSE2 is part of the x86_64 baseline.
            unsafe { find_json_escape_byte_sse2(bytes) }
        }
    }

    pub(super) fn find_html_escape_byte(bytes: &[u8]) -> Option<usize> {
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support is checked immediately above.
            unsafe { find_html_escape_byte_avx2(bytes) }
        } else {
            // SAFETY: SSE2 is part of the x86_64 baseline.
            unsafe { find_html_escape_byte_sse2(bytes) }
        }
    }

    pub(super) fn ascii_identifier_continue_chunk_len(bytes: &[u8]) -> usize {
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support is checked immediately above.
            unsafe { ascii_identifier_continue_chunk_len_avx2(bytes) }
        } else {
            // SAFETY: SSE2 is part of the x86_64 baseline.
            unsafe { ascii_identifier_continue_chunk_len_sse2(bytes) }
        }
    }

    pub(super) fn ascii_digit_run_len(bytes: &[u8]) -> usize {
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support is checked immediately above.
            unsafe { ascii_digit_run_len_avx2(bytes) }
        } else {
            // SAFETY: SSE2 is part of the x86_64 baseline.
            unsafe { ascii_digit_run_len_sse2(bytes) }
        }
    }

    pub(super) fn find_non_ascii_whitespace(bytes: &[u8]) -> Option<usize> {
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support is checked immediately above.
            unsafe { find_non_ascii_whitespace_avx2(bytes) }
        } else {
            // SAFETY: SSE2 is part of the x86_64 baseline.
            unsafe { find_non_ascii_whitespace_sse2(bytes) }
        }
    }

    pub(super) fn contains_ascii_lowercase(bytes: &[u8]) -> bool {
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support is checked immediately above.
            unsafe { contains_ascii_range_avx2(bytes, b'a', b'z') }
        } else {
            // SAFETY: SSE2 is part of the x86_64 baseline.
            unsafe { contains_ascii_range_sse2(bytes, b'a', b'z') }
        }
    }

    pub(super) fn contains_ascii_uppercase(bytes: &[u8]) -> bool {
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support is checked immediately above.
            unsafe { contains_ascii_range_avx2(bytes, b'A', b'Z') }
        } else {
            // SAFETY: SSE2 is part of the x86_64 baseline.
            unsafe { contains_ascii_range_sse2(bytes, b'A', b'Z') }
        }
    }

    pub(super) fn ascii_uppercase_in_place(bytes: &mut [u8]) {
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support is checked immediately above.
            unsafe { ascii_case_in_place_avx2(bytes, b'a', b'z', !0x20) }
        } else {
            // SAFETY: SSE2 is part of the x86_64 baseline.
            unsafe { ascii_case_in_place_sse2(bytes, b'a', b'z', !0x20) }
        }
    }

    pub(super) fn ascii_lowercase_in_place(bytes: &mut [u8]) {
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support is checked immediately above.
            unsafe { ascii_case_in_place_avx2(bytes, b'A', b'Z', 0x20) }
        } else {
            // SAFETY: SSE2 is part of the x86_64 baseline.
            unsafe { ascii_case_in_place_sse2(bytes, b'A', b'Z', 0x20) }
        }
    }

    pub(super) fn trim_default_bounds(bytes: &[u8]) -> (usize, usize) {
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support is checked immediately above.
            unsafe { trim_default_bounds_avx2(bytes) }
        } else {
            // SAFETY: SSE2 is part of the x86_64 baseline.
            unsafe { trim_default_bounds_sse2(bytes) }
        }
    }

    #[target_feature(enable = "avx2")]
    unsafe fn find_json_escape_byte_avx2(bytes: &[u8]) -> Option<usize> {
        unsafe { find_escape_avx2(bytes, b"\"\\/", true) }
    }

    unsafe fn find_json_escape_byte_sse2(bytes: &[u8]) -> Option<usize> {
        unsafe { find_escape_sse2(bytes, b"\"\\/", true) }
    }

    #[target_feature(enable = "avx2")]
    unsafe fn find_html_escape_byte_avx2(bytes: &[u8]) -> Option<usize> {
        unsafe { find_escape_avx2(bytes, b"&<>\"'", false) }
    }

    unsafe fn find_html_escape_byte_sse2(bytes: &[u8]) -> Option<usize> {
        unsafe { find_escape_sse2(bytes, b"&<>\"'", false) }
    }

    #[target_feature(enable = "avx2")]
    unsafe fn find_escape_avx2(
        bytes: &[u8],
        needles: &[u8],
        reject_non_printable_ascii: bool,
    ) -> Option<usize> {
        let mut offset = 0;
        let len = bytes.len();
        while offset + 32 <= len {
            // SAFETY: The loop bound guarantees a full unaligned 32-byte load.
            let chunk = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(offset).cast()) };
            let mut mask = 0i32;
            for needle in needles {
                let needle = _mm256_set1_epi8(*needle as i8);
                mask |= _mm256_movemask_epi8(_mm256_cmpeq_epi8(chunk, needle));
            }
            if reject_non_printable_ascii {
                let limit = _mm256_set1_epi8(0x20);
                mask |= _mm256_movemask_epi8(_mm256_cmpgt_epi8(limit, chunk));
            }
            if mask != 0 {
                return Some(offset + mask.trailing_zeros() as usize);
            }
            offset += 32;
        }
        scalar_tail_escape(bytes, offset, needles, reject_non_printable_ascii)
    }

    unsafe fn find_escape_sse2(
        bytes: &[u8],
        needles: &[u8],
        reject_non_printable_ascii: bool,
    ) -> Option<usize> {
        let mut offset = 0;
        let len = bytes.len();
        while offset + 16 <= len {
            // SAFETY: The loop bound guarantees a full unaligned 16-byte load.
            let chunk = unsafe { _mm_loadu_si128(bytes.as_ptr().add(offset).cast()) };
            let mut mask = 0i32;
            for needle in needles {
                let needle = _mm_set1_epi8(*needle as i8);
                mask |= _mm_movemask_epi8(_mm_cmpeq_epi8(chunk, needle));
            }
            if reject_non_printable_ascii {
                let limit = _mm_set1_epi8(0x20);
                mask |= _mm_movemask_epi8(_mm_cmpgt_epi8(limit, chunk));
            }
            if mask != 0 {
                return Some(offset + mask.trailing_zeros() as usize);
            }
            offset += 16;
        }
        scalar_tail_escape(bytes, offset, needles, reject_non_printable_ascii)
    }

    #[target_feature(enable = "avx2")]
    unsafe fn ascii_identifier_continue_chunk_len_avx2(bytes: &[u8]) -> usize {
        let mut offset = 0;
        while offset + 32 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 32-byte load.
            let chunk = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(offset).cast()) };
            let valid = identifier_mask_avx2(chunk);
            let mask = _mm256_movemask_epi8(valid) as u32;
            if mask != u32::MAX {
                return offset + (!mask).trailing_zeros() as usize;
            }
            offset += 32;
        }
        offset + scalar::ascii_identifier_continue_chunk_len(&bytes[offset..])
    }

    unsafe fn ascii_identifier_continue_chunk_len_sse2(bytes: &[u8]) -> usize {
        let mut offset = 0;
        while offset + 16 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 16-byte load.
            let chunk = unsafe { _mm_loadu_si128(bytes.as_ptr().add(offset).cast()) };
            let valid = identifier_mask_sse2(chunk);
            let mask = _mm_movemask_epi8(valid) as u32;
            if mask != 0xffff {
                return offset + (!mask).trailing_zeros() as usize;
            }
            offset += 16;
        }
        offset + scalar::ascii_identifier_continue_chunk_len(&bytes[offset..])
    }

    #[target_feature(enable = "avx2")]
    unsafe fn ascii_digit_run_len_avx2(bytes: &[u8]) -> usize {
        let mut offset = 0;
        while offset + 32 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 32-byte load.
            let chunk = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(offset).cast()) };
            let valid = ascii_range_mask_avx2(chunk, b'0', b'9');
            let mask = _mm256_movemask_epi8(valid) as u32;
            if mask != u32::MAX {
                return offset + (!mask).trailing_zeros() as usize;
            }
            offset += 32;
        }
        offset + scalar::ascii_digit_run_len(&bytes[offset..])
    }

    unsafe fn ascii_digit_run_len_sse2(bytes: &[u8]) -> usize {
        let mut offset = 0;
        while offset + 16 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 16-byte load.
            let chunk = unsafe { _mm_loadu_si128(bytes.as_ptr().add(offset).cast()) };
            let valid = ascii_range_mask_sse2(chunk, b'0', b'9');
            let mask = _mm_movemask_epi8(valid) as u32;
            if mask != 0xffff {
                return offset + (!mask).trailing_zeros() as usize;
            }
            offset += 16;
        }
        offset + scalar::ascii_digit_run_len(&bytes[offset..])
    }

    #[target_feature(enable = "avx2")]
    unsafe fn find_non_ascii_whitespace_avx2(bytes: &[u8]) -> Option<usize> {
        let mut offset = 0;
        while offset + 32 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 32-byte load.
            let chunk = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(offset).cast()) };
            let mask = _mm256_movemask_epi8(ascii_whitespace_mask_avx2(chunk)) as u32;
            if mask != u32::MAX {
                return Some(offset + (!mask).trailing_zeros() as usize);
            }
            offset += 32;
        }
        scalar::find_non_ascii_whitespace(&bytes[offset..]).map(|index| offset + index)
    }

    unsafe fn find_non_ascii_whitespace_sse2(bytes: &[u8]) -> Option<usize> {
        let mut offset = 0;
        while offset + 16 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 16-byte load.
            let chunk = unsafe { _mm_loadu_si128(bytes.as_ptr().add(offset).cast()) };
            let mask = _mm_movemask_epi8(ascii_whitespace_mask_sse2(chunk)) as u32;
            if mask != 0xffff {
                return Some(offset + (!mask).trailing_zeros() as usize);
            }
            offset += 16;
        }
        scalar::find_non_ascii_whitespace(&bytes[offset..]).map(|index| offset + index)
    }

    #[target_feature(enable = "avx2")]
    unsafe fn contains_ascii_range_avx2(bytes: &[u8], low: u8, high: u8) -> bool {
        let mut offset = 0;
        while offset + 32 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 32-byte load.
            let chunk = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(offset).cast()) };
            if _mm256_movemask_epi8(ascii_range_mask_avx2(chunk, low, high)) != 0 {
                return true;
            }
            offset += 32;
        }
        bytes[offset..]
            .iter()
            .any(|byte| (low..=high).contains(byte))
    }

    unsafe fn contains_ascii_range_sse2(bytes: &[u8], low: u8, high: u8) -> bool {
        let mut offset = 0;
        while offset + 16 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 16-byte load.
            let chunk = unsafe { _mm_loadu_si128(bytes.as_ptr().add(offset).cast()) };
            if _mm_movemask_epi8(ascii_range_mask_sse2(chunk, low, high)) != 0 {
                return true;
            }
            offset += 16;
        }
        bytes[offset..]
            .iter()
            .any(|byte| (low..=high).contains(byte))
    }

    #[target_feature(enable = "avx2")]
    unsafe fn ascii_case_in_place_avx2(bytes: &mut [u8], low: u8, high: u8, bit_byte: u8) {
        let mut offset = 0;
        let bit = _mm256_set1_epi8(bit_byte as i8);
        while offset + 32 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 32-byte load/store.
            let ptr = unsafe { bytes.as_mut_ptr().add(offset) };
            let chunk = unsafe { _mm256_loadu_si256(ptr.cast()) };
            let mask = ascii_range_mask_avx2(chunk, low, high);
            let changed = if bit_byte == 0x20 {
                _mm256_or_si256(chunk, _mm256_and_si256(mask, bit))
            } else {
                let unchanged = _mm256_andnot_si256(mask, chunk);
                let uppercased = _mm256_and_si256(_mm256_and_si256(chunk, bit), mask);
                _mm256_or_si256(unchanged, uppercased)
            };
            unsafe { _mm256_storeu_si256(ptr.cast(), changed) };
            offset += 32;
        }
        if bit_byte == 0x20 {
            for byte in &mut bytes[offset..] {
                byte.make_ascii_lowercase();
            }
        } else {
            for byte in &mut bytes[offset..] {
                byte.make_ascii_uppercase();
            }
        }
    }

    unsafe fn ascii_case_in_place_sse2(bytes: &mut [u8], low: u8, high: u8, bit_byte: u8) {
        let mut offset = 0;
        let bit = _mm_set1_epi8(bit_byte as i8);
        while offset + 16 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 16-byte load/store.
            let ptr = unsafe { bytes.as_mut_ptr().add(offset) };
            let chunk = unsafe { _mm_loadu_si128(ptr.cast()) };
            let mask = ascii_range_mask_sse2(chunk, low, high);
            let changed = if bit_byte == 0x20 {
                _mm_or_si128(chunk, _mm_and_si128(mask, bit))
            } else {
                let unchanged = _mm_andnot_si128(mask, chunk);
                let uppercased = _mm_and_si128(_mm_and_si128(chunk, bit), mask);
                _mm_or_si128(unchanged, uppercased)
            };
            unsafe { _mm_storeu_si128(ptr.cast(), changed) };
            offset += 16;
        }
        if bit_byte == 0x20 {
            for byte in &mut bytes[offset..] {
                byte.make_ascii_lowercase();
            }
        } else {
            for byte in &mut bytes[offset..] {
                byte.make_ascii_uppercase();
            }
        }
    }

    #[target_feature(enable = "avx2")]
    unsafe fn trim_default_bounds_avx2(bytes: &[u8]) -> (usize, usize) {
        let start = first_not_trim_avx2(bytes);
        if start == bytes.len() {
            return (start, start);
        }
        let end = last_not_trim_avx2(bytes).map_or(start, |index| index + 1);
        (start, end)
    }

    unsafe fn trim_default_bounds_sse2(bytes: &[u8]) -> (usize, usize) {
        let start = first_not_trim_sse2(bytes);
        if start == bytes.len() {
            return (start, start);
        }
        let end = last_not_trim_sse2(bytes).map_or(start, |index| index + 1);
        (start, end)
    }

    #[target_feature(enable = "avx2")]
    unsafe fn first_not_trim_avx2(bytes: &[u8]) -> usize {
        let mut offset = 0;
        while offset + 32 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 32-byte load.
            let chunk = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(offset).cast()) };
            let mask = _mm256_movemask_epi8(default_trim_mask_avx2(chunk)) as u32;
            if mask != u32::MAX {
                return offset + (!mask).trailing_zeros() as usize;
            }
            offset += 32;
        }
        offset
            + bytes[offset..]
                .iter()
                .position(|byte| !super::is_default_trim_byte(*byte))
                .unwrap_or(bytes.len() - offset)
    }

    #[target_feature(enable = "avx2")]
    unsafe fn last_not_trim_avx2(bytes: &[u8]) -> Option<usize> {
        let mut end = bytes.len();
        while end >= 32 {
            let offset = end - 32;
            // SAFETY: The loop bound guarantees a full unaligned 32-byte load.
            let chunk = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(offset).cast()) };
            let mask = _mm256_movemask_epi8(default_trim_mask_avx2(chunk)) as u32;
            if mask != u32::MAX {
                let inverse = !mask;
                return Some(offset + (31 - inverse.leading_zeros() as usize));
            }
            end -= 32;
        }
        bytes[..end]
            .iter()
            .rposition(|byte| !super::is_default_trim_byte(*byte))
    }

    unsafe fn first_not_trim_sse2(bytes: &[u8]) -> usize {
        let mut offset = 0;
        while offset + 16 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 16-byte load.
            let chunk = unsafe { _mm_loadu_si128(bytes.as_ptr().add(offset).cast()) };
            let mask = _mm_movemask_epi8(default_trim_mask_sse2(chunk)) as u32;
            if mask != 0xffff {
                return offset + (!mask).trailing_zeros() as usize;
            }
            offset += 16;
        }
        offset
            + bytes[offset..]
                .iter()
                .position(|byte| !super::is_default_trim_byte(*byte))
                .unwrap_or(bytes.len() - offset)
    }

    unsafe fn last_not_trim_sse2(bytes: &[u8]) -> Option<usize> {
        let mut end = bytes.len();
        while end >= 16 {
            let offset = end - 16;
            // SAFETY: The loop bound guarantees a full unaligned 16-byte load.
            let chunk = unsafe { _mm_loadu_si128(bytes.as_ptr().add(offset).cast()) };
            let mask = _mm_movemask_epi8(default_trim_mask_sse2(chunk)) as u32;
            if mask != 0xffff {
                let inverse = !mask & 0xffff;
                return Some(offset + (15 - inverse.leading_zeros() as usize + 16));
            }
            end -= 16;
        }
        bytes[..end]
            .iter()
            .rposition(|byte| !super::is_default_trim_byte(*byte))
    }

    #[target_feature(enable = "avx2")]
    unsafe fn ascii_range_mask_avx2(chunk: __m256i, low: u8, high: u8) -> __m256i {
        let above_low = _mm256_cmpgt_epi8(chunk, _mm256_set1_epi8(low.wrapping_sub(1) as i8));
        let below_high = _mm256_cmpgt_epi8(_mm256_set1_epi8(high.wrapping_add(1) as i8), chunk);
        _mm256_and_si256(above_low, below_high)
    }

    unsafe fn ascii_range_mask_sse2(chunk: __m128i, low: u8, high: u8) -> __m128i {
        let above_low = _mm_cmpgt_epi8(chunk, _mm_set1_epi8(low.wrapping_sub(1) as i8));
        let below_high = _mm_cmpgt_epi8(_mm_set1_epi8(high.wrapping_add(1) as i8), chunk);
        _mm_and_si128(above_low, below_high)
    }

    #[target_feature(enable = "avx2")]
    unsafe fn identifier_mask_avx2(chunk: __m256i) -> __m256i {
        let upper = ascii_range_mask_avx2(chunk, b'A', b'Z');
        let lower = ascii_range_mask_avx2(chunk, b'a', b'z');
        let digit = ascii_range_mask_avx2(chunk, b'0', b'9');
        let underscore = _mm256_cmpeq_epi8(chunk, _mm256_set1_epi8(b'_' as i8));
        _mm256_or_si256(
            _mm256_or_si256(upper, lower),
            _mm256_or_si256(digit, underscore),
        )
    }

    unsafe fn identifier_mask_sse2(chunk: __m128i) -> __m128i {
        let upper = ascii_range_mask_sse2(chunk, b'A', b'Z');
        let lower = ascii_range_mask_sse2(chunk, b'a', b'z');
        let digit = ascii_range_mask_sse2(chunk, b'0', b'9');
        let underscore = _mm_cmpeq_epi8(chunk, _mm_set1_epi8(b'_' as i8));
        _mm_or_si128(_mm_or_si128(upper, lower), _mm_or_si128(digit, underscore))
    }

    #[target_feature(enable = "avx2")]
    unsafe fn default_trim_mask_avx2(chunk: __m256i) -> __m256i {
        let mut mask = _mm256_cmpeq_epi8(chunk, _mm256_set1_epi8(b' ' as i8));
        for byte in [b'\t', b'\n', b'\r', b'\0', 0x0b] {
            mask = _mm256_or_si256(mask, _mm256_cmpeq_epi8(chunk, _mm256_set1_epi8(byte as i8)));
        }
        mask
    }

    unsafe fn default_trim_mask_sse2(chunk: __m128i) -> __m128i {
        let mut mask = _mm_cmpeq_epi8(chunk, _mm_set1_epi8(b' ' as i8));
        for byte in [b'\t', b'\n', b'\r', b'\0', 0x0b] {
            mask = _mm_or_si128(mask, _mm_cmpeq_epi8(chunk, _mm_set1_epi8(byte as i8)));
        }
        mask
    }

    #[target_feature(enable = "avx2")]
    unsafe fn ascii_whitespace_mask_avx2(chunk: __m256i) -> __m256i {
        let mut mask = _mm256_cmpeq_epi8(chunk, _mm256_set1_epi8(b' ' as i8));
        for byte in [b'\t', b'\n', 0x0c, b'\r'] {
            mask = _mm256_or_si256(mask, _mm256_cmpeq_epi8(chunk, _mm256_set1_epi8(byte as i8)));
        }
        mask
    }

    unsafe fn ascii_whitespace_mask_sse2(chunk: __m128i) -> __m128i {
        let mut mask = _mm_cmpeq_epi8(chunk, _mm_set1_epi8(b' ' as i8));
        for byte in [b'\t', b'\n', 0x0c, b'\r'] {
            mask = _mm_or_si128(mask, _mm_cmpeq_epi8(chunk, _mm_set1_epi8(byte as i8)));
        }
        mask
    }

    fn scalar_tail_escape(
        bytes: &[u8],
        offset: usize,
        needles: &[u8],
        reject_non_printable_ascii: bool,
    ) -> Option<usize> {
        bytes[offset..]
            .iter()
            .position(|byte| {
                needles.contains(byte)
                    || (reject_non_printable_ascii && (*byte < 0x20 || *byte >= 0x80))
            })
            .map(|index| index + offset)
    }
}

#[cfg(target_arch = "aarch64")]
mod arch {
    // SIMD intrinsics require `unsafe`; the hardening gate's `-D unsafe-code`
    // reaches workspace dependencies, so scope the allowance to this backend.
    #![allow(unsafe_code)]
    #![allow(unsafe_op_in_unsafe_fn)]

    use super::scalar;
    use core::arch::aarch64::*;

    pub(super) fn active_backend_name() -> &'static str {
        "arm64-neon"
    }

    pub(super) fn find_json_escape_byte(bytes: &[u8]) -> Option<usize> {
        // SAFETY: NEON is part of the supported aarch64 baseline here.
        unsafe { find_escape_neon(bytes, b"\"\\/", true) }
    }

    pub(super) fn find_html_escape_byte(bytes: &[u8]) -> Option<usize> {
        // SAFETY: NEON is part of the supported aarch64 baseline here.
        unsafe { find_escape_neon(bytes, b"&<>\"'", false) }
    }

    pub(super) fn ascii_identifier_continue_chunk_len(bytes: &[u8]) -> usize {
        // SAFETY: NEON is part of the supported aarch64 baseline here.
        unsafe { ascii_identifier_continue_chunk_len_neon(bytes) }
    }

    pub(super) fn ascii_digit_run_len(bytes: &[u8]) -> usize {
        // SAFETY: NEON is part of the supported aarch64 baseline here.
        unsafe { ascii_digit_run_len_neon(bytes) }
    }

    pub(super) fn find_non_ascii_whitespace(bytes: &[u8]) -> Option<usize> {
        // SAFETY: NEON is part of the supported aarch64 baseline here.
        unsafe { find_non_ascii_whitespace_neon(bytes) }
    }

    pub(super) fn contains_ascii_lowercase(bytes: &[u8]) -> bool {
        // SAFETY: NEON is part of the supported aarch64 baseline here.
        unsafe { contains_ascii_range_neon(bytes, b'a', b'z') }
    }

    pub(super) fn contains_ascii_uppercase(bytes: &[u8]) -> bool {
        // SAFETY: NEON is part of the supported aarch64 baseline here.
        unsafe { contains_ascii_range_neon(bytes, b'A', b'Z') }
    }

    pub(super) fn ascii_uppercase_in_place(bytes: &mut [u8]) {
        // SAFETY: NEON is part of the supported aarch64 baseline here.
        unsafe { ascii_case_in_place_neon(bytes, b'a', b'z', !0x20) }
    }

    pub(super) fn ascii_lowercase_in_place(bytes: &mut [u8]) {
        // SAFETY: NEON is part of the supported aarch64 baseline here.
        unsafe { ascii_case_in_place_neon(bytes, b'A', b'Z', 0x20) }
    }

    pub(super) fn trim_default_bounds(bytes: &[u8]) -> (usize, usize) {
        // SAFETY: NEON is part of the supported aarch64 baseline here.
        unsafe { trim_default_bounds_neon(bytes) }
    }

    #[target_feature(enable = "neon")]
    unsafe fn find_escape_neon(
        bytes: &[u8],
        needles: &[u8],
        reject_non_printable_ascii: bool,
    ) -> Option<usize> {
        let mut offset = 0;
        while offset + 16 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 16-byte load.
            let chunk = unsafe { vld1q_u8(bytes.as_ptr().add(offset)) };
            let mut mask = vdupq_n_u8(0);
            for needle in needles {
                mask = vorrq_u8(mask, vceqq_u8(chunk, vdupq_n_u8(*needle)));
            }
            if reject_non_printable_ascii {
                mask = vorrq_u8(mask, vcltq_u8(chunk, vdupq_n_u8(0x20)));
                mask = vorrq_u8(mask, vcgeq_u8(chunk, vdupq_n_u8(0x80)));
            }
            if let Some(index) = first_mask_lane(mask) {
                return Some(offset + index);
            }
            offset += 16;
        }
        scalar_tail_escape(bytes, offset, needles, reject_non_printable_ascii)
    }

    #[target_feature(enable = "neon")]
    unsafe fn ascii_identifier_continue_chunk_len_neon(bytes: &[u8]) -> usize {
        let mut offset = 0;
        while offset + 16 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 16-byte load.
            let chunk = unsafe { vld1q_u8(bytes.as_ptr().add(offset)) };
            let valid = identifier_mask_neon(chunk);
            let mut lanes = [0u8; 16];
            // SAFETY: `lanes` has exactly 16 writable bytes.
            unsafe { vst1q_u8(lanes.as_mut_ptr(), valid) };
            if let Some(index) = lanes.iter().position(|lane| *lane == 0) {
                return offset + index;
            }
            offset += 16;
        }
        offset + scalar::ascii_identifier_continue_chunk_len(&bytes[offset..])
    }

    #[target_feature(enable = "neon")]
    unsafe fn ascii_digit_run_len_neon(bytes: &[u8]) -> usize {
        let mut offset = 0;
        while offset + 16 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 16-byte load.
            let chunk = unsafe { vld1q_u8(bytes.as_ptr().add(offset)) };
            let valid = ascii_range_mask_neon(chunk, b'0', b'9');
            let mut lanes = [0u8; 16];
            // SAFETY: `lanes` has exactly 16 writable bytes.
            unsafe { vst1q_u8(lanes.as_mut_ptr(), valid) };
            if let Some(index) = lanes.iter().position(|lane| *lane == 0) {
                return offset + index;
            }
            offset += 16;
        }
        offset + scalar::ascii_digit_run_len(&bytes[offset..])
    }

    #[target_feature(enable = "neon")]
    unsafe fn find_non_ascii_whitespace_neon(bytes: &[u8]) -> Option<usize> {
        let mut offset = 0;
        while offset + 16 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 16-byte load.
            let chunk = unsafe { vld1q_u8(bytes.as_ptr().add(offset)) };
            let whitespace = ascii_whitespace_mask_neon(chunk);
            let mut lanes = [0u8; 16];
            // SAFETY: `lanes` has exactly 16 writable bytes.
            unsafe { vst1q_u8(lanes.as_mut_ptr(), whitespace) };
            if let Some(index) = lanes.iter().position(|lane| *lane == 0) {
                return Some(offset + index);
            }
            offset += 16;
        }
        scalar::find_non_ascii_whitespace(&bytes[offset..]).map(|index| offset + index)
    }

    #[target_feature(enable = "neon")]
    unsafe fn contains_ascii_range_neon(bytes: &[u8], low: u8, high: u8) -> bool {
        let mut offset = 0;
        while offset + 16 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 16-byte load.
            let chunk = unsafe { vld1q_u8(bytes.as_ptr().add(offset)) };
            if first_mask_lane(ascii_range_mask_neon(chunk, low, high)).is_some() {
                return true;
            }
            offset += 16;
        }
        bytes[offset..]
            .iter()
            .any(|byte| (low..=high).contains(byte))
    }

    #[target_feature(enable = "neon")]
    unsafe fn ascii_case_in_place_neon(bytes: &mut [u8], low: u8, high: u8, bit: u8) {
        let mut offset = 0;
        while offset + 16 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 16-byte load/store.
            let ptr = unsafe { bytes.as_mut_ptr().add(offset) };
            let chunk = unsafe { vld1q_u8(ptr) };
            let mask = ascii_range_mask_neon(chunk, low, high);
            let changed = if bit == 0x20 {
                vorrq_u8(chunk, vandq_u8(mask, vdupq_n_u8(bit)))
            } else {
                let unchanged = vandq_u8(chunk, vmvnq_u8(mask));
                let uppercased = vandq_u8(vandq_u8(chunk, vdupq_n_u8(bit)), mask);
                vorrq_u8(unchanged, uppercased)
            };
            // SAFETY: The loop bound guarantees a full unaligned 16-byte store.
            unsafe { vst1q_u8(ptr, changed) };
            offset += 16;
        }
        if bit == 0x20 {
            for byte in &mut bytes[offset..] {
                byte.make_ascii_lowercase();
            }
        } else {
            for byte in &mut bytes[offset..] {
                byte.make_ascii_uppercase();
            }
        }
    }

    #[target_feature(enable = "neon")]
    unsafe fn trim_default_bounds_neon(bytes: &[u8]) -> (usize, usize) {
        let start = first_not_trim_neon(bytes);
        if start == bytes.len() {
            return (start, start);
        }
        let end = last_not_trim_neon(bytes).map_or(start, |index| index + 1);
        (start, end)
    }

    #[target_feature(enable = "neon")]
    unsafe fn first_not_trim_neon(bytes: &[u8]) -> usize {
        let mut offset = 0;
        while offset + 16 <= bytes.len() {
            // SAFETY: The loop bound guarantees a full unaligned 16-byte load.
            let chunk = unsafe { vld1q_u8(bytes.as_ptr().add(offset)) };
            let trim = default_trim_mask_neon(chunk);
            let mut lanes = [0u8; 16];
            // SAFETY: `lanes` has exactly 16 writable bytes.
            unsafe { vst1q_u8(lanes.as_mut_ptr(), trim) };
            if let Some(index) = lanes.iter().position(|lane| *lane == 0) {
                return offset + index;
            }
            offset += 16;
        }
        offset
            + bytes[offset..]
                .iter()
                .position(|byte| !super::is_default_trim_byte(*byte))
                .unwrap_or(bytes.len() - offset)
    }

    #[target_feature(enable = "neon")]
    unsafe fn last_not_trim_neon(bytes: &[u8]) -> Option<usize> {
        let mut end = bytes.len();
        while end >= 16 {
            let offset = end - 16;
            // SAFETY: The loop bound guarantees a full unaligned 16-byte load.
            let chunk = unsafe { vld1q_u8(bytes.as_ptr().add(offset)) };
            let trim = default_trim_mask_neon(chunk);
            let mut lanes = [0u8; 16];
            // SAFETY: `lanes` has exactly 16 writable bytes.
            unsafe { vst1q_u8(lanes.as_mut_ptr(), trim) };
            if let Some(index) = lanes.iter().rposition(|lane| *lane == 0) {
                return Some(offset + index);
            }
            end -= 16;
        }
        bytes[..end]
            .iter()
            .rposition(|byte| !super::is_default_trim_byte(*byte))
    }

    #[target_feature(enable = "neon")]
    unsafe fn ascii_range_mask_neon(chunk: uint8x16_t, low: u8, high: u8) -> uint8x16_t {
        vandq_u8(
            vcgeq_u8(chunk, vdupq_n_u8(low)),
            vcleq_u8(chunk, vdupq_n_u8(high)),
        )
    }

    #[target_feature(enable = "neon")]
    unsafe fn identifier_mask_neon(chunk: uint8x16_t) -> uint8x16_t {
        let upper = ascii_range_mask_neon(chunk, b'A', b'Z');
        let lower = ascii_range_mask_neon(chunk, b'a', b'z');
        let digit = ascii_range_mask_neon(chunk, b'0', b'9');
        let underscore = vceqq_u8(chunk, vdupq_n_u8(b'_'));
        vorrq_u8(vorrq_u8(upper, lower), vorrq_u8(digit, underscore))
    }

    #[target_feature(enable = "neon")]
    unsafe fn default_trim_mask_neon(chunk: uint8x16_t) -> uint8x16_t {
        let mut mask = vceqq_u8(chunk, vdupq_n_u8(b' '));
        for byte in [b'\t', b'\n', b'\r', b'\0', 0x0b] {
            mask = vorrq_u8(mask, vceqq_u8(chunk, vdupq_n_u8(byte)));
        }
        mask
    }

    #[target_feature(enable = "neon")]
    unsafe fn ascii_whitespace_mask_neon(chunk: uint8x16_t) -> uint8x16_t {
        let mut mask = vceqq_u8(chunk, vdupq_n_u8(b' '));
        for byte in [b'\t', b'\n', 0x0c, b'\r'] {
            mask = vorrq_u8(mask, vceqq_u8(chunk, vdupq_n_u8(byte)));
        }
        mask
    }

    #[target_feature(enable = "neon")]
    unsafe fn first_mask_lane(mask: uint8x16_t) -> Option<usize> {
        let mut lanes = [0u8; 16];
        // SAFETY: `lanes` has exactly 16 writable bytes.
        unsafe { vst1q_u8(lanes.as_mut_ptr(), mask) };
        lanes.iter().position(|lane| *lane != 0)
    }

    fn scalar_tail_escape(
        bytes: &[u8],
        offset: usize,
        needles: &[u8],
        reject_non_printable_ascii: bool,
    ) -> Option<usize> {
        bytes[offset..]
            .iter()
            .position(|byte| {
                needles.contains(byte)
                    || (reject_non_printable_ascii && (*byte < 0x20 || *byte >= 0x80))
            })
            .map(|index| index + offset)
    }
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
mod arch {
    pub(super) use super::scalar::*;
}

#[cfg(test)]
mod tests {
    use super::{
        all_ascii_digits, all_ascii_digits_scalar, all_ascii_whitespace,
        all_ascii_whitespace_scalar, ascii_digit_run_len, ascii_digit_run_len_scalar,
        ascii_identifier_continue_chunk_len, ascii_identifier_continue_chunk_len_scalar,
        ascii_lowercase_copy, ascii_lowercase_in_place, ascii_uppercase_copy,
        ascii_uppercase_in_place, contains_ascii_lowercase, contains_ascii_lowercase_scalar,
        contains_ascii_uppercase, contains_ascii_uppercase_scalar, contains_ascii_whitespace,
        contains_ascii_whitespace_scalar, count_byte, count_byte_scalar, count_newlines,
        count_newlines_scalar, find_any2, find_any2_scalar, find_any3, find_any3_scalar,
        find_ascii_whitespace, find_ascii_whitespace_scalar, find_byte, find_byte_scalar,
        find_bytes, find_bytes_ascii_case_insensitive_from,
        find_bytes_ascii_case_insensitive_from_scalar, find_bytes_from, find_bytes_from_scalar,
        find_html_escape_byte, find_html_escape_byte_scalar, find_json_escape_byte,
        find_json_escape_byte_scalar, find_non_ascii_whitespace, find_non_ascii_whitespace_scalar,
        is_all_ascii, is_all_ascii_scalar, is_ascii_identifier_continue, is_ascii_identifier_start,
        rfind_ascii_whitespace, rfind_ascii_whitespace_scalar, rfind_byte, rfind_byte_scalar,
        rfind_bytes_ascii_case_insensitive_before,
        rfind_bytes_ascii_case_insensitive_before_scalar, rfind_bytes_before,
        rfind_bytes_before_scalar, trim_default_bounds, trim_default_bounds_scalar,
    };

    fn corpus() -> Vec<Vec<u8>> {
        let mut cases = vec![
            Vec::new(),
            b"a".to_vec(),
            b"\n".to_vec(),
            b"\r".to_vec(),
            b"\r\n".to_vec(),
            b"\xff".to_vec(),
            b"abc_def123".to_vec(),
            b"abc-def".to_vec(),
            b"\xff\xfeabc\n\r\nz\r".to_vec(),
            (0u8..=255).collect(),
        ];

        for len in [
            2usize, 3, 7, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129, 1024, 4096,
        ] {
            let generated = (0..len)
                .map(|index| {
                    let value = ((index * 37) + (len * 11)) % 251;
                    match index % 97 {
                        0 => b'\n',
                        1 => b'\r',
                        2 => b'_',
                        3 => b'A',
                        4 => b'9',
                        _ => value as u8,
                    }
                })
                .collect();
            cases.push(generated);
        }

        for len in [1usize, 2, 8, 16, 32, 64, 128] {
            for needle in [0u8, b'a', b'\n', b'\r', 0xff] {
                let mut start = vec![b'x'; len];
                start[0] = needle;
                cases.push(start);

                let mut middle = vec![b'x'; len];
                middle[len / 2] = needle;
                cases.push(middle);

                let mut end = vec![b'x'; len];
                end[len - 1] = needle;
                cases.push(end);
            }
        }

        cases
    }

    #[test]
    fn byte_search_matches_scalar_reference() {
        for bytes in corpus() {
            for needle in [0u8, b'a', b'_', b'\n', b'\r', b'9', 0x80, 0xff] {
                assert_eq!(find_byte(&bytes, needle), find_byte_scalar(&bytes, needle));
                assert_eq!(
                    rfind_byte(&bytes, needle),
                    rfind_byte_scalar(&bytes, needle)
                );
            }
        }
    }

    #[test]
    fn multi_byte_search_matches_scalar_reference() {
        let pairs = [(b'\n', b'\r'), (b'a', b'z'), (0, 0xff), (b'_', b'-')];
        let triples = [
            (b'\n', b'\r', b';'),
            (b'a', b'z', b'_'),
            (0, 0x80, 0xff),
            (b'0', b'9', b'.'),
        ];

        for bytes in corpus() {
            for (first, second) in pairs {
                assert_eq!(
                    find_any2(&bytes, first, second),
                    find_any2_scalar(&bytes, first, second)
                );
            }

            for (first, second, third) in triples {
                assert_eq!(
                    find_any3(&bytes, first, second, third),
                    find_any3_scalar(&bytes, first, second, third)
                );
            }
        }
    }

    #[test]
    fn substring_search_matches_scalar_reference() {
        let needles: &[&[u8]] = &[
            b"",
            b"a",
            b"\n",
            b"abc",
            b"\xff\xfe",
            b"not-present",
            b"\r\nz",
        ];
        for bytes in corpus() {
            for needle in needles {
                for start in [0, 1, bytes.len() / 2, bytes.len(), bytes.len() + 1] {
                    assert_eq!(
                        find_bytes_from(&bytes, needle, start),
                        find_bytes_from_scalar(&bytes, needle, start)
                    );
                }
                assert_eq!(
                    find_bytes(&bytes, needle),
                    find_bytes_from_scalar(&bytes, needle, 0)
                );
                for end in [0, 1, bytes.len() / 2, bytes.len(), bytes.len() + 1] {
                    assert_eq!(
                        rfind_bytes_before(&bytes, needle, end),
                        rfind_bytes_before_scalar(&bytes, needle, end)
                    );
                }
            }
        }
    }

    #[test]
    fn ascii_case_insensitive_substring_search_matches_scalar_reference() {
        let needles: &[&[u8]] = &[
            b"",
            b"a",
            b"A",
            b"abc",
            b"ABC",
            b"aBc",
            b"\xffA",
            b"not-present",
        ];
        for mut bytes in corpus() {
            bytes.extend_from_slice(b"xxAbCdEfxx");
            for needle in needles {
                for start in [0, 1, bytes.len() / 2, bytes.len(), bytes.len() + 1] {
                    assert_eq!(
                        find_bytes_ascii_case_insensitive_from(&bytes, needle, start),
                        find_bytes_ascii_case_insensitive_from_scalar(&bytes, needle, start)
                    );
                }
                for end in [0, 1, bytes.len() / 2, bytes.len(), bytes.len() + 1] {
                    assert_eq!(
                        rfind_bytes_ascii_case_insensitive_before(&bytes, needle, end),
                        rfind_bytes_ascii_case_insensitive_before_scalar(&bytes, needle, end)
                    );
                }
            }
        }
    }

    #[test]
    fn byte_count_matches_scalar_reference() {
        for bytes in corpus() {
            for needle in [0u8, b'a', b'_', b'\n', b'\r', b'9', 0x80, 0xff] {
                assert_eq!(
                    count_byte(&bytes, needle),
                    count_byte_scalar(&bytes, needle)
                );
            }
        }
    }

    #[test]
    fn newline_count_matches_source_line_break_rules() {
        for bytes in corpus() {
            assert_eq!(count_newlines(&bytes), count_newlines_scalar(&bytes));
        }

        assert_eq!(count_newlines(b"a\nb"), 1);
        assert_eq!(count_newlines(b"a\r\nb"), 1);
        assert_eq!(count_newlines(b"a\rb"), 1);
        assert_eq!(count_newlines(b"\r\n\n\r"), 3);
    }

    #[test]
    fn ascii_detection_matches_scalar_reference() {
        for bytes in corpus() {
            assert_eq!(is_all_ascii(&bytes), is_all_ascii_scalar(&bytes));
        }

        assert!(is_all_ascii(b"abc_123"));
        assert!(!is_all_ascii(b"abc\xff"));
    }

    #[test]
    fn escape_scans_match_scalar_reference() {
        for bytes in corpus() {
            assert_eq!(
                find_json_escape_byte(&bytes),
                find_json_escape_byte_scalar(&bytes)
            );
            assert_eq!(
                find_html_escape_byte(&bytes),
                find_html_escape_byte_scalar(&bytes)
            );
        }

        assert_eq!(find_json_escape_byte(b"plain ascii"), None);
        assert_eq!(find_json_escape_byte(b"needs/slash"), Some(5));
        assert_eq!(find_html_escape_byte(b"a & b"), Some(2));
    }

    #[test]
    fn ascii_identifier_helpers_are_ascii_only() {
        assert!(is_ascii_identifier_start(b'_'));
        assert!(is_ascii_identifier_start(b'A'));
        assert!(is_ascii_identifier_start(b'z'));
        assert!(!is_ascii_identifier_start(b'9'));
        assert!(!is_ascii_identifier_start(0x80));

        assert!(is_ascii_identifier_continue(b'9'));
        assert!(is_ascii_identifier_continue(b'_'));
        assert!(!is_ascii_identifier_continue(b'-'));
        assert!(!is_ascii_identifier_continue(0xff));

        for bytes in corpus() {
            assert_eq!(
                ascii_identifier_continue_chunk_len(&bytes),
                ascii_identifier_continue_chunk_len_scalar(&bytes)
            );
        }

        assert_eq!(ascii_identifier_continue_chunk_len(b"abc_123-z"), 7);
        assert_eq!(ascii_identifier_continue_chunk_len(b"\xffabc"), 0);
    }

    #[test]
    fn numeric_ascii_helpers_match_scalar_reference() {
        for bytes in corpus() {
            assert_eq!(
                ascii_digit_run_len(&bytes),
                ascii_digit_run_len_scalar(&bytes)
            );
            assert_eq!(all_ascii_digits(&bytes), all_ascii_digits_scalar(&bytes));
            assert_eq!(
                find_non_ascii_whitespace(&bytes),
                find_non_ascii_whitespace_scalar(&bytes)
            );
            assert_eq!(
                find_ascii_whitespace(&bytes),
                find_ascii_whitespace_scalar(&bytes)
            );
            assert_eq!(
                rfind_ascii_whitespace(&bytes),
                rfind_ascii_whitespace_scalar(&bytes)
            );
            assert_eq!(
                contains_ascii_whitespace(&bytes),
                contains_ascii_whitespace_scalar(&bytes)
            );
            assert_eq!(
                all_ascii_whitespace(&bytes),
                all_ascii_whitespace_scalar(&bytes)
            );
        }

        assert_eq!(ascii_digit_run_len(b"12345abc"), 5);
        assert!(all_ascii_digits(b"12345"));
        assert_eq!(find_non_ascii_whitespace(b" \t\n\x0c\rabc"), Some(5));
        assert!(all_ascii_whitespace(b" \t\n\x0c\r"));
    }

    #[test]
    fn ascii_case_helpers_match_standard_byte_semantics() {
        for bytes in corpus() {
            let expected_upper = bytes.to_ascii_uppercase();
            let expected_lower = bytes.to_ascii_lowercase();

            assert_eq!(
                contains_ascii_lowercase(&bytes),
                contains_ascii_lowercase_scalar(&bytes)
            );
            assert_eq!(
                contains_ascii_uppercase(&bytes),
                contains_ascii_uppercase_scalar(&bytes)
            );
            assert_eq!(ascii_uppercase_copy(&bytes), expected_upper);
            assert_eq!(ascii_lowercase_copy(&bytes), expected_lower);

            let mut upper = bytes.clone();
            ascii_uppercase_in_place(&mut upper);
            assert_eq!(upper, expected_upper);

            let mut lower = bytes;
            ascii_lowercase_in_place(&mut lower);
            assert_eq!(lower, expected_lower);
        }
    }

    #[test]
    fn trim_default_bounds_match_scalar_reference() {
        for bytes in corpus() {
            assert_eq!(
                trim_default_bounds(&bytes),
                trim_default_bounds_scalar(&bytes)
            );
        }

        assert_eq!(trim_default_bounds(b"  abc\n"), (2, 5));
        assert_eq!(trim_default_bounds(b"\t\n\r\0\x0b"), (5, 5));
        assert_eq!(trim_default_bounds(b"abc"), (0, 3));
    }
}
