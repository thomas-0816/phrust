//! Pratt-style expression grammar.

use crate::grammar::{arrays, classes, functions, named, names, php85, strings, symbol, variables};
use crate::parser::core::Parser;
use crate::parser::precedence::{
    PREFIX_RIGHT_BP, PRINT_RIGHT_BP, TERNARY_BP, binary_operator, is_assignment_operator,
};
use crate::recovery;
use crate::{ParseDiagnosticId, SyntaxKind, SyntaxNodeKind};
use php_lexer::{TokenKind, TokenName};

/// Parses an expression using binding powers.
pub(crate) fn parse_expression(parser: &mut Parser<'_>) -> bool {
    parse_expression_bp(parser, 0)
}

fn parse_expression_bp(parser: &mut Parser<'_>, min_bp: u8) -> bool {
    if at_expression_stop(parser) {
        return false;
    }

    let expr = parser.start();
    bump_trivia(parser);
    if !parse_prefix_or_primary(parser) {
        parser.error_expected("expected expression", &["expression"]);
        recovery::recover_to_expression_boundary(parser);
        let _completed = expr.complete(parser, SyntaxKind::ERROR);
        return false;
    }

    let mut binary = false;
    let mut assignment = false;
    let mut pipe = false;
    let mut ternary = false;
    loop {
        bump_trivia(parser);
        if at_expression_stop(parser) {
            break;
        }

        if parser.at(symbol(b'(')) {
            parse_call_tail(parser);
            continue;
        }
        if parser.at(symbol(b'[')) {
            arrays::parse_dim_fetch_tail(parser, b'[', b']');
            continue;
        }
        if parser.at(symbol(b'{')) {
            arrays::parse_dim_fetch_tail(parser, b'{', b'}');
            continue;
        }
        if parser.at(named(TokenName::ObjectOperator))
            || parser.at(named(TokenName::NullsafeObjectOperator))
        {
            parse_property_fetch_tail(parser);
            continue;
        }
        if parser.at(named(TokenName::DoubleColon)) {
            parse_static_access_tail(parser);
            continue;
        }
        if parser.at(named(TokenName::Inc)) || parser.at(named(TokenName::Dec)) {
            parse_postfix_update_tail(parser);
            continue;
        }

        if parser.at(symbol(b'?')) {
            if TERNARY_BP < min_bp {
                break;
            }
            ternary = true;
            parser.bump();
            if !parser.at(symbol(b':')) && !parse_expression_bp(parser, 0) {
                parser.error_expected("expected expression after `?`", &["expression", ":"]);
            }
            if parser.at(symbol(b':')) {
                parser.bump();
                if !parse_expression_bp(parser, TERNARY_BP) {
                    parser.error_expected("expected expression after `:`", &["expression"]);
                }
            } else {
                parser.error_expected("expected `:` in ternary expression", &[":"]);
            }
            continue;
        }

        let Some(operator) = binary_operator(parser.current()) else {
            break;
        };
        if operator.left_bp < min_bp {
            break;
        }

        binary = true;
        assignment |= is_assignment_operator(parser.current());
        pipe |= php85::is_pipe_operator(parser.current());
        let is_plain_assignment = parser.at(symbol(b'='));
        parser.bump();
        if is_plain_assignment && has_by_ref_marker_after_trivia(parser) {
            bump_trivia(parser);
            parser.bump();
        }
        if !parse_expression_bp(parser, operator.right_bp) {
            parser.error_expected("expected expression after binary operator", &["expression"]);
            break;
        }
    }

    let kind = if ternary {
        SyntaxNodeKind::TernaryExpr
    } else if pipe {
        SyntaxNodeKind::PipeExpr
    } else if binary {
        if assignment {
            SyntaxNodeKind::AssignExpr
        } else {
            SyntaxNodeKind::BinaryExpr
        }
    } else {
        SyntaxNodeKind::Expr
    };
    let _completed = expr.complete(parser, SyntaxKind::Node(kind));
    true
}

