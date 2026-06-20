//! Byte-oriented PHP lexer surface for Phase 1.
//!
//! This crate intentionally exposes only lexer and tokenization types. It does
//! not contain a parser, AST/CST model, VM, runtime values, JIT, extensions, or
//! Zend ABI emulation.
//!
//! The scanner covers the Phase 1 tokenization surface and records normalized
//! PHP token names, original byte spans, start lines, and recoverable
//! diagnostics.

mod cursor;
mod diagnostics;
mod keywords;
mod lexer;
mod modes;
mod strings;
mod token;

pub use diagnostics::{LexDiagnostic, LexDiagnosticKind};
pub use lexer::{LexResult, Lexer, LexerConfig, lex_all};
pub use modes::LexerMode;
pub use php_source::{BytePos, LineCol, LineIndex, SourceText, TextRange};
pub use token::{SymbolKind, Token, TokenKind, TokenName};
