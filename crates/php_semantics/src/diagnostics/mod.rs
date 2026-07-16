//! Stable semantic diagnostic model.

pub mod ids;

use php_diagnostics::{
    DiagnosticEnvelope, DiagnosticLabel as EnvelopeLabel, DiagnosticLayer,
    DiagnosticLocation as EnvelopeLocation, DiagnosticPhase as EnvelopePhase,
    DiagnosticSeverity as EnvelopeSeverity, DiagnosticSpan,
};
use php_source::TextRange;

use crate::symbols::declarations::DeclarationKind;

pub use ids::DiagnosticId;

/// Diagnostic severity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSeverity {
    /// Compile-time error.
    Error,
    /// Warning.
    Warning,
    /// Deprecation.
    Deprecation,
    /// Informational note.
    Note,
}

impl DiagnosticSeverity {
    /// Returns the stable lowercase name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Deprecation => "deprecation",
            Self::Note => "note",
        }
    }

    /// Returns the shared diagnostic envelope severity.
    #[must_use]
    pub const fn envelope_severity(self) -> EnvelopeSeverity {
        match self {
            Self::Error => EnvelopeSeverity::Error,
            Self::Warning => EnvelopeSeverity::Warning,
            Self::Deprecation => EnvelopeSeverity::Deprecation,
            Self::Note => EnvelopeSeverity::Note,
        }
    }
}

/// Backwards-compatible severity name.
pub type Severity = DiagnosticSeverity;

/// Semantic analysis phase that emitted a diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticPhase {
    /// AST view construction.
    AstView,
    /// Declaration collection.
    DeclarationCollection,
    /// Name resolution.
    NameResolution,
    /// HIR lowering.
    HirLowering,
    /// Type lowering.
    TypeLowering,
    /// Constant-expression validation.
    ConstExpression,
    /// Attribute lowering.
    AttributeLowering,
    /// Modifier validation.
    ModifierValidation,
    /// Control-flow validation.
    ControlFlowValidation,
    /// Class-like validation.
    ClassLikeValidation,
}

impl DiagnosticPhase {
    /// Returns the stable phase name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AstView => "ast_view",
            Self::DeclarationCollection => "declaration_collection",
            Self::NameResolution => "name_resolution",
            Self::HirLowering => "hir_lowering",
            Self::TypeLowering => "type_lowering",
            Self::ConstExpression => "const_expression",
            Self::AttributeLowering => "attribute_lowering",
            Self::ModifierValidation => "modifier_validation",
            Self::ControlFlowValidation => "control_flow_validation",
            Self::ClassLikeValidation => "class_like_validation",
        }
    }
}

/// Secondary diagnostic label over a source span.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticLabel {
    range: TextRange,
    message: String,
}

impl DiagnosticLabel {
    /// Creates a source label.
    #[must_use]
    pub fn new(range: TextRange, message: impl Into<String>) -> Self {
        Self {
            range,
            message: message.into(),
        }
    }

    /// Returns label range.
    #[must_use]
    pub const fn range(&self) -> TextRange {
        self.range
    }

    /// Returns label message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Semantic diagnostic emitted by Semantic frontend checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticDiagnostic {
    id: DiagnosticId,
    severity: DiagnosticSeverity,
    phase: DiagnosticPhase,
    message: String,
    span: Option<TextRange>,
    labels: Vec<DiagnosticLabel>,
    notes: Vec<String>,
    duplicate_declaration: Option<(String, DeclarationKind)>,
}