fn has_by_ref_marker_after_trivia(parser: &Parser<'_>) -> bool {
    let mut index = 0;
    while parser.nth(index).is_trivia() {
        index += 1;
    }
    parser.nth(index) == named(TokenName::AmpersandFollowedByVarOrVararg)
        || parser.nth(index) == named(TokenName::AmpersandNotFollowedByVarOrVararg)
}

fn parse_prefix_or_primary(parser: &mut Parser<'_>) -> bool {
    bump_trivia(parser);
    if at_prefix_operator(parser) {
        let prefix = parser.start();
        let is_void_cast = php85::is_void_cast(parser.current());
        parser.bump();
        if !parse_expression_bp(parser, PREFIX_RIGHT_BP) {
            parser.error_expected("expected expression after prefix operator", &["expression"]);
        }
        let kind = if is_void_cast {
            SyntaxNodeKind::VoidCastExpr
        } else {
            SyntaxNodeKind::PrefixExpr
        };
        let _completed = prefix.complete(parser, SyntaxKind::Node(kind));
        return true;
    }

    parse_primary(parser)
}

fn parse_primary(parser: &mut Parser<'_>) -> bool {
    if parser.at(named(TokenName::Yield)) || parser.at(named(TokenName::YieldFrom)) {
        parse_yield_expression(parser);
        return true;
    }

    if parser.at(named(TokenName::New)) {
        parse_new_expression(parser);
        return true;
    }

    if php85::is_clone_keyword(parser.current()) {
        parse_clone_expression(parser);
        return true;
    }

    if parser.at(named(TokenName::Throw)) {
        parse_throw_expression(parser);
        return true;
    }

    if parser.at(named(TokenName::Match)) {
        parse_match_expression(parser);
        return true;
    }

    if at_construct_expression(parser) {
        parse_construct_expression(parser);
        return true;
    }

    if strings::parse_string_like(parser) {
        return true;
    }

    if arrays::parse_array_expression(parser) {
        return true;
    }

    if functions::parse_closure_expression(parser)
        || functions::parse_arrow_function_expression(parser)
    {
        return true;
    }

    if variables::parse_simple_variable(parser) {
        return true;
    }

    if at_literal(parser) {
        let literal = parser.start();
        parser.bump();
        let _completed = literal.complete(parser, SyntaxKind::Node(SyntaxNodeKind::Literal));
        return true;
    }

    if at_static_class_context_name(parser) {
        let name = parser.start();
        parser.bump();
        let _completed = name.complete(parser, SyntaxKind::Node(SyntaxNodeKind::Name));
        return true;
    }

    if at_name(parser) {
        let name = parser.start();
        parser.bump();
        let _completed = name.complete(parser, SyntaxKind::Node(SyntaxNodeKind::Name));
        return true;
    }

    if parser.at(symbol(b'(')) {
        let paren = parser.start();
        parser.bump();
        let _has_inner = parse_expression_bp(parser, 0);
        if parser.at(symbol(b')')) {
            parser.bump();
        } else {
            parser.error_expected("expected `)` after parenthesized expression", &[")"]);
        }
        let _completed =
            paren.complete(parser, SyntaxKind::Node(SyntaxNodeKind::ParenthesizedExpr));
        return true;
    }

    false
}

fn parse_yield_expression(parser: &mut Parser<'_>) {
    let yield_expr = parser.start();
    let is_yield_from = parser.at(named(TokenName::YieldFrom));
    parser.bump();

    if is_yield_from {
        if !parse_expression_bp(parser, 0) {
            parser.error_expected("expected expression after `yield from`", &["expression"]);
        }
    } else if !at_expression_stop(parser)
        && parse_expression_bp(parser, 0)
        && parser.at(named(TokenName::DoubleArrow))
    {
        parser.bump();
        if !parse_expression_bp(parser, 0) {
            parser.error_expected("expected yield value after `=>`", &["expression"]);
        }
    }

    let _completed = yield_expr.complete(parser, SyntaxKind::Node(SyntaxNodeKind::YieldExpr));
}

