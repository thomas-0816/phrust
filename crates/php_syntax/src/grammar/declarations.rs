//! Declaration grammar.

use crate::grammar::{
    attributes, classes, expressions, functions, named, names, statements, symbol,
};
use crate::parser::core::Parser;
use crate::recovery;
use crate::{SyntaxKind, SyntaxNodeKind};
use php_lexer::TokenName;

/// Parses a function declaration at declaration/statement position.
pub(crate) fn parse_function_declaration(parser: &mut Parser<'_>) {
    functions::parse_function_declaration(parser);
}

/// Returns true when an attribute group is followed by a declaration keyword.
pub(crate) fn at_attributed_declaration(parser: &Parser<'_>) -> bool {
    if !parser.at(named(TokenName::Attribute)) {
        return false;
    }
    matches!(
        attributes::first_token_after_attribute_groups(parser),
        kind if kind == named(TokenName::Function)
            || kind == named(TokenName::Class)
            || kind == named(TokenName::Interface)
            || kind == named(TokenName::Trait)
            || kind == named(TokenName::Enum)
            || kind == named(TokenName::Abstract)
            || kind == named(TokenName::Final)
            || kind == named(TokenName::Readonly)
    )
}

/// Parses an attributed declaration.
pub(crate) fn parse_attributed_declaration(parser: &mut Parser<'_>) {
    let next = attributes::first_token_after_attribute_groups(parser);
    if next == named(TokenName::Function) {
        functions::parse_function_declaration(parser);
    } else if next == named(TokenName::Class)
        || next == named(TokenName::Interface)
        || next == named(TokenName::Trait)
        || next == named(TokenName::Enum)
        || next == named(TokenName::Abstract)
        || next == named(TokenName::Final)
        || next == named(TokenName::Readonly)
    {
        classes::parse_class_like_declaration(parser);
    } else {
        parser.error_expected("expected declaration after attributes", &["declaration"]);
        attributes::parse_attribute_groups(parser);
    }
}

/// Parses a class-like declaration.
pub(crate) fn parse_class_like_declaration(parser: &mut Parser<'_>) {
    classes::parse_class_like_declaration(parser);
}

/// Parses a namespace-level `const` declaration.
pub(crate) fn parse_const_declaration(parser: &mut Parser<'_>) {
    let const_decl = parser.start();
    parser.bump();

    loop {
        bump_trivia(parser);
        if parser.at(named(TokenName::String)) {
            parser.bump();
        } else {
            parser.error_expected("expected constant name", &["T_STRING"]);
            recovery::recover_to_statement_boundary(parser);
            break;
        }

        bump_trivia(parser);
        if parser.at(symbol(b'=')) {
            parser.bump();
            if !expressions::parse_expression(parser) {
                parser.error_expected("expected constant value expression", &["expression"]);
            }
        } else {
            parser.error_expected("expected `=` in const declaration", &["="]);
            recovery::recover_to_statement_boundary(parser);
            break;
        }

        bump_trivia(parser);
        if parser.at(symbol(b',')) {
            parser.bump();
            continue;
        }
        break;
    }

    bump_trivia(parser);
    if parser.at(symbol(b';')) {
        parser.bump();
    } else {
        parser.error_expected("expected `;` after const declaration", &[";"]);
        recovery::recover_to_statement_boundary(parser);
        if parser.at(symbol(b';')) {
            parser.bump();
        }
    }

    let _completed = const_decl.complete(parser, SyntaxKind::Node(SyntaxNodeKind::ConstDecl));
}

/// Parses a namespace declaration.
pub(crate) fn parse_namespace_declaration(parser: &mut Parser<'_>) {
    let namespace = parser.start();
    parser.bump();
    bump_trivia(parser);

    if parser.at(symbol(b'{')) {
        statements::parse_block_statement(parser);
        let _completed =
            namespace.complete(parser, SyntaxKind::Node(SyntaxNodeKind::NamespaceStmt));
        return;
    }

    if !names::parse_name(parser) {
        parser.error_expected("expected namespace name or body", &["name", "{"]);
    }
    bump_trivia(parser);
    if parser.at(symbol(b'{')) {
        statements::parse_block_statement(parser);
    } else if parser.at(symbol(b';')) {
        parser.bump();
    } else {
        parser.error_expected("expected `;` or namespace body", &[";", "{"]);
        recovery::recover_to_statement_boundary(parser);
        if parser.at(symbol(b';')) {
            parser.bump();
        }
    }

    let _completed = namespace.complete(parser, SyntaxKind::Node(SyntaxNodeKind::NamespaceStmt));
}