impl SemanticDiagnostic {
    /// Creates a semantic diagnostic without a primary span.
    #[must_use]
    pub fn new(
        id: DiagnosticId,
        severity: DiagnosticSeverity,
        phase: DiagnosticPhase,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id,
            severity,
            phase,
            message: message.into(),
            span: None,
            labels: Vec::new(),
            notes: Vec::new(),
            duplicate_declaration: None,
        }
    }

    /// Creates a semantic diagnostic with a primary span.
    #[must_use]
    pub fn with_span(
        id: DiagnosticId,
        severity: DiagnosticSeverity,
        phase: DiagnosticPhase,
        message: impl Into<String>,
        span: TextRange,
    ) -> Self {
        Self::new(id, severity, phase, message).with_primary_span(span)
    }

    /// Adds or replaces the primary span.
    #[must_use]
    pub const fn with_primary_span(mut self, span: TextRange) -> Self {
        self.span = Some(span);
        self
    }

    /// Adds a secondary label.
    #[must_use]
    pub fn with_label(mut self, label: DiagnosticLabel) -> Self {
        self.labels.push(label);
        self
    }

    /// Adds a diagnostic note.
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Attaches typed duplicate-declaration details for downstream renderers.
    #[must_use]
    pub fn with_duplicate_declaration(
        mut self,
        name: impl Into<String>,
        kind: DeclarationKind,
    ) -> Self {
        self.duplicate_declaration = Some((name.into(), kind));
        self
    }

    /// Returns the stable diagnostic ID.
    #[must_use]
    pub const fn id(&self) -> DiagnosticId {
        self.id
    }

    /// Returns severity.
    #[must_use]
    pub const fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }

    /// Returns diagnostic phase.
    #[must_use]
    pub const fn phase(&self) -> DiagnosticPhase {
        self.phase
    }

    /// Returns the diagnostic message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the primary span, when one is known.
    #[must_use]
    pub const fn span(&self) -> Option<TextRange> {
        self.span
    }

    /// Returns labels.
    #[must_use]
    pub fn labels(&self) -> &[DiagnosticLabel] {
        &self.labels
    }

    /// Returns notes.
    #[must_use]
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// Returns typed duplicate-declaration details, when applicable.
    #[must_use]
    pub fn duplicate_declaration(&self) -> Option<(&str, DeclarationKind)> {
        self.duplicate_declaration
            .as_ref()
            .map(|(name, kind)| (name.as_str(), *kind))
    }

    /// Renders this diagnostic as a stable JSON object.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\"id\":\"");
        out.push_str(self.id.as_str());
        out.push_str("\",\"severity\":\"");
        out.push_str(self.severity.as_str());
        out.push_str("\",\"phase\":\"");
        out.push_str(self.phase.as_str());
        out.push_str("\",\"message\":\"");
        out.push_str(&escape_json(&self.message));
        out.push_str("\",\"span\":");
        push_span_json(&mut out, self.span);
        out.push_str(",\"labels\":[");
        for (index, label) in self.labels.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"message\":\"");
            out.push_str(&escape_json(label.message()));
            out.push_str("\",\"span\":");
            push_span_json(&mut out, Some(label.range()));
            out.push('}');
        }
        out.push_str("],\"notes\":[");
        for (index, note) in self.notes.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push('"');
            out.push_str(&escape_json(note));
            out.push('"');
        }
        out.push_str("]}");
        out
    }

    /// Returns a structured diagnostic envelope for this semantic diagnostic.
    #[must_use]
    pub fn to_diagnostic_envelope(
        &self,
        source: Option<&php_source::SourceText>,
        source_id: Option<&str>,
        path: Option<&str>,
    ) -> DiagnosticEnvelope {
        let mut envelope = DiagnosticEnvelope::new(
            self.id.as_str(),
            DiagnosticLayer::semantic(),
            EnvelopePhase::new(self.phase.as_str()),
            self.severity.envelope_severity(),
            self.message.clone(),
        );
        envelope.legacy_id = Some(self.id.as_str().to_owned());
        envelope.php_visible = true;
        envelope.location = self.span.map(|span| match source {
            Some(source) => EnvelopeLocation::from_source_range(
                path.map(str::to_owned),
                source_id.map(str::to_owned),
                source,
                span,
            ),
            None => EnvelopeLocation::new(
                path.map(str::to_owned),
                source_id.map(str::to_owned),
                Some(DiagnosticSpan::from_range(span)),
            ),
        });
        envelope.labels = self
            .labels
            .iter()
            .map(|label| {
                EnvelopeLabel::new(
                    label.message().to_owned(),
                    DiagnosticSpan::from_range(label.range()),
                )
            })
            .collect();
        envelope.notes = self.notes.clone();
        envelope
    }
}