fn parse_throw_expression(parser: &mut Parser<'_>) {
    let throw_expr = parser.start();
    parser.bump();
    if !parse_expression_bp(parser, PREFIX_RIGHT_BP) {
        parser.error_expected("expected expression after `throw`", &["expression"]);
    }
    let _completed = throw_expr.complete(parser, SyntaxKind::Node(SyntaxNodeKind::ThrowExpr));
}

fn parse_match_expression(parser: &mut Parser<'_>) {
    let match_expr = parser.start();
    parser.bump();
    bump_trivia(parser);
    if parser.at(symbol(b'(')) {
        parser.bump();
        if !parse_expression_bp(parser, 0) {
            parser.error_expected("expected match subject expression", &["expression"]);
        }
        bump_trivia(parser);
        if parser.at(symbol(b')')) {
            parser.bump();
        } else {
            parser.error_expected("expected `)` after match subject", &[")"]);
        }
    } else {
        parser.error_expected("expected `(` after match", &["("]);
    }

    bump_trivia(parser);
    if parser.at(symbol(b'{')) {
        parser.bump();
        parse_match_arms(parser);
        if parser.at(symbol(b'}')) {
            parser.bump();
        } else {
            parser.error_expected("expected `}` after match arms", &["}"]);
        }
    } else {
        parser.error_expected("expected match arm list", &["{"]);
    }

    let _completed = match_expr.complete(parser, SyntaxKind::Node(SyntaxNodeKind::MatchExpr));
}

fn parse_match_arms(parser: &mut Parser<'_>) {
    loop {
        bump_trivia(parser);
        if parser.is_eof() || parser.at(symbol(b'}')) || parser.at(named(TokenName::CloseTag)) {
            break;
        }
        if parser.at(named(TokenName::Default)) {
            parser.bump();
        } else {
            if !parse_expression_bp(parser, 0) {
                parser.error_expected("expected match arm condition", &["expression", "default"]);
                recovery::recover_to_statement_boundary(parser);
                break;
            }
            while parser.at(symbol(b',')) {
                parser.bump();
                bump_trivia(parser);
                if parser.at(named(TokenName::DoubleArrow)) {
                    break;
                }
                if !parse_expression_bp(parser, 0) {
                    parser.error_expected("expected match arm condition", &["expression"]);
                    break;
                }
                bump_trivia(parser);
            }
        }

        bump_trivia(parser);
        if parser.at(named(TokenName::DoubleArrow)) {
            parser.bump();
        } else {
            parser.error_expected("expected `=>` in match arm", &["T_DOUBLE_ARROW"]);
            break;
        }

        if !parse_expression_bp(parser, 0) {
            parser.error_expected("expected match arm expression", &["expression"]);
            break;
        }

        bump_trivia(parser);
        if parser.at(symbol(b',')) {
            parser.bump();
        } else {
            break;
        }
    }
}

fn parse_construct_expression(parser: &mut Parser<'_>) {
    let construct = parser.start();
    let token = parser.current();
    parser.bump();
    bump_trivia(parser);

    if token == named(TokenName::Isset)
        || token == named(TokenName::Empty)
        || token == named(TokenName::Eval)
        || (token == named(TokenName::Exit) && parser.at(symbol(b'(')))
    {
        parse_balanced_parenthesized_construct(parser);
    } else if token == named(TokenName::Exit) {
        if !at_expression_stop(parser) {
            let _has_argument = parse_expression_bp(parser, PREFIX_RIGHT_BP);
        }
    } else if token == named(TokenName::Print) {
        if !parse_expression_bp(parser, PRINT_RIGHT_BP) {
            parser.error_expected("expected construct expression argument", &["expression"]);
        }
    } else if !parse_expression_bp(parser, PREFIX_RIGHT_BP) {
        parser.error_expected("expected construct expression argument", &["expression"]);
    }

    let _completed = construct.complete(parser, SyntaxKind::Node(SyntaxNodeKind::ConstructExpr));
}

