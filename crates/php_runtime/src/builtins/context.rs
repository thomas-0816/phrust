//! Runtime services passed to internal builtins.

use crate::{
    FilesystemCapabilities, IniRegistry, OutputBuffer, PHP_E_DEPRECATED, PHP_E_WARNING, PcreCache,
    PhpDiagnosticChannel, PhpDiagnosticDisplayOptions, ReferenceCell, ResourceTable,
    RuntimeDiagnostic, RuntimeHttpResponseState, RuntimeSeverity, SessionState, Value, datetime,
    emit_php_diagnostic, pcre,
};
use std::path::{Path, PathBuf};

pub(in crate::builtins) const JSON_ERROR_NONE: i64 = 0;
pub(in crate::builtins) const JSON_ERROR_DEPTH: i64 = 1;
pub(in crate::builtins) const JSON_ERROR_SYNTAX: i64 = 4;
pub(in crate::builtins) const JSON_ERROR_UTF8: i64 = 5;
pub(in crate::builtins) const JSON_OBJECT_AS_ARRAY: i64 = 1;
pub(in crate::builtins) const JSON_PRETTY_PRINT: i64 = 128;
pub(in crate::builtins) const JSON_UNESCAPED_SLASHES: i64 = 64;
pub(in crate::builtins) const JSON_UNESCAPED_UNICODE: i64 = 256;
pub(in crate::builtins) const JSON_PRESERVE_ZERO_FRACTION: i64 = 1024;
pub(in crate::builtins) const JSON_THROW_ON_ERROR: i64 = 4_194_304;

/// Request-local state for `strtok`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StrtokState {
    input: Vec<u8>,
    offset: usize,
    mode: StrtokMode,
    emitted_token: bool,
}

impl StrtokState {
    /// Starts tokenization over a new input string.
    pub fn reset(&mut self, input: Vec<u8>) {
        self.input = input;
        self.offset = 0;
        self.mode = StrtokMode::Active;
        self.emitted_token = false;
    }

    /// Whether one-argument `strtok()` needs a new input string first.
    #[must_use]
    pub const fn requires_input(&self) -> bool {
        matches!(self.mode, StrtokMode::NeedsInput)
    }

    /// Returns the next token separated by any byte in `delimiters`.
    pub fn next_token(&mut self, delimiters: &[u8]) -> Option<Vec<u8>> {
        if delimiters.is_empty() {
            return if self.offset == 0 {
                let token = self.input.clone();
                self.offset = self.input.len();
                Some(token)
            } else {
                None
            };
        }
        let skipped_start = self.offset;
        while self.offset < self.input.len() && delimiters.contains(&self.input[self.offset]) {
            self.offset += 1;
        }
        if self.offset >= self.input.len() {
            // We reached the end while skipping leading delimiters. Because a
            // token's terminating delimiter is now consumed eagerly (see below),
            // reaching the end without skipping any further delimiter is a clean
            // exhaustion; skipping one or more extra trailing delimiters leaves a
            // grace state where the next bare strtok() warns, matching PHP.
            self.mode = if self.input.is_empty()
                || (self.emitted_token && self.offset.saturating_sub(skipped_start) == 0)
            {
                StrtokMode::Exhausted
            } else {
                StrtokMode::NeedsInput
            };
            return None;
        }
        let start = self.offset;
        while self.offset < self.input.len() && !delimiters.contains(&self.input[self.offset]) {
            self.offset += 1;
        }
        let token = self.input[start..self.offset].to_vec();
        // Consume the delimiter that terminated this token so the next call (which
        // may use a different delimiter set) does not re-read it, matching PHP's
        // php_strtok_r, which advances the saved pointer past the delimiter.
        if self.offset < self.input.len() {
            self.offset += 1;
        }
        self.mode = StrtokMode::Active;
        self.emitted_token = true;
        Some(token)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum StrtokMode {
    #[default]
    Exhausted,
    Active,
    NeedsInput,
}

/// Source location passed to internal builtins.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimeSourceSpan {
    /// Optional source file path.
    pub file: Option<String>,
    /// Start byte offset.
    pub start: u32,
    /// End byte offset.
    pub end: u32,
}

/// Mutable runtime services available to internal builtins.
pub struct BuiltinContext<'a> {
    output: &'a mut OutputBuffer,
    cwd: PathBuf,
    include_path: Vec<PathBuf>,
    ini: IniRegistry,
    default_timezone: String,
    filesystem: FilesystemCapabilities,
    resources: Option<&'a mut ResourceTable>,
    http_response: RuntimeHttpResponseState,
    http_response_state: Option<&'a mut RuntimeHttpResponseState>,
    pcre_cache: PcreCache,
    preg_last_error: pcre::PcreLastErrorState,
    preg_last_error_state: Option<&'a mut pcre::PcreLastErrorState>,
    json_last_error: i64,
    json_last_error_msg: String,
    strtok_state: Option<&'a mut StrtokState>,
    mb_internal_encoding: String,
    session_state: Option<&'a mut SessionState>,
    session_global: Option<ReferenceCell>,
    diagnostic_display: PhpDiagnosticDisplayOptions,
    diagnostics: Vec<RuntimeDiagnostic>,
}

