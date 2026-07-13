//! PHP numeric-string classification for runtime conversion.

use crate::PhpString;
use std::cell::RefCell;
use std::collections::HashMap;

const CACHE_LIMIT: usize = 4096;

thread_local! {
    static CLASSIFICATION_CACHE: RefCell<HashMap<NumericStringCacheKey, NumericString>> =
        RefCell::new(HashMap::new());
    static CLASSIFICATION_STATS: RefCell<NumericStringCacheStats> =
        RefCell::new(NumericStringCacheStats::default());
}

/// Numeric value parsed from a PHP string.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NumericStringValue {
    /// Integer payload.
    Int(i64),
    /// Floating-point payload.
    Float(f64),
}

impl NumericStringValue {
    /// Returns the value as an `f64`.
    #[must_use]
    pub const fn as_f64(self) -> f64 {
        match self {
            Self::Int(value) => value as f64,
            Self::Float(value) => value,
        }
    }

    /// Returns the integer truncation used by explicit casts.
    #[must_use]
    pub const fn to_i64(self) -> i64 {
        match self {
            Self::Int(value) => value,
            Self::Float(value) => value as i64,
        }
    }

    /// Returns true when this value is represented as a float.
    #[must_use]
    pub const fn is_float(self) -> bool {
        matches!(self, Self::Float(_))
    }
}

/// PHP numeric-string class in the runtime-semantics conversion subset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumericStringKind {
    /// The whole trimmed string is an integer numeric string.
    IntString,
    /// The whole trimmed string is a floating-point numeric string.
    FloatString,
    /// The string starts with a numeric prefix followed by non-whitespace.
    LeadingNumeric,
    /// The string does not start with a numeric prefix.
    NonNumeric,
}

/// Canonical form recognized by the numeric-string classifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumericStringCanonicalKind {
    /// Not a canonical numeric string for specialization purposes.
    None,
    /// Canonical decimal integer string that can also normalize to an array key.
    Integer,
    /// Canonical full float string in the modeled runtime subset.
    Float,
}

/// PHP array-key classification derived from the numeric-string model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumericStringArrayKey {
    /// String key normalizes to this integer key.
    Integer(i64),
    /// String key remains a string key.
    String,
}

/// Classification result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumericString {
    /// Numeric-string class.
    pub kind: NumericStringKind,
    /// Parsed value for full or leading numeric strings.
    pub value: Option<NumericStringValue>,
    /// Canonical representation class for guarded specialization.
    pub canonical: NumericStringCanonicalKind,
    /// True when classification fell back through a precision-sensitive path.
    pub overflow_or_precision_sensitive: bool,
}

/// Numeric-string cache stats collected by the VM when counters are enabled.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NumericStringCacheStats {
    /// Calls to the cached classifier.
    pub classify_calls: u64,
    /// Cache hits.
    pub hits: u64,
    /// Cache misses.
    pub misses: u64,
    /// Leading-numeric classifications that require warning-sensitive handling.
    pub warning_sensitive_fallbacks: u64,
    /// Classifications that crossed an overflow or precision-sensitive boundary.
    pub overflow_precision_fallbacks: u64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct NumericStringCacheKey {
    storage_id: usize,
    len: usize,
    fingerprint: u64,
}

impl NumericString {
    /// Returns true when the string has a PHP numeric prefix.
    #[must_use]
    pub const fn has_numeric_value(self) -> bool {
        self.value.is_some()
    }
}

/// Classifies a PHP string through a conservative request-local cache.
///
/// The key includes storage identity, byte length, and a stable byte
/// fingerprint. That keeps COW and in-place test mutations safe: changed bytes
/// cannot reuse an old classification even when the backing allocation is the
/// same.
#[must_use]
pub fn classify_php_string(value: &PhpString) -> NumericString {
    // Stats share the layout-stats enable gate so uninstrumented executions
    // skip the thread-local stat traffic on this hot conversion path.
    let stats_enabled = crate::layout_stats::stats_enabled();
    if stats_enabled {
        CLASSIFICATION_STATS.with(|stats| stats.borrow_mut().classify_calls += 1);
    }
    let key = NumericStringCacheKey {
        storage_id: value.storage_id(),
        len: value.len(),
        fingerprint: fingerprint(value.as_bytes()),
    };
    if let Some(classified) = CLASSIFICATION_CACHE.with(|cache| cache.borrow().get(&key).copied()) {
        if stats_enabled {
            CLASSIFICATION_STATS.with(|stats| stats.borrow_mut().hits += 1);
            record_sensitive_classification(classified);
        }
        return classified;
    }
    let classified = classify(value.as_bytes());
    CLASSIFICATION_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, classified);
    });
    if stats_enabled {
        CLASSIFICATION_STATS.with(|stats| stats.borrow_mut().misses += 1);
        record_sensitive_classification(classified);
    }
    classified
}