fn parse_balanced_parenthesized_construct(parser: &mut Parser<'_>) {
    if !parser.at(symbol(b'(')) {
        parser.error_expected("expected construct argument list", &["("]);
        return;
    }

    parser.bump();
    let mut depth = 1usize;
    while !parser.is_eof() && depth > 0 {
        if parser.at(symbol(b'(')) {
            depth += 1;
            parser.bump();
        } else if parser.at(symbol(b')')) {
            depth -= 1;
            parser.bump();
        } else {
            parser.bump();
        }
    }
    if depth > 0 {
        parser.error_expected("expected `)` to close construct argument list", &[")"]);
    }
}

fn parse_new_expression(parser: &mut Parser<'_>) {
    let new_expr = parser.start();
    parser.bump();
    bump_trivia(parser);

    if parser.at(named(TokenName::Class)) {
        classes::parse_anonymous_class(parser);
    } else if names::parse_name(parser)
        || parse_static_class_name(parser)
        || parse_dynamic_new_class_reference(parser)
    {
        bump_trivia(parser);
        if parser.at(symbol(b'(')) {
            parse_call_tail(parser);
        }
    } else {
        parser.error_expected(
            "expected class name or anonymous class after `new`",
            &["class", "name"],
        );
    }

    let _completed = new_expr.complete(parser, SyntaxKind::Node(SyntaxNodeKind::NewExpr));
}

fn parse_dynamic_new_class_reference(parser: &mut Parser<'_>) -> bool {
    if !at_dynamic_new_class_reference_start(parser) {
        return false;
    }

    let expr = parser.start();
    if !parse_prefix_or_primary(parser) {
        let _completed = expr.complete(parser, SyntaxKind::ERROR);
        return false;
    }

    loop {
        bump_trivia(parser);
        if parser.at(symbol(b'(')) || at_expression_stop(parser) {
            break;
        }
        if parser.at(symbol(b'[')) {
            arrays::parse_dim_fetch_tail(parser, b'[', b']');
            continue;
        }
        if parser.at(symbol(b'{')) {
            arrays::parse_dim_fetch_tail(parser, b'{', b'}');
            continue;
        }
        if parser.at(named(TokenName::ObjectOperator))
            || parser.at(named(TokenName::NullsafeObjectOperator))
        {
            parse_property_fetch_tail(parser);
            continue;
        }
        if parser.at(named(TokenName::DoubleColon)) {
            parse_static_access_tail(parser);
            continue;
        }
        break;
    }

    let _completed = expr.complete(parser, SyntaxKind::Node(SyntaxNodeKind::Expr));
    true
}

fn at_dynamic_new_class_reference_start(parser: &Parser<'_>) -> bool {
    parser.at(named(TokenName::Variable)) || parser.at(symbol(b'('))
}

fn parse_static_class_name(parser: &mut Parser<'_>) -> bool {
    if !parser.at(named(TokenName::Static)) {
        return false;
    }
    let name = parser.start();
    parser.bump();
    let _completed = name.complete(parser, SyntaxKind::Node(SyntaxNodeKind::Name));
    true
}

fn parse_clone_expression(parser: &mut Parser<'_>) {
    let clone_expr = parser.start();
    parser.bump();
    bump_trivia(parser);

    if parser.at(symbol(b'(')) {
        parse_call_tail(parser);
        let _completed =
            clone_expr.complete(parser, SyntaxKind::Node(SyntaxNodeKind::CloneWithExpr));
    } else {
        if !parse_expression_bp(parser, PREFIX_RIGHT_BP) {
            parser.error_expected("expected expression after `clone`", &["expression"]);
        }
        let _completed = clone_expr.complete(parser, SyntaxKind::Node(SyntaxNodeKind::CloneExpr));
    }
}

