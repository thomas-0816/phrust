//! Builtin error reporting.

use super::RuntimeSourceSpan;
use php_diagnostics::{
    DiagnosticEnvelope, DiagnosticLayer, DiagnosticLocation, DiagnosticPhase, DiagnosticSeverity,
    DiagnosticSpan, DiagnosticSuggestion,
};
use std::collections::BTreeMap;

/// Runtime error reported by a builtin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuiltinError {
    diagnostic_id: &'static str,
    message: String,
    context: Option<Box<BuiltinErrorContext>>,
}

/// Optional builtin-call diagnostic context.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BuiltinErrorContext {
    /// PHP function that reported the error.
    pub function_name: Option<String>,
    /// Zero-based argument index.
    pub argument_index: Option<usize>,
    /// PHP argument name.
    pub argument_name: Option<String>,
    /// Expected PHP type or shape.
    pub expected_type: Option<String>,
    /// Actual PHP type or shape.
    pub actual_type: Option<String>,
    /// Human-facing remediation.
    pub suggestion: Option<String>,
    /// JSON error code used when a JSON builtin throws JsonException.
    pub json_error_code: Option<i64>,
    /// PHP source line inside a tokenizer input string for ParseError objects.
    pub tokenizer_parse_line: Option<i64>,
}

impl BuiltinError {
    /// Creates a builtin error with a stable diagnostic ID.
    #[must_use]
    pub fn new(diagnostic_id: &'static str, message: impl Into<String>) -> Self {
        Self {
            diagnostic_id,
            message: message.into(),
            context: None,
        }
    }

    /// Adds the reporting PHP function.
    #[must_use]
    pub fn with_function_name(mut self, function_name: impl Into<String>) -> Self {
        self.context_mut().function_name = Some(function_name.into());
        self
    }

    /// Adds argument context.
    #[must_use]
    pub fn with_argument(mut self, index: usize, name: impl Into<String>) -> Self {
        let context = self.context_mut();
        context.argument_index = Some(index);
        context.argument_name = Some(name.into());
        self
    }

    /// Adds expected and actual PHP type context.
    #[must_use]
    pub fn with_expected_actual(
        mut self,
        expected_type: impl Into<String>,
        actual_type: impl Into<String>,
    ) -> Self {
        let context = self.context_mut();
        context.expected_type = Some(expected_type.into());
        context.actual_type = Some(actual_type.into());
        self
    }

    /// Adds a human-facing suggestion.
    #[must_use]
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.context_mut().suggestion = Some(suggestion.into());
        self
    }

    /// Adds the JSON error code for JsonException construction.
    #[must_use]
    pub fn with_json_error_code(mut self, code: i64) -> Self {
        self.context_mut().json_error_code = Some(code);
        self
    }

    /// Adds the tokenizer input source line used for ParseError construction.
    #[must_use]
    pub fn with_tokenizer_parse_line(mut self, line: i64) -> Self {
        self.context_mut().tokenizer_parse_line = Some(line);
        self
    }

    /// Stable diagnostic ID.
    #[must_use]
    pub const fn diagnostic_id(&self) -> &'static str {
        self.diagnostic_id
    }

    /// Human-readable message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Optional structured context.
    #[must_use]
    pub fn context(&self) -> Option<&BuiltinErrorContext> {
        self.context.as_deref()
    }

    /// Combines ID and message for VM runtime errors.
    #[must_use]
    pub fn display_message(&self) -> String {
        format!("{}: {}", self.diagnostic_id, self.message)
    }

    /// Converts this builtin error to the shared diagnostic envelope.
    #[must_use]
    pub fn to_diagnostic_envelope(&self, span: Option<&RuntimeSourceSpan>) -> DiagnosticEnvelope {
        let mut context = BTreeMap::new();
        if let Some(error_context) = self.context() {
            if let Some(function_name) = &error_context.function_name {
                context.insert("function".to_string(), function_name.clone());
            }
            if let Some(argument_index) = error_context.argument_index {
                context.insert("argument_index".to_string(), argument_index.to_string());
            }
            if let Some(argument_name) = &error_context.argument_name {
                context.insert("argument".to_string(), argument_name.clone());
            }
            if let Some(expected_type) = &error_context.expected_type {
                context.insert("expected_type".to_string(), expected_type.clone());
            }
            if let Some(actual_type) = &error_context.actual_type {
                context.insert("actual_type".to_string(), actual_type.clone());
            }
            if let Some(json_error_code) = error_context.json_error_code {
                context.insert("json_error_code".to_string(), json_error_code.to_string());
            }
        }

        let mut envelope = DiagnosticEnvelope::new(
            self.diagnostic_id,
            DiagnosticLayer::builtin(),
            DiagnosticPhase::new("call"),
            DiagnosticSeverity::FatalError,
            self.message.clone(),
        )
        .with_context(context);
        if let Some(span) = span {
            envelope.location = Some(DiagnosticLocation::new(
                span.file.as_deref(),
                None::<&str>,
                Some(DiagnosticSpan::new(span.start as usize, span.end as usize)),
            ));
        }
        if let Some(suggestion) = self
            .context()
            .and_then(|context| context.suggestion.as_ref())
        {
            envelope.suggestion = Some(DiagnosticSuggestion::new(suggestion.clone()));
        }
        envelope.php_visible = true;
        envelope
    }

    fn context_mut(&mut self) -> &mut BuiltinErrorContext {
        self.context
            .get_or_insert_with(|| Box::new(BuiltinErrorContext::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_argument_error_has_shared_envelope_context() {
        let error = BuiltinError::new(
            "E_PHP_BUILTIN_CORE_ARG_TYPE",
            "strlen(): Argument #1 ($string) must be of type string, array given",
        )
        .with_function_name("strlen")
        .with_argument(0, "string")
        .with_expected_actual("string", "array")
        .with_suggestion("pass a string value");
        let span = RuntimeSourceSpan {
            file: Some("builtin.php".to_string()),
            start: 4,
            end: 10,
        };

        let envelope = error.to_diagnostic_envelope(Some(&span));
        let json: serde_json::Value =
            serde_json::from_str(&envelope.compact_json().expect("json")).expect("parse json");

        assert_eq!(json["code"], "E_PHP_BUILTIN_CORE_ARG_TYPE");
        assert_eq!(json["layer"], "builtin");
        assert_eq!(json["phase"], "call");
        assert_eq!(json["context"]["function"], "strlen");
        assert_eq!(json["context"]["argument_index"], "0");
        assert_eq!(json["context"]["argument"], "string");
        assert_eq!(json["context"]["expected_type"], "string");
        assert_eq!(json["context"]["actual_type"], "array");
        assert_eq!(json["location"]["path"], "builtin.php");
    }
}
