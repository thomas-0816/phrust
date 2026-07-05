//! standard-library PCRE2-backed helpers for ext/pcre MVP builtins.

use crate::{ArrayKey, PhpArray, PhpString, Value};
use pcre2::bytes::{Captures, Regex, RegexBuilder};
use std::collections::BTreeMap;
use std::sync::Arc;

/// preg_last_error success code.
pub const PREG_NO_ERROR: i64 = 0;
/// Generic internal PCRE error code.
pub const PREG_INTERNAL_ERROR: i64 = 1;
/// Backtracking limit error code.
pub const PREG_BACKTRACK_LIMIT_ERROR: i64 = 2;
/// Recursion limit error code.
pub const PREG_RECURSION_LIMIT_ERROR: i64 = 3;
/// Bad UTF-8 subject error code.
pub const PREG_BAD_UTF8_ERROR: i64 = 4;
/// Bad UTF-8 offset error code.
pub const PREG_BAD_UTF8_OFFSET_ERROR: i64 = 5;
/// JIT stack limit error code.
pub const PREG_JIT_STACKLIMIT_ERROR: i64 = 6;

/// `preg_match_all` pattern-order result layout.
pub const PREG_PATTERN_ORDER: i64 = 1;
/// `preg_match_all` set-order result layout.
pub const PREG_SET_ORDER: i64 = 2;
/// Capture values with byte offsets.
pub const PREG_OFFSET_CAPTURE: i64 = 256;
/// Preserve unmatched captures as null.
pub const PREG_UNMATCHED_AS_NULL: i64 = 512;

/// Drop empty pieces from `preg_split`.
pub const PREG_SPLIT_NO_EMPTY: i64 = 1;
/// Include captured delimiters in `preg_split`.
pub const PREG_SPLIT_DELIM_CAPTURE: i64 = 2;
/// Return split pieces with byte offsets.
pub const PREG_SPLIT_OFFSET_CAPTURE: i64 = 4;

/// Invert `preg_grep` result selection.
pub const PREG_GREP_INVERT: i64 = 1;

/// Request-local `preg_last_error` state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcreLastErrorState {
    code: i64,
    message: String,
}

impl Default for PcreLastErrorState {
    fn default() -> Self {
        Self::new(PREG_NO_ERROR, preg_error_message(PREG_NO_ERROR))
    }
}

impl PcreLastErrorState {
    /// Creates PCRE last-error state.
    #[must_use]
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Updates the stored error code and message.
    pub fn set(&mut self, code: i64, message: impl Into<String>) {
        self.code = code;
        self.message = message.into();
    }

    /// Clears the state to `PREG_NO_ERROR`.
    pub fn clear(&mut self) {
        self.set(PREG_NO_ERROR, preg_error_message(PREG_NO_ERROR));
    }

    /// Current PHP preg error code.
    #[must_use]
    pub const fn code(&self) -> i64 {
        self.code
    }

    /// Current PHP preg error message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Request-local compiled-pattern cache.
#[derive(Default)]
pub struct PcreCache {
    entries: BTreeMap<String, Arc<CompiledPattern>>,
}

impl PcreCache {
    /// Compile or reuse a delimited PHP PCRE pattern.
    pub fn compile(&mut self, pattern: &PhpString) -> Result<Arc<CompiledPattern>, PcreFailure> {
        let key = pattern.to_string_lossy();
        if let Some(compiled) = self.entries.get(&key) {
            return Ok(Arc::clone(compiled));
        }

        let parsed = parse_delimited_pattern(pattern.as_bytes())?;
        let compiled = Arc::new(CompiledPattern {
            regex: compile_regex(&parsed.body, &parsed.modifiers)?,
        });
        self.entries.insert(key, Arc::clone(&compiled));
        Ok(compiled)
    }
}

/// Compiled PCRE2 pattern.
pub struct CompiledPattern {
    regex: Regex,
}

impl CompiledPattern {
    /// Match the subject once.
    pub fn captures<'s>(&self, subject: &'s [u8]) -> Result<Option<Captures<'s>>, PcreFailure> {
        self.regex
            .captures(subject)
            .map_err(PcreFailure::from_pcre_error)
    }

    /// Match the subject repeatedly.
    pub fn captures_iter<'r, 's>(
        &'r self,
        subject: &'s [u8],
    ) -> pcre2::bytes::CaptureMatches<'r, 's> {
        self.regex.captures_iter(subject)
    }

    /// Test whether the subject matches.
    pub fn is_match(&self, subject: &[u8]) -> Result<bool, PcreFailure> {
        self.regex
            .is_match(subject)
            .map_err(PcreFailure::from_pcre_error)
    }

    /// Capturing group names indexed by capture slot.
    #[must_use]
    pub fn capture_names(&self) -> &[Option<String>] {
        self.regex.capture_names()
    }
}

