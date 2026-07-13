//! Lowering for `declare` directives and file-level metadata.

use crate::FrontendDatabase;
use crate::diagnostics::{DiagnosticId, DiagnosticPhase, DiagnosticSeverity, SemanticDiagnostic};
use crate::hir::{DeclareDirective, DeclareValue, HirDeclare, ModuleId};
use php_ast::{
    AstNode, AstToken, DeclareStmt, InlineHtmlStmt, StatementList, TokenView, descendant_nodes,
    descendant_tokens, syntax_child_nodes,
};
use php_source::TextRange;
use php_syntax::SyntaxNode;

/// Collects `declare` metadata and emits reference-safe diagnostics.
pub fn collect_declare_directives(
    source_file: &SyntaxNode,
    database: &mut FrontendDatabase,
    module_id: ModuleId,
) -> Vec<SemanticDiagnostic> {
    let first_statement = first_file_statement_span(source_file);
    let mut diagnostics = Vec::new();

    for declare in descendant_nodes::<DeclareStmt<'_>>(source_file) {
        let directives = parse_directives(declare.syntax());
        for directive in &directives {
            check_directive(
                directive,
                declare.text_range(),
                first_statement,
                &mut diagnostics,
            );
        }
        let hir_declare = HirDeclare::new(directives.clone(), declare.text_range());
        let module = database
            .module_mut(module_id)
            .expect("module allocated before declare lowering");
        for directive in directives {
            module.file_directives_mut().record(directive);
        }
        module.push_declare(hir_declare);
    }

    diagnostics
}

fn check_directive(
    directive: &DeclareDirective,
    declare_span: TextRange,
    first_statement: Option<TextRange>,
    diagnostics: &mut Vec<SemanticDiagnostic>,
) {
    if directive.canonical_name() != "strict_types" {
        return;
    }
    if !matches!(directive.value(), DeclareValue::Int(0 | 1)) {
        diagnostics.push(SemanticDiagnostic::with_span(
            DiagnosticId::InvalidStrictTypesDeclare,
            DiagnosticSeverity::Error,
            DiagnosticPhase::DeclarationCollection,
            "strict_types declaration must have 0 or 1 as its value",
            directive.value_span().unwrap_or_else(|| directive.span()),
        ));
    }
    if first_statement.is_some_and(|span| span != declare_span) {
        diagnostics.push(SemanticDiagnostic::with_span(
            DiagnosticId::StrictTypesDeclareNotFirst,
            DiagnosticSeverity::Error,
            DiagnosticPhase::DeclarationCollection,
            "strict_types declaration must be the first statement in the script",
            directive.span(),
        ));
    }
}

fn parse_directives(node: &SyntaxNode) -> Vec<DeclareDirective> {
    let tokens: Vec<_> = descendant_tokens::<TokenView<'_>>(node)
        .filter(|token| !token.kind().is_trivia())
        .skip_while(|token| token.text() != "(")
        .skip(1)
        .take_while(|token| token.text() != ")")
        .collect();
    let mut directives = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if token.kind().name() != "T_STRING" {
            index += 1;
            continue;
        }
        let name = token.text().to_owned();
        let name_span = token.text_range();
        index += 1;
        while index < tokens.len() && tokens[index].text() != "=" && tokens[index].text() != "," {
            if tokens[index].text() == ")" {
                break;
            }
            index += 1;
        }
        if index >= tokens.len() || tokens[index].text() != "=" {
            directives.push(DeclareDirective::new(
                name,
                DeclareValue::Unknown(String::new()),
                name_span,
                None,
            ));
            continue;
        }
        index += 1;
        let Some(value_token) = tokens.get(index).copied() else {
            directives.push(DeclareDirective::new(
                name,
                DeclareValue::Unknown(String::new()),
                name_span,
                None,
            ));
            break;
        };
        let value = declare_value(&value_token);
        let span = TextRange::new(
            name_span.start().to_usize(),
            value_token.text_range().end().to_usize(),
        );
        directives.push(DeclareDirective::new(
            name,
            value,
            span,
            Some(value_token.text_range()),
        ));
        index += 1;
    }
    directives
}

fn declare_value(token: &TokenView<'_>) -> DeclareValue {
    match token.kind().name().as_str() {
        "T_LNUMBER" => token
            .text()
            .parse::<i64>()
            .map(DeclareValue::Int)
            .unwrap_or_else(|_| DeclareValue::Unknown(token.text().to_owned())),
        "T_CONSTANT_ENCAPSED_STRING" => DeclareValue::String(unquote_string(token.text())),
        _ => DeclareValue::Unknown(token.text().to_owned()),
    }
}

fn unquote_string(text: &str) -> String {
    let bytes = text.as_bytes();
    let quote_start = if matches!(bytes, [b'b' | b'B', b'\'' | b'"', ..]) {
        1
    } else {
        0
    };
    if text.len() >= quote_start + 2
        && ((bytes[quote_start] == b'"' && bytes[text.len() - 1] == b'"')
            || (bytes[quote_start] == b'\'' && bytes[text.len() - 1] == b'\''))
    {
        return text[quote_start + 1..text.len() - 1].to_owned();
    }
    text.to_owned()
}

fn first_file_statement_span(source_file: &SyntaxNode) -> Option<TextRange> {
    for child in syntax_child_nodes(source_file) {
        if InlineHtmlStmt::cast(child).is_some() {
            return Some(child.text_range());
        }
        if child.kind().name() == "PHP_BLOCK" {
            for block_child in syntax_child_nodes(child) {
                if StatementList::cast(block_child).is_none() {
                    continue;
                }
                if let Some(statement) = syntax_child_nodes(block_child).next() {
                    return Some(statement.text_range());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::collect_declare_directives;
    use crate::FrontendDatabase;
    use crate::diagnostics::DiagnosticId;
    use crate::hir::{DeclareValue, HirModule};
    use php_ast::{AstNode, source_file};
    use php_syntax::parse_source_file;

    #[test]
    fn records_file_directives_and_declares() {
        let parse =
            parse_source_file("<?php declare(strict_types=1, ticks=100, encoding=\"UTF-8\");\n");
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 64));

        let diagnostics = collect_declare_directives(root.syntax(), &mut database, module_id);

        assert!(diagnostics.is_empty());
        let module = database.module(module_id).expect("module");
        assert_eq!(module.declares().len(), 1);
        assert!(matches!(
            module
                .file_directives()
                .strict_types()
                .map(|directive| directive.value()),
            Some(DeclareValue::Int(1))
        ));
        assert!(module.file_directives().ticks().is_some());
        assert!(module.file_directives().encoding().is_some());
    }

    #[test]
    fn diagnoses_invalid_or_late_strict_types() {
        let parse = parse_source_file("<?php echo 1; declare(strict_types=2);\n");
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 39));

        let diagnostics = collect_declare_directives(root.syntax(), &mut database, module_id);

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.id() == DiagnosticId::InvalidStrictTypesDeclare })
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.id() == DiagnosticId::StrictTypesDeclareNotFirst })
        );
    }
}
