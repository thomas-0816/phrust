use crate::SyntaxKind;
use crate::diagnostics::{ParseDiagnostic, ParseDiagnosticId};
use crate::parser::core::Parser;
use crate::syntax_node::{SyntaxElement, SyntaxNode, SyntaxToken};
use crate::token_source::TokenSource;
use crate::tree_sink::TreeSink;
use php_lexer::{LexerConfig, lex_all};
use php_source::SourceText;

/// Optional caller-owned source identity carried by parse results.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SourceId(String);

impl SourceId {
    /// Creates a source identity from a stable caller-owned string.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the source identity string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Parser context that can carry optional host metadata without global state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ParseContext {
    source_id: Option<SourceId>,
}

impl ParseContext {
    /// Creates an empty parser context.
    #[must_use]
    pub const fn new() -> Self {
        Self { source_id: None }
    }

    /// Returns a context with a caller-owned source identity.
    #[must_use]
    pub fn with_source_id(mut self, source_id: SourceId) -> Self {
        self.source_id = Some(source_id);
        self
    }

    /// Returns the optional source identity.
    #[must_use]
    pub const fn source_id(&self) -> Option<&SourceId> {
        self.source_id.as_ref()
    }
}

/// Result of parsing a PHP source file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parse {
    context: ParseContext,
    root: SyntaxNode,
    diagnostics: Vec<ParseDiagnostic>,
    reconstructed_text: String,
}

impl Parse {
    /// Returns parser diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[ParseDiagnostic] {
        &self.diagnostics
    }

    /// Returns parser diagnostics as structured diagnostic envelopes.
    #[must_use]
    pub fn diagnostic_envelopes(
        &self,
        source: Option<&SourceText>,
        path: Option<&str>,
    ) -> Vec<php_diagnostics::DiagnosticEnvelope> {
        let source_id = self.source_id().map(SourceId::as_str);
        self.diagnostics
            .iter()
            .map(|diagnostic| diagnostic.to_diagnostic_envelope(source, source_id, path))
            .collect()
    }

    /// Returns the parser context metadata carried by this result.
    #[must_use]
    pub const fn context(&self) -> &ParseContext {
        &self.context
    }

    /// Returns the optional source identity.
    #[must_use]
    pub const fn source_id(&self) -> Option<&SourceId> {
        self.context.source_id()
    }

    /// Returns true when any diagnostics were emitted.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    /// Returns the root CST node.
    #[must_use]
    pub const fn root(&self) -> &SyntaxNode {
        &self.root
    }

    /// Returns the root CST kind.
    #[must_use]
    pub const fn root_kind(&self) -> SyntaxKind {
        *self.root.kind()
    }

    /// Returns a stable debug representation of the current CST.
    #[must_use]
    pub fn debug_tree(&self) -> String {
        let mut out = String::new();
        render_node(&mut out, &self.root, 0);
        out
    }

    /// Reconstructs source from CST token leaves.
    #[must_use]
    pub fn reconstructed_text(&self) -> &str {
        &self.reconstructed_text
    }
}

/// Parses a PHP source file.
#[must_use]
pub fn parse_source_file(source: &str) -> Parse {
    parse_source_file_with_context(source, ParseContext::default())
}

/// Parses an owned source wrapper without coupling the parser to source storage.
#[must_use]
pub fn parse_source_text(source: &SourceText) -> Parse {
    parse_source_file(source.as_str())
}

/// Parses a PHP source file with optional caller metadata.
#[must_use]
pub fn parse_source_file_with_context(source: &str, context: ParseContext) -> Parse {
    let lexed = lex_all(
        source,
        LexerConfig {
            short_open_tag: false,
            token_parse: false,
            emit_eof: false,
        },
    );

    let lexer_diagnostics: Vec<_> = lexed
        .diagnostics
        .into_iter()
        .map(|diagnostic| {
            ParseDiagnostic::new(
                ParseDiagnosticId::LexerDiagnostic,
                diagnostic.message,
                diagnostic.span,
            )
        })
        .collect();

    let mut parser = Parser::new(TokenSource::new(source, lexed.tokens));
    for diagnostic in lexer_diagnostics {
        parser.error(diagnostic);
    }
    parser.parse_source_file();
    let (events, tokens, diagnostics) = parser.finish();
    let root = TreeSink::new(source, tokens).finish(events);
    let reconstructed_text = reconstruct_text(&root);

    Parse {
        context,
        root,
        diagnostics,
        reconstructed_text,
    }
}

