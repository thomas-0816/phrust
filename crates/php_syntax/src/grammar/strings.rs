//! String, encapsed string, heredoc, and nowdoc syntax.

use crate::grammar::{named, symbol};
use crate::parser::core::Parser;
use crate::{SyntaxKind, SyntaxNodeKind};
use php_lexer::TokenName;

/// Parses any string-like expression token sequence.
pub(crate) fn parse_string_like(parser: &mut Parser<'_>) -> bool {
    if parser.at(named(TokenName::ConstantEncapsedString)) {
        let string = parser.start();
        parser.bump();
        let _completed = string.complete(parser, SyntaxKind::Node(SyntaxNodeKind::String));
        return true;
    }

    if parser.at(symbol(b'"')) {
        parse_encapsed_string(parser, b'"');
        return true;
    }

    if parser.at(symbol(b'`')) {
        parse_encapsed_string(parser, b'`');
        return true;
    }

    if parser.at(named(TokenName::StartHeredoc)) {
        parse_heredoc(parser);
        return true;
    }

    false
}

fn parse_encapsed_string(parser: &mut Parser<'_>, delimiter: u8) {
    let encapsed = parser.start();
    parser.bump();
    while !parser.is_eof() && !parser.at(symbol(delimiter)) {
        parse_encapsed_part(parser);
    }
    if parser.at(symbol(delimiter)) {
        parser.bump();
    } else {
        parser.error_expected("expected string delimiter", &[delimiter_name(delimiter)]);
    }
    let _completed = encapsed.complete(parser, SyntaxKind::Node(SyntaxNodeKind::Encapsed));
}

fn parse_heredoc(parser: &mut Parser<'_>) {
    let heredoc = parser.start();
    parser.bump();
    while !parser.is_eof() && !parser.at(named(TokenName::EndHeredoc)) {
        parse_encapsed_part(parser);
    }
    if parser.at(named(TokenName::EndHeredoc)) {
        parser.bump();
    } else {
        parser.error_expected("expected heredoc terminator", &["T_END_HEREDOC"]);
    }
    let _completed = heredoc.complete(parser, SyntaxKind::Node(SyntaxNodeKind::Heredoc));
}

fn parse_encapsed_part(parser: &mut Parser<'_>) {
    if parser.at(named(TokenName::Variable)) {
        parse_simple_interpolation(parser);
    } else if parser.at(named(TokenName::CurlyOpen))
        || parser.at(named(TokenName::DollarOpenCurlyBraces))
    {
        parse_complex_interpolation(parser);
    } else {
        parser.bump();
    }
}

fn parse_simple_interpolation(parser: &mut Parser<'_>) {
    let variable = parser.start();
    parser.bump();
    loop {
        if parser.at(symbol(b'[')) {
            consume_string_offset(parser);
        } else if parser.at(named(TokenName::ObjectOperator)) {
            parser.bump();
            if parser.at(named(TokenName::String)) {
                parser.bump();
            } else {
                parser.error_expected("expected interpolated property name", &["T_STRING"]);
                break;
            }
        } else {
            break;
        }
    }
    let _completed = variable.complete(parser, SyntaxKind::Node(SyntaxNodeKind::Variable));
}

fn parse_complex_interpolation(parser: &mut Parser<'_>) {
    let expression = parser.start();
    parser.bump();
    while !parser.is_eof() && !parser.at(symbol(b'}')) {
        parser.bump();
    }
    if parser.at(symbol(b'}')) {
        parser.bump();
    } else {
        parser.error_expected("expected `}` after interpolation", &["}"]);
    }
    let _completed = expression.complete(parser, SyntaxKind::Node(SyntaxNodeKind::Expr));
}

fn consume_string_offset(parser: &mut Parser<'_>) {
    parser.bump();
    while !parser.is_eof() {
        if parser.at(symbol(b']')) {
            parser.bump();
            return;
        }
        if parser.at(named(TokenName::EncapsedAndWhitespace)) && parser.current_text().contains(']')
        {
            parser.bump();
            return;
        }
        if parser.at(symbol(b'"'))
            || parser.at(symbol(b'`'))
            || parser.at(named(TokenName::EndHeredoc))
        {
            break;
        } else {
            parser.bump();
        }
    }
    parser.error_expected("expected interpolation offset close delimiter", &["]"]);
}

const fn delimiter_name(delimiter: u8) -> &'static str {
    match delimiter {
        b'"' => "\"",
        b'`' => "`",
        _ => "delimiter",
    }
}
