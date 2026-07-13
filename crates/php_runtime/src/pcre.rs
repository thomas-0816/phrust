//! standard-library PCRE2-backed helpers for ext/pcre MVP builtins.

use crate::{ArrayKey, PhpArray, PhpString, Value};
use pcre2::bytes::{Captures, MatchOptions, Regex, RegexBuilder};
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

const PHP_PCRE_JIT_STACK_MAX_SIZE: usize = 192 * 1024;

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
        let message = preg_error_message(PREG_NO_ERROR);
        if self.code == PREG_NO_ERROR && self.message == message {
            return;
        }
        self.set(PREG_NO_ERROR, message);
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
    utf8_subjects: BTreeMap<PcreUtf8SubjectKey, Utf8SubjectValidation>,
    last_utf8_subject: Option<(PhpString, Utf8SubjectValidation)>,
}

impl std::fmt::Debug for PcreCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PcreCache")
            .field("entries_len", &self.entries.len())
            .field("utf8_subjects_len", &self.utf8_subjects.len())
            .finish()
    }
}

impl PcreCache {
    /// Compile or reuse a delimited PHP PCRE pattern.
    pub fn compile(&mut self, pattern: &PhpString) -> Result<Arc<CompiledPattern>, PcreFailure> {
        self.compile_with_limits(pattern, PcreMatchLimits::default())
    }

    /// Compile or reuse a delimited PHP PCRE pattern with match-time limits.
    pub fn compile_with_limits(
        &mut self,
        pattern: &PhpString,
        limits: PcreMatchLimits,
    ) -> Result<Arc<CompiledPattern>, PcreFailure> {
        let key = pattern.to_string_lossy();
        let key = format!(
            "{key}\0{:?}\0{:?}\0{}",
            limits.backtrack_limit, limits.recursion_limit, limits.jit
        );
        if let Some(compiled) = self.entries.get(&key) {
            return Ok(Arc::clone(compiled));
        }

        let parsed = parse_delimited_pattern(pattern.as_bytes())?;
        let utf8_mode = parsed.modifiers.chars().any(|modifier| modifier == 'u');
        let start_offset_anchored = pattern_starts_with_start_offset_anchor(&parsed.body);
        let fast_path = PcreFastPath::classify(&parsed.body, utf8_mode);
        let compiled = Arc::new(CompiledPattern {
            regex: compile_regex(&parsed.body, &parsed.modifiers, limits)?,
            utf8_mode,
            start_offset_anchored,
            fast_path,
        });
        self.entries.insert(key, Arc::clone(&compiled));
        Ok(compiled)
    }

    /// Validate a PHP string subject for a UTF-8 pattern using request-local
    /// validation state.
    pub fn validate_utf8_subject_for_pattern(
        &mut self,
        pattern: &CompiledPattern,
        subject: &PhpString,
    ) -> Result<(), PcreFailure> {
        self.match_options_for_subject_at_offset(pattern, subject, 0)
            .map(|_| ())
    }

    /// Validate a PHP string subject and offset for a UTF-8 pattern using
    /// request-local validation state.
    pub fn validate_utf8_subject_for_pattern_at_offset(
        &mut self,
        pattern: &CompiledPattern,
        subject: &PhpString,
        offset: usize,
    ) -> Result<(), PcreFailure> {
        self.match_options_for_subject_at_offset(pattern, subject, offset)
            .map(|_| ())
    }

    /// Validates a UTF-8 subject and offset through the request-local cache
    /// and reports whether the validated subject is pure ASCII.
    pub fn validate_utf8_ascii_subject_at_offset(
        &mut self,
        subject: &PhpString,
        offset: usize,
    ) -> Result<bool, PcreFailure> {
        let subject_bytes = subject.as_bytes();
        let validation = self.cached_utf8_subject_validation(subject, subject_bytes);
        validation.validate_offset(subject_bytes, offset)?;
        Ok(matches!(
            validation,
            Utf8SubjectValidation::WholeValid { ascii: true }
        ))
    }

