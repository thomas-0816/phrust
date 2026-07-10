//! Class-context validation that can be checked without autoloading.

use crate::diagnostics::{DiagnosticId, DiagnosticPhase, DiagnosticSeverity, SemanticDiagnostic};
use crate::hir::MagicMethodKind;
use php_ast::{
    ArrowFunctionExpr, AstNode, AstToken, ClassDecl, ClosureExpr, EnumDecl, FunctionDecl,
    InterfaceDecl, MethodDecl, Param, TokenView, TraitDecl, descendant_tokens, modifier_tokens,
    syntax_child_nodes, syntax_child_tokens,
};
use php_source::TextRange;
use php_syntax::{SyntaxNode, SyntaxToken};

/// Returns class-context diagnostics for one source file.
#[must_use]
pub fn check_source_file(source_file: &SyntaxNode) -> Vec<SemanticDiagnostic> {
    let mut checker = ClassContextChecker::default();
    checker.visit(source_file, None);
    checker.diagnostics
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ClassContext {
    parent_allowed: bool,
}

#[derive(Default)]
struct ClassContextChecker {
    diagnostics: Vec<SemanticDiagnostic>,
}

impl ClassContextChecker {
    fn visit(&mut self, node: &SyntaxNode, context: Option<ClassContext>) {
        if let Some(class_decl) = ClassDecl::cast(node) {
            let parent_allowed = class_decl_has_extends(class_decl.syntax());
            self.visit_children(class_decl.syntax(), Some(ClassContext { parent_allowed }));
            return;
        }
        if let Some(interface_decl) = InterfaceDecl::cast(node) {
            self.visit_children(
                interface_decl.syntax(),
                Some(ClassContext {
                    parent_allowed: false,
                }),
            );
            return;
        }
        if let Some(trait_decl) = TraitDecl::cast(node) {
            self.visit_children(
                trait_decl.syntax(),
                Some(ClassContext {
                    // PHP lint allows `parent::` in traits because the
                    // consuming class determines the actual parent relation.
                    parent_allowed: true,
                }),
            );
            return;
        }
        if let Some(enum_decl) = EnumDecl::cast(node) {
            self.visit_children(
                enum_decl.syntax(),
                Some(ClassContext {
                    parent_allowed: false,
                }),
            );
            return;
        }
        if let Some(method) = MethodDecl::cast(node) {
            self.check_magic_method(method);
        }
        if ClosureExpr::cast(node).is_some() || ArrowFunctionExpr::cast(node).is_some() {
            // The reference engine compiles `self`/`static`/`parent` inside
            // closure and arrow-function bodies regardless of the lexical
            // scope: `Closure::bind` can supply or replace the class scope,
            // so the check is deferred to invocation time.
            self.visit_children(
                node,
                Some(ClassContext {
                    parent_allowed: true,
                }),
            );
            return;
        }
        if FunctionDecl::cast(node).is_some() {
            // A named function body is global scope even when its
            // declaration is nested inside a class member or closure.
            self.visit_children(node, None);
            return;
        }

        self.check_context_tokens(node, context);
        self.visit_children(node, context);
    }

    fn visit_children(&mut self, node: &SyntaxNode, context: Option<ClassContext>) {
        for child in syntax_child_nodes(node) {
            self.visit(child, context);
        }
    }

    fn check_context_tokens(&mut self, node: &SyntaxNode, context: Option<ClassContext>) {
        for token in syntax_child_tokens(node).filter(|token| !token.kind().is_trivia()) {
            match token.text().to_ascii_lowercase().as_str() {
                "self" if is_contextual_class_keyword_token(node, token) => {
                    self.check_self_or_static(token, context, "self");
                }
                "parent" if is_contextual_class_keyword_token(node, token) => {
                    self.check_parent(token, context);
                }
                "static" if is_contextual_static_token(node, token) => {
                    self.check_self_or_static(token, context, "static");
                }
                _ if token.kind().name() == "T_VARIABLE" && token.text() == "$this" => {
                    self.check_this_token(node, token);
                }
                _ => {}
            }
        }
    }

    fn check_self_or_static(
        &mut self,
        token: &SyntaxToken,
        context: Option<ClassContext>,
        keyword: &'static str,
    ) {
        if context.is_none() {
            self.error(
                DiagnosticId::InvalidClassContextName,
                format!("cannot use `{keyword}` when no class scope is active"),
                token.text_range(),
            );
        }
    }

    fn check_parent(&mut self, token: &SyntaxToken, context: Option<ClassContext>) {
        match context {
            None => self.error(
                DiagnosticId::InvalidClassContextName,
                "cannot use `parent` when no class scope is active",
                token.text_range(),
            ),
            Some(context) if !context.parent_allowed => self.error(
                DiagnosticId::InvalidClassContextName,
                "cannot use `parent` when current class scope has no parent",
                token.text_range(),
            ),
            Some(_) => {}
        }
    }

    fn check_this_token(&mut self, _node: &SyntaxNode, _token: &SyntaxToken) {
        // PHP 8.5 lint accepts `$this` inside static methods and static
        // closures; the runtime may still fail. object-runtime keeps this as a
        // documented deferred runtime check rather than emitting diagnostics.
    }

    fn check_magic_method(&mut self, method: MethodDecl<'_>) {
        let Some(name) = method.name_text() else {
            return;
        };
        let Some(kind) = MagicMethodKind::from_name(name) else {
            return;
        };
        let is_static = has_modifier(method.syntax(), "T_STATIC");
        let param_count = direct_param_count(method.syntax());

        if let Some(expected) = magic_method_required_param_count(kind)
            && param_count != expected
        {
            self.error(
                DiagnosticId::InvalidMagicMethodSignature,
                format!(
                    "magic method `{}` must declare exactly {expected} parameter(s)",
                    kind.as_str()
                ),
                method.text_range(),
            );
        }

        match kind {
            MagicMethodKind::CallStatic | MagicMethodKind::SetState if !is_static => self.error(
                DiagnosticId::InvalidMagicMethodSignature,
                format!("magic method `{}` must be static", kind.as_str()),
                method.text_range(),
            ),
            MagicMethodKind::CallStatic | MagicMethodKind::SetState => {}
            _ if is_static => self.error(
                DiagnosticId::InvalidMagicMethodSignature,
                format!("magic method `{}` cannot be static", kind.as_str()),
                method.text_range(),
            ),
            _ => {}
        }
    }

    fn error(&mut self, id: DiagnosticId, message: impl Into<String>, span: TextRange) {
        self.diagnostics.push(SemanticDiagnostic::with_span(
            id,
            DiagnosticSeverity::Error,
            DiagnosticPhase::ClassLikeValidation,
            message,
            span,
        ));
    }
}

fn class_decl_has_extends(node: &SyntaxNode) -> bool {
    syntax_child_tokens(node).any(|token| token.kind().name() == "T_EXTENDS")
}

fn has_modifier(node: &SyntaxNode, kind: &str) -> bool {
    modifier_tokens(node).any(|token| token.kind().name() == kind)
}

fn direct_param_count(node: &SyntaxNode) -> usize {
    syntax_child_nodes(node)
        .find(|child| child.kind().name() == "PARAM_LIST")
        .map(|param_list| {
            syntax_child_nodes(param_list)
                .filter(|child| Param::cast(child).is_some())
                .count()
        })
        .unwrap_or(0)
}

fn magic_method_required_param_count(kind: MagicMethodKind) -> Option<usize> {
    match kind {
        MagicMethodKind::Construct | MagicMethodKind::Invoke => None,
        MagicMethodKind::Destruct
        | MagicMethodKind::Sleep
        | MagicMethodKind::Wakeup
        | MagicMethodKind::Serialize
        | MagicMethodKind::ToString
        | MagicMethodKind::Clone
        | MagicMethodKind::DebugInfo => Some(0),
        MagicMethodKind::Get
        | MagicMethodKind::Isset
        | MagicMethodKind::Unset
        | MagicMethodKind::Unserialize
        | MagicMethodKind::SetState => Some(1),
        MagicMethodKind::Call | MagicMethodKind::CallStatic | MagicMethodKind::Set => Some(2),
    }
}

fn is_contextual_static_token(node: &SyntaxNode, token: &SyntaxToken) -> bool {
    if is_static_modifier_or_statement(node) {
        return next_significant_token_text(node, token.text_range().end().to_usize())
            .as_deref()
            .is_some_and(|text| text == "::");
    }
    if node.kind().name() == "ERROR" {
        return true;
    }
    let previous = previous_significant_token_text(node, token.text_range().start().to_usize());
    let next = next_significant_token_text(node, token.text_range().end().to_usize());
    matches!(node.kind().name().as_str(), "NAME" | "TYPE")
        || previous.as_deref().is_some_and(|text| text == "new")
        || next.as_deref().is_some_and(|text| text == "::")
}

fn is_contextual_class_keyword_token(node: &SyntaxNode, token: &SyntaxToken) -> bool {
    let next = next_significant_token_text(node, token.text_range().end().to_usize());
    matches!(node.kind().name().as_str(), "NAME" | "TYPE")
        || next.as_deref().is_some_and(|text| text == "::")
}

fn is_static_modifier_or_statement(node: &SyntaxNode) -> bool {
    matches!(
        node.kind().name().as_str(),
        "STATIC_STMT" | "METHOD_DECL" | "PROPERTY_DECL"
    ) || ClosureExpr::cast(node).is_some()
        || ArrowFunctionExpr::cast(node).is_some()
}

fn previous_significant_token_text(node: &SyntaxNode, offset: usize) -> Option<String> {
    descendant_tokens::<TokenView<'_>>(node)
        .filter(|token| {
            !token.kind().is_trivia() && token.syntax().text_range().end().to_usize() <= offset
        })
        .last()
        .map(|token| token.text().to_owned())
}

fn next_significant_token_text(node: &SyntaxNode, offset: usize) -> Option<String> {
    descendant_tokens::<TokenView<'_>>(node)
        .filter(|token| {
            !token.kind().is_trivia() && token.syntax().text_range().start().to_usize() >= offset
        })
        .find(|token| token.syntax().text_range().start().to_usize() > offset)
        .map(|token| token.text().to_owned())
}

#[cfg(test)]
mod tests {
    use super::check_source_file;
    use crate::DiagnosticId;
    use php_ast::{AstNode, source_file};
    use php_syntax::parse_source_file;

    fn diagnostics(source: &str) -> Vec<crate::SemanticDiagnostic> {
        let parse = parse_source_file(source);
        let root = source_file(parse.root()).expect("source file");
        check_source_file(root.syntax())
    }

    #[test]
    fn diagnoses_context_keywords_outside_class_scope() {
        let diagnostics = diagnostics("<?php function f(){ self::m(); static::m(); }\n");
        assert_eq!(diagnostics.len(), 2);
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| { diagnostic.id() == DiagnosticId::InvalidClassContextName })
        );
    }

    #[test]
    fn diagnoses_parent_without_parent_but_allows_traits() {
        let diagnostics = diagnostics(
            "<?php class C { function f(){ parent::f(); } } trait T { function f(){ parent::f(); } }\n",
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].id(), DiagnosticId::InvalidClassContextName);
    }

    #[test]
    fn defers_context_keywords_inside_closures_and_arrow_functions() {
        // Reference-confirmed (PHP 8.5.7): closure and arrow-function bodies
        // compile with self/static/parent in any lexical scope; the scope is
        // resolved when the closure is invoked or rebound.
        let diagnostics = diagnostics(
            "<?php $a = function () { return self::class; }; $b = fn () => static::class; $c = static function () { return parent::class; }; class C { function f() { return function () { return parent::class; }; } }\n",
        );
        assert_eq!(diagnostics.len(), 0, "{diagnostics:?}");
    }

    #[test]
    fn named_function_bodies_stay_global_scope_inside_closures() {
        let diagnostics =
            diagnostics("<?php $a = function () { function g() { return self::class; } };\n");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].id(), DiagnosticId::InvalidClassContextName);
    }

    #[test]
    fn diagnoses_reference_confirmed_magic_method_rules() {
        let diagnostics = diagnostics(
            "<?php class C { public static function __toString(): string { return ''; } public function __call($name): mixed {} }\n",
        );
        assert_eq!(diagnostics.len(), 2);
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| { diagnostic.id() == DiagnosticId::InvalidMagicMethodSignature })
        );
    }
}
