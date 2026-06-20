//! Function, closure, arrow-function, and parameter grammar.

use crate::grammar::{attributes, named, statements, symbol, types};
use crate::parser::core::Parser;
use crate::recovery;
use crate::{SyntaxKind, SyntaxNodeKind};
use php_lexer::TokenName;

/// Parses a named function declaration.
pub(crate) fn parse_function_declaration(parser: &mut Parser<'_>) {
    let function = parser.start();
    attributes::parse_attribute_groups(parser);
    bump_trivia(parser);
    expect_function_keyword(parser);
    bump_trivia(parser);
    parse_optional_ampersand(parser);
    bump_trivia(parser);
    if parser.at(named(TokenName::String)) {
        parser.bump();
    } else {
        parser.error_expected("expected function name", &["T_STRING"]);
    }
    parse_parameter_list(parser);
    types::parse_optional_return_type(parser);
    parse_optional_function_body(parser, "function declaration");
    let _completed = function.complete(parser, SyntaxKind::Node(SyntaxNodeKind::FunctionDecl));
}

/// Parses a closure expression when the current tokens form one.
pub(crate) fn parse_closure_expression(parser: &mut Parser<'_>) -> bool {
    if !at_closure_start(parser) {
        return false;
    }

    let closure = parser.start();
    attributes::parse_attribute_groups(parser);
    bump_trivia(parser);
    if parser.at(named(TokenName::Static)) {
        parser.bump();
        bump_trivia(parser);
    }
    expect_function_keyword(parser);
    bump_trivia(parser);
    parse_optional_ampersand(parser);
    parse_parameter_list(parser);
    parse_optional_closure_use_list(parser);
    types::parse_optional_return_type(parser);
    parse_required_block_body(parser, "closure");
    let _completed = closure.complete(parser, SyntaxKind::Node(SyntaxNodeKind::ClosureExpr));
    true
}

/// Parses an arrow-function expression when the current tokens form one.
pub(crate) fn parse_arrow_function_expression(parser: &mut Parser<'_>) -> bool {
    if !at_arrow_function_start(parser) {
        return false;
    }

    let arrow = parser.start();
    attributes::parse_attribute_groups(parser);
    bump_trivia(parser);
    if parser.at(named(TokenName::Static)) {
        parser.bump();
        bump_trivia(parser);
    }
    if parser.at(named(TokenName::Fn)) {
        parser.bump();
    } else {
        parser.error_expected("expected `fn`", &["T_FN"]);
    }
    parse_parameter_list(parser);
    types::parse_optional_return_type(parser);
    bump_trivia(parser);
    if parser.at(named(TokenName::DoubleArrow)) {
        parser.bump();
        if !crate::grammar::expressions::parse_expression(parser) {
            parser.error_expected("expected expression after `=>`", &["expression"]);
        }
    } else {
        parser.error_expected("expected `=>` in arrow function", &["T_DOUBLE_ARROW"]);
    }
    let _completed = arrow.complete(parser, SyntaxKind::Node(SyntaxNodeKind::ArrowFunctionExpr));
    true
}

fn at_closure_start(parser: &Parser<'_>) -> bool {
    parser.at(named(TokenName::Function))
        || (parser.at(named(TokenName::Static))
            && nth_non_trivia_is(parser, 1, named(TokenName::Function)))
        || (parser.at(named(TokenName::Attribute))
            && attributes::first_token_after_attribute_groups(parser) == named(TokenName::Function))
}

fn at_arrow_function_start(parser: &Parser<'_>) -> bool {
    parser.at(named(TokenName::Fn))
        || (parser.at(named(TokenName::Static))
            && nth_non_trivia_is(parser, 1, named(TokenName::Fn)))
        || (parser.at(named(TokenName::Attribute))
            && attributes::first_token_after_attribute_groups(parser) == named(TokenName::Fn))
}

fn parse_parameter_list(parser: &mut Parser<'_>) {
    bump_trivia(parser);
    let list = parser.start();
    if parser.at(symbol(b'(')) {
        parser.bump();
    } else {
        parser.error_expected("expected parameter list", &["("]);
        let _completed = list.complete(parser, SyntaxKind::Node(SyntaxNodeKind::ParamList));
        return;
    }

    while !at_parameter_list_end(parser) {
        bump_trivia(parser);
        if at_parameter_list_end(parser) {
            break;
        }
        if parser.at(symbol(b',')) {
            parser.bump();
            continue;
        }

        parse_parameter(parser);

        bump_trivia(parser);
        if parser.at(symbol(b',')) {
            parser.bump();
        } else if !parser.at(symbol(b')')) {
            parser.error_expected("expected `,` or `)` in parameter list", &[",", ")"]);
            recover_to_parameter_boundary(parser);
            if parser.at(symbol(b',')) {
                parser.bump();
            }
        }
    }

    if parser.at(symbol(b')')) {
        parser.bump();
    } else {
        parser.error_expected("expected `)` to close parameter list", &[")"]);
    }
    let _completed = list.complete(parser, SyntaxKind::Node(SyntaxNodeKind::ParamList));
}