fn render_node(out: &mut String, node: &SyntaxNode, indent: usize) {
    out.push_str(&"  ".repeat(indent));
    out.push_str(&node.kind().name());
    out.push_str(&format!(
        " @{}..{}\n",
        node.range().start().to_usize(),
        node.range().end().to_usize()
    ));

    for child in node.children() {
        match child {
            SyntaxElement::Node(node) => render_node(out, node, indent + 1),
            SyntaxElement::Token(token) => render_token(out, token, indent + 1),
        }
    }
}

fn render_token(out: &mut String, token: &SyntaxToken, indent: usize) {
    out.push_str(&"  ".repeat(indent));
    out.push_str(&token.kind().name());
    out.push_str(&format!(
        " @{}..{} line={} text=\"{}\"\n",
        token.range().start().to_usize(),
        token.range().end().to_usize(),
        token.line(),
        escape_debug_text(token.text())
    ));
}

fn escape_debug_text(text: &str) -> String {
    text.escape_default().to_string()
}

fn reconstruct_text(node: &SyntaxNode) -> String {
    let mut out = String::new();
    push_reconstructed_text(&mut out, node);
    out
}

fn push_reconstructed_text(out: &mut String, node: &SyntaxNode) {
    for child in node.children() {
        match child {
            SyntaxElement::Node(node) => push_reconstructed_text(out, node),
            SyntaxElement::Token(token) => out.push_str(token.text()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ParseContext, SourceId, parse_source_file, parse_source_file_with_context};
    use crate::ParseDiagnosticId;
    use php_source::SourceText;
    use serde_json::Value;

    #[test]
    fn stub_parser_roundtrips_valid_source() {
        let source = "<?php echo 1;\n";
        let parse = parse_source_file(source);
        assert!(!parse.has_errors());
        assert_eq!(parse.reconstructed_text(), source);
        assert_eq!(parse.root_kind(), crate::SyntaxKind::SOURCE_FILE);
        assert!(parse.debug_tree().contains("SOURCE_FILE"));
    }

    #[test]
    fn stub_parser_does_not_panic_on_invalid_input() {
        let source = "<?php \"unterminated";
        let parse = parse_source_file(source);
        assert!(parse.has_errors());
        assert_eq!(parse.reconstructed_text(), source);
    }

    #[test]
    fn roundtrips_inline_html_only() {
        assert_roundtrip("<h1>Hello</h1>\n<?= $name ?>");
    }

    #[test]
    fn roundtrips_simple_php_file() {
        assert_roundtrip("<?php echo 1;");
    }

    #[test]
    fn roundtrips_whitespace_and_comment() {
        assert_roundtrip("<?php\n// comment\n  echo 1; /* trailing */\n");
    }

    #[test]
    fn roundtrips_close_and_reopen_tags() {
        assert_roundtrip("<?php echo 1; ?>\nhtml\n<?php echo 2; ?>");
    }

    #[test]
    fn parse_context_carries_optional_source_id() {
        let parse = parse_source_file_with_context(
            "<?php echo 1;",
            ParseContext::new().with_source_id(SourceId::new("fixtures/example.php")),
        );

        assert_eq!(
            parse.source_id().map(SourceId::as_str),
            Some("fixtures/example.php")
        );
        assert_eq!(parse.reconstructed_text(), "<?php echo 1;");
    }

    #[test]
    fn reparsing_changed_text_keeps_lossless_properties() {
        let first = parse_source_file("<?php echo 1;");
        let second = parse_source_file("<?php echo 12;");

        assert_eq!(first.reconstructed_text(), "<?php echo 1;");
        assert_eq!(second.reconstructed_text(), "<?php echo 12;");
        assert_ne!(first.debug_tree(), second.debug_tree());
        assert_eq!(
            second.root().range().end().to_usize(),
            "<?php echo 12;".len()
        );
    }

    #[test]
    fn parses_static_class_context_forms_without_recovery() {
        let source = "<?php class B {} class C extends B { public function f(): static { static::make(); return new static(); } }";
        let parse = parse_source_file(source);
        let debug = parse.debug_tree();

        assert!(
            !parse.has_errors(),
            "unexpected parser diagnostics: {:?}",
            parse.diagnostics()
        );
        assert!(debug.contains("STATIC_ACCESS_EXPR"));
        assert!(debug.contains("NEW_EXPR"));
        assert_eq!(parse.reconstructed_text(), source);
    }

    #[test]
    fn parses_dynamic_new_class_references_without_recovery() {
        let source = "<?php $arr = [new stdClass, 'stdClass']; new $arr[0](); new $arr[1]();";
        let parse = parse_source_file(source);
        let debug = parse.debug_tree();

        assert!(
            !parse.has_errors(),
            "unexpected parser diagnostics: {:?}",
            parse.diagnostics()
        );
        assert_eq!(debug.matches("NEW_EXPR").count(), 3);
        assert!(debug.contains("ARRAY_DIM_FETCH_EXPR"));
        assert_eq!(parse.reconstructed_text(), source);
    }

    #[test]
    fn parses_keyword_named_arguments_without_recovery() {
        let source = "<?php preg_grep(pattern: '', array: []); f(match: 1, print: 2);";
        let parse = parse_source_file(source);

        assert!(
            !parse.has_errors(),
            "unexpected parser diagnostics: {:?}",
            parse.diagnostics()
        );
        assert_eq!(parse.reconstructed_text(), source);
    }

    #[test]
    fn print_construct_keeps_concat_operand() {
        let source = "<?php print(bin2hex($char)).\" => \".bin2hex($char).\"\\n\";";
        let parse = parse_source_file(source);
        let debug = parse.debug_tree();

        assert!(
            !parse.has_errors(),
            "unexpected parser diagnostics: {:?}",
            parse.diagnostics()
        );
        assert!(debug.contains("CONSTRUCT_EXPR"));
        assert!(debug.contains("BINARY_EXPR"));
        assert_eq!(parse.reconstructed_text(), source);
    }

    #[test]
    fn require_construct_keeps_concat_operand() {
        let source = "<?php require __DIR__ . '/wp-blog-header.php';";
        let parse = parse_source_file(source);
        let debug = parse.debug_tree();

        assert!(
            !parse.has_errors(),
            "unexpected parser diagnostics: {:?}",
            parse.diagnostics()
        );
        assert!(debug.contains("CONSTRUCT_EXPR"));
        assert!(debug.contains("BINARY_EXPR"));
        assert!(debug.contains("T_REQUIRE"));
        assert!(debug.contains("T_DIR"));
        assert!(debug.contains("T_CONSTANT_ENCAPSED_STRING"));
        assert_eq!(parse.reconstructed_text(), source);
    }

    #[test]
    fn nested_statement_lists_accept_inline_html_segments() {
        let source = "<?php if (false) { ?>\n<p>install check</p><?php echo 1; ?>\n<?php }";
        let parse = parse_source_file(source);
        let debug = parse.debug_tree();

        assert!(
            !parse.has_errors(),
            "unexpected parser diagnostics: {:?}",
            parse.diagnostics()
        );
        assert_eq!(parse.reconstructed_text(), source);
        assert!(debug.contains("IF_STMT"));
        assert!(debug.contains("BLOCK_STMT"));
        assert!(debug.contains("INLINE_HTML"));
        assert!(debug.contains("ECHO_STMT"));
        assert_eq!(debug.matches("PHP_BLOCK").count(), 1);
    }

    #[test]
    fn source_file_grammar_models_php_and_html_segments() {
        let source = "<p>A</p><?php echo 1; ?><?= $b ?><p>C</p>";
        let parse = parse_source_file(source);
        let debug = parse.debug_tree();

        assert_eq!(parse.reconstructed_text(), source);
        assert!(debug.contains("INLINE_HTML"));
        assert_eq!(debug.matches("PHP_BLOCK").count(), 2);
        assert_eq!(debug.matches("ECHO_STMT").count(), 2);
    }

    #[test]
    fn recovery_diagnostics_have_stable_ids_and_expected_syntax() {
        let cases = [
            (
                "<?php echo ;",
                ParseDiagnosticId::ExpectedExpression,
                "expression",
            ),
            (
                "<?php function f(): | {}",
                ParseDiagnosticId::ExpectedType,
                "type",
            ),
            (
                "<?php function () {}",
                ParseDiagnosticId::ExpectedIdentifier,
                "T_STRING",
            ),
            (
                "<?php echo (1 + 2;",
                ParseDiagnosticId::UnclosedDelimiter,
                ")",
            ),
            ("<?php echo 1", ParseDiagnosticId::ExpectedToken, ";"),
        ];

        for (source, id, expected) in cases {
            let parse = parse_source_file(source);
            let diagnostic = parse.diagnostics().first().expect(source);

            assert_eq!(diagnostic.id, id, "{source}");
            assert!(diagnostic.span.start() <= diagnostic.span.end(), "{source}");
            assert!(
                diagnostic.expected.contains(&expected.to_owned()),
                "{source}: {:?}",
                diagnostic.expected
            );
        }
    }

    #[test]
    fn parser_unexpected_token_diagnostic_has_structured_envelope() {
        let source = SourceText::new("<?php function () {}");
        let parse = parse_source_file_with_context(
            source.as_str(),
            ParseContext::new().with_source_id(SourceId::new("unit-test-source")),
        );
        let diagnostic = parse
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.id == ParseDiagnosticId::ExpectedIdentifier)
            .expect("expected identifier diagnostic");
        let envelope = diagnostic.to_diagnostic_envelope(
            Some(&source),
            parse.source_id().map(SourceId::as_str),
            Some("parse.php"),
        );
        let json = envelope.compact_json().expect("diagnostic json renders");
        let decoded: Value = serde_json::from_str(&json).expect("valid diagnostic json");

        assert_eq!(decoded["code"], "E_PHP_PARSE_EXPECTED_IDENTIFIER");
        assert_eq!(decoded["legacy_id"], "expected_identifier");
        assert_eq!(decoded["layer"], "parser");
        assert_eq!(decoded["phase"], "parse");
        assert_eq!(decoded["location"]["path"], "parse.php");
        assert_eq!(decoded["location"]["source_id"], "unit-test-source");
        assert_eq!(decoded["context"]["expected"], "T_STRING");
        assert_eq!(decoded["suggestion"], "insert a valid identifier or name");
    }

    #[test]
    fn parse_result_exports_diagnostic_envelopes_with_source_id() {
        let source = SourceText::new("<?php echo ;");
        let parse = parse_source_file_with_context(
            source.as_str(),
            ParseContext::new().with_source_id(SourceId::new("envelope-source")),
        );
        let envelopes = parse.diagnostic_envelopes(Some(&source), Some("echo.php"));

        assert_eq!(envelopes.len(), parse.diagnostics().len());
        assert_eq!(envelopes[0].code, "E_PHP_PARSE_EXPECTED_EXPRESSION");
        assert_eq!(
            envelopes[0]
                .location
                .as_ref()
                .and_then(|location| location.source_id.as_deref()),
            Some("envelope-source")
        );
        assert!(envelopes[0].text_line().contains("expected=expression"));
    }

    #[test]
    fn recovery_error_nodes_cover_faulty_tokens() {
        let source = "<?php echo ;";
        let parse = parse_source_file(source);
        let debug = parse.debug_tree();

        assert!(parse.has_errors());
        assert_eq!(parse.reconstructed_text(), source);
        assert!(debug.contains("ERROR @11..12"));
    }

    #[test]
    fn parser_does_not_hang_on_recovery_smoke_inputs() {
        let cases = [
            "",
            "\0\0<?php echo @@@ ;",
            "<?php #[,] function f(, $x = ];",
            "<?php class C { public function f(#[A(] $x): ?| {}",
            "<?php match ($x) { 1 => , default => 2,",
        ];

        for source in cases {
            let parse = parse_source_file(source);
            assert_eq!(parse.reconstructed_text(), source);
            assert!(
                parse.debug_tree().starts_with("SOURCE_FILE @0.."),
                "{source}"
            );
        }
    }

    fn assert_roundtrip(source: &str) {
        let parse = parse_source_file(source);
        assert_eq!(parse.reconstructed_text(), source);
        assert_eq!(parse.root_kind(), crate::SyntaxKind::SOURCE_FILE);
        assert!(parse.debug_tree().starts_with("SOURCE_FILE @0.."));
    }
}