/// PCRE failure represented as PHP preg error metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcreFailure {
    code: i64,
    message: String,
}

impl PcreFailure {
    /// Creates a failure.
    #[must_use]
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// PHP preg error code.
    #[must_use]
    pub const fn code(&self) -> i64 {
        self.code
    }

    /// Human-readable message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    fn from_pcre_error(error: pcre2::Error) -> Self {
        Self::new(classify_pcre_error(&error), error.to_string())
    }
}

impl From<pcre2::Error> for PcreFailure {
    fn from(error: pcre2::Error) -> Self {
        Self::from_pcre_error(error)
    }
}

#[derive(Debug)]
struct ParsedPattern {
    body: String,
    modifiers: String,
}

fn parse_delimited_pattern(pattern: &[u8]) -> Result<ParsedPattern, PcreFailure> {
    if pattern.len() < 2 {
        return Err(PcreFailure::new(
            PREG_INTERNAL_ERROR,
            "No ending delimiter found",
        ));
    }
    let delimiter = pattern[0];
    if delimiter.is_ascii_alphanumeric() || delimiter == b'\\' || delimiter.is_ascii_whitespace() {
        return Err(PcreFailure::new(
            PREG_INTERNAL_ERROR,
            "Delimiter must not be alphanumeric, backslash, or whitespace",
        ));
    }

    let closing_delimiter = closing_delimiter(delimiter);
    let paired_delimiter = closing_delimiter != delimiter;
    let mut nesting_depth = 0usize;
    let mut escaped = false;
    let mut in_class = false;
    for index in 1..pattern.len() {
        let byte = pattern[index];
        if escaped {
            escaped = false;
            continue;
        }
        if byte == b'\\' {
            escaped = true;
            continue;
        }

        if in_class {
            if byte == b']' {
                in_class = false;
            }
            continue;
        }

        if paired_delimiter && byte == delimiter {
            nesting_depth += 1;
            continue;
        }
        if byte == b'[' {
            in_class = true;
            continue;
        }
        if byte == closing_delimiter {
            if paired_delimiter && nesting_depth > 0 {
                nesting_depth -= 1;
                continue;
            }
            let body = std::str::from_utf8(&pattern[1..index])
                .map_err(|_| PcreFailure::new(PREG_BAD_UTF8_ERROR, "Pattern is not valid UTF-8"))?;
            let modifiers = std::str::from_utf8(&pattern[index + 1..]).map_err(|_| {
                PcreFailure::new(PREG_BAD_UTF8_ERROR, "Pattern modifiers are not valid UTF-8")
            })?;
            return Ok(ParsedPattern {
                body: body.to_string(),
                modifiers: modifiers.to_string(),
            });
        }
    }

    if paired_delimiter {
        return Err(PcreFailure::new(
            PREG_INTERNAL_ERROR,
            format!(
                "No ending matching delimiter '{}' found",
                closing_delimiter as char
            ),
        ));
    }

    Err(PcreFailure::new(
        PREG_INTERNAL_ERROR,
        "No ending delimiter found",
    ))
}

fn closing_delimiter(delimiter: u8) -> u8 {
    match delimiter {
        b'(' => b')',
        b'[' => b']',
        b'{' => b'}',
        b'<' => b'>',
        _ => delimiter,
    }
}

fn compile_regex(body: &str, modifiers: &str) -> Result<Regex, PcreFailure> {
    let mut builder = RegexBuilder::new();
    builder.jit_if_available(true);
    for modifier in modifiers.chars() {
        match modifier {
            'i' => {
                builder.caseless(true);
            }
            'm' => {
                builder.multi_line(true);
            }
            's' => {
                builder.dotall(true);
            }
            'x' => {
                builder.extended(true);
            }
            'u' => {
                builder.utf(true).ucp(true);
            }
            'A' | 'D' | 'S' | 'U' | 'J' => {}
            _ => {
                return Err(PcreFailure::new(
                    PREG_INTERNAL_ERROR,
                    format!("Unsupported PCRE modifier `{modifier}`"),
                ));
            }
        }
    }
    builder.build(body).map_err(PcreFailure::from_pcre_error)
}

/// Convert one capture set to a PHP array.
#[must_use]
pub fn captures_to_array(captures: &Captures<'_>, flags: i64) -> Value {
    captures_to_array_with_names(captures, &[], flags, 0)
}

