//! Name syntax grammar.

use crate::grammar::{named, symbol};
use crate::parser::core::Parser;
use crate::{SyntaxKind, SyntaxNodeKind};
use php_lexer::TokenName;

/// Parses a qualified, fully-qualified, relative, or simple name.
pub(crate) fn parse_name(parser: &mut Parser<'_>) -> bool {
    if !at_name_start(parser) {
        return false;
    }

    let name = parser.start();
    parser.bump();

    loop {
        bump_trivia(parser);
        // A lone backslash lexes as `T_NS_SEPARATOR` (tokenizer parity);
        // adjacent segments are already folded into one qualified-name token.
        if !parser.at(named(TokenName::NsSeparator)) {
            break;
        }
        if next_non_trivia_is(parser, 1, symbol(b'{')) {
            break;
        }
        parser.bump();
        bump_trivia(parser);
        if parser.at(named(TokenName::String)) {
            parser.bump();
        } else {
            parser.error_expected(
                "expected name segment after namespace separator",
                &["T_STRING"],
            );
            break;
        }
    }

    let _completed = name.complete(parser, SyntaxKind::Node(SyntaxNodeKind::Name));
    true
}

fn at_name_start(parser: &Parser<'_>) -> bool {
    parser.at(named(TokenName::String))
        || parser.at(named(TokenName::NameFullyQualified))
        || parser.at(named(TokenName::NameQualified))
        || parser.at(named(TokenName::NameRelative))
}

fn next_non_trivia_is(parser: &Parser<'_>, start: usize, expected: SyntaxKind) -> bool {
    let mut index = start;
    loop {
        let kind = parser.nth(index);
        if kind.is_trivia() {
            index += 1;
            continue;
        }
        return kind == expected;
    }
}

fn bump_trivia(parser: &mut Parser<'_>) {
    while parser.current().is_trivia() {
        parser.bump();
    }
}