    /// Returns PCRE2 match options that are valid for this already-validated
    /// PHP subject at the requested offset.
    pub fn match_options_for_subject_at_offset(
        &mut self,
        pattern: &CompiledPattern,
        subject: &PhpString,
        offset: usize,
    ) -> Result<MatchOptions, PcreFailure> {
        let mut options = MatchOptions::default();
        if pattern.is_start_offset_anchored() {
            options = options.anchored(true);
        }
        if !pattern.is_utf8_mode() {
            return Ok(options);
        }
        let subject_bytes = subject.as_bytes();
        let validation = self.cached_utf8_subject_validation(subject, subject_bytes);
        validation.validate_offset(subject_bytes, offset)?;
        Ok(options)
    }

    fn cached_utf8_subject_validation(
        &mut self,
        subject: &PhpString,
        subject_bytes: &[u8],
    ) -> Utf8SubjectValidation {
        if let Some((cached_subject, validation)) = &self.last_utf8_subject
            && cached_subject.storage_id() == subject.storage_id()
            && cached_subject.len() == subject.len()
        {
            return *validation;
        }
        let key = PcreUtf8SubjectKey::new(subject);
        let validation = *self
            .utf8_subjects
            .entry(key)
            .or_insert_with(|| Utf8SubjectValidation::scan(subject_bytes));
        self.last_utf8_subject = Some((subject.clone(), validation));
        validation
    }
}

/// A delimited PCRE pattern that can be matched as a byte literal without
/// invoking PCRE2.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimpleLiteralPattern {
    bytes: Vec<u8>,
}

impl SimpleLiteralPattern {
    /// Literal bytes matched by this pattern.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Returns a simple literal pattern when a PHP PCRE pattern has no active
/// modifiers and an ASCII alphanumeric body.
///
/// This intentionally stays narrow: broader quoting, UTF-8, case folding,
/// anchoring, or extended-mode semantics stay on the PCRE2 path.
pub fn simple_literal_pattern(
    pattern: &PhpString,
) -> Result<Option<SimpleLiteralPattern>, PcreFailure> {
    if let Some(literal) = simple_literal_pattern_fast(pattern.as_bytes())? {
        return Ok(Some(literal));
    }
    let parsed = parse_delimited_pattern(pattern.as_bytes())?;
    for modifier in parsed.modifiers.chars() {
        match modifier {
            modifier if modifier.is_ascii_whitespace() => {}
            '\0' => {
                return Err(PcreFailure::new(
                    PREG_INTERNAL_ERROR,
                    "NUL byte is not a valid modifier",
                ));
            }
            'S' | 'X' => {}
            'i' | 'm' | 's' | 'x' | 'u' | 'A' | 'D' | 'J' | 'n' | 'r' | 'U' => {
                return Ok(None);
            }
            _ => {
                return Err(PcreFailure::new(
                    PREG_INTERNAL_ERROR,
                    format!("Unknown modifier '{modifier}'"),
                ));
            }
        }
    }
    let bytes = parsed.body.into_bytes();
    if bytes.is_empty()
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        return Ok(None);
    }
    Ok(Some(SimpleLiteralPattern { bytes }))
}

fn simple_literal_pattern_fast(
    pattern: &[u8],
) -> Result<Option<SimpleLiteralPattern>, PcreFailure> {
    if pattern.len() < 3 {
        return Ok(None);
    }
    let delimiter = pattern[0];
    if delimiter.is_ascii_alphanumeric()
        || delimiter.is_ascii_whitespace()
        || delimiter == b'\\'
        || delimiter == b'\0'
    {
        return Err(PcreFailure::new(
            PREG_INTERNAL_ERROR,
            "Delimiter must not be alphanumeric, backslash, or NUL byte",
        ));
    }
    let closing_delimiter = closing_delimiter(delimiter);
    if pattern.last().copied() != Some(closing_delimiter) {
        return Ok(None);
    }
    let body = &pattern[1..pattern.len() - 1];
    if body.is_empty()
        || !body
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        return Ok(None);
    }
    Ok(Some(SimpleLiteralPattern {
        bytes: body.to_vec(),
    }))
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PcreUtf8SubjectKey {
    storage_id: usize,
    len: usize,
    hash: u64,
}

