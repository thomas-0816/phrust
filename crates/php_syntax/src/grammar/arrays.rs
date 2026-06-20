//! Array and dimension grammar helpers.

use crate::grammar::{expressions, named, symbol};
use crate::parser::core::Parser;
use crate::{SyntaxKind, SyntaxNodeKind};
use php_lexer::TokenName;

/// Parses a short-array, long-array, or list destructuring expression.
pub(crate) fn parse_array_expression(parser: &mut Parser<'_>) -> bool {
    if parser.at(symbol(b'[')) {
        parse_array_body(parser, b'[', b']', SyntaxNodeKind::ArrayExpr);
        return true;
    }

    if parser.at(named(TokenName::Array)) || parser.at(named(TokenName::List)) {
        let array = parser.start();
        parser.bump();
        if parser.at(symbol(b'(')) {
            parser.bump();
            parse_array_pairs_until(parser, b')');
            if parser.at(symbol(b')')) {
                parser.bump();
            } else {
                parser.error_expected("expected `)` to close array/list expression", &[")"]);
            }
        } else {
            parser.error_expected("expected `(` after array/list keyword", &["("]);
        }
        let _completed = array.complete(parser, SyntaxKind::Node(SyntaxNodeKind::ArrayExpr));
        return true;
    }

    false
}

/// Parses an array/dimension fetch tail such as `[0]`, `[]`, or `{0}`.
pub(crate) fn parse_dim_fetch_tail(parser: &mut Parser<'_>, _open: u8, close: u8) {
    let fetch = parser.start();
    parser.bump();
    while !parser.is_eof() && !parser.at(symbol(close)) {
        parser.bump();
    }
    if parser.at(symbol(close)) {
        parser.bump();
    } else {
        let expected = if close == b']' { "]" } else { "}" };
        parser.error_expected("expected dimension fetch terminator", &[expected]);
    }
    let _completed = fetch.complete(parser, SyntaxKind::Node(SyntaxNodeKind::ArrayDimFetchExpr));
}

fn parse_array_body(parser: &mut Parser<'_>, open: u8, close: u8, kind: SyntaxNodeKind) {
    let array = parser.start();
    debug_assert!(parser.at(symbol(open)));
    parser.bump();
    parse_array_pairs_until(parser, close);
    if parser.at(symbol(close)) {
        parser.bump();
    } else {
        let expected = if close == b']' { "]" } else { ")" };
        parser.error_expected("expected array terminator", &[expected]);
    }
    let _completed = array.complete(parser, SyntaxKind::Node(kind));
}

fn parse_array_pairs_until(parser: &mut Parser<'_>, close: u8) {
    while !parser.is_eof() && !parser.at(symbol(close)) {
        if parser.current().is_trivia() || parser.at(symbol(b',')) {
            parser.bump();
            continue;
        }

        let pair = parser.start();
        if parser.at(named(TokenName::Ellipsis))
            || parser.at(named(TokenName::AmpersandFollowedByVarOrVararg))
            || parser.at(named(TokenName::AmpersandNotFollowedByVarOrVararg))
        {
            parser.bump();
        }

        if !expressions::parse_expression(parser) {
            parser.error_expected("expected array element", &["expression"]);
        }
        if parser.at(named(TokenName::DoubleArrow)) {
            parser.bump();
            if !expressions::parse_expression(parser) {
                parser.error_expected("expected array value after `=>`", &["expression"]);
            }
        }
        let _completed = pair.complete(parser, SyntaxKind::Node(SyntaxNodeKind::ArrayPair));

        if parser.at(symbol(b',')) {
            parser.bump();
        } else if !parser.at(symbol(close)) {
            parser.error_expected("expected `,` or array terminator", &[",", "]", ")"]);
            while !parser.is_eof() && !parser.at(symbol(b',')) && !parser.at(symbol(close)) {
                parser.bump();
            }
        }
    }
}
