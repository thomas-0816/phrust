//! Class-like declaration surface grammar.

use crate::grammar::{attributes, expressions, functions, named, symbol, types};
use crate::parser::core::Parser;
use crate::recovery;
use crate::{SyntaxKind, SyntaxNodeKind, SyntaxTokenKind};
use php_lexer::{TokenKind, TokenName};

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
    } else if parser.at(symbol(b'(')) {
        expressions::parse_call_tail(parser);
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
        functions::parse_method_member_body(parser);
        let _completed = member.complete(parser, SyntaxKind::Node(SyntaxNodeKind::MethodDecl));
    } else if parser.at(named(TokenName::Const)) {
        parse_class_const_member(parser);
        let _completed = member.complete(parser, SyntaxKind::Node(SyntaxNodeKind::ClassConstDecl));
    } else if parser.at(named(TokenName::Use)) {
        consume_member_with_optional_body(parser);
        let _completed = member.complete(parser, SyntaxKind::Node(SyntaxNodeKind::TraitUseDecl));
    } else {
        consume_property_member(parser);
        let _completed = member.complete(parser, SyntaxKind::Node(SyntaxNodeKind::PropertyDecl));
    }
}

fn parse_class_const_member(parser: &mut Parser<'_>) {
    parser.bump();
    bump_trivia(parser);
    if at_typed_class_const(parser) {
        types::parse_type(parser);
    }

    loop {
        bump_trivia(parser);
        if at_contextual_identifier(parser) {
            parser.bump();
        } else {
            parser.error_expected("expected class constant name", &["T_STRING"]);
            recover_to_class_const_boundary(parser);
            break;
        }

        bump_trivia(parser);
        if parser.at(symbol(b'=')) {
            parser.bump();
            if !expressions::parse_expression(parser) {
                parser.error_expected("expected class constant value expression", &["expression"]);
                recover_to_class_const_boundary(parser);
            }
        } else {
            parser.error_expected("expected `=` in class constant declaration", &["="]);
            recover_to_class_const_boundary(parser);
        }

        bump_trivia(parser);
        if parser.at(symbol(b',')) {
            parser.bump();
            continue;
        }
        break;
    }

    if parser.at(symbol(b';')) {
        parser.bump();
    } else {
        parser.error_expected("expected `;` after class constant declaration", &[";"]);
        recovery::recover_to_statement_boundary(parser);
        if parser.at(symbol(b';')) {
            parser.bump();
        }
    }
}

fn at_typed_class_const(parser: &Parser<'_>) -> bool {
    if !types::at_type_start(parser) {
        return false;
    }

    let mut index = 0;
    let mut significant = Vec::new();
    loop {
        let kind = parser.nth(index);
        if kind.is_trivia() {
            index += 1;
            continue;
        }
        if kind == symbol(b'=') {
            break;
        }
        if kind == symbol(b',')
            || kind == symbol(b';')
            || kind == SyntaxKind::from_token_kind(TokenKind::Eof)
        {
            return false;
        }
        significant.push(kind);
        index += 1;
    }

    significant.len() >= 2 && significant.last().copied().is_some_and(is_identifier_kind)
}

fn at_contextual_identifier(parser: &Parser<'_>) -> bool {
    parser.current_keyword_context().is_some()
}

fn is_identifier_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Token(SyntaxTokenKind::Named(
            TokenName::String | TokenName::Match | TokenName::Readonly | TokenName::Include
        ))
    )
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

fn consume_property_member(parser: &mut Parser<'_>) {
    while !parser.is_eof()
        && !parser.at(symbol(b';'))
        && !parser.at(symbol(b'{'))
        && !parser.at(symbol(b'}'))
        && !parser.at(named(TokenName::CloseTag))
    {
        if parser.at(symbol(b'=')) {
            parser.bump();
            if !expressions::parse_expression(parser) {
                parser.error_expected("expected property default expression", &["expression"]);
                recover_to_property_boundary(parser);
            }
        } else {
            parser.bump();
        }
    }
    if parser.at(symbol(b';')) {
        parser.bump();
    } else if parser.at(symbol(b'{')) {
        parse_property_hook_list(parser);
    } else if !parser.at(symbol(b'}')) {
        parser.error_expected("expected `;` or property hook body", &[";", "{"]);
        recovery::recover_to_statement_boundary(parser);
    }
}

fn parse_property_hook_list(parser: &mut Parser<'_>) {
    parser.bump();
    while !parser.is_eof() && !parser.at(symbol(b'}')) {
        if parser.current().is_trivia() || parser.at(symbol(b';')) {
            parser.bump();
            continue;
        }
        let hook = parser.start();
        parse_modifiers(parser);
        bump_trivia(parser);
        if at_contextual_identifier(parser) {
            parser.bump();
        } else {
            parser.error_expected("expected property hook name", &["get", "set"]);
            recover_to_property_hook_boundary(parser);
        }
        bump_trivia(parser);
        if parser.at(symbol(b'{')) {
            parser.bump();
            crate::grammar::statements::parse_statement_list_contents(parser);
            if parser.at(symbol(b'}')) {
                parser.bump();
            } else {
                parser.error_expected("expected `}` to close property hook body", &["}"]);
            }
        } else if parser.at(named(TokenName::DoubleArrow)) {
            parser.bump();
            if !crate::grammar::expressions::parse_expression(parser) {
                parser.error_expected("expected property hook expression", &["expression"]);
            }
            if parser.at(symbol(b';')) {
                parser.bump();
            } else {
                parser.error_expected("expected `;` after property hook expression", &[";"]);
                recover_to_property_hook_boundary(parser);
            }
        } else {
            parser.error_expected("expected property hook body", &["{", "=>"]);
            recover_to_property_hook_boundary(parser);
        }
        let _completed = hook.complete(parser, SyntaxKind::Node(SyntaxNodeKind::PropertyHookDecl));
    }
    if parser.at(symbol(b'}')) {
        parser.bump();
    } else {
        parser.error_expected("expected `}` to close property hook list", &["}"]);
    }
}

fn recover_to_property_hook_boundary(parser: &mut Parser<'_>) {
    while !parser.is_eof()
        && !parser.at(symbol(b';'))
        && !parser.at(symbol(b'}'))
        && !parser.at(named(TokenName::CloseTag))
    {
        parser.bump();
    }
    if parser.at(symbol(b';')) {
        parser.bump();
    }
}

fn recover_to_class_const_boundary(parser: &mut Parser<'_>) {
    while !parser.is_eof()
        && !parser.at(symbol(b','))
        && !parser.at(symbol(b';'))
        && !parser.at(symbol(b'}'))
        && !parser.at(named(TokenName::CloseTag))
    {
        parser.bump();
    }
}

fn recover_to_property_boundary(parser: &mut Parser<'_>) {
    while !parser.is_eof()
        && !parser.at(symbol(b','))
        && !parser.at(symbol(b';'))
        && !parser.at(symbol(b'{'))
        && !parser.at(symbol(b'}'))
        && !parser.at(named(TokenName::CloseTag))
    {
        parser.bump();
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
