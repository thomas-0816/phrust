//! Tokenizer extension helpers backed by the Lexer lexer.

use crate::{ArrayKey, BuiltinError, PhpArray, Value};
use php_lexer::{
    LexerConfig, TOKENIZER_TOKEN_ID_BASE, TOKENIZER_TOKEN_NAMES, Token, TokenKind, TokenName,
    lex_all,
};

/// PHP `TOKEN_PARSE` flag value for the tokenizer extension.
pub const TOKEN_PARSE: i64 = 1;

/// One token returned by the tokenizer bridge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenizerToken {
    /// Local tokenizer token ID, or the byte value for symbol tokens.
    pub id: i64,
    /// Normalized token name, or the symbol text for symbol tokens.
    pub name: String,
    /// Original token text.
    pub text: String,
    /// One-based source line.
    pub line: u32,
    /// Zero-based source byte offset.
    pub pos: u32,
    /// Whether PHP's `token_get_all` returns an array for this token.
    pub named: bool,
    /// Named token kind, when available.
    pub token_name: Option<TokenName>,
}

/// Returns the local tokenizer ID for a named token.
#[must_use]
pub fn token_name_id(name: TokenName) -> i64 {
    TOKENIZER_TOKEN_NAMES
        .iter()
        .position(|candidate| *candidate == name)
        .map_or(TOKENIZER_TOKEN_ID_BASE, |index| {
            TOKENIZER_TOKEN_ID_BASE + index as i64
        })
}

/// Resolves a local tokenizer ID to its PHP token name.
#[must_use]
pub fn token_name_for_id(id: i64) -> Option<&'static str> {
    token_name_kind_for_id(id).map(|name| name.as_php_name())
}

/// Resolves a local tokenizer ID to the lexer token kind.
#[must_use]
pub fn token_name_kind_for_id(id: i64) -> Option<TokenName> {
    if id < TOKENIZER_TOKEN_ID_BASE {
        return None;
    }
    let index = (id - TOKENIZER_TOKEN_ID_BASE) as usize;
    TOKENIZER_TOKEN_NAMES.get(index).copied()
}

/// Returns whether a local tokenizer ID is ignorable for `PhpToken::isIgnorable`.
#[must_use]
pub fn is_ignorable_id(id: i64) -> bool {
    is_ignorable_name(token_name_kind_for_id(id))
}

/// Tokenizes source code with the existing Lexer lexer.
pub fn tokenize(source: &str, flags: i64) -> Result<Vec<TokenizerToken>, BuiltinError> {
    let config = LexerConfig {
        token_parse: flags & TOKEN_PARSE != 0,
        ..LexerConfig::default()
    };
    let result = lex_all(source, config);
    if let Some(diagnostic) = result.diagnostics.first() {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_TOKENIZER_LEX",
            format!("{diagnostic:?}"),
        ));
    }
    Ok(result
        .tokens
        .into_iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| tokenizer_token(source, &token))
        .collect())
}

/// Converts tokenizer output to `token_get_all` array/string shape.
#[must_use]
pub fn token_get_all_value(tokens: Vec<TokenizerToken>) -> Value {
    Value::packed_array(tokens.into_iter().map(token_get_all_entry).collect())
}

/// Returns whether a token is ignorable for `PhpToken::isIgnorable`.
#[must_use]
pub fn is_ignorable_name(name: Option<TokenName>) -> bool {
    matches!(
        name,
        Some(
            TokenName::OpenTag | TokenName::Whitespace | TokenName::Comment | TokenName::DocComment
        )
    )
}

fn tokenizer_token(source: &str, token: &Token) -> TokenizerToken {
    let text = token.text(source).unwrap_or_default().to_owned();
    let pos = token.range.start().to_usize() as u32;
    match token.kind {
        TokenKind::Named(name) => TokenizerToken {
            id: token_name_id(name),
            name: name.as_php_name().to_owned(),
            text,
            line: token.line,
            pos,
            named: true,
            token_name: Some(name),
        },
        TokenKind::Symbol(symbol) => {
            let name = symbol.reference_name();
            let id = text.as_bytes().first().copied().map_or(0, i64::from);
            TokenizerToken {
                id,
                name,
                text,
                line: token.line,
                pos,
                named: false,
                token_name: None,
            }
        }
        TokenKind::Eof => TokenizerToken {
            id: 0,
            name: "EOF".to_owned(),
            text,
            line: token.line,
            pos,
            named: false,
            token_name: None,
        },
    }
}

fn token_get_all_entry(token: TokenizerToken) -> Value {
    if !token.named {
        return Value::string(token.text.into_bytes());
    }
    let mut entry = PhpArray::new();
    entry.insert(ArrayKey::Int(0), Value::Int(token.id));
    entry.insert(ArrayKey::Int(1), Value::string(token.text.into_bytes()));
    entry.insert(ArrayKey::Int(2), Value::Int(i64::from(token.line)));
    Value::Array(entry)
}