impl<'a> BuiltinContext<'a> {
    /// Creates a runtime context backed by the VM output buffer.
    #[must_use]
    pub fn new(output: &'a mut OutputBuffer) -> Self {
        Self {
            output,
            cwd: PathBuf::from("."),
            include_path: vec![PathBuf::from(".")],
            ini: IniRegistry::default(),
            default_timezone: datetime::DEFAULT_TIMEZONE.to_string(),
            filesystem: FilesystemCapabilities::none(),
            resources: None,
            http_response: RuntimeHttpResponseState::default(),
            http_response_state: None,
            pcre_cache: PcreCache::default(),
            preg_last_error: pcre::PcreLastErrorState::default(),
            preg_last_error_state: None,
            json_last_error: JSON_ERROR_NONE,
            json_last_error_msg: json_error_message(JSON_ERROR_NONE).to_string(),
            strtok_state: None,
            mb_internal_encoding: "UTF-8".to_owned(),
            session_state: None,
            session_global: None,
            diagnostic_display: PhpDiagnosticDisplayOptions::default(),
            diagnostics: Vec::new(),
        }
    }

    /// Creates a runtime context with deterministic host capability policy.
    #[must_use]
    pub fn with_runtime(
        output: &'a mut OutputBuffer,
        cwd: impl Into<PathBuf>,
        filesystem: FilesystemCapabilities,
        resources: Option<&'a mut ResourceTable>,
    ) -> Self {
        Self {
            output,
            cwd: cwd.into(),
            include_path: vec![PathBuf::from(".")],
            ini: IniRegistry::default(),
            default_timezone: datetime::DEFAULT_TIMEZONE.to_string(),
            filesystem,
            resources,
            http_response: RuntimeHttpResponseState::default(),
            http_response_state: None,
            pcre_cache: PcreCache::default(),
            preg_last_error: pcre::PcreLastErrorState::default(),
            preg_last_error_state: None,
            json_last_error: JSON_ERROR_NONE,
            json_last_error_msg: json_error_message(JSON_ERROR_NONE).to_string(),
            strtok_state: None,
            mb_internal_encoding: "UTF-8".to_owned(),
            session_state: None,
            session_global: None,
            diagnostic_display: PhpDiagnosticDisplayOptions::default(),
            diagnostics: Vec::new(),
        }
    }

    /// Returns the output buffer.
    pub fn output(&mut self) -> &mut OutputBuffer {
        self.output
    }

    /// Sets request-local warning/error output controls.
    pub fn set_diagnostic_display(&mut self, options: PhpDiagnosticDisplayOptions) {
        self.diagnostic_display = options;
    }

    /// Emits a PHP display_errors-style warning into stdout and records a
    /// structured diagnostic for VM/report consumers.
    pub fn php_warning(
        &mut self,
        id: impl Into<String>,
        message: impl Into<String>,
        source_span: RuntimeSourceSpan,
    ) {
        let message = message.into();
        let diagnostic = RuntimeDiagnostic::new(
            id,
            RuntimeSeverity::Warning,
            message,
            source_span,
            Vec::new(),
            None,
        );
        emit_php_diagnostic(
            self.output,
            &diagnostic,
            PhpDiagnosticChannel::Warning,
            PHP_E_WARNING,
            self.diagnostic_display,
        );
        self.diagnostics.push(diagnostic);
    }

