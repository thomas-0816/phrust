use crate::SyntaxKind;
use crate::diagnostics::{ParseDiagnostic, ParseDiagnosticId};
use crate::grammar;
use crate::parser::event::Event;
use crate::parser::marker::Marker;
use crate::token_source::{KeywordContext, TokenSource};
use php_lexer::Token;
use php_source::TextRange;

/// Event-based parser core.
#[derive(Debug)]
pub struct Parser<'src> {
    token_source: TokenSource<'src>,
    events: Vec<Event>,
    diagnostics: Vec<ParseDiagnostic>,
}

impl<'src> Parser<'src> {
    /// Creates a parser.
    #[must_use]
    pub fn new(token_source: TokenSource<'src>) -> Self {
        Self {
            token_source,
            events: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Starts a node marker.
    pub fn start(&mut self) -> Marker {
        let pos = self.events.len();
        self.events.push(Event::Placeholder);
        Marker::new(pos)
    }

    /// Completes a previously started marker.
    pub(crate) fn complete_marker(&mut self, pos: usize, kind: SyntaxKind) {
        self.events[pos] = Event::StartNode(kind);
        self.events.push(Event::FinishNode);
    }

    /// Returns the current token kind.
    #[must_use]
    pub fn current(&self) -> SyntaxKind {
        self.token_source.current()
    }

    /// Returns the nth token kind from the current position.
    #[must_use]
    pub fn nth(&self, n: usize) -> SyntaxKind {
        self.token_source.nth(n)
    }

    /// Returns current token text.
    #[must_use]
    pub fn current_text(&self) -> &str {
        self.token_source.current_text()
    }

    /// Returns current token range, or an empty EOF range.
    #[must_use]
    pub fn current_range(&self) -> TextRange {
        self.token_source.current_range()
    }

    /// Returns contextual keyword/name information for the current token.
    #[must_use]
    pub fn current_keyword_context(&self) -> Option<KeywordContext<'src>> {
        self.token_source.current_keyword_context()
    }

    /// Returns true at EOF.
    #[must_use]
    pub fn is_eof(&self) -> bool {
        self.token_source.is_eof()
    }

    /// Returns the current token index.
    #[must_use]
    pub(crate) const fn position(&self) -> usize {
        self.token_source.position()
    }

    /// Returns true when current token has `kind`.
    #[must_use]
    pub fn at(&self, kind: SyntaxKind) -> bool {
        self.token_source.at(kind)
    }

    /// Consumes the current token if it has `kind`.
    pub fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Consumes the current token and records an add-token event.
    pub fn bump(&mut self) {
        if !self.token_source.is_eof() {
            self.events.push(Event::AddToken);
            self.token_source.bump();
        }
    }

    /// Emits a parser error.
    pub fn error(&mut self, diagnostic: ParseDiagnostic) {
        self.events.push(Event::Error(diagnostic.clone()));
        self.diagnostics.push(diagnostic);
    }

    /// Emits an unexpected-token error and consumes one token when possible.
    pub fn error_and_bump(&mut self, expected: impl Into<String>) {
        let expected = expected.into();
        let message = format!(
            "unexpected token `{}`, expected {}",
            self.current_text().escape_default(),
            expected
        );
        let range = self.current_range();
        self.error(
            ParseDiagnostic::new(ParseDiagnosticId::UnexpectedToken, message, range)
                .with_expected([expected]),
        );
        self.bump();
    }

    /// Emits an unexpected-token diagnostic with an expected syntax set.
    pub fn error_expected(&mut self, message: impl Into<String>, expected: &[&str]) {
        let message = message.into();
        let id = classify_expected_diagnostic(&message, expected);
        let diagnostic = ParseDiagnostic::new(id, message, self.current_range())
            .with_expected(expected.iter().copied());
        self.error(diagnostic);
    }

    /// Emits a diagnostic with an explicit stable identifier and expected syntax set.
    pub fn error_expected_with_id(
        &mut self,
        id: ParseDiagnosticId,
        message: impl Into<String>,
        expected: &[&str],
    ) {
        let diagnostic = ParseDiagnostic::new(id, message, self.current_range())
            .with_expected(expected.iter().copied());
        self.error(diagnostic);
    }

    /// Minimal flat source-file parse.
    pub fn parse_source_file(&mut self) {
        grammar::source_file::parse(self);
    }

    /// Finishes parsing.
    #[must_use]
    pub fn finish(self) -> (Vec<Event>, Vec<Token>, Vec<ParseDiagnostic>) {
        let tokens = self.token_source.into_tokens();
        (self.events, tokens, self.diagnostics)
    }
}

fn classify_expected_diagnostic(message: &str, expected: &[&str]) -> ParseDiagnosticId {
    if is_unclosed_delimiter(message, expected) {
        return ParseDiagnosticId::UnclosedDelimiter;
    }
    if expected.contains(&"expression") {
        return ParseDiagnosticId::ExpectedExpression;
    }
    if expected
        .iter()
        .any(|item| matches!(*item, "statement" | "declaration"))
    {
        return ParseDiagnosticId::ExpectedStatement;
    }
    if expected.contains(&"type") {
        return ParseDiagnosticId::ExpectedType;
    }
    if expected
        .iter()
        .any(|item| matches!(*item, "name" | "T_STRING" | "T_VARIABLE"))
        || message.contains("identifier")
        || message.contains("name")
    {
        return ParseDiagnosticId::ExpectedIdentifier;
    }
    if expected.is_empty() {
        return ParseDiagnosticId::UnexpectedToken;
    }
    ParseDiagnosticId::ExpectedToken
}

fn is_unclosed_delimiter(message: &str, expected: &[&str]) -> bool {
    if message.contains("to close") || message.contains("after interpolation") {
        return true;
    }
    expected.len() == 1
        && expected
            .iter()
            .any(|item| matches!(*item, ")" | "]" | "}" | "T_END_HEREDOC"))
}

#[cfg(test)]
mod tests {
    use super::Parser;
    use crate::SyntaxKind;
    use crate::parser::event::Event;
    use crate::token_source::TokenSource;
    use php_lexer::{LexerConfig, TokenKind, lex_all};