/// Classifies PHP array-key conversion for a string through the same cached
/// numeric-string model used by scalar conversion.
#[must_use]
pub fn classify_array_key(value: &PhpString) -> NumericStringArrayKey {
    let classified = classify_php_string(value);
    match (classified.canonical, classified.value) {
        (NumericStringCanonicalKind::Integer, Some(NumericStringValue::Int(value))) => {
            NumericStringArrayKey::Integer(value)
        }
        _ => NumericStringArrayKey::String,
    }
}

/// Returns true when a string key looks numeric but does not normalize to an
/// integer key under PHP array-key rules.
#[must_use]
pub fn array_key_has_numeric_string_ambiguity(value: &PhpString) -> bool {
    let Some(first) = value.as_bytes().first() else {
        return false;
    };
    if !(first.is_ascii_digit() || matches!(first, b'+' | b'-' | b' ' | b'\t' | b'\n' | b'\r')) {
        return false;
    }
    matches!(classify_array_key(value), NumericStringArrayKey::String)
}

/// Clears cache contents and hit/miss stats for deterministic VM executions.
/// Also enables stats recording (sticky; see `layout_stats` module docs).
pub fn reset_cache_and_stats() {
    CLASSIFICATION_CACHE.with(|cache| cache.borrow_mut().clear());
    reset_cache_stats();
}

/// Clears only numeric-string cache hit/miss stats and enables recording.
pub fn reset_cache_stats() {
    crate::layout_stats::enable_stats();
    CLASSIFICATION_STATS.with(|stats| *stats.borrow_mut() = NumericStringCacheStats::default());
}

/// Returns and clears numeric-string cache hit/miss stats.
#[must_use]
pub fn take_cache_stats() -> NumericStringCacheStats {
    CLASSIFICATION_STATS.with(|stats| {
        let mut stats = stats.borrow_mut();
        let current = *stats;
        *stats = NumericStringCacheStats::default();
        current
    })
}

/// Returns a non-destructive stats snapshot for nested request profiling.
#[must_use]
pub fn snapshot_cache_stats() -> NumericStringCacheStats {
    CLASSIFICATION_STATS.with(|stats| *stats.borrow())
}

/// Classifies a byte string using the runtime-semantics PHP numeric-string subset.
#[must_use]
pub fn classify(bytes: &[u8]) -> NumericString {
    let start = php_source::byte_kernel::find_non_ascii_whitespace(bytes).unwrap_or(bytes.len());
    let original = bytes;
    let trimmed_start = &bytes[start..];
    if trimmed_start.is_empty() {
        return non_numeric();
    }
    let Some(prefix) = numeric_prefix_len(trimmed_start) else {
        return non_numeric();
    };
    let parsed = parse_numeric_prefix(&trimmed_start[..prefix]);
    let Some(parsed) = parsed else {
        return non_numeric();
    };
    let trailing = &trimmed_start[prefix..];
    if php_source::byte_kernel::all_ascii_whitespace(trailing) {
        let kind = if parsed.value.is_float() {
            NumericStringKind::FloatString
        } else {
            NumericStringKind::IntString
        };
        let has_surrounding_whitespace = start != 0 || !trailing.is_empty();
        return NumericString {
            kind,
            value: Some(parsed.value),
            canonical: canonical_kind(
                original,
                &trimmed_start[..prefix],
                has_surrounding_whitespace,
                parsed.value,
            ),
            overflow_or_precision_sensitive: parsed.overflow_or_precision_sensitive,
        };
    }
    NumericString {
        kind: NumericStringKind::LeadingNumeric,
        value: Some(parsed.value),
        canonical: NumericStringCanonicalKind::None,
        overflow_or_precision_sensitive: parsed.overflow_or_precision_sensitive,
    }
}

fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn non_numeric() -> NumericString {
    NumericString {
        kind: NumericStringKind::NonNumeric,
        value: None,
        canonical: NumericStringCanonicalKind::None,
        overflow_or_precision_sensitive: false,
    }
}