impl PcreUtf8SubjectKey {
    fn new(subject: &PhpString) -> Self {
        Self {
            storage_id: subject.storage_id(),
            len: subject.len(),
            hash: subject.stable_hash(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Utf8SubjectValidation {
    WholeValid { ascii: bool },
    Invalid,
}

impl Utf8SubjectValidation {
    fn scan(subject: &[u8]) -> Self {
        match std::str::from_utf8(subject) {
            Ok(_) => Self::WholeValid {
                ascii: subject.is_ascii(),
            },
            Err(_) => Self::Invalid,
        }
    }

    fn validate_offset(self, subject: &[u8], offset: usize) -> Result<(), PcreFailure> {
        match self {
            Self::WholeValid { ascii } => {
                if ascii
                    || std::str::from_utf8(subject).is_ok_and(|text| text.is_char_boundary(offset))
                {
                    Ok(())
                } else {
                    Err(PcreFailure::new(
                        PREG_BAD_UTF8_OFFSET_ERROR,
                        preg_error_message(PREG_BAD_UTF8_OFFSET_ERROR),
                    ))
                }
            }
            Self::Invalid if std::str::from_utf8(&subject[offset..]).is_ok() => Ok(()),
            Self::Invalid => Err(PcreFailure::new(
                PREG_BAD_UTF8_ERROR,
                preg_error_message(PREG_BAD_UTF8_ERROR),
            )),
        }
    }
}

/// PCRE match-time limits derived from PHP `pcre.*` INI settings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcreMatchLimits {
    /// PHP `pcre.backtrack_limit`.
    pub backtrack_limit: Option<u32>,
    /// PHP `pcre.recursion_limit`.
    pub recursion_limit: Option<u32>,
    /// PHP `pcre.jit`.
    pub jit: bool,
}

impl Default for PcreMatchLimits {
    fn default() -> Self {
        Self {
            backtrack_limit: None,
            recursion_limit: None,
            jit: true,
        }
    }
}

/// Returns whether the linked PCRE2 library reports JIT support.
#[must_use]
pub fn is_jit_available() -> bool {
    pcre2::is_jit_available()
}

/// Compiled PCRE2 pattern.
pub struct CompiledPattern {
    regex: Regex,
    utf8_mode: bool,
    start_offset_anchored: bool,
    fast_path: PcreFastPath,
}

impl CompiledPattern {
    /// Match the subject once.
    pub fn captures<'s>(&self, subject: &'s [u8]) -> Result<Option<Captures<'s>>, PcreFailure> {
        self.regex
            .captures(subject)
            .map_err(PcreFailure::from_pcre_error)
    }

    /// Match the subject once at or after a byte offset.
    pub fn captures_at<'s>(
        &self,
        subject: &'s [u8],
        start: usize,
    ) -> Result<Option<Captures<'s>>, PcreFailure> {
        self.regex
            .captures_at(subject, start)
            .map_err(PcreFailure::from_pcre_error)
    }

    /// Match the subject once at or after a byte offset with PCRE2 options.
    pub fn captures_at_with_options<'s>(
        &self,
        subject: &'s [u8],
        start: usize,
        options: MatchOptions,
    ) -> Result<Option<Captures<'s>>, PcreFailure> {
        self.regex
            .captures_at_with_options(subject, start, options)
            .map_err(PcreFailure::from_pcre_error)
    }

    /// Match the subject repeatedly.
    pub fn captures_iter<'r, 's>(
        &'r self,
        subject: &'s [u8],
    ) -> pcre2::bytes::CaptureMatches<'r, 's> {
        self.regex.captures_iter(subject)
    }

    /// Match repeatedly using PHP's bump-along behavior for empty matches.
    pub fn for_each_php_match<'s, E>(
        &self,
        subject: &'s [u8],
        start_offset: usize,
        handle_match: impl FnMut(Captures<'s>) -> Result<bool, E>,
        map_error: impl FnMut(PcreFailure) -> E,
    ) -> Result<(), E> {
        self.for_each_php_match_with_options(
            subject,
            start_offset,
            MatchOptions::default(),
            handle_match,
            map_error,
        )
    }

    /// Match repeatedly using PHP's bump-along behavior and base PCRE2 options.
    pub fn for_each_php_match_with_options<'s, E>(
        &self,
        subject: &'s [u8],
        start_offset: usize,
        base_options: MatchOptions,
        mut handle_match: impl FnMut(Captures<'s>) -> Result<bool, E>,
        mut map_error: impl FnMut(PcreFailure) -> E,
    ) -> Result<(), E> {
        let mut search_start = start_offset;
        let mut retry_after_empty_match = false;
        let mut retry_allows_start_reset = false;

        while search_start <= subject.len() {
            let captures = if retry_after_empty_match {
                let mut options = base_options.not_empty_at_start(true);
                if !retry_allows_start_reset {
                    options = options.anchored(true);
                }
                match self.captures_at_with_options(subject, search_start, options) {
                    Ok(Some(captures)) => Some(captures),
                    Ok(None) => {
                        retry_after_empty_match = false;
                        search_start =
                            next_preg_search_offset(subject, search_start, self.is_utf8_mode());
                        continue;
                    }
                    Err(error) => return Err(map_error(error)),
                }
            } else {
                match self.captures_at_with_options(subject, search_start, base_options) {
                    Ok(captures) => captures,
                    Err(error) => return Err(map_error(error)),
                }
            };
            let Some(captures) = captures else {
                break;
            };
            let Some(full) = captures.get(0) else {
                continue;
            };
            let match_start = full.start();
            let empty_match = full.start() == full.end();
            let next_start = full.end();
            if !handle_match(captures)? {
                break;
            }
            retry_after_empty_match = empty_match;
            retry_allows_start_reset = empty_match && match_start > search_start;
            search_start = next_start;
        }

        Ok(())
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

    /// Whether this pattern was compiled with PHP's UTF-8 (`u`) modifier.
    #[must_use]
    pub const fn is_utf8_mode(&self) -> bool {
        self.utf8_mode
    }

    /// Whether the pattern can only match at the caller-provided start offset.
    #[must_use]
    pub const fn is_start_offset_anchored(&self) -> bool {
        self.start_offset_anchored
    }

    /// Return a direct match result for simple patterns handled without PCRE2.
    #[must_use]
    pub fn fast_match_at(&self, subject: &[u8], start: usize) -> Option<Option<(usize, usize)>> {
        self.fast_path.match_at(subject, start)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum PcreFastPath {
    #[default]
    None,
    StartOffsetAsciiWord,
}

impl PcreFastPath {
    fn classify(body: &str, utf8_mode: bool) -> Self {
        if utf8_mode && body == r"\G\w" {
            Self::StartOffsetAsciiWord
        } else {
            Self::None
        }
    }

    fn match_at(self, subject: &[u8], start: usize) -> Option<Option<(usize, usize)>> {
        match self {
            Self::None => None,
            Self::StartOffsetAsciiWord => match subject.get(start).copied() {
                Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_' => {
                    Some(Some((start, start + 1)))
                }
                Some(byte) if byte.is_ascii() => Some(None),
                Some(_) => None,
                None => Some(None),
            },
        }
    }
}

fn pattern_starts_with_start_offset_anchor(body: &str) -> bool {
    body.as_bytes().starts_with(br"\G")
}

fn next_preg_search_offset(subject: &[u8], offset: usize, utf8_mode: bool) -> usize {
    if offset >= subject.len() {
        return subject.len() + 1;
    }
    if !utf8_mode {
        return offset + 1;
    }
    std::str::from_utf8(&subject[offset..])
        .ok()
        .and_then(|rest| rest.chars().next())
        .map_or(offset + 1, |character| offset + character.len_utf8())
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

    fn from_pcre_compile_error(error: pcre2::Error) -> Self {
        Self::new(
            classify_pcre_error(&error),
            php_compile_error_message(&error.to_string()),
        )
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
    if pattern.is_empty() || pattern.iter().all(u8::is_ascii_whitespace) {
        return Err(PcreFailure::new(
            PREG_INTERNAL_ERROR,
            "Empty regular expression",
        ));
    }
    let delimiter = pattern[0];
    if delimiter.is_ascii_alphanumeric()
        || delimiter.is_ascii_whitespace()
        || delimiter == b'\\'
        || delimiter == b'\0'
    {
        return Err(PcreFailure::new(
            PREG_INTERNAL_ERROR,
            "Delimiter must not be alphanumeric, backslash, or NUL byte",
        ));
    }

    let closing_delimiter = closing_delimiter(delimiter);
    let paired_delimiter = closing_delimiter != delimiter;
    let mut nesting_depth = 0usize;
    let mut escaped = false;
    let mut in_class = false;
    let mut class_delimiter_candidate = None;
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
                class_delimiter_candidate = None;
            } else if !paired_delimiter && byte == closing_delimiter {
                class_delimiter_candidate = Some(index);
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

    if !paired_delimiter
        && in_class
        && let Some(index) = class_delimiter_candidate
    {
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
        format!("No ending delimiter '{}' found", delimiter as char),
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

fn compile_regex(
    body: &str,
    modifiers: &str,
    limits: PcreMatchLimits,
) -> Result<Regex, PcreFailure> {
    let mut builder = RegexBuilder::new();
    builder.jit_if_available(limits.jit);
    if limits.jit {
        builder.max_jit_stack_size(Some(PHP_PCRE_JIT_STACK_MAX_SIZE));
    }
    builder.match_limit(limits.backtrack_limit);
    builder.depth_limit(limits.recursion_limit);
    let mut inline_options = String::new();
    let mut anchored = false;
    let mut dollar_endonly = false;
    let mut multiline = false;
    for modifier in modifiers.chars() {
        match modifier {
            modifier if modifier.is_ascii_whitespace() => {}
            '\0' => {
                return Err(PcreFailure::new(
                    PREG_INTERNAL_ERROR,
                    "NUL byte is not a valid modifier",
                ));
            }
            'i' => {
                builder.caseless(true);
            }
            'm' => {
                builder.multi_line(true);
                multiline = true;
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
            'A' => {
                anchored = true;
            }
            'D' => {
                dollar_endonly = true;
            }
            'J' | 'n' | 'r' | 'U' => {
                inline_options.push(modifier);
            }
            'S' | 'X' => {}
            _ => {
                return Err(PcreFailure::new(
                    PREG_INTERNAL_ERROR,
                    format!("Unknown modifier '{modifier}'"),
                ));
            }
        }
    }
    let body = php_compile_pattern_body(
        body,
        &inline_options,
        anchored,
        dollar_endonly && !multiline,
    );
    builder
        .build(&body)
        .map_err(PcreFailure::from_pcre_compile_error)
}

fn php_compile_pattern_body(
    body: &str,
    inline_options: &str,
    anchored: bool,
    dollar_endonly: bool,
) -> String {
    if inline_options.is_empty() && !anchored && !dollar_endonly {
        return body.to_owned();
    }

    let body = if dollar_endonly {
        php_rewrite_dollar_endonly(body)
    } else {
        body.to_owned()
    };

    let mut compiled = String::new();
    if !inline_options.is_empty() {
        compiled.push_str("(?");
        compiled.push_str(inline_options);
        compiled.push(')');
    }
    if anchored {
        compiled.push_str("\\A(?:");
        compiled.push_str(&body);
        compiled.push(')');
    } else {
        compiled.push_str(&body);
    }
    compiled
}

fn php_rewrite_dollar_endonly(body: &str) -> String {
    let chars: Vec<char> = body.chars().collect();
    let mut rewritten = String::with_capacity(body.len());
    let mut index = 0;
    let mut in_class = false;
    let mut in_quote = false;

    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();

        if in_quote {
            rewritten.push(current);
            if current == '\\' && next == Some('E') {
                rewritten.push('E');
                index += 2;
                in_quote = false;
            } else {
                index += 1;
            }
            continue;
        }

        if current == '\\' {
            rewritten.push(current);
            if let Some(next) = next {
                rewritten.push(next);
                index += 2;
                if next == 'Q' {
                    in_quote = true;
                }
            } else {
                index += 1;
            }
            continue;
        }

        if in_class {
            rewritten.push(current);
            if current == ']' {
                in_class = false;
            }
            index += 1;
            continue;
        }

        match current {
            '[' => {
                in_class = true;
                rewritten.push(current);
            }
            '$' => rewritten.push_str("\\z"),
            _ => rewritten.push(current),
        }
        index += 1;
    }

    rewritten
}

fn php_compile_error_message(message: &str) -> String {
    let Some(rest) = message.strip_prefix("PCRE2: error compiling pattern at offset ") else {
        return message.to_owned();
    };
    let Some((offset, detail)) = rest.split_once(": ") else {
        return message.to_owned();
    };
    format!("Compilation failed: {detail} at offset {offset}")
}

/// Returns whether a byte offset can be used as a PHP `/u` subject offset.
#[must_use]
pub fn is_valid_utf8_offset(subject: &[u8], offset: usize) -> bool {
    match std::str::from_utf8(subject) {
        Ok(subject) => subject.is_char_boundary(offset),
        Err(_) => true,
    }
}

/// Validate PHP `/u` subject bytes before matching.
pub fn validate_utf8_subject_for_pattern(
    pattern: &CompiledPattern,
    subject: &[u8],
) -> Result<(), PcreFailure> {
    validate_utf8_subject_for_pattern_at_offset(pattern, subject, 0)
}

/// Validate PHP `/u` subject bytes from the requested match offset.
pub fn validate_utf8_subject_for_pattern_at_offset(
    pattern: &CompiledPattern,
    subject: &[u8],
    offset: usize,
) -> Result<(), PcreFailure> {
    if !pattern.is_utf8_mode() {
        return Ok(());
    }
    match std::str::from_utf8(subject) {
        Ok(subject) if !subject.is_char_boundary(offset) => Err(PcreFailure::new(
            PREG_BAD_UTF8_OFFSET_ERROR,
            preg_error_message(PREG_BAD_UTF8_OFFSET_ERROR),
        )),
        Ok(_) => Ok(()),
        Err(_) if std::str::from_utf8(&subject[offset..]).is_ok() => Ok(()),
        Err(_) => Err(PcreFailure::new(
            PREG_BAD_UTF8_ERROR,
            preg_error_message(PREG_BAD_UTF8_ERROR),
        )),
    }
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
    captures_to_array_with_names_for_order(captures, capture_names, flags, offset_base, true)
}

/// Convert one capture set to a PHP array, optionally retaining trailing
/// unmatched captures for `preg_match_all` pattern-order grouping.
#[must_use]
pub fn captures_to_array_with_names_for_order(
    captures: &Captures<'_>,
    capture_names: &[Option<String>],
    flags: i64,
    offset_base: usize,
    trim_trailing_unmatched: bool,
) -> Value {
    let mut array = PhpArray::new();
    let capture_count = capture_count_for_output(captures, flags, trim_trailing_unmatched);
    for index in 0..capture_count {
        let value = capture_slot(captures, index, flags, offset_base);
        if let Some(Some(name)) = capture_names.get(index) {
            insert_named_capture(
                &mut array,
                name,
                value.clone(),
                captures.get(index).is_none(),
            );
        }
        array.insert(ArrayKey::Int(index as i64), value);
    }
    if let Some(mark) = captures.mark() {
        array.insert(
            ArrayKey::String(PhpString::from("MARK")),
            Value::string(mark.to_vec()),
        );
    }
    Value::Array(array)
}

fn capture_count_for_output(
    captures: &Captures<'_>,
    flags: i64,
    trim_trailing_unmatched: bool,
) -> usize {
    if flags & PREG_UNMATCHED_AS_NULL != 0 || !trim_trailing_unmatched {
        return captures.len();
    }
    (0..captures.len())
        .rev()
        .find(|&index| captures.get(index).is_some())
        .map_or(0, |index| index + 1)
}

fn insert_named_capture(array: &mut PhpArray, name: &str, value: Value, unmatched: bool) {
    let key = ArrayKey::String(name.as_bytes().to_vec().into());
    if unmatched && array.get(&key).is_some() {
        return;
    }
    array.insert(key, value);
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
        PREG_BAD_UTF8_ERROR => "Malformed UTF-8 characters, possibly incorrectly encoded",
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
        if byte == b'\0' {
            quoted.extend_from_slice(br"\000");
            continue;
        }
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
    } else if message.contains("recursion") || message.contains("depth limit") {
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

    #[test]
    fn reports_php_malformed_pattern_messages() {
        let error = parse_delimited_pattern(b"").unwrap_err();
        assert_eq!(error.message(), "Empty regular expression");

        let error = parse_delimited_pattern(b"      ").unwrap_err();
        assert_eq!(error.message(), "Empty regular expression");

        let error = parse_delimited_pattern(b"abc").unwrap_err();
        assert_eq!(
            error.message(),
            "Delimiter must not be alphanumeric, backslash, or NUL byte"
        );

        let error = parse_delimited_pattern(b" a ").unwrap_err();
        assert_eq!(
            error.message(),
            "Delimiter must not be alphanumeric, backslash, or NUL byte"
        );

        let error = parse_delimited_pattern(b"/abc").unwrap_err();
        assert_eq!(error.message(), "No ending delimiter '/' found");

        let error = compile_regex("abc", "F", PcreMatchLimits::default()).unwrap_err();
        assert_eq!(error.message(), "Unknown modifier 'F'");

        let error = compile_regex("abc", "\0", PcreMatchLimits::default()).unwrap_err();
        assert_eq!(error.message(), "NUL byte is not a valid modifier");
    }

    #[test]
    fn formats_pcre2_compile_errors_like_php() {
        // The error offset comes straight from the linked PCRE2. The pinned
        // PHP 8.5.7 reference (and the pcre2-sys bundled 10.46) report offset
        // 0 for a leading quantifier; PCRE2 10.47 moved it to 1. The oracle
        // defines correctness here, so this expects the 10.46 behavior — if
        // the build ever links a system PCRE2 via pkg-config instead of the
        // bundled one, this test flags the divergence.
        let error = compile_regex("*", "", PcreMatchLimits::default()).unwrap_err();
        assert_eq!(
            error.message(),
            "Compilation failed: quantifier does not follow a repeatable item at offset 0"
        );
    }

    #[test]
    fn preg_quote_escapes_nul_as_php_octal_escape() {
        assert_eq!(preg_quote(b"a\0b", None), br"a\000b");
    }

    #[test]
    fn pcre_cache_reuses_valid_utf8_subject_validation_but_checks_offsets() {
        let mut cache = PcreCache::default();
        let pattern = cache.compile(&PhpString::from_test_str("/./u")).unwrap();
        let subject = PhpString::from_test_str("\u{00e9}x");

        cache
            .validate_utf8_subject_for_pattern_at_offset(&pattern, &subject, 0)
            .unwrap();
        let error = cache
            .validate_utf8_subject_for_pattern_at_offset(&pattern, &subject, 1)
            .unwrap_err();
        assert_eq!(error.code(), PREG_BAD_UTF8_OFFSET_ERROR);
        cache
            .validate_utf8_subject_for_pattern_at_offset(&pattern, &subject, 2)
            .unwrap();
    }

    #[test]
    fn classifies_start_offset_word_fast_path_for_ascii_subjects() {
        let mut cache = PcreCache::default();
        let pattern = cache
            .compile(&PhpString::from_test_str(r"/\G\w/u"))
            .unwrap();

        assert_eq!(pattern.fast_match_at(b"ab", 1), Some(Some((1, 2))));
        assert_eq!(pattern.fast_match_at(b"a-", 1), Some(None));
        assert_eq!(pattern.fast_match_at("\u{00e9}".as_bytes(), 0), None);
    }

    #[test]
    fn maps_php_delimiter_modifiers_to_pcre_options() {
        let ungreedy = compile_regex("<.*>", "U", PcreMatchLimits::default()).unwrap();
        let captures = ungreedy.captures(b"<aa> <bb> <cc>").unwrap().unwrap();
        assert_eq!(captures.get(0).unwrap().as_bytes(), b"<aa>");

        let anchored = compile_regex(r"\PN+", "A", PcreMatchLimits::default()).unwrap();
        assert!(anchored.captures(b"123abc").unwrap().is_none());

        let no_auto_capture = compile_regex(".(.).", "n", PcreMatchLimits::default()).unwrap();
        let captures = no_auto_capture.captures(b"abc").unwrap().unwrap();
        assert_eq!(captures.len(), 1);

        let named_capture =
            compile_regex(".(?P<test>.).", "n", PcreMatchLimits::default()).unwrap();
        let captures = named_capture.captures(b"abc").unwrap().unwrap();
        assert_eq!(captures.get(1).unwrap().as_bytes(), b"b");

        assert!(compile_regex(r"(?<g>foo)|(?<g>bar)", "J", PcreMatchLimits::default()).is_ok());
        assert!(compile_regex(r"(?<g>foo)|(?<g>bar)", "", PcreMatchLimits::default()).is_err());

        let restricted = compile_regex("k", "iur", PcreMatchLimits::default()).unwrap();
        assert!(restricted.captures("K".as_bytes()).unwrap().is_some());
        assert!(
            restricted
                .captures("\u{212A}".as_bytes())
                .unwrap()
                .is_none()
        );

        let dollar_endonly = compile_regex(r"^\S+.+$", "D", PcreMatchLimits::default()).unwrap();
        assert!(dollar_endonly.captures(b"aeiou\n").unwrap().is_none());
        let dollar_multiline = compile_regex(r"^\S+.+$", "Dm", PcreMatchLimits::default()).unwrap();
        assert!(dollar_multiline.captures(b"aeiou\n").unwrap().is_some());

        let spaced_study = compile_regex("a", "  S\r\n", PcreMatchLimits::default()).unwrap();
        assert!(spaced_study.captures(b"a").unwrap().is_some());

        let legacy_extra = compile_regex("a", "X", PcreMatchLimits::default()).unwrap();
        assert!(legacy_extra.captures(b"a").unwrap().is_some());
    }

    #[test]
    fn rewrites_dollar_endonly_anchors_without_touching_literals() {
        assert_eq!(php_rewrite_dollar_endonly(r"foo$"), r"foo\z");
        assert_eq!(php_rewrite_dollar_endonly(r"foo\$"), r"foo\$");
        assert_eq!(php_rewrite_dollar_endonly(r"[$]$"), r"[$]\z");
        assert_eq!(php_rewrite_dollar_endonly(r"\Q$\E$"), r"\Q$\E\z");
    }
}
