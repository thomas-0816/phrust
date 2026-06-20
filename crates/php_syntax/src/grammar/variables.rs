//! Variable grammar.

use crate::grammar::named;
use crate::parser::core::Parser;
use crate::{SyntaxKind, SyntaxNodeKind};
use php_lexer::TokenName;

/// Parses a simple variable such as `$x`.
pub(crate) fn parse_simple_variable(parser: &mut Parser<'_>) -> bool {
    if !parser.at(named(TokenName::Variable)) {
        return false;
    }

    let variable = parser.start();
    parser.bump();
    let _completed = variable.complete(parser, SyntaxKind::Node(SyntaxNodeKind::Variable));
    true
}
