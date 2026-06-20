use crate::SyntaxKind;
use php_lexer::{Token, TokenKind, TokenName};
use php_source::TextRange;

/// Parser-facing view of a keyword-like token in a contextual name position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeywordContext<'src> {
    /// Lexer token name kept by the lossless token stream.
    pub token_name: TokenName,
    /// Original token text.
    pub text: &'src str,
}

/// Lightweight token cursor for parser code.
#[derive(Clone, Debug)]
pub struct TokenSource<'src> {
    source: &'src str,
    tokens: Vec<Token>,
    cursor: usize,
}

impl<'src> TokenSource<'src> {
    /// Creates a token source.
    #[must_use]
    pub fn new(source: &'src str, tokens: Vec<Token>) -> Self {
        Self {
            source,
            tokens,
            cursor: 0,
        }
    }

    /// Returns the current syntax kind.
    #[must_use]
    pub fn current(&self) -> SyntaxKind {
        self.nth(0)
    }

    /// Returns the nth syntax kind from the current position.
    #[must_use]
    pub fn nth(&self, n: usize) -> SyntaxKind {
        self.tokens.get(self.cursor + n).map_or_else(
            || SyntaxKind::from_token_kind(TokenKind::Eof),
            |token| SyntaxKind::from_token_kind(token.kind),
        )
    }

    /// Returns the current token text, or an empty string at EOF.
    #[must_use]
    pub fn current_text(&self) -> &'src str {
        self.tokens
            .get(self.cursor)
            .and_then(|token| token.text(self.source))
            .unwrap_or_default()
    }

    /// Returns the current token range, or an empty EOF range.
    #[must_use]
    pub fn current_range(&self) -> TextRange {
        self.tokens
            .get(self.cursor)
            .map_or_else(|| TextRange::empty(self.source.len()), |token| token.range)
    }

    /// Returns contextual keyword/name information for the current token.
    ///
    /// This does not re-tokenize or emulate PHP numeric token values. It only
    /// exposes the existing lexer classification plus source text so grammar
    /// sites can opt into PHP's contextual identifier positions.
    #[must_use]
    pub fn current_keyword_context(&self) -> Option<KeywordContext<'src>> {
        let token = self.tokens.get(self.cursor)?;
        let TokenKind::Named(token_name) = token.kind else {
            return None;
        };
        let text = token.text(self.source)?;
        if is_contextual_name_token(token_name) && is_contextual_name_text(text) {
            Some(KeywordContext { token_name, text })
        } else {
            None
        }
    }

    /// Advances to the next token.
    pub fn bump(&mut self) {
        if self.cursor < self.tokens.len() {
            self.cursor += 1;
        }
    }

    /// Returns true when all tokens have been consumed.
    #[must_use]
    pub fn is_eof(&self) -> bool {
        self.cursor >= self.tokens.len()
    }

    /// Returns true when the current token has `kind`.
    #[must_use]
    pub fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == kind
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

    /// Returns the current token index.
    #[must_use]
    pub const fn position(&self) -> usize {
        self.cursor
    }

    /// Returns all owned tokens.
    #[must_use]
    pub fn into_tokens(self) -> Vec<Token> {
        self.tokens
    }
}

fn is_contextual_name_token(token_name: TokenName) -> bool {
    matches!(
        token_name,
        TokenName::String
            | TokenName::NameFullyQualified
            | TokenName::NameQualified
            | TokenName::NameRelative
            | TokenName::If
            | TokenName::Else
            | TokenName::ElseIf
            | TokenName::EndIf
            | TokenName::Function
            | TokenName::Class
            | TokenName::Abstract
            | TokenName::Final
            | TokenName::Interface
            | TokenName::Trait
            | TokenName::Enum
            | TokenName::Namespace
            | TokenName::Use
            | TokenName::As
            | TokenName::InsteadOf
            | TokenName::Match
            | TokenName::Readonly
            | TokenName::Fn
            | TokenName::Yield
            | TokenName::Case
            | TokenName::Default
            | TokenName::Echo
            | TokenName::Print
            | TokenName::Return
            | TokenName::Break
            | TokenName::Continue
            | TokenName::Extends
            | TokenName::Implements
            | TokenName::Public
            | TokenName::Protected
            | TokenName::Private
            | TokenName::Static
            | TokenName::Const
            | TokenName::Var
            | TokenName::Declare
            | TokenName::EndDeclare
            | TokenName::Global
            | TokenName::Callable
            | TokenName::Clone
            | TokenName::New
            | TokenName::While
            | TokenName::EndWhile
            | TokenName::Do
            | TokenName::For
            | TokenName::EndFor
            | TokenName::Foreach
            | TokenName::EndForeach
            | TokenName::Switch
            | TokenName::EndSwitch
            | TokenName::Try
            | TokenName::Throw
            | TokenName::Catch
            | TokenName::Finally
            | TokenName::Include
            | TokenName::IncludeOnce
            | TokenName::Require
            | TokenName::RequireOnce
            | TokenName::Eval
            | TokenName::Isset
            | TokenName::Empty
            | TokenName::Unset
            | TokenName::List
            | TokenName::Array
            | TokenName::Instanceof
            | TokenName::Goto
            | TokenName::Exit
            | TokenName::HaltCompiler
    )
}

fn is_contextual_name_text(text: &str) -> bool {
    let mut bytes = text.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    is_name_start(first) && bytes.all(is_name_continue)
}

const fn is_name_start(byte: u8) -> bool {
    byte == b'_' || byte == b'\\' || byte.is_ascii_alphabetic() || byte >= 0x80
}

const fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::TokenSource;
    use crate::SyntaxKind;
    use php_lexer::{LexerConfig, TokenKind, TokenName, lex_all};

    #[test]
    fn token_source_reports_eof_and_advances() {
        let source = "<?php echo 1;";
        let lexed = lex_all(source, LexerConfig::default());
        let mut tokens = TokenSource::new(source, lexed.tokens);

        assert_eq!(
            tokens.current(),
            SyntaxKind::from_token_kind(TokenKind::Named(TokenName::OpenTag))
        );
        assert_eq!(tokens.current_text(), "<?php ");

        while !tokens.is_eof() {
            let before = tokens.position();
            tokens.bump();
            assert!(tokens.position() > before);
        }

        assert_eq!(
            tokens.current(),
            SyntaxKind::from_token_kind(TokenKind::Eof)
        );
        assert_eq!(tokens.current_text(), "");
        assert_eq!(
            tokens.current_range(),
            php_source::TextRange::empty(source.len())
        );
    }

    #[test]
    fn keyword_context_exposes_lexer_classification_without_retokenizing() {
        let source = "<?php $object->match();";
        let lexed = lex_all(source, LexerConfig::default());
        let mut tokens = TokenSource::new(source, lexed.tokens);

        while !tokens.is_eof() && tokens.current_text() != "match" {
            tokens.bump();
        }

        let context = tokens
            .current_keyword_context()
            .expect("match keyword is available as contextual name");
        assert_eq!(context.token_name, TokenName::Match);
        assert_eq!(context.text, "match");
    }

    #[test]
    fn keyword_context_ignores_non_name_tokens() {
        let source = "<?php $object->1;";
        let lexed = lex_all(source, LexerConfig::default());
        let mut tokens = TokenSource::new(source, lexed.tokens);

        while !tokens.is_eof() && tokens.current_text() != "1" {
            tokens.bump();
        }

        assert_eq!(tokens.current_keyword_context(), None);
    }
}
