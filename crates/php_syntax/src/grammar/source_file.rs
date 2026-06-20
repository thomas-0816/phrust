//! Top-level source-file grammar.

use crate::grammar::{named, statements};
use crate::parser::core::Parser;
use crate::{SyntaxKind, SyntaxNodeKind};
use php_lexer::TokenName;

/// Parses a complete PHP source file.
pub(crate) fn parse(parser: &mut Parser<'_>) {
    let source_file = parser.start();

    while !parser.is_eof() {
        if parser.at(named(TokenName::InlineHtml)) {
            parse_inline_html(parser);
        } else if parser.at(named(TokenName::OpenTag))
            || parser.at(named(TokenName::OpenTagWithEcho))
        {
            parse_php_block(parser);
        } else {
            let error = parser.start();
            parser.error_and_bump("inline HTML or PHP open tag");
            let _completed = error.complete(parser, SyntaxKind::ERROR);
        }
    }

    let _completed = source_file.complete(parser, SyntaxKind::SOURCE_FILE);
}

fn parse_inline_html(parser: &mut Parser<'_>) {
    let node = parser.start();
    while parser.at(named(TokenName::InlineHtml)) {
        parser.bump();
    }
    let _completed = node.complete(parser, SyntaxKind::Node(SyntaxNodeKind::InlineHtml));
}

fn parse_php_block(parser: &mut Parser<'_>) {
    let php_block = parser.start();
    let is_short_echo = parser.at(named(TokenName::OpenTagWithEcho));
    parser.bump();

    let statement_list = parser.start();
    if is_short_echo {
        statements::parse_short_echo_statement(parser);
    } else {
        statements::parse_statement_list_contents(parser);
    }
    let _completed =
        statement_list.complete(parser, SyntaxKind::Node(SyntaxNodeKind::StatementList));

    if parser.at(named(TokenName::CloseTag)) {
        parser.bump();
    }

    let _completed = php_block.complete(parser, SyntaxKind::Node(SyntaxNodeKind::PhpBlock));
}

#[cfg(test)]
mod tests {
    use crate::parse_source_file;

    #[test]
    fn parses_html_and_multiple_php_blocks() {
        let source = "<h1>x</h1><?php echo 1; ?>tail<?php echo 2; ?>";
        let parse = parse_source_file(source);
        let debug = parse.debug_tree();

        assert_eq!(parse.reconstructed_text(), source);
        assert!(debug.contains("INLINE_HTML"));
        assert_eq!(debug.matches("PHP_BLOCK").count(), 2);
        assert_eq!(debug.matches("STATEMENT_LIST").count(), 2);
    }

    #[test]
    fn short_echo_has_echo_statement_shape() {
        let source = "<?= $value ?>";
        let parse = parse_source_file(source);
        let debug = parse.debug_tree();

        assert_eq!(parse.reconstructed_text(), source);
        assert!(debug.contains("ECHO_STMT"));
        assert!(debug.contains("T_OPEN_TAG_WITH_ECHO"));
    }
}
