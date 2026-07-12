//! PHP-compatible diagnostic output rendering.

use crate::{OutputBuffer, RuntimeDiagnostic, RuntimeSeverity, RuntimeSourceSpan};

/// PHP `E_ERROR` bit used by `error_reporting`.
pub const PHP_E_ERROR: i64 = 1;
/// PHP `E_WARNING` bit used by `error_reporting`.
pub const PHP_E_WARNING: i64 = 2;
/// PHP `E_NOTICE` bit used by `error_reporting`.
pub const PHP_E_NOTICE: i64 = 8;
/// PHP `E_DEPRECATED` bit used by `error_reporting`.
pub const PHP_E_DEPRECATED: i64 = 8192;

/// User-triggered PHP error bits.
pub const PHP_E_USER_ERROR: i64 = 256;
/// User-triggered PHP warning bits.
pub const PHP_E_USER_WARNING: i64 = 512;
/// User-triggered PHP notice bits.
pub const PHP_E_USER_NOTICE: i64 = 1024;
/// User-triggered PHP deprecation bits.
pub const PHP_E_USER_DEPRECATED: i64 = 16384;

/// Request-local error display controls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhpDiagnosticDisplayOptions {
    /// Whether `display_errors` allows writing the diagnostic to stdout.
    pub display_errors: bool,
    /// Active `error_reporting` mask.
    pub error_reporting: i64,
    /// Whether to prefix the rendered diagnostic line with a newline.
    pub leading_newline: bool,
}

impl Default for PhpDiagnosticDisplayOptions {
    fn default() -> Self {
        Self {
            display_errors: true,
            error_reporting: -1,
            leading_newline: true,
        }
    }
}

/// PHP diagnostic channel name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhpDiagnosticChannel {
    /// `Notice`.
    Notice,
    /// `Warning`.
    Warning,
    /// `Deprecated`.
    Deprecated,
    /// `Fatal error`.
    FatalError,
}

impl PhpDiagnosticChannel {
    /// Display spelling used by PHP CLI.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Notice => "Notice",
            Self::Warning => "Warning",
            Self::Deprecated => "Deprecated",
            Self::FatalError => "Fatal error",
        }
    }

    /// Maps runtime severity to the closest PHP output channel.
    #[must_use]
    pub const fn from_runtime_severity(severity: RuntimeSeverity) -> Self {
        match severity {
            RuntimeSeverity::Notice => Self::Notice,
            RuntimeSeverity::Deprecation => Self::Deprecated,
            RuntimeSeverity::FatalError
            | RuntimeSeverity::RecoverableError
            | RuntimeSeverity::UnsupportedFeature => Self::FatalError,
            RuntimeSeverity::Warning => Self::Warning,
        }
    }
}

/// Source location shown in PHP diagnostic output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhpDiagnosticLocation {
    /// Display file path.
    pub file: String,
    /// One-based display line when known; `0` is the deterministic fallback.
    pub line: u32,
}

impl PhpDiagnosticLocation {
    /// Creates a display location.
    #[must_use]
    pub fn new(file: impl Into<String>, line: u32) -> Self {
        Self {
            file: file.into(),
            line,
        }
    }

    /// Creates a deterministic fallback location from a runtime source span.
    #[must_use]
    pub fn from_span(span: &RuntimeSourceSpan) -> Self {
        let file = span.file.clone().unwrap_or_else(|| "<unknown>".to_owned());
        let line = span
            .file
            .as_deref()
            .and_then(|file| line_number_for_offset(file, span.start))
            .unwrap_or(span.start);
        Self { file, line }
    }
}

fn line_number_for_offset(file: &str, offset: u32) -> Option<u32> {
    let bytes = std::fs::read(file).ok()?;
    let offset = usize::try_from(offset).ok()?.min(bytes.len());
    Some(
        1 + bytes[..offset]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count() as u32,
    )
}

/// Returns whether the current error-reporting mask allows the level.
#[must_use]
pub const fn error_reporting_allows_level(error_reporting: i64, level: i64) -> bool {
    error_reporting == -1 || (error_reporting & level) != 0
}

