//! PHP parser and lossless CST surface.
//!
//! This crate consumes tokens from `php_lexer`. It does not implement a second
//! lexer and does not perform semantic lowering, runtime execution, or VM work.

pub mod diagnostics;
pub mod grammar;
pub mod parse;
pub mod parser;
pub mod recovery;
pub mod syntax_kind;
pub mod syntax_node;
pub mod token_source;
pub mod tree_sink;

pub use diagnostics::{ParseDiagnostic, ParseDiagnosticId, ParseSeverity};
pub use parse::{
    Parse, ParseContext, SourceId, parse_source_file, parse_source_file_with_context,
    parse_source_text,
};
pub use parser::core::Parser;
pub use parser::event::Event;
pub use parser::marker::{CompletedMarker, Marker};
pub use syntax_kind::{SyntaxKind, SyntaxNodeKind, SyntaxTokenKind};
pub use syntax_node::{SyntaxElement, SyntaxNode, SyntaxToken};
pub use token_source::TokenSource;
pub use tree_sink::TreeSink;
