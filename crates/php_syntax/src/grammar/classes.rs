//! Class-like declaration surface grammar.

use crate::grammar::{attributes, named, symbol};
use crate::parser::core::Parser;
use crate::recovery;
use crate::{SyntaxKind, SyntaxNodeKind};
use php_lexer::TokenName;

/// Returns true when the current token begins a class-like declaration.
pub(crate) fn at_class_like_declaration(parser: &Parser<'_>) -> bool {
    parser.at(named(TokenName::Class))
        || parser.at(named(TokenName::Interface))
        || parser.at(named(TokenName::Trait))
        || parser.at(named(TokenName::Enum))
        || parser.at(named(TokenName::Abstract))
        || parser.at(named(TokenName::Final))
        || parser.at(named(TokenName::Readonly))
}

/// Parses class, interface, trait, and enum declarations shallowly.
pub(crate) fn parse_class_like_declaration(parser: &mut Parser<'_>) {
    parse_class_like(parser, true);
}

/// Parses an anonymous class after `new`.
pub(crate) fn parse_anonymous_class(parser: &mut Parser<'_>) {
    parse_class_like(parser, false);
}

fn parse_class_like(parser: &mut Parser<'_>, named_required: bool) {
    let declaration = parser.start();
    attributes::parse_attribute_groups(parser);
    parse_modifiers(parser);

    let kind = if parser.at(named(TokenName::Class)) {
        parser.bump();
        SyntaxNodeKind::ClassDecl
    } else if parser.at(named(TokenName::Interface)) {
        parser.bump();
        SyntaxNodeKind::InterfaceDecl
    } else if parser.at(named(TokenName::Trait)) {
        parser.bump();
        SyntaxNodeKind::TraitDecl
    } else if parser.at(named(TokenName::Enum)) {
        parser.bump();
        SyntaxNodeKind::EnumDecl
    } else {
        parser.error_expected(
            "expected class-like declaration",
            &["class", "interface", "trait", "enum"],
        );
        recovery::recover_to_statement_boundary(parser);
        let _completed = declaration.complete(parser, SyntaxKind::ERROR);
        return;
    };

    bump_trivia(parser);
    if named_required {
        if parser.at(named(TokenName::String)) {
            parser.bump();
        } else {
            parser.error_expected("expected class-like declaration name", &["T_STRING"]);
        }
    }

    consume_until_class_body(parser);
    if parser.at(symbol(b'{')) {
        parse_member_list(parser);
    } else {
        parser.error_expected("expected class-like declaration body", &["{"]);
        recovery::recover_to_statement_boundary(parser);
    }

    let _completed = declaration.complete(parser, SyntaxKind::Node(kind));
}

fn parse_member_list(parser: &mut Parser<'_>) {
    let members = parser.start();
    parser.bump();
    while !parser.is_eof() && !parser.at(symbol(b'}')) {
        if parser.current().is_trivia() || parser.at(symbol(b';')) {
            parser.bump();
        } else {
            parse_member(parser);
        }
    }
    if parser.at(symbol(b'}')) {
        parser.bump();
    } else {
        parser.error_expected("expected `}` to close class member list", &["}"]);
    }
    let _completed = members.complete(parser, SyntaxKind::Node(SyntaxNodeKind::ClassMemberList));
}

fn parse_member(parser: &mut Parser<'_>) {
    let member = parser.start();
    attributes::parse_attribute_groups(parser);
    parse_modifiers(parser);

    if parser.at(named(TokenName::Function)) {
        consume_member_with_optional_body(parser);
        let _completed = member.complete(parser, SyntaxKind::Node(SyntaxNodeKind::MethodDecl));
    } else if parser.at(named(TokenName::Const)) {
        consume_member_until_semicolon(parser);
        let _completed = member.complete(parser, SyntaxKind::Node(SyntaxNodeKind::ClassConstDecl));
    } else if parser.at(named(TokenName::Use)) {
        consume_member_with_optional_body(parser);
        let _completed = member.complete(parser, SyntaxKind::Node(SyntaxNodeKind::TraitUseDecl));
    } else {
        consume_property_member(parser);
        let _completed = member.complete(parser, SyntaxKind::Node(SyntaxNodeKind::PropertyDecl));
    }
}

