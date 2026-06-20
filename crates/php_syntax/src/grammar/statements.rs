//! Statement-list grammar with recovery-oriented generic statements.

use crate::grammar::{declarations, expressions, named, symbol};
use crate::parser::core::Parser;
use crate::recovery;
use crate::{SyntaxKind, SyntaxNodeKind};
use php_lexer::TokenName;

/// Parses statements until the end of the current PHP block.
pub(crate) fn parse_statement_list_contents(parser: &mut Parser<'_>) {
    while !parser.is_eof() && !parser.at(named(TokenName::CloseTag)) && !parser.at(symbol(b'}')) {
        if parser.current().is_trivia() {
            parser.bump();
        } else {
            parse_statement(parser);
        }
    }
}

/// Parses the synthetic statement introduced by `<?=`.
pub(crate) fn parse_short_echo_statement(parser: &mut Parser<'_>) {
    let statement = parser.start();
    if !expressions::parse_expression(parser) {
        parser.error_expected("expected expression after short echo tag", &["expression"]);
    }
    let _completed = statement.complete(parser, SyntaxKind::Node(SyntaxNodeKind::EchoStmt));
}

fn parse_statement(parser: &mut Parser<'_>) {
    if parser.at(symbol(b';')) {
        let statement = parser.start();
        parser.bump();
        let _completed = statement.complete(parser, SyntaxKind::Node(SyntaxNodeKind::EmptyStmt));
    } else if parser.at(named(TokenName::Echo)) {
        parse_echo_statement(parser);
    } else if parser.at(named(TokenName::If)) {
        parse_if_statement(parser);
    } else if parser.at(named(TokenName::While)) {
        parse_while_statement(parser);
    } else if parser.at(named(TokenName::Do)) {
        parse_do_while_statement(parser);
    } else if parser.at(named(TokenName::For)) {
        parse_loop_statement(
            parser,
            TokenName::For,
            TokenName::EndFor,
            SyntaxNodeKind::ForStmt,
        );
    } else if parser.at(named(TokenName::Foreach)) {
        parse_loop_statement(
            parser,
            TokenName::Foreach,
            TokenName::EndForeach,
            SyntaxNodeKind::ForeachStmt,
        );
    } else if parser.at(named(TokenName::Switch)) {
        parse_switch_statement(parser);
    } else if parser.at(named(TokenName::Break)) {
        parse_optional_expression_statement(parser, SyntaxNodeKind::BreakStmt);
    } else if parser.at(named(TokenName::Continue)) {
        parse_optional_expression_statement(parser, SyntaxNodeKind::ContinueStmt);
    } else if parser.at(named(TokenName::Return)) {
        parse_optional_expression_statement(parser, SyntaxNodeKind::ReturnStmt);
    } else if parser.at(named(TokenName::Throw)) {
        parse_required_expression_statement(parser, SyntaxNodeKind::ThrowStmt);
    } else if parser.at(named(TokenName::Try)) {
        parse_try_statement(parser);
    } else if parser.at(named(TokenName::Declare)) {
        parse_declare_statement(parser);
    } else if parser.at(named(TokenName::Global)) {
        parse_misc_until_semicolon_statement(parser, SyntaxNodeKind::GlobalStmt);
    } else if parser.at(named(TokenName::Static)) {
        parse_misc_until_semicolon_statement(parser, SyntaxNodeKind::StaticStmt);
    } else if parser.at(named(TokenName::Unset)) {
        parse_parenthesized_construct_statement(parser, SyntaxNodeKind::UnsetStmt);
    } else if parser.at(named(TokenName::Goto)) {
        parse_misc_until_semicolon_statement(parser, SyntaxNodeKind::GotoStmt);
    } else if at_label_statement(parser) {
        parse_label_statement(parser);
    } else if declarations::at_attributed_declaration(parser) {
        declarations::parse_attributed_declaration(parser);
    } else if parser.at(named(TokenName::Namespace)) {
        declarations::parse_namespace_declaration(parser);
    } else if parser.at(named(TokenName::Use)) {
        declarations::parse_use_declaration(parser);
    } else if parser.at(named(TokenName::Function)) {
        declarations::parse_function_declaration(parser);
    } else if crate::grammar::classes::at_class_like_declaration(parser) {
        declarations::parse_class_like_declaration(parser);
    } else if parser.at(symbol(b'{')) {
        parse_block_statement(parser);
    } else {
        parse_expression_statement(parser);
    }
}