    #[test]
    fn marker_pairs_start_and_finish_node() {
        let source = "<?php";
        let lexed = lex_all(source, LexerConfig::default());
        let mut parser = Parser::new(TokenSource::new(source, lexed.tokens));
        let marker = parser.start();
        let completed = marker.complete(&mut parser, SyntaxKind::SOURCE_FILE);
        let (events, _, _) = parser.finish();

        assert_eq!(completed.kind(), SyntaxKind::SOURCE_FILE);
        assert!(matches!(
            events[0],
            Event::StartNode(SyntaxKind::SOURCE_FILE)
        ));
        assert!(matches!(events[1], Event::FinishNode));
    }

    #[test]
    fn bump_makes_progress_to_eof() {
        let source = "<?php echo 1;";
        let lexed = lex_all(source, LexerConfig::default());
        let mut parser = Parser::new(TokenSource::new(source, lexed.tokens));
        let mut bumps = 0;
        while !parser.is_eof() {
            parser.bump();
            bumps += 1;
            assert!(bumps < 100);
        }
        assert_eq!(
            parser.current(),
            SyntaxKind::from_token_kind(TokenKind::Eof)
        );
    }

    #[test]
    fn error_event_is_recorded() {
        let source = "";
        let lexed = lex_all(source, LexerConfig::default());
        let mut parser = Parser::new(TokenSource::new(source, lexed.tokens));
        parser.error_and_bump("test token");
        let (events, _, diagnostics) = parser.finish();

        assert!(events.iter().any(|event| matches!(event, Event::Error(_))));
        assert_eq!(diagnostics.len(), 1);
    }
}