fn numeric_prefix_len(bytes: &[u8]) -> Option<usize> {
    let mut index = usize::from(matches!(bytes.first(), Some(b'+') | Some(b'-')));
    let mut digits = 0usize;
    let integer_digits = php_source::byte_kernel::ascii_digit_run_len(&bytes[index..]);
    digits += integer_digits;
    index += integer_digits;
    let mut has_fraction = false;
    if bytes.get(index) == Some(&b'.') {
        has_fraction = true;
        index += 1;
        let fraction_digits = php_source::byte_kernel::ascii_digit_run_len(&bytes[index..]);
        digits += fraction_digits;
        index += fraction_digits;
    }
    if digits == 0 {
        return None;
    }
    if matches!(bytes.get(index), Some(b'e') | Some(b'E')) {
        let exponent_marker = index;
        index += 1;
        if matches!(bytes.get(index), Some(b'+') | Some(b'-')) {
            index += 1;
        }
        let exponent_start = index;
        index += php_source::byte_kernel::ascii_digit_run_len(&bytes[index..]);
        if index == exponent_start {
            return Some(exponent_marker);
        }
    }
    if has_fraction || matches!(bytes.get(index), Some(b'e') | Some(b'E')) {
        return Some(index);
    }
    Some(index)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ParsedNumeric {
    value: NumericStringValue,
    overflow_or_precision_sensitive: bool,
}

fn parse_numeric_prefix(bytes: &[u8]) -> Option<ParsedNumeric> {
    let text = std::str::from_utf8(bytes).ok()?;
    let is_float = bytes.iter().any(|byte| matches!(byte, b'.' | b'e' | b'E'));
    if is_float {
        let value = text.parse::<f64>().ok()?;
        return Some(ParsedNumeric {
            value: NumericStringValue::Float(value),
            overflow_or_precision_sensitive: !value.is_finite() || significant_digits(bytes) > 15,
        });
    }
    match text.parse::<i64>() {
        Ok(value) => Some(ParsedNumeric {
            value: NumericStringValue::Int(value),
            overflow_or_precision_sensitive: false,
        }),
        Err(_) => {
            let value = text.parse::<f64>().ok()?;
            Some(ParsedNumeric {
                value: NumericStringValue::Float(value),
                overflow_or_precision_sensitive: true,
            })
        }
    }
}

fn canonical_kind(
    original: &[u8],
    numeric_prefix: &[u8],
    has_surrounding_whitespace: bool,
    value: NumericStringValue,
) -> NumericStringCanonicalKind {
    if has_surrounding_whitespace || original != numeric_prefix {
        return NumericStringCanonicalKind::None;
    }
    if canonical_integer_value(numeric_prefix, value).is_some() {
        return NumericStringCanonicalKind::Integer;
    }
    if matches!(value, NumericStringValue::Float(_))
        && numeric_prefix
            .iter()
            .any(|byte| matches!(byte, b'.' | b'e' | b'E'))
    {
        return NumericStringCanonicalKind::Float;
    }
    NumericStringCanonicalKind::None
}

fn canonical_integer_value(bytes: &[u8], value: NumericStringValue) -> Option<i64> {
    let NumericStringValue::Int(value) = value else {
        return None;
    };
    let (negative, digits) = bytes
        .strip_prefix(b"-")
        .map(|digits| (true, digits))
        .unwrap_or((false, bytes));
    if bytes.starts_with(b"+")
        || digits.is_empty()
        || !php_source::byte_kernel::all_ascii_digits(digits)
    {
        return None;
    }
    if digits.len() > 1 && digits[0] == b'0' {
        return None;
    }
    if negative && value == 0 {
        return None;
    }
    Some(value)
}

fn significant_digits(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .filter(|byte| byte.is_ascii_digit())
        .skip_while(|byte| **byte == b'0')
        .count()
}

fn record_sensitive_classification(classified: NumericString) {
    CLASSIFICATION_STATS.with(|stats| {
        let mut stats = stats.borrow_mut();
        if classified.kind == NumericStringKind::LeadingNumeric {
            stats.warning_sensitive_fallbacks += 1;
        }
        if classified.overflow_or_precision_sensitive {
            stats.overflow_precision_fallbacks += 1;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{
        NumericStringArrayKey, NumericStringCanonicalKind, NumericStringKind, NumericStringValue,
        array_key_has_numeric_string_ambiguity, classify, classify_array_key, classify_php_string,
        reset_cache_and_stats, take_cache_stats,
    };
    use crate::PhpString;

    #[test]
    fn numeric_string_classifies_full_int_float_and_whitespace() {
        assert_eq!(classify(b"0").kind, NumericStringKind::IntString);
        assert_eq!(classify(b"0").value, Some(NumericStringValue::Int(0)));
        assert_eq!(
            classify(b"0").canonical,
            NumericStringCanonicalKind::Integer
        );
        assert_eq!(classify(b"0.0").kind, NumericStringKind::FloatString);
        assert_eq!(classify(b"0.0").value, Some(NumericStringValue::Float(0.0)));
        assert_eq!(
            classify(b"0.0").canonical,
            NumericStringCanonicalKind::Float
        );
        assert_eq!(classify(b" 42\t").kind, NumericStringKind::IntString);
        assert_eq!(classify(b" 42\t").value, Some(NumericStringValue::Int(42)));
        assert_eq!(
            classify(b" 42\t").canonical,
            NumericStringCanonicalKind::None
        );
    }

    #[test]
    fn numeric_string_classifies_leading_and_non_numeric() {
        assert_eq!(classify(b"42abc").kind, NumericStringKind::LeadingNumeric);
        assert_eq!(classify(b"42abc").value, Some(NumericStringValue::Int(42)));
        assert_eq!(classify(b"").kind, NumericStringKind::NonNumeric);
        assert_eq!(classify(b"abc").kind, NumericStringKind::NonNumeric);
    }

    #[test]
    fn numeric_string_cache_records_hits_misses_and_overflow() {
        reset_cache_and_stats();
        let value = PhpString::from("9223372036854775808");

        let first = classify_php_string(&value);
        let second = classify_php_string(&value);

        assert_eq!(first, second);
        assert_eq!(first.kind, NumericStringKind::FloatString);
        assert!(matches!(first.value, Some(NumericStringValue::Float(_))));
        assert!(first.overflow_or_precision_sensitive);
        let stats = take_cache_stats();
        assert_eq!(stats.classify_calls, 2);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.overflow_precision_fallbacks, 2);
    }

    #[test]
    fn numeric_string_cache_separates_whitespace_and_non_numeric_cases() {
        reset_cache_and_stats();
        let int_with_space = PhpString::from(" 42\t");
        let leading = PhpString::from("42abc");
        let non_numeric = PhpString::from("abc");

        assert_eq!(
            classify_php_string(&int_with_space).kind,
            NumericStringKind::IntString
        );
        assert_eq!(
            classify_php_string(&leading).kind,
            NumericStringKind::LeadingNumeric
        );
        assert_eq!(
            classify_php_string(&non_numeric).kind,
            NumericStringKind::NonNumeric
        );
        assert_eq!(
            classify_php_string(&non_numeric).kind,
            NumericStringKind::NonNumeric
        );
        let stats = take_cache_stats();
        assert_eq!(stats.classify_calls, 4);
        assert_eq!(stats.misses, 3);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.warning_sensitive_fallbacks, 1);
    }

    #[test]
    fn numeric_string_cache_does_not_reuse_after_cow_or_in_place_mutation() {
        reset_cache_and_stats();
        let original = PhpString::from("12");
        let mut shared = original.clone();

        assert_eq!(
            classify_php_string(&original).kind,
            NumericStringKind::IntString
        );
        shared.bytes_mut()[0] = b'x';
        assert_eq!(
            classify_php_string(&shared).kind,
            NumericStringKind::NonNumeric
        );

        let mut unique = PhpString::from("34");
        assert_eq!(
            classify_php_string(&unique).kind,
            NumericStringKind::IntString
        );
        unique.bytes_mut()[0] = b'y';
        assert_eq!(
            classify_php_string(&unique).kind,
            NumericStringKind::NonNumeric
        );

        let stats = take_cache_stats();
        assert_eq!(stats.misses, 4);
        assert_eq!(stats.hits, 0);
    }

    #[test]
    fn numeric_string_array_key_classification_matches_php_key_rules() {
        reset_cache_and_stats();

        assert_eq!(
            classify_array_key(&PhpString::from("42")),
            NumericStringArrayKey::Integer(42)
        );
        assert_eq!(
            classify_array_key(&PhpString::from("-42")),
            NumericStringArrayKey::Integer(-42)
        );
        assert_eq!(
            classify_array_key(&PhpString::from("042")),
            NumericStringArrayKey::String
        );
        assert_eq!(
            classify_array_key(&PhpString::from("+42")),
            NumericStringArrayKey::String
        );
        assert_eq!(
            classify_array_key(&PhpString::from(" 42")),
            NumericStringArrayKey::String
        );
        assert_eq!(
            classify_array_key(&PhpString::from("42.0")),
            NumericStringArrayKey::String
        );
        assert!(array_key_has_numeric_string_ambiguity(&PhpString::from(
            "042"
        )));
        assert!(!array_key_has_numeric_string_ambiguity(&PhpString::from(
            "name"
        )));
    }
}
