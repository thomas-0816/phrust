//! Variable grammar.

use crate::grammar::{expressions, named, symbol};
use crate::parser::core::Parser;
use crate::{SyntaxKind, SyntaxNodeKind};
use php_lexer::TokenName;

/// Parses a variable such as `$x`, `$$name`, or `${$name}`.
pub(crate) fn parse_simple_variable(parser: &mut Parser<'_>) -> bool {
    if !(parser.at(named(TokenName::Variable)) || parser.at(symbol(b'$'))) {
        return false;
    }

    let variable = parser.start();
    if parser.at(named(TokenName::Variable)) {
        parser.bump();
    } else {
        parser.bump();
        bump_trivia(parser);
        if parser.at(symbol(b'{')) {
            parser.bump();
            bump_trivia(parser);
            if !expressions::parse_expression(parser) {
                parser.error_expected("expected expression in braced variable", &["expression"]);
            }
            bump_trivia(parser);
            if parser.at(symbol(b'}')) {
                parser.bump();
            } else {
                parser.error_expected("expected `}` after braced variable", &["}"]);
            }
        } else if parser.at(named(TokenName::Variable)) || parser.at(symbol(b'$')) {
            let _nested = parse_simple_variable(parser);
        } else {
            parser.error_expected(
                "expected variable after variable-variable sigil",
                &["T_VARIABLE", "{"],
            );
        }
    }
    let _completed = variable.complete(parser, SyntaxKind::Node(SyntaxNodeKind::Variable));
    true
}

fn bump_trivia(parser: &mut Parser<'_>) {
    while parser.current().is_trivia() {
        parser.bump();
    }
}
