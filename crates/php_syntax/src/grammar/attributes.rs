//! Attribute grammar helpers.

use crate::grammar::{named, names, symbol};
use crate::parser::core::Parser;
use crate::{SyntaxKind, SyntaxNodeKind, SyntaxTokenKind};
use php_lexer::TokenName;

/// Parses zero or more PHP attribute groups.
pub(crate) fn parse_attribute_groups(parser: &mut Parser<'_>) {
    while parser.at(named(TokenName::Attribute)) {
        parse_attribute_group(parser);
    }
}

fn parse_attribute_group(parser: &mut Parser<'_>) {
    let group = parser.start();
    parser.bump();
    let mut parsed_attribute = false;

    while !parser.is_eof() && !parser.at(symbol(b']')) && !at_attribute_recovery_end(parser) {
        bump_trivia(parser);
        if parser.at(symbol(b',')) {
            if !parsed_attribute {
                parser.error_expected("expected attribute name", &["name"]);
            }
            parser.bump();
            continue;
        }

        parse_attribute(parser);
        parsed_attribute = true;
        bump_trivia(parser);
        if parser.at(symbol(b',')) {
            parser.bump();
        } else if !parser.at(symbol(b']')) && !at_attribute_recovery_end(parser) {
            parser.error_expected("expected `,` or `]` in attribute group", &[",", "]"]);
            recover_to_attribute_boundary(parser);
        }
    }

    if parser.at(symbol(b']')) {
        if !parsed_attribute {
            parser.error_expected("expected attribute name", &["name"]);
        }
        parser.bump();
    } else {
        parser.error_expected("expected `]` to close attribute group", &["]"]);
    }
    let _completed = group.complete(parser, SyntaxKind::Node(SyntaxNodeKind::AttributeGroup));
}

fn parse_attribute(parser: &mut Parser<'_>) {
    let attribute = parser.start();
    if !names::parse_name(parser) {
        parser.error_expected("expected attribute name", &["name"]);
        recover_to_attribute_boundary(parser);
        let _completed = attribute.complete(parser, SyntaxKind::Node(SyntaxNodeKind::Attribute));
        return;
    }

    bump_trivia(parser);
    if parser.at(symbol(b'(')) {
        parse_balanced_argument_list(parser);
    }
    let _completed = attribute.complete(parser, SyntaxKind::Node(SyntaxNodeKind::Attribute));
}

fn parse_balanced_argument_list(parser: &mut Parser<'_>) {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    parser.bump();
    paren_depth += 1;

    while !parser.is_eof() && paren_depth > 0 {
        if parser.at(symbol(b'(')) {
            paren_depth += 1;
        } else if parser.at(symbol(b')')) {
            paren_depth -= 1;
        } else if parser.at(symbol(b'[')) {
            bracket_depth += 1;
        } else if parser.at(symbol(b']')) && bracket_depth > 0 {
            bracket_depth -= 1;
        } else if parser.at(symbol(b'{')) {
            brace_depth += 1;
        } else if parser.at(symbol(b'}')) && brace_depth > 0 {
            brace_depth -= 1;
        } else if paren_depth == 1
            && bracket_depth == 0
            && brace_depth == 0
            && parser.at(symbol(b']'))
        {
            break;
        }
        parser.bump();
    }

    if paren_depth > 0 {
        parser.error_expected("expected `)` to close attribute argument list", &[")"]);
    }
}

/// Returns the first non-trivia token after one or more attribute groups.
pub(crate) fn first_token_after_attribute_groups(parser: &Parser<'_>) -> SyntaxKind {
    let mut index = 0usize;
    loop {
        while parser.nth(index).is_trivia() {
            index += 1;
        }
        if parser.nth(index) != named(TokenName::Attribute) {
            return parser.nth(index);
        }
        index += 1;

        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        loop {
            let kind = parser.nth(index);
            if matches!(kind, SyntaxKind::Token(SyntaxTokenKind::Eof)) {
                return kind;
            }
            if paren_depth == 0
                && bracket_depth == 0
                && brace_depth == 0
                && is_attribute_recovery_kind(kind)
            {
                return kind;
            }
            if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 && kind == symbol(b']') {
                index += 1;
                break;
            }
            if kind == symbol(b'(') {
                paren_depth += 1;
            } else if kind == symbol(b')') && paren_depth > 0 {
                paren_depth -= 1;
            } else if kind == symbol(b'[') {
                bracket_depth += 1;
            } else if kind == symbol(b']') && bracket_depth > 0 {
                bracket_depth -= 1;
            } else if kind == symbol(b'{') {
                brace_depth += 1;
            } else if kind == symbol(b'}') && brace_depth > 0 {
                brace_depth -= 1;
            }
            index += 1;
        }
    }
}

fn recover_to_attribute_boundary(parser: &mut Parser<'_>) {
    while !parser.is_eof()
        && !parser.at(symbol(b','))
        && !parser.at(symbol(b']'))
        && !at_attribute_recovery_end(parser)
    {
        parser.bump();
    }
}

fn at_attribute_recovery_end(parser: &Parser<'_>) -> bool {
    is_attribute_recovery_kind(parser.current())
}

fn is_attribute_recovery_kind(kind: SyntaxKind) -> bool {
    kind == named(TokenName::Function)
        || kind == named(TokenName::Class)
        || kind == named(TokenName::Interface)
        || kind == named(TokenName::Trait)
        || kind == named(TokenName::Enum)
        || kind == named(TokenName::Fn)
        || kind == named(TokenName::Static)
        || kind == named(TokenName::Public)
        || kind == named(TokenName::Protected)
        || kind == named(TokenName::Private)
        || kind == named(TokenName::Readonly)
        || kind == named(TokenName::Variable)
        || kind == named(TokenName::CloseTag)
        || kind == symbol(b')')
        || kind == symbol(b'{')
        || kind == symbol(b'}')
        || kind == symbol(b';')
}

fn bump_trivia(parser: &mut Parser<'_>) {
    while parser.current().is_trivia() {
        parser.bump();
    }
}