fn parse_call_tail(parser: &mut Parser<'_>) {
    let call = parser.start();
    parser.bump();
    let mut saw_argument = false;

    while !parser.is_eof() && !parser.at(symbol(b')')) {
        bump_trivia(parser);
        if parser.at(symbol(b')')) {
            break;
        }
        if parser.at(symbol(b',')) {
            let (message, expected) = if saw_argument {
                (
                    "syntax error, unexpected token \",\", expecting \")\"",
                    &[")"][..],
                )
            } else {
                ("syntax error, unexpected token \",\"", &[][..])
            };
            parser.error_expected_with_id(ParseDiagnosticId::UnexpectedToken, message, expected);
            parser.bump();
            continue;
        }

        parse_argument(parser);
        saw_argument = true;

        bump_trivia(parser);
        if parser.at(symbol(b',')) {
            parser.bump();
        } else if !parser.at(symbol(b')')) {
            parser.error_expected("expected `,` or `)` in argument list", &[",", ")"]);
            recover_to_argument_boundary(parser);
            if parser.at(symbol(b',')) {
                parser.bump();
            }
        }
    }

    if parser.at(symbol(b')')) {
        parser.bump();
    } else {
        parser.error_expected("expected `)` to close argument list", &[")"]);
    }
    let _completed = call.complete(parser, SyntaxKind::Node(SyntaxNodeKind::CallExpr));
}

fn parse_argument(parser: &mut Parser<'_>) {
    if at_named_argument_label(parser) {
        parser.bump();
        bump_trivia(parser);
        parser.bump();
        bump_trivia(parser);
    }

    if parser.at(named(TokenName::Ellipsis)) {
        parser.bump();
        bump_trivia(parser);
        if parser.at(symbol(b')')) {
            return;
        }
    }

    if parser.at(named(TokenName::AmpersandFollowedByVarOrVararg))
        || parser.at(named(TokenName::AmpersandNotFollowedByVarOrVararg))
        || parser.at(symbol(b'&'))
    {
        parser.bump();
        bump_trivia(parser);
    }

    if !parse_expression_bp(parser, 0) {
        parser.error_expected("expected argument expression", &["expression"]);
        recover_to_argument_boundary(parser);
    }
}

fn parse_postfix_update_tail(parser: &mut Parser<'_>) {
    let postfix = parser.start();
    parser.bump();
    let _completed = postfix.complete(parser, SyntaxKind::Node(SyntaxNodeKind::PostfixExpr));
}

fn parse_property_fetch_tail(parser: &mut Parser<'_>) {
    let fetch = parser.start();
    parser.bump();
    if parser.at(named(TokenName::Variable)) || at_contextual_member_name(parser) {
        parser.bump();
    } else if parser.at(symbol(b'{')) {
        parser.bump();
        let _has_expr = parse_expression_bp(parser, 0);
        if parser.at(symbol(b'}')) {
            parser.bump();
        } else {
            parser.error_expected("expected `}` after property expression", &["}"]);
        }
    } else {
        parser.error_expected("expected property name", &["T_STRING", "T_VARIABLE", "{"]);
    }
    let _completed = fetch.complete(parser, SyntaxKind::Node(SyntaxNodeKind::PropertyFetchExpr));
}

fn parse_static_access_tail(parser: &mut Parser<'_>) {
    let access = parser.start();
    parser.bump();
    if parser.at(named(TokenName::Variable)) || at_contextual_member_name(parser) {
        parser.bump();
    } else {
        parser.error_expected(
            "expected static member name",
            &["T_STRING", "T_CLASS", "T_VARIABLE"],
        );
    }
    let _completed = access.complete(parser, SyntaxKind::Node(SyntaxNodeKind::StaticAccessExpr));
}

