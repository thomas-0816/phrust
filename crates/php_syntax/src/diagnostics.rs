use php_source::TextRange;

/// Stable parser diagnostic identifiers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseDiagnosticId {
    /// Diagnostic forwarded from the lexer during stub parsing.
    LexerDiagnostic,
    /// Parser encountered an unexpected token.
    UnexpectedToken,
    /// Parser expected a specific token.
    ExpectedToken,
    /// Parser expected an expression.
    ExpectedExpression,
    /// Parser expected a statement.
    ExpectedStatement,
    /// Parser expected a type.
    ExpectedType,
    /// Parser expected an identifier or name.
    ExpectedIdentifier,
    /// Parser reached a recovery point before a delimiter closed.
    UnclosedDelimiter,
}

impl ParseDiagnosticId {
    /// Returns the stable external diagnostic identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LexerDiagnostic => "lexer_diagnostic",
            Self::UnexpectedToken => "unexpected_token",
            Self::ExpectedToken => "expected_token",
            Self::ExpectedExpression => "expected_expression",
            Self::ExpectedStatement => "expected_statement",
            Self::ExpectedType => "expected_type",
            Self::ExpectedIdentifier => "expected_identifier",
            Self::UnclosedDelimiter => "unclosed_delimiter",
        }
    }
}

/// Parser diagnostic severity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseSeverity {
    /// Syntax or tokenization error.
    Error,
}

/// A recoverable parse diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseDiagnostic {
    /// Stable diagnostic identifier.
    pub id: ParseDiagnosticId,
    /// Human-readable message.
    pub message: String,
    /// Byte span in the original source.
    pub span: TextRange,
    /// Expected syntax names, empty for forwarded lexer diagnostics.
    pub expected: Vec<String>,
    /// Diagnostic severity.
    pub severity: ParseSeverity,
}

impl ParseDiagnostic {
    /// Creates a parse diagnostic.
    #[must_use]
    pub fn new(id: ParseDiagnosticId, message: impl Into<String>, span: TextRange) -> Self {
        Self {
            id,
            message: message.into(),
            span,
            expected: Vec::new(),
            severity: ParseSeverity::Error,
        }
    }

    /// Adds expected syntax names to the diagnostic.
    #[must_use]
    pub fn with_expected(mut self, expected: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.expected = expected.into_iter().map(Into::into).collect();
        self
    }
}