fn parse_parameter(parser: &mut Parser<'_>) {
    let param = parser.start();
    attributes::parse_attribute_groups(parser);
    bump_trivia(parser);
    parse_promoted_parameter_modifiers(parser);
    bump_trivia(parser);
    types::parse_optional_type(parser);
    bump_trivia(parser);
    parse_optional_ampersand(parser);
    bump_trivia(parser);
    if parser.at(named(TokenName::Ellipsis)) {
        parser.bump();
    }
    bump_trivia(parser);
    if parser.at(named(TokenName::Variable)) {
        parser.bump();
    } else {
        parser.error_expected("expected parameter variable", &["T_VARIABLE"]);
        recover_to_parameter_boundary(parser);
    }
    bump_trivia(parser);
    if parser.at(symbol(b'=')) {
        parser.bump();
        if !crate::grammar::expressions::parse_expression(parser) {
            parser.error_expected("expected default value expression", &["expression"]);
        }
    }
    let _completed = param.complete(parser, SyntaxKind::Node(SyntaxNodeKind::Param));
}

fn parse_promoted_parameter_modifiers(parser: &mut Parser<'_>) {
    while parser.at(named(TokenName::Public))
        || parser.at(named(TokenName::Protected))
        || parser.at(named(TokenName::Private))
        || parser.at(named(TokenName::Readonly))
        || parser.at(named(TokenName::PublicSet))
        || parser.at(named(TokenName::ProtectedSet))
        || parser.at(named(TokenName::PrivateSet))
    {
        parser.bump();
        bump_trivia(parser);
    }
}

fn parse_optional_closure_use_list(parser: &mut Parser<'_>) {
    bump_trivia(parser);
    if !parser.at(named(TokenName::Use)) {
        return;
    }

    let list = parser.start();
    parser.bump();
    bump_trivia(parser);
    if parser.at(symbol(b'(')) {
        parser.bump();
    } else {
        parser.error_expected("expected `(` after closure use", &["("]);
        let _completed = list.complete(parser, SyntaxKind::Node(SyntaxNodeKind::ParamList));
        return;
    }

    while !parser.is_eof() && !parser.at(symbol(b')')) {
        bump_trivia(parser);
        if parser.at(symbol(b',')) {
            parser.bump();
            continue;
        }
        let use_var = parser.start();
        parse_optional_ampersand(parser);
        bump_trivia(parser);
        if parser.at(named(TokenName::Variable)) {
            parser.bump();
        } else {
            parser.error_expected("expected closure use variable", &["T_VARIABLE"]);
            recover_to_parameter_boundary(parser);
        }
        let _completed = use_var.complete(parser, SyntaxKind::Node(SyntaxNodeKind::Param));
        bump_trivia(parser);
        if parser.at(symbol(b',')) {
            parser.bump();
        } else if !parser.at(symbol(b')')) {
            parser.error_expected("expected `,` or `)` in closure use list", &[",", ")"]);
            recover_to_parameter_boundary(parser);
        }
    }

    if parser.at(symbol(b')')) {
        parser.bump();
    } else {
        parser.error_expected("expected `)` to close closure use list", &[")"]);
    }
    let _completed = list.complete(parser, SyntaxKind::Node(SyntaxNodeKind::ParamList));
}

fn parse_optional_function_body(parser: &mut Parser<'_>, context: &str) {
    bump_trivia(parser);
    if parser.at(symbol(b';')) {
        parser.bump();
    } else {
        parse_required_block_body(parser, context);
    }
}

fn parse_required_block_body(parser: &mut Parser<'_>, context: &str) {
    bump_trivia(parser);
    if parser.at(symbol(b'{')) {
        statements::parse_block_statement(parser);
    } else {
        let message = format!("expected body for {context}");
        parser.error_expected(message, &["{"]);
        recovery::recover_to_statement_boundary(parser);
        if parser.at(symbol(b';')) {
            parser.bump();
        }
    }
}

fn expect_function_keyword(parser: &mut Parser<'_>) {
    if parser.at(named(TokenName::Function)) {
        parser.bump();
    } else {
        parser.error_expected("expected `function`", &["T_FUNCTION"]);
    }
}

fn parse_optional_ampersand(parser: &mut Parser<'_>) {
    if parser.at(symbol(b'&'))
        || parser.at(named(TokenName::AmpersandFollowedByVarOrVararg))
        || parser.at(named(TokenName::AmpersandNotFollowedByVarOrVararg))
    {
        parser.bump();
    }
}

fn recover_to_parameter_boundary(parser: &mut Parser<'_>) {
    while !parser.is_eof()
        && !parser.at(symbol(b','))
        && !parser.at(symbol(b')'))
        && !parser.at(symbol(b'{'))
        && !parser.at(symbol(b'}'))
        && !recovery::at_statement_recovery_token(parser)
    {
        parser.bump();
    }
}

fn at_parameter_list_end(parser: &Parser<'_>) -> bool {
    parser.is_eof()
        || parser.at(symbol(b')'))
        || parser.at(symbol(b'{'))
        || parser.at(symbol(b'}'))
        || recovery::at_statement_recovery_token(parser)
}

fn nth_non_trivia_is(parser: &Parser<'_>, start: usize, expected: SyntaxKind) -> bool {
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