/// Collector for semantic diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticReporter {
    diagnostics: Vec<SemanticDiagnostic>,
    max_diagnostics: usize,
    capped: bool,
}

impl DiagnosticReporter {
    /// Creates an empty reporter.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            max_diagnostics: 128,
            capped: false,
        }
    }

    /// Creates a reporter with a caller-provided diagnostic cap.
    #[must_use]
    pub const fn with_max_diagnostics(max_diagnostics: usize) -> Self {
        Self {
            diagnostics: Vec::new(),
            max_diagnostics,
            capped: false,
        }
    }

    /// Adds a diagnostic.
    pub fn report(&mut self, diagnostic: SemanticDiagnostic) {
        if self.is_duplicate(&diagnostic) {
            return;
        }
        if self.diagnostics.len() >= self.max_diagnostics {
            if !self.capped {
                self.capped = true;
                self.diagnostics.push(SemanticDiagnostic::new(
                    DiagnosticId::HirMissingChild,
                    DiagnosticSeverity::Note,
                    DiagnosticPhase::HirLowering,
                    "additional semantic diagnostics suppressed after recovery limit",
                ));
            }
            return;
        }
        self.diagnostics.push(diagnostic);
    }

    /// Adds an error diagnostic with an optional primary span.
    pub fn error(
        &mut self,
        id: DiagnosticId,
        phase: DiagnosticPhase,
        message: impl Into<String>,
        span: Option<TextRange>,
    ) {
        let mut diagnostic = SemanticDiagnostic::new(id, DiagnosticSeverity::Error, phase, message);
        if let Some(span) = span {
            diagnostic = diagnostic.with_primary_span(span);
        }
        self.report(diagnostic);
    }

    /// Adds a note diagnostic with an optional primary span.
    pub fn note(
        &mut self,
        id: DiagnosticId,
        phase: DiagnosticPhase,
        message: impl Into<String>,
        span: Option<TextRange>,
    ) {
        let mut diagnostic = SemanticDiagnostic::new(id, DiagnosticSeverity::Note, phase, message);
        if let Some(span) = span {
            diagnostic = diagnostic.with_primary_span(span);
        }
        self.report(diagnostic);
    }

    /// Returns collected diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[SemanticDiagnostic] {
        &self.diagnostics
    }

    /// Consumes the reporter and returns diagnostics.
    #[must_use]
    pub fn into_diagnostics(self) -> Vec<SemanticDiagnostic> {
        self.diagnostics
    }

    fn is_duplicate(&self, diagnostic: &SemanticDiagnostic) -> bool {
        self.diagnostics.iter().any(|existing| {
            existing.id() == diagnostic.id()
                && existing.phase() == diagnostic.phase()
                && existing.severity() == diagnostic.severity()
                && existing.message() == diagnostic.message()
                && existing.span() == diagnostic.span()
        })
    }
}

impl Default for DiagnosticReporter {
    fn default() -> Self {
        Self::new()
    }
}

