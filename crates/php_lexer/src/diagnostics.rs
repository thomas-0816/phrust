use crate::TextRange;

/// Diagnostic categories emitted by the lexer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LexDiagnosticKind {
    /// Input was malformed for the active scanner mode.
    InvalidInput,
    /// A block comment reached EOF before `*/`.
    UnterminatedBlockComment,
    /// A quoted string reached EOF before its closing delimiter.
    UnterminatedString,
    /// A heredoc or nowdoc block reached EOF before its closing label.
    UnterminatedHeredoc,
    /// A bad control byte was emitted as `T_BAD_CHARACTER`.
    BadCharacter,
}

/// A recoverable lexer diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexDiagnostic {
    /// Diagnostic category.
    pub kind: LexDiagnosticKind,
    /// Human-readable diagnostic message.
    pub message: String,
    /// Byte span associated with the diagnostic.
    pub span: TextRange,
    /// One-based source line associated with the diagnostic.
    pub line: usize,
}

impl LexDiagnostic {
    /// Creates a diagnostic.
    #[must_use]
    pub fn new(
        kind: LexDiagnosticKind,
        message: impl Into<String>,
        span: TextRange,
        line: usize,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            span,
            line,
        }
    }
}