    /// Emits a PHP display_errors-style deprecation into stdout and records a
    /// structured diagnostic for VM/report consumers.
    pub fn php_deprecation(
        &mut self,
        id: impl Into<String>,
        message: impl Into<String>,
        source_span: RuntimeSourceSpan,
    ) {
        let message = message.into();
        let diagnostic = RuntimeDiagnostic::new(
            id,
            RuntimeSeverity::Deprecation,
            message,
            source_span,
            Vec::new(),
            None,
        );
        emit_php_diagnostic(
            self.output,
            &diagnostic,
            PhpDiagnosticChannel::Deprecated,
            PHP_E_DEPRECATED,
            self.diagnostic_display,
        );
        self.diagnostics.push(diagnostic);
    }

    /// Drains structured diagnostics emitted by builtins.
    pub fn take_diagnostics(&mut self) -> Vec<RuntimeDiagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    /// Current working directory for path and filesystem builtins.
    #[must_use]
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Updates the request-local current working directory for filesystem builtins.
    pub fn set_cwd(&mut self, cwd: impl Into<PathBuf>) {
        self.cwd = cwd.into();
    }

    /// Include path entries used by stream include-path resolution.
    #[must_use]
    pub fn include_path(&self) -> &[PathBuf] {
        &self.include_path
    }

    /// Sets request-local include path entries.
    pub fn set_include_path(&mut self, include_path: Vec<PathBuf>) {
        self.include_path = include_path;
    }

    /// Reads a request-local INI option visible to standard-library builtins.
    #[must_use]
    pub fn ini_get(&self, name: &str) -> Option<&str> {
        self.ini.get(name)
    }

    /// Sets request-local INI options visible to standard-library builtins.
    pub fn set_ini_registry(&mut self, ini: IniRegistry) {
        self.ini = ini;
    }

    /// Current request-local default timezone.
    #[must_use]
    pub fn default_timezone(&self) -> &str {
        &self.default_timezone
    }

    /// Updates the request-local default timezone.
    pub fn set_default_timezone(&mut self, identifier: impl Into<String>) {
        self.default_timezone = identifier.into();
    }

    /// Filesystem capabilities for path and filesystem builtins.
    #[must_use]
    pub const fn filesystem_capabilities(&self) -> &FilesystemCapabilities {
        &self.filesystem
    }

    /// Request-local resource table for stream builtins.
    pub fn resources(&mut self) -> Option<&mut ResourceTable> {
        self.resources.as_deref_mut()
    }

    /// Sets request-local HTTP response state.
    pub fn set_http_response_state(&mut self, state: &'a mut RuntimeHttpResponseState) {
        self.http_response_state = Some(state);
    }

    /// Current request-local HTTP response state.
    #[must_use]
    pub fn http_response(&self) -> &RuntimeHttpResponseState {
        self.http_response_state
            .as_deref()
            .unwrap_or(&self.http_response)
    }

    /// Mutable request-local HTTP response state.
    pub fn http_response_mut(&mut self) -> &mut RuntimeHttpResponseState {
        match self.http_response_state.as_deref_mut() {
            Some(state) => state,
            None => &mut self.http_response,
        }
    }

    /// Sets request-local `strtok` state.
    pub fn set_strtok_state(&mut self, state: &'a mut StrtokState) {
        self.strtok_state = Some(state);
    }

    /// Returns request-local `strtok` state.
    pub fn strtok_state(&mut self) -> Option<&mut StrtokState> {
        self.strtok_state.as_deref_mut()
    }

    /// Current request-local mbstring internal encoding.
    #[must_use]
    pub fn mb_internal_encoding(&self) -> &str {
        &self.mb_internal_encoding
    }

    /// Updates the request-local mbstring internal encoding.
    pub fn set_mb_internal_encoding(&mut self, encoding: impl Into<String>) {
        self.mb_internal_encoding = encoding.into();
    }

    /// Sets request-local session state and the live `$_SESSION` global slot.
    pub fn set_session_state(
        &mut self,
        state: &'a mut SessionState,
        session_global: ReferenceCell,
    ) {
        self.session_state = Some(state);
        self.session_global = Some(session_global);
    }