fn parse_echo_statement(parser: &mut Parser<'_>) {
    let statement = parser.start();
    parser.bump();
    if !expressions::parse_expression(parser) {
        parser.error_expected("expected expression after echo", &["expression"]);
    }
    while parser.at(symbol(b',')) {
        parser.bump();
        if !expressions::parse_expression(parser) {
            parser.error_expected("expected expression after comma", &["expression"]);
            break;
        }
    }
    consume_statement_terminator(parser);
    let _completed = statement.complete(parser, SyntaxKind::Node(SyntaxNodeKind::EchoStmt));
}

fn parse_if_statement(parser: &mut Parser<'_>) {
    let statement = parser.start();
    parser.bump();
    parse_parenthesized_header(parser, "if condition");

    bump_trivia(parser);
    if parser.at(symbol(b':')) {
        parser.bump();
        parse_statements_until(parser, at_if_alternative_boundary);
        while parser.at(named(TokenName::ElseIf)) {
            parser.bump();
            parse_parenthesized_header(parser, "elseif condition");
            bump_trivia(parser);
            if parser.at(symbol(b':')) {
                parser.bump();
            } else {
                parser.error_expected("expected `:` after elseif condition", &[":"]);
            }
            parse_statements_until(parser, at_if_alternative_boundary);
        }
        if parser.at(named(TokenName::Else)) {
            parser.bump();
            if parser.at(symbol(b':')) {
                parser.bump();
            } else {
                parser.error_expected("expected `:` after else", &[":"]);
            }
            parse_statements_until(parser, at_if_end);
        }
        consume_end_keyword(parser, TokenName::EndIf, "expected `endif`");
        consume_statement_terminator(parser);
    } else {
        parse_control_body(parser);
        bump_trivia(parser);
        while parser.at(named(TokenName::ElseIf)) {
            parser.bump();
            parse_parenthesized_header(parser, "elseif condition");
            parse_control_body(parser);
            bump_trivia(parser);
        }
        if parser.at(named(TokenName::Else)) {
            parser.bump();
            parse_control_body(parser);
        }
    }

    let _completed = statement.complete(parser, SyntaxKind::Node(SyntaxNodeKind::IfStmt));
}

fn parse_while_statement(parser: &mut Parser<'_>) {
    parse_loop_statement(
        parser,
        TokenName::While,
        TokenName::EndWhile,
        SyntaxNodeKind::WhileStmt,
    );
}

fn parse_loop_statement(
    parser: &mut Parser<'_>,
    start_keyword: TokenName,
    end_keyword: TokenName,
    kind: SyntaxNodeKind,
) {
    let statement = parser.start();
    parser.bump();
    let label = match start_keyword {
        TokenName::For => "for header",
        TokenName::Foreach => "foreach header",
        TokenName::While => "while condition",
        _ => "loop header",
    };
    parse_parenthesized_header(parser, label);

    bump_trivia(parser);
    if parser.at(symbol(b':')) {
        parser.bump();
        parse_statements_until(parser, |parser| parser.at(named(end_keyword)));
        consume_end_keyword(parser, end_keyword, "expected loop end keyword");
        consume_statement_terminator(parser);
    } else {
        parse_control_body(parser);
    }

    let _completed = statement.complete(parser, SyntaxKind::Node(kind));
}