fn push_span_json(out: &mut String, span: Option<TextRange>) {
    if let Some(span) = span {
        out.push_str("{\"start\":");
        out.push_str(&span.start().to_usize().to_string());
        out.push_str(",\"end\":");
        out.push_str(&span.end().to_usize().to_string());
        out.push('}');
    } else {
        out.push_str("null");
    }
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0C}' => escaped.push_str("\\f"),
            ch if ch <= '\u{1F}' => escaped.push_str(&format!("\\u{:04X}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::{
        DiagnosticId, DiagnosticLabel, DiagnosticPhase, DiagnosticReporter, DiagnosticSeverity,
        SemanticDiagnostic,
    };
    use php_source::TextRange;
    use serde_json::Value;

    #[test]
    fn diagnostic_ids_are_stable_strings() {
        assert_eq!(
            DiagnosticId::DuplicateParameter.as_str(),
            "E_PHP_DUPLICATE_PARAMETER"
        );
        assert_eq!(
            DiagnosticId::ReferenceBehaviorDeferred.as_str(),
            "W_PHP_REFERENCE_BEHAVIOR_DEFERRED"
        );
        assert_eq!(
            DiagnosticId::RuntimeCheckDeferred.as_str(),
            "N_PHP_RUNTIME_CHECK_DEFERRED"
        );
    }

    #[test]
    fn reporter_collects_diagnostics_without_span() {
        let mut reporter = DiagnosticReporter::new();
        reporter.error(
            DiagnosticId::BreakNotInLoopOrSwitch,
            DiagnosticPhase::ControlFlowValidation,
            "break is not in a loop or switch",
            None,
        );

        let diagnostics = reporter.diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].span(), None);
        assert!(diagnostics[0].to_json().contains("\"span\":null"));
    }

    #[test]
    fn diagnostic_json_contains_span_labels_and_notes() {
        let diagnostic = SemanticDiagnostic::with_span(
            DiagnosticId::DuplicateUseAlias,
            DiagnosticSeverity::Error,
            DiagnosticPhase::DeclarationCollection,
            "duplicate import alias",
            TextRange::new(10, 15),
        )
        .with_label(DiagnosticLabel::new(
            TextRange::new(1, 5),
            "previous alias is here",
        ))
        .with_note("aliases are compared case-insensitively where PHP does");
        let json = diagnostic.to_json();

        assert!(json.contains("\"id\":\"E_PHP_DUPLICATE_USE_ALIAS\""));
        assert!(json.contains("\"severity\":\"error\""));
        assert!(json.contains("\"phase\":\"declaration_collection\""));
        assert!(json.contains("\"span\":{\"start\":10,\"end\":15}"));
        assert!(json.contains("\"labels\":[{\"message\":\"previous alias is here\""));
        assert!(json.contains("\"notes\":[\"aliases are compared"));
    }

    #[test]
    fn duplicate_parameter_diagnostic_has_structured_envelope() {
        let source = php_source::SourceText::new("<?php function f($x, $x) {}");
        let diagnostic = SemanticDiagnostic::with_span(
            DiagnosticId::DuplicateParameter,
            DiagnosticSeverity::Error,
            DiagnosticPhase::HirLowering,
            "duplicate parameter name `$x`",
            TextRange::new(22, 24),
        )
        .with_label(DiagnosticLabel::new(
            TextRange::new(18, 20),
            "previous parameter is here",
        ))
        .with_note("parameter names are unique within a function signature");
        let envelope =
            diagnostic.to_diagnostic_envelope(Some(&source), Some("sem-source"), Some("sem.php"));
        let json = envelope.compact_json().expect("diagnostic json renders");
        let decoded: Value = serde_json::from_str(&json).expect("valid diagnostic json");

        assert_eq!(decoded["code"], "E_PHP_DUPLICATE_PARAMETER");
        assert_eq!(decoded["layer"], "semantic");
        assert_eq!(decoded["phase"], "hir_lowering");
        assert_eq!(decoded["severity"], "error");
        assert_eq!(decoded["location"]["path"], "sem.php");
        assert_eq!(decoded["location"]["source_id"], "sem-source");
        assert_eq!(decoded["location"]["span"]["start"], 22);
        assert_eq!(
            decoded["labels"][0]["message"],
            "previous parameter is here"
        );
        assert_eq!(
            decoded["notes"][0],
            "parameter names are unique within a function signature"
        );
    }

    #[test]
    fn break_outside_loop_diagnostic_preserves_code_and_text() {
        let diagnostic = SemanticDiagnostic::with_span(
            DiagnosticId::BreakNotInLoopOrSwitch,
            DiagnosticSeverity::Error,
            DiagnosticPhase::ControlFlowValidation,
            "break is not in a loop or switch",
            TextRange::new(6, 11),
        );
        let envelope = diagnostic.to_diagnostic_envelope(None, None, Some("break.php"));

        assert_eq!(envelope.code, "E_PHP_BREAK_NOT_IN_LOOP_OR_SWITCH");
        assert!(
            envelope
                .text_line()
                .contains("phase=control_flow_validation")
        );
        assert!(envelope.text_line().contains("path=break.php"));
        assert!(envelope.text_line().contains("span=6..11"));
    }
}