/// Formats one PHP CLI diagnostic line.
#[must_use]
pub fn format_php_diagnostic_line(
    channel: PhpDiagnosticChannel,
    message: &str,
    location: &PhpDiagnosticLocation,
) -> String {
    format_php_diagnostic_line_with_prefix(channel, message, location, true)
}

/// Formats one PHP CLI diagnostic line with explicit leading newline control.
#[must_use]
pub fn format_php_diagnostic_line_with_prefix(
    channel: PhpDiagnosticChannel,
    message: &str,
    location: &PhpDiagnosticLocation,
    leading_newline: bool,
) -> String {
    let prefix = if leading_newline { "\n" } else { "" };
    format!(
        "{}{}: {} in {} on line {}\n",
        prefix,
        channel.display_name(),
        message,
        location.file,
        location.line
    )
}

/// Writes a PHP-style diagnostic when display/reporting options allow it.
pub fn emit_php_diagnostic(
    output: &mut OutputBuffer,
    diagnostic: &RuntimeDiagnostic,
    channel: PhpDiagnosticChannel,
    level: i64,
    options: PhpDiagnosticDisplayOptions,
) -> bool {
    if !options.display_errors || !error_reporting_allows_level(options.error_reporting, level) {
        return false;
    }
    let location = PhpDiagnosticLocation::from_span(diagnostic.source_span());
    output.write_bytes(
        format_php_diagnostic_line_with_prefix(
            channel,
            diagnostic.message(),
            &location,
            options.leading_newline,
        )
        .as_bytes(),
    );
    true
}

#[cfg(test)]
mod tests {
    use super::{
        PHP_E_WARNING, PhpDiagnosticChannel, PhpDiagnosticDisplayOptions, PhpDiagnosticLocation,
        emit_php_diagnostic, error_reporting_allows_level, format_php_diagnostic_line,
    };
    use crate::api::{
        OutputBuffer, RuntimeDiagnostic, RuntimeSeverity, RuntimeSourceSpan, RuntimeStackFrame,
    };

    #[test]
    fn formats_php_warning_with_file_and_line() {
        let line = format_php_diagnostic_line(
            PhpDiagnosticChannel::Warning,
            "Undefined variable $x",
            &PhpDiagnosticLocation::new("fixture.php", 7),
        );

        assert_eq!(
            line,
            "\nWarning: Undefined variable $x in fixture.php on line 7\n"
        );
    }

    #[test]
    fn display_location_resolves_byte_offset_to_line_when_file_exists() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "phrust-runtime-source-span-{}.php",
            std::process::id()
        ));
        std::fs::write(&path, "<?php\n// comment\necho $missing;\n").expect("write fixture");
        let start = "<?php\n// comment\necho ".len() as u32;
        let location = PhpDiagnosticLocation::from_span(&RuntimeSourceSpan {
            file: Some(path.display().to_string()),
            start,
            end: start + 8,
        });
        let _ = std::fs::remove_file(&path);

        assert_eq!(location.line, 3);
    }

    #[test]
    fn display_and_reporting_gate_emission() {
        let diagnostic = RuntimeDiagnostic::new(
            "E_TEST",
            RuntimeSeverity::Warning,
            "Undefined variable $x",
            RuntimeSourceSpan {
                file: Some("fixture.php".to_owned()),
                start: 7,
                end: 7,
            },
            vec![RuntimeStackFrame::new("main")],
            None,
        );
        let mut output = OutputBuffer::new();

        assert!(!emit_php_diagnostic(
            &mut output,
            &diagnostic,
            PhpDiagnosticChannel::Warning,
            PHP_E_WARNING,
            PhpDiagnosticDisplayOptions {
                display_errors: false,
                error_reporting: -1,
                ..PhpDiagnosticDisplayOptions::default()
            },
        ));
        assert!(output.is_empty());

        assert!(!error_reporting_allows_level(0, PHP_E_WARNING));
        assert!(emit_php_diagnostic(
            &mut output,
            &diagnostic,
            PhpDiagnosticChannel::Warning,
            PHP_E_WARNING,
            PhpDiagnosticDisplayOptions::default(),
        ));
        assert_eq!(
            output.to_string_lossy(),
            "\nWarning: Undefined variable $x in fixture.php on line 7\n"
        );
    }
}