fn parse_do_while_statement(parser: &mut Parser<'_>) {
    let statement = parser.start();
    parser.bump();
    parse_control_body(parser);
    bump_trivia(parser);
    if parser.at(named(TokenName::While)) {
        parser.bump();
        parse_parenthesized_header(parser, "do-while condition");
        consume_statement_terminator(parser);
    } else {
        parser.error_expected("expected `while` after do body", &["T_WHILE"]);
        recovery::recover_to_statement_boundary(parser);
    }
    let _completed = statement.complete(parser, SyntaxKind::Node(SyntaxNodeKind::DoWhileStmt));
}

fn parse_switch_statement(parser: &mut Parser<'_>) {
    let statement = parser.start();
    parser.bump();
    parse_parenthesized_header(parser, "switch expression");

    bump_trivia(parser);
    if parser.at(symbol(b':')) {
        parser.bump();
        parse_switch_body(parser, at_switch_alternative_end);
        consume_end_keyword(parser, TokenName::EndSwitch, "expected `endswitch`");
        consume_statement_terminator(parser);
    } else if parser.at(symbol(b'{')) {
        parser.bump();
        parse_switch_body(parser, |parser| parser.at(symbol(b'}')));
        if parser.at(symbol(b'}')) {
            parser.bump();
        } else {
            parser.error_expected("expected `}` to close switch", &["}"]);
        }
    } else {
        parser.error_expected("expected switch body", &["{", ":"]);
        recovery::recover_to_statement_boundary(parser);
    }

    let _completed = statement.complete(parser, SyntaxKind::Node(SyntaxNodeKind::SwitchStmt));
}

fn parse_try_statement(parser: &mut Parser<'_>) {
    let statement = parser.start();
    parser.bump();
    parse_required_block(parser, "expected try block");

    let mut has_handler = false;
    loop {
        bump_trivia(parser);
        if parser.at(named(TokenName::Catch)) {
            parse_catch_clause(parser);
            has_handler = true;
        } else {
            break;
        }
    }

    bump_trivia(parser);
    if parser.at(named(TokenName::Finally)) {
        let finally = parser.start();
        parser.bump();
        parse_required_block(parser, "expected finally block");
        let _completed = finally.complete(parser, SyntaxKind::Node(SyntaxNodeKind::FinallyClause));
        has_handler = true;
    }

    if !has_handler {
        parser.error_expected(
            "expected catch or finally after try block",
            &["catch", "finally"],
        );
    }

    let _completed = statement.complete(parser, SyntaxKind::Node(SyntaxNodeKind::TryStmt));
}

fn parse_catch_clause(parser: &mut Parser<'_>) {
    let catch = parser.start();
    parser.bump();
    parse_parenthesized_header(parser, "catch clause");
    parse_required_block(parser, "expected catch block");
    let _completed = catch.complete(parser, SyntaxKind::Node(SyntaxNodeKind::CatchClause));
}

fn parse_declare_statement(parser: &mut Parser<'_>) {
    let statement = parser.start();
    parser.bump();
    parse_parenthesized_header(parser, "declare directive list");

    bump_trivia(parser);
    if parser.at(symbol(b';')) {
        parser.bump();
    } else if parser.at(symbol(b'{')) {
        parse_block_statement(parser);
    } else if parser.at(symbol(b':')) {
        parser.bump();
        parse_statements_until(parser, |parser| parser.at(named(TokenName::EndDeclare)));
        consume_end_keyword(parser, TokenName::EndDeclare, "expected `enddeclare`");
        consume_statement_terminator(parser);
    } else {
        parser.error_expected("expected declare body or terminator", &[";", "{", ":"]);
        recovery::recover_to_statement_boundary(parser);
    }

    let _completed = statement.complete(parser, SyntaxKind::Node(SyntaxNodeKind::DeclareStmt));
}

fn parse_misc_until_semicolon_statement(parser: &mut Parser<'_>, kind: SyntaxNodeKind) {
    let statement = parser.start();
    parser.bump();
    while !parser.is_eof()
        && !parser.at(symbol(b';'))
        && !parser.at(symbol(b'}'))
        && !parser.at(named(TokenName::CloseTag))
    {
        parser.bump();
    }
    consume_statement_terminator(parser);
    let _completed = statement.complete(parser, SyntaxKind::Node(kind));
}