/// Convert one capture set to a PHP array with named captures and an offset base.
#[must_use]
pub fn captures_to_array_with_names(
    captures: &Captures<'_>,
    capture_names: &[Option<String>],
    flags: i64,
    offset_base: usize,
) -> Value {
    let mut array = PhpArray::new();
    for index in 0..captures.len() {
        if let Some(Some(name)) = capture_names.get(index) {
            array.insert(
                ArrayKey::String(name.as_bytes().to_vec().into()),
                capture_slot(captures, index, flags, offset_base),
            );
        }
        let value = capture_slot(captures, index, flags, offset_base);
        array.insert(ArrayKey::Int(index as i64), value);
    }
    Value::Array(array)
}

/// Convert one capture slot to a PHP value.
#[must_use]
pub fn capture_slot(
    captures: &Captures<'_>,
    index: usize,
    flags: i64,
    offset_base: usize,
) -> Value {
    match captures.get(index) {
        Some(value) if flags & PREG_OFFSET_CAPTURE != 0 => Value::packed_array(vec![
            Value::string(value.as_bytes().to_vec()),
            Value::Int((offset_base + value.start()) as i64),
        ]),
        Some(value) => Value::string(value.as_bytes().to_vec()),
        None if flags & PREG_OFFSET_CAPTURE != 0 && flags & PREG_UNMATCHED_AS_NULL != 0 => {
            Value::packed_array(vec![Value::Null, Value::Int(-1)])
        }
        None if flags & PREG_OFFSET_CAPTURE != 0 => {
            Value::packed_array(vec![Value::string(Vec::new()), Value::Int(-1)])
        }
        None if flags & PREG_UNMATCHED_AS_NULL != 0 => Value::Null,
        None => Value::string(Vec::new()),
    }
}

/// Returns a PHP preg_last_error message.
#[must_use]
pub const fn preg_error_message(code: i64) -> &'static str {
    match code {
        PREG_NO_ERROR => "No error",
        PREG_INTERNAL_ERROR => "Internal error",
        PREG_BACKTRACK_LIMIT_ERROR => "Backtrack limit exhausted",
        PREG_RECURSION_LIMIT_ERROR => "Recursion limit exhausted",
        PREG_BAD_UTF8_ERROR => "Malformed UTF-8 data",
        PREG_BAD_UTF8_OFFSET_ERROR => {
            "The offset did not correspond to the beginning of a valid UTF-8 code point"
        }
        PREG_JIT_STACKLIMIT_ERROR => "JIT stack limit exhausted",
        _ => "PCRE error",
    }
}

/// Quote literal text for use in a PCRE pattern.
#[must_use]
pub fn preg_quote(text: &[u8], delimiter: Option<u8>) -> Vec<u8> {
    let mut quoted = Vec::with_capacity(text.len());
    for &byte in text {
        let should_quote = matches!(
            byte,
            b'.' | b'\\'
                | b'+'
                | b'*'
                | b'?'
                | b'['
                | b'^'
                | b']'
                | b'$'
                | b'('
                | b')'
                | b'{'
                | b'}'
                | b'='
                | b'!'
                | b'<'
                | b'>'
                | b'|'
                | b':'
                | b'-'
                | b'#'
        ) || Some(byte) == delimiter;
        if should_quote {
            quoted.push(b'\\');
        }
        quoted.push(byte);
    }
    quoted
}

fn classify_pcre_error(error: &pcre2::Error) -> i64 {
    let message = error.to_string();
    if message.contains("UTF") {
        PREG_BAD_UTF8_ERROR
    } else if message.contains("JIT stack") {
        PREG_JIT_STACKLIMIT_ERROR
    } else if message.contains("match limit") {
        PREG_BACKTRACK_LIMIT_ERROR
    } else if message.contains("recursion") {
        PREG_RECURSION_LIMIT_ERROR
    } else {
        PREG_INTERNAL_ERROR
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed_body(pattern: &[u8]) -> String {
        parse_delimited_pattern(pattern).unwrap().body
    }

    #[test]
    fn parses_php_paired_delimiters_with_nesting() {
        assert_eq!(parsed_body(b"{a{1,2}}"), "a{1,2}");
        assert_eq!(parsed_body(b"{[a-z]{1,2}}i"), "[a-z]{1,2}");
        assert_eq!(parsed_body(b"(a(b)c)"), "a(b)c");
        assert_eq!(parsed_body(b"[[a-z]]"), "[a-z]");
        assert_eq!(parsed_body(b"<(?<word>[a-z]+)>"), "(?<word>[a-z]+)");
    }

    #[test]
    fn reports_expected_closing_delimiter_for_paired_patterns() {
        let error = parse_delimited_pattern(b"{abc").unwrap_err();
        assert_eq!(error.code(), PREG_INTERNAL_ERROR);
        assert_eq!(error.message(), "No ending matching delimiter '}' found");
    }
}
