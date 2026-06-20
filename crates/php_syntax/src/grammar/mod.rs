//! PHP grammar entry points.

pub(crate) mod arrays;
pub(crate) mod attributes;
pub(crate) mod classes;
pub(crate) mod declarations;
pub(crate) mod expressions;
pub(crate) mod functions;
pub(crate) mod names;
pub(crate) mod php85;
pub(crate) mod source_file;
pub(crate) mod statements;
pub(crate) mod strings;
pub(crate) mod types;
pub(crate) mod variables;

use crate::SyntaxKind;
use php_lexer::{SymbolKind, TokenKind, TokenName};

pub(crate) fn named(name: TokenName) -> SyntaxKind {
    SyntaxKind::from_token_kind(TokenKind::Named(name))
}

pub(crate) fn symbol(byte: u8) -> SyntaxKind {
    SyntaxKind::from_token_kind(TokenKind::Symbol(SymbolKind::Char(byte)))
}