fn at_contextual_member_name(parser: &Parser<'_>) -> bool {
    parser.current_keyword_context().is_some()
}

fn at_expression_stop(parser: &Parser<'_>) -> bool {
    recovery::at_expr_recovery_token(parser) || parser.at(named(TokenName::Echo))
}

fn bump_trivia(parser: &mut Parser<'_>) {
    while parser.current().is_trivia() {
        parser.bump();
    }
}

fn at_prefix_operator(parser: &Parser<'_>) -> bool {
    parser.at(symbol(b'+'))
        || parser.at(symbol(b'-'))
        || parser.at(symbol(b'!'))
        || parser.at(symbol(b'~'))
        || parser.at(symbol(b'@'))
        || parser.at(named(TokenName::Inc))
        || parser.at(named(TokenName::Dec))
        || parser.at(named(TokenName::IntCast))
        || parser.at(named(TokenName::DoubleCast))
        || parser.at(named(TokenName::StringCast))
        || parser.at(named(TokenName::ArrayCast))
        || parser.at(named(TokenName::ObjectCast))
        || parser.at(named(TokenName::BoolCast))
        || parser.at(named(TokenName::UnsetCast))
        || parser.at(named(TokenName::VoidCast))
}

fn at_named_argument_label(parser: &Parser<'_>) -> bool {
    parser.at(named(TokenName::String)) && nth_non_trivia_is(parser, 1, symbol(b':'))
}

fn at_static_class_context_name(parser: &Parser<'_>) -> bool {
    parser.at(named(TokenName::Static))
        && nth_non_trivia_is(parser, 1, named(TokenName::DoubleColon))
}

fn recover_to_argument_boundary(parser: &mut Parser<'_>) {
    while !parser.is_eof()
        && !parser.at(symbol(b','))
        && !parser.at(symbol(b')'))
        && !parser.at(named(TokenName::CloseTag))
    {
        parser.bump();
    }
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

fn at_literal(parser: &Parser<'_>) -> bool {
    parser.at(named(TokenName::LNumber))
        || parser.at(named(TokenName::DNumber))
        || parser.at(named(TokenName::ConstantEncapsedString))
        || parser.at(named(TokenName::Line))
        || parser.at(named(TokenName::File))
        || parser.at(named(TokenName::Dir))
        || parser.at(named(TokenName::ClassC))
        || parser.at(named(TokenName::TraitC))
        || parser.at(named(TokenName::MethodC))
        || parser.at(named(TokenName::FuncC))
        || parser.at(named(TokenName::NamespaceC))
        || parser.at(named(TokenName::PropertyC))
        || matches!(
            parser.current(),
            SyntaxKind::Token(crate::SyntaxTokenKind::Named(TokenName::String))
        ) && matches!(
            parser.current_text().to_ascii_lowercase().as_str(),
            "null" | "true" | "false"
        )
}

fn at_name(parser: &Parser<'_>) -> bool {
    matches!(
        parser.current(),
        SyntaxKind::Token(crate::SyntaxTokenKind::Named(
            TokenName::String
                | TokenName::NameFullyQualified
                | TokenName::NameQualified
                | TokenName::NameRelative
        ))
    )
}

fn at_construct_expression(parser: &Parser<'_>) -> bool {
    parser.at(named(TokenName::Include))
        || parser.at(named(TokenName::IncludeOnce))
        || parser.at(named(TokenName::Require))
        || parser.at(named(TokenName::RequireOnce))
        || parser.at(named(TokenName::Print))
        || parser.at(named(TokenName::Isset))
        || parser.at(named(TokenName::Empty))
        || parser.at(named(TokenName::Eval))
        || parser.at(named(TokenName::Exit))
}

#[allow(dead_code)]
fn _token_kind_for_doc(kind: TokenKind) -> SyntaxKind {
    SyntaxKind::from_token_kind(kind)
}