    /// Request-local session state.
    pub fn session_state(&mut self) -> Option<&mut SessionState> {
        self.session_state.as_deref_mut()
    }

    /// Writes the current session data into the live `$_SESSION` global.
    pub fn sync_session_global_from_state(&mut self) {
        let Some(data) = self.session_state.as_ref().map(|state| state.data_value()) else {
            return;
        };
        if let Some(global) = &self.session_global {
            global.set(data);
        }
    }

    /// Captures the live `$_SESSION` global back into request-local session state.
    pub fn sync_session_state_from_global(&mut self) {
        let Some(global) = &self.session_global else {
            return;
        };
        let Value::Array(array) = global.get() else {
            return;
        };
        if let Some(state) = self.session_state.as_deref_mut() {
            state.set_data(array);
        }
    }

    /// Request-local PCRE pattern cache.
    pub fn pcre_cache(&mut self) -> &mut PcreCache {
        &mut self.pcre_cache
    }

    /// Sets request-local `preg_last_error` state storage.
    pub fn set_preg_last_error_state(&mut self, state: &'a mut pcre::PcreLastErrorState) {
        self.preg_last_error_state = Some(state);
    }

    /// Updates request-local PCRE last-error state.
    pub fn set_preg_last_error(&mut self, code: i64, message: impl Into<String>) {
        match self.preg_last_error_state.as_deref_mut() {
            Some(state) => state.set(code, message),
            None => self.preg_last_error.set(code, message),
        }
    }

    /// Clears request-local PCRE last-error state.
    pub fn clear_preg_last_error(&mut self) {
        match self.preg_last_error_state.as_deref_mut() {
            Some(state) => state.clear(),
            None => self.preg_last_error.clear(),
        }
    }

    /// Returns request-local PCRE last-error code and message.
    #[must_use]
    pub fn preg_last_error(&self) -> (i64, &str) {
        let state = self
            .preg_last_error_state
            .as_deref()
            .unwrap_or(&self.preg_last_error);
        (state.code(), state.message())
    }

    /// Updates request-local JSON last-error state.
    pub fn set_json_last_error(&mut self, code: i64) {
        self.json_last_error = code;
        self.json_last_error_msg = json_error_message(code).to_string();
    }

    /// Returns request-local JSON last-error code and message.
    #[must_use]
    pub fn json_last_error(&self) -> (i64, &str) {
        (self.json_last_error, &self.json_last_error_msg)
    }
}

pub(in crate::builtins) const fn json_error_message(code: i64) -> &'static str {
    match code {
        JSON_ERROR_NONE => "No error",
        JSON_ERROR_DEPTH => "Maximum stack depth exceeded",
        JSON_ERROR_SYNTAX => "Syntax error",
        JSON_ERROR_UTF8 => "Malformed UTF-8 characters, possibly incorrectly encoded",
        _ => "Unknown error",
    }
}

#[cfg(test)]
mod tests {
    use super::StrtokState;

    #[test]
    fn strtok_consumes_terminating_delimiter_across_delimiter_sets() {
        // Regression: each strtok() call must advance past the delimiter that
        // terminated the previous token, so a later call with a different
        // delimiter set does not re-read it. Mirrors tests/strings/001.phpt.
        let mut state = StrtokState::default();
        state.reset(b"testing 1/2\\3".to_vec());
        assert_eq!(state.next_token(b" "), Some(b"testing".to_vec()));
        assert_eq!(state.next_token(b"/"), Some(b"1".to_vec()));
        assert_eq!(state.next_token(b"\\"), Some(b"2".to_vec()));
        assert_eq!(state.next_token(b"."), Some(b"3".to_vec()));
        assert_eq!(state.next_token(b" "), None);
    }

    #[test]
    fn strtok_skips_leading_and_repeated_delimiters() {
        let mut state = StrtokState::default();
        state.reset(b"a,,b".to_vec());
        assert_eq!(state.next_token(b","), Some(b"a".to_vec()));
        assert_eq!(state.next_token(b","), Some(b"b".to_vec()));
        assert_eq!(state.next_token(b","), None);
    }
}