fn parse_parenthesized_construct_statement(parser: &mut Parser<'_>, kind: SyntaxNodeKind) {
    let statement = parser.start();
    parser.bump();
    parse_parenthesized_header(parser, "statement argument list");
    consume_statement_terminator(parser);
    let _completed = statement.complete(parser, SyntaxKind::Node(kind));
}

fn parse_label_statement(parser: &mut Parser<'_>) {
    let statement = parser.start();
    parser.bump();
    parser.bump();
    let _completed = statement.complete(parser, SyntaxKind::Node(SyntaxNodeKind::LabelStmt));
}

fn parse_switch_body(parser: &mut Parser<'_>, at_end: impl Fn(&Parser<'_>) -> bool + Copy) {
    while !parser.is_eof() && !at_end(parser) && !parser.at(named(TokenName::CloseTag)) {
        if parser.current().is_trivia() || parser.at(symbol(b';')) {
            parser.bump();
        } else if parser.at(named(TokenName::Case)) || parser.at(named(TokenName::Default)) {
            parse_switch_label(parser);
            parse_statements_until(parser, |parser| {
                parser.at(named(TokenName::Case))
                    || parser.at(named(TokenName::Default))
                    || at_end(parser)
            });
        } else {
            parser.error_expected("expected switch case or default", &["case", "default"]);
            parse_statement(parser);
        }
    }
}

fn parse_switch_label(parser: &mut Parser<'_>) {
    if parser.at(named(TokenName::Case)) {
        parser.bump();
        bump_trivia(parser);
        if !expressions::parse_expression(parser) {
            parser.error_expected("expected case expression", &["expression"]);
        }
    } else if parser.at(named(TokenName::Default)) {
        parser.bump();
    }

    bump_trivia(parser);
    if parser.at(symbol(b':')) || parser.at(symbol(b';')) {
        parser.bump();
    } else {
        parser.error_expected("expected switch label separator", &[":", ";"]);
    }
}

fn parse_optional_expression_statement(parser: &mut Parser<'_>, kind: SyntaxNodeKind) {
    let statement = parser.start();
    parser.bump();
    bump_trivia(parser);
    if !at_simple_statement_end(parser) {
        let _has_expression = expressions::parse_expression(parser);
    }
    consume_statement_terminator(parser);
    let _completed = statement.complete(parser, SyntaxKind::Node(kind));
}

fn parse_required_expression_statement(parser: &mut Parser<'_>, kind: SyntaxNodeKind) {
    let statement = parser.start();
    parser.bump();
    bump_trivia(parser);
    if !expressions::parse_expression(parser) {
        parser.error_expected("expected expression", &["expression"]);
    }
    consume_statement_terminator(parser);
    let _completed = statement.complete(parser, SyntaxKind::Node(kind));
}

fn parse_expression_statement(parser: &mut Parser<'_>) {
    let statement = parser.start();
    if !expressions::parse_expression(parser) {
        parser.error_expected("expected expression statement", &["expression"]);
        recovery::recover_to_statement_boundary(parser);
    }
    consume_statement_terminator(parser);
    let _completed = statement.complete(parser, SyntaxKind::Node(SyntaxNodeKind::ExprStmt));
}

pub(crate) fn parse_block_statement(parser: &mut Parser<'_>) {
    let statement = parser.start();
    parser.bump();
    while !parser.is_eof() && !parser.at(symbol(b'}')) && !parser.at(named(TokenName::CloseTag)) {
        if parser.current().is_trivia() {
            parser.bump();
        } else {
            parse_statement(parser);
        }
    }
    if parser.at(symbol(b'}')) {
        parser.bump();
    } else {
        parser.error_expected("expected `}` to close block statement", &["}"]);
    }
    let _completed = statement.complete(parser, SyntaxKind::Node(SyntaxNodeKind::BlockStmt));
}