fn parse_modifiers(parser: &mut Parser<'_>) {
    loop {
        bump_trivia(parser);
        if parser.at(named(TokenName::Abstract))
            || parser.at(named(TokenName::Final))
            || parser.at(named(TokenName::Readonly))
            || parser.at(named(TokenName::Public))
            || parser.at(named(TokenName::Protected))
            || parser.at(named(TokenName::Private))
            || parser.at(named(TokenName::PublicSet))
            || parser.at(named(TokenName::ProtectedSet))
            || parser.at(named(TokenName::PrivateSet))
            || parser.at(named(TokenName::Static))
            || parser.at(named(TokenName::Var))
        {
            parser.bump();
        } else {
            break;
        }
    }
}

fn consume_until_class_body(parser: &mut Parser<'_>) {
    while !parser.is_eof()
        && !parser.at(symbol(b'{'))
        && !parser.at(symbol(b';'))
        && !parser.at(named(TokenName::CloseTag))
    {
        parser.bump();
    }
}

fn consume_member_until_semicolon(parser: &mut Parser<'_>) {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    while !parser.is_eof() && !parser.at(named(TokenName::CloseTag)) {
        if parser.at(symbol(b'(')) {
            paren_depth += 1;
        } else if parser.at(symbol(b')')) && paren_depth > 0 {
            paren_depth -= 1;
        } else if parser.at(symbol(b'[')) {
            bracket_depth += 1;
        } else if parser.at(symbol(b']')) && bracket_depth > 0 {
            bracket_depth -= 1;
        } else if parser.at(symbol(b'{')) {
            brace_depth += 1;
        } else if parser.at(symbol(b'}')) {
            if brace_depth == 0 {
                break;
            }
            brace_depth -= 1;
        } else if parser.at(symbol(b';'))
            && paren_depth == 0
            && bracket_depth == 0
            && brace_depth == 0
        {
            break;
        }
        parser.bump();
    }
    if parser.at(symbol(b';')) {
        parser.bump();
    } else if !parser.at(symbol(b'}')) {
        parser.error_expected("expected `;` after class member", &[";"]);
        recovery::recover_to_statement_boundary(parser);
    }
}

fn consume_property_member(parser: &mut Parser<'_>) {
    while !parser.is_eof()
        && !parser.at(symbol(b';'))
        && !parser.at(symbol(b'{'))
        && !parser.at(symbol(b'}'))
        && !parser.at(named(TokenName::CloseTag))
    {
        parser.bump();
    }
    if parser.at(symbol(b';')) {
        parser.bump();
    } else if parser.at(symbol(b'{')) {
        consume_balanced_block(parser);
    } else if !parser.at(symbol(b'}')) {
        parser.error_expected("expected `;` or property hook body", &[";", "{"]);
        recovery::recover_to_statement_boundary(parser);
    }
}

fn consume_member_with_optional_body(parser: &mut Parser<'_>) {
    while !parser.is_eof()
        && !parser.at(symbol(b'{'))
        && !parser.at(symbol(b';'))
        && !parser.at(symbol(b'}'))
        && !parser.at(named(TokenName::CloseTag))
    {
        parser.bump();
    }
    if parser.at(symbol(b'{')) {
        consume_balanced_block(parser);
    } else if parser.at(symbol(b';')) {
        parser.bump();
    } else if !parser.at(symbol(b'}')) {
        parser.error_expected("expected method body or `;`", &["{", ";"]);
    }
}

fn consume_balanced_block(parser: &mut Parser<'_>) {
    let mut depth = 0usize;
    while !parser.is_eof() {
        if parser.at(symbol(b'{')) {
            depth += 1;
        } else if parser.at(symbol(b'}')) {
            depth -= 1;
            parser.bump();
            if depth == 0 {
                return;
            }
            continue;
        }
        parser.bump();
    }
    parser.error_expected("expected `}` to close block", &["}"]);
}

fn bump_trivia(parser: &mut Parser<'_>) {
    while parser.current().is_trivia() {
        parser.bump();
    }
}
