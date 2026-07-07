//! Type syntax grammar.

use crate::grammar::{named, names, symbol};
use crate::parser::core::Parser;
use crate::{SyntaxKind, SyntaxNodeKind};
use php_lexer::TokenName;

/// Parses an optional type when the current token can begin one.
pub(crate) fn parse_optional_type(parser: &mut Parser<'_>) -> bool {
    if !at_type_start(parser) {
        return false;
    }
    parse_type(parser);
    true
}

/// Parses an optional return type after `:`.
pub(crate) fn parse_optional_return_type(parser: &mut Parser<'_>) {
    bump_trivia(parser);
    if !parser.at(symbol(b':')) {
        return;
    }

    parser.bump();
    bump_trivia(parser);
    if !parse_optional_type(parser) {
        parser.error_expected("expected return type after `:`", &["type"]);
    }
}

/// Parses nullable, union, intersection, and DNF type syntax.
pub(crate) fn parse_type(parser: &mut Parser<'_>) {
    let type_node = parser.start();
    let shape = parse_union_type(parser);
    let kind = shape.node_kind();
    let _completed = type_node.complete(parser, SyntaxKind::Node(kind));
}

fn parse_union_type(parser: &mut Parser<'_>) -> TypeShape {
    let mut shape = parse_intersection_type(parser);
    let mut saw_union = false;
    let mut saw_dnf_term = shape.contains_intersection || shape.parenthesized;

    loop {
        bump_trivia(parser);
        if !parser.at(symbol(b'|')) {
            break;
        }
        saw_union = true;
        parser.bump();
        bump_trivia(parser);
        if !at_type_start(parser) {
            parser.error_expected("expected type after `|`", &["type"]);
            break;
        }
        let rhs = parse_intersection_type(parser);
        saw_dnf_term |= rhs.contains_intersection || rhs.parenthesized;
        shape.contains_nullable |= rhs.contains_nullable;
        shape.contains_intersection |= rhs.contains_intersection;
    }

    if saw_union {
        shape.contains_union = true;
        shape.contains_dnf = saw_dnf_term;
    }
    shape
}

fn parse_intersection_type(parser: &mut Parser<'_>) -> TypeShape {
    let mut shape = parse_type_primary(parser);
    let mut saw_intersection = false;

    loop {
        bump_trivia(parser);
        if !at_intersection_operator(parser) {
            break;
        }
        saw_intersection = true;
        parser.bump();
        bump_trivia(parser);
        if !at_type_start(parser) {
            parser.error_expected("expected type after `&`", &["type"]);
            break;
        }
        let rhs = parse_type_primary(parser);
        shape.contains_nullable |= rhs.contains_nullable;
        shape.parenthesized |= rhs.parenthesized;
    }

    if saw_intersection {
        shape.contains_intersection = true;
    }
    shape
}

fn parse_type_primary(parser: &mut Parser<'_>) -> TypeShape {
    let mut shape = TypeShape::default();

    if parser.at(symbol(b'?')) {
        shape.contains_nullable = true;
        parser.bump();
        bump_trivia(parser);
    }

    if parser.at(symbol(b'(')) {
        shape.parenthesized = true;
        parser.bump();
        bump_trivia(parser);
        let inner = parse_intersection_type(parser);
        shape.contains_nullable |= inner.contains_nullable;
        shape.contains_intersection |= inner.contains_intersection;
        if parser.at(symbol(b')')) {
            parser.bump();
        } else {
            parser.error_expected("expected `)` to close parenthesized type", &[")"]);
        }
        parse_array_suffix(parser);
        return shape;
    }

    if parse_type_atom(parser) {
        return shape;
    }

    parser.error_expected("expected type", &["type"]);
    shape
}

fn parse_type_atom(parser: &mut Parser<'_>) -> bool {
    if parser.at(named(TokenName::Array))
        || parser.at(named(TokenName::Callable))
        || parser.at(named(TokenName::Static))
    {
        parser.bump();
        parse_array_suffix(parser);
        return true;
    }

    if names::parse_name(parser) {
        parse_array_suffix(parser);
        return true;
    }

    false
}

fn parse_array_suffix(parser: &mut Parser<'_>) {
    loop {
        bump_trivia(parser);
        if !parser.at(symbol(b'[')) {
            break;
        }
        parser.bump();
        bump_trivia(parser);
        if parser.at(symbol(b']')) {
            parser.bump();
        } else {
            parser.error_expected("expected `]` to close array type", &["]"]);
            break;
        }
    }
}

pub(crate) fn at_type_start(parser: &Parser<'_>) -> bool {
    parser.at(symbol(b'?'))
        || parser.at(symbol(b'('))
        || parser.at(named(TokenName::String))
        || parser.at(named(TokenName::NameFullyQualified))
        || parser.at(named(TokenName::NameQualified))
        || parser.at(named(TokenName::NameRelative))
        || parser.at(named(TokenName::Array))
        || parser.at(named(TokenName::Callable))
        || parser.at(named(TokenName::Static))
}

fn at_intersection_operator(parser: &Parser<'_>) -> bool {
    parser.at(symbol(b'&')) || parser.at(named(TokenName::AmpersandNotFollowedByVarOrVararg))
}

fn bump_trivia(parser: &mut Parser<'_>) {
    while parser.current().is_trivia() {
        parser.bump();
    }
}

#[derive(Default)]
struct TypeShape {
    contains_union: bool,
    contains_intersection: bool,
    contains_nullable: bool,
    contains_dnf: bool,
    parenthesized: bool,
}

impl TypeShape {
    fn node_kind(&self) -> SyntaxNodeKind {
        if self.contains_dnf {
            SyntaxNodeKind::DnfType
        } else if self.contains_union {
            SyntaxNodeKind::UnionType
        } else if self.contains_intersection {
            SyntaxNodeKind::IntersectionType
        } else if self.contains_nullable {
            SyntaxNodeKind::NullableType
        } else {
            SyntaxNodeKind::Type
        }
    }
}