fn parse_required_block(parser: &mut Parser<'_>, message: &str) {
    bump_trivia(parser);
    if parser.at(symbol(b'{')) {
        parse_block_statement(parser);
    } else {
        parser.error_expected(message, &["{"]);
        recovery::recover_to_statement_boundary(parser);
    }
}

fn parse_control_body(parser: &mut Parser<'_>) {
    if parser.current().is_trivia() {
        while parser.current().is_trivia() {
            parser.bump();
        }
    }
    if parser.is_eof() || parser.at(named(TokenName::CloseTag)) {
        parser.error_expected("expected control-flow body", &["statement"]);
    } else {
        parse_statement(parser);
    }
}

fn parse_parenthesized_header(parser: &mut Parser<'_>, context: &str) {
    bump_trivia(parser);
    if !parser.at(symbol(b'(')) {
        parser.error_expected(format!("expected `(` to start {context}"), &["("]);
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
        parser.error_expected(format!("expected `)` to close {context}"), &[")"]);
    }
}

fn parse_statements_until(parser: &mut Parser<'_>, at_end: impl Fn(&Parser<'_>) -> bool + Copy) {
    while !parser.is_eof() && !parser.at(named(TokenName::CloseTag)) && !at_end(parser) {
        if parser.current().is_trivia() {
            parser.bump();
        } else {
            parse_statement(parser);
        }
    }
}

fn consume_end_keyword(parser: &mut Parser<'_>, keyword: TokenName, message: &str) {
    bump_trivia(parser);
    if parser.at(named(keyword)) {
        parser.bump();
    } else {
        parser.error_expected(message, &[keyword.as_php_name()]);
    }
}

fn at_if_alternative_boundary(parser: &Parser<'_>) -> bool {
    parser.at(named(TokenName::ElseIf)) || parser.at(named(TokenName::Else)) || at_if_end(parser)
}

fn at_if_end(parser: &Parser<'_>) -> bool {
    parser.at(named(TokenName::EndIf))
}

fn at_switch_alternative_end(parser: &Parser<'_>) -> bool {
    parser.at(named(TokenName::EndSwitch))
}

fn at_simple_statement_end(parser: &Parser<'_>) -> bool {
    parser.at(symbol(b';')) || parser.at(named(TokenName::CloseTag)) || parser.at(symbol(b'}'))
}

fn at_label_statement(parser: &Parser<'_>) -> bool {
    parser.at(named(TokenName::String)) && parser.nth(1) == symbol(b':')
}

fn bump_trivia(parser: &mut Parser<'_>) {
    while parser.current().is_trivia() {
        parser.bump();
    }
}

fn consume_statement_terminator(parser: &mut Parser<'_>) {
    if parser.at(symbol(b';')) {
        parser.bump();
    } else if parser.at(named(TokenName::CloseTag)) {
        // PHP close tags terminate the current statement without consuming the tag.
    } else {
        parser.error_expected(
            "expected statement terminator",
            recovery::STATEMENT_TERMINATORS,
        );
        recovery::recover_to_statement_boundary(parser);
        if parser.at(symbol(b';')) {
            parser.bump();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parse_source_file;

    #[test]
    fn missing_semicolon_emits_expected_terminators() {
        let parse = parse_source_file("<?php echo 1");

        assert!(parse.has_errors());
        let diagnostic = parse.diagnostics().first().expect("diagnostic");
        assert!(diagnostic.expected.contains(&";".to_owned()));
        assert!(diagnostic.expected.contains(&"T_CLOSE_TAG".to_owned()));
        assert_eq!(parse.reconstructed_text(), "<?php echo 1");
    }

    #[test]
    fn statement_recovery_makes_progress_to_next_boundary() {
        let source = "<?php echo 1 echo 2; echo 3;";
        let parse = parse_source_file(source);

        assert_eq!(parse.reconstructed_text(), source);
        assert!(parse.debug_tree().contains("ECHO_STMT"));
    }
}