/// Parses a `use` import declaration.
pub(crate) fn parse_use_declaration(parser: &mut Parser<'_>) {
    let use_decl = parser.start();
    parser.bump();
    bump_trivia(parser);

    parse_optional_use_kind(parser);
    parse_use_import_clause(parser, false);

    loop {
        bump_trivia(parser);
        if parser.at(symbol(b',')) {
            parser.bump();
            parse_use_import_clause(parser, false);
        } else {
            break;
        }
    }

    bump_trivia(parser);
    if parser.at(symbol(b';')) {
        parser.bump();
    } else {
        parser.error_expected("expected `;` after use declaration", &[";"]);
        recovery::recover_to_statement_boundary(parser);
        if parser.at(symbol(b';')) {
            parser.bump();
        }
    }

    let _completed = use_decl.complete(parser, SyntaxKind::Node(SyntaxNodeKind::UseDecl));
}

fn parse_use_import_clause(parser: &mut Parser<'_>, allow_item_kind: bool) {
    bump_trivia(parser);
    if allow_item_kind {
        parse_optional_use_kind(parser);
    }

    if !names::parse_name(parser) {
        parser.error_expected("expected import name", &["name"]);
        recover_to_use_boundary(parser);
        return;
    }

    bump_trivia(parser);
    if parser.at(named(TokenName::NsSeparator)) && next_non_trivia_is(parser, 1, symbol(b'{')) {
        parser.bump();
        parse_group_use_list(parser);
        return;
    }

    parse_optional_alias(parser);
}

fn parse_group_use_list(parser: &mut Parser<'_>) {
    bump_trivia(parser);
    if parser.at(symbol(b'{')) {
        parser.bump();
    } else {
        parser.error_expected("expected `{` in group use declaration", &["{"]);
        return;
    }

    while !at_group_use_end(parser) {
        bump_trivia(parser);
        if at_group_use_end(parser) {
            break;
        }
        if parser.at(symbol(b',')) {
            parser.bump();
            continue;
        }
        parse_use_import_clause(parser, true);
        bump_trivia(parser);
        if parser.at(symbol(b',')) {
            parser.bump();
        } else if !parser.at(symbol(b'}')) {
            parser.error_expected("expected `,` or `}` in group use declaration", &[",", "}"]);
            recover_to_group_use_boundary(parser);
            if parser.at(symbol(b',')) {
                parser.bump();
            }
        }
    }

    if parser.at(symbol(b'}')) {
        parser.bump();
    } else {
        parser.error_expected("expected `}` to close group use declaration", &["}"]);
    }
}

fn parse_optional_alias(parser: &mut Parser<'_>) {
    bump_trivia(parser);
    if !parser.at(named(TokenName::As)) {
        return;
    }
    parser.bump();
    bump_trivia(parser);
    if parser.at(named(TokenName::String)) {
        parser.bump();
    } else {
        parser.error_expected("expected import alias", &["T_STRING"]);
    }
}

fn parse_optional_use_kind(parser: &mut Parser<'_>) {
    if parser.at(named(TokenName::Function)) || parser.at(named(TokenName::Const)) {
        parser.bump();
        bump_trivia(parser);
    }
}

fn recover_to_use_boundary(parser: &mut Parser<'_>) {
    while !parser.is_eof()
        && !parser.at(symbol(b','))
        && !parser.at(symbol(b';'))
        && !parser.at(symbol(b'}'))
        && !recovery::at_statement_recovery_token(parser)
    {
        parser.bump();
    }
}

fn recover_to_group_use_boundary(parser: &mut Parser<'_>) {
    while !parser.is_eof()
        && !parser.at(symbol(b','))
        && !parser.at(symbol(b'}'))
        && !parser.at(symbol(b';'))
    {
        parser.bump();
    }
}

fn at_group_use_end(parser: &Parser<'_>) -> bool {
    parser.is_eof()
        || parser.at(symbol(b'}'))
        || parser.at(symbol(b';'))
        || parser.at(named(TokenName::CloseTag))
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
