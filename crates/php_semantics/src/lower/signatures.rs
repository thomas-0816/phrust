//! Function-like signature lowering and validation.

use std::collections::HashSet;

use crate::FrontendDatabase;
use crate::diagnostics::{
    DiagnosticId, DiagnosticPhase, DiagnosticReporter, DiagnosticSeverity, SemanticDiagnostic,
};
use crate::hir::{
    DefaultValueRef, FunctionLikeFlags, FunctionSignature, ModuleId, Parameter, ParameterAttribute,
    ParameterFlags, PromotedPropertyInfo, ReturnType, SignatureKind, TypeContext, TypeId,
    Visibility,
};
use crate::lower::types::{
    TypeLoweringScope, TypeToken, is_type_atom_token, lower_type, lower_type_tokens,
    non_empty_type_tokens, token_range, type_token_from_syntax_token,
};
use php_ast::{
    ArrowFunctionExpr, AstNode, AstToken, AttributeGroup, BlockStmt, CatchClause, ClassDecl,
    ClosureExpr, EnumDecl, FunctionDecl, InterfaceDecl, MethodDecl, Param, ParamList, Stmt,
    TokenView, TraitDecl, TypeView, YieldExpr, descendant_tokens, syntax_child_nodes,
    syntax_child_tokens,
};
use php_source::TextRange;
use php_syntax::{SyntaxNode, SyntaxToken};

/// Collects lowered function-like signatures in one top-level node.
pub fn collect_signatures_in_node(
    node: &SyntaxNode,
    database: &mut FrontendDatabase,
    module_id: ModuleId,
    reporter: &mut DiagnosticReporter,
    scope: TypeLoweringScope,
) {
    let mut lowerer = SignatureLowerer {
        database,
        module_id,
        reporter,
        scope,
        class_stack: Vec::new(),
    };
    lowerer.collect_node(node);
}

struct SignatureLowerer<'a> {
    database: &'a mut FrontendDatabase,
    module_id: ModuleId,
    reporter: &'a mut DiagnosticReporter,
    scope: TypeLoweringScope,
    class_stack: Vec<ClassContext>,
}

impl SignatureLowerer<'_> {
    fn collect_node(&mut self, node: &SyntaxNode) {
        if TypeView::cast(node).is_some() {
            return;
        }
        if Param::cast(node).is_some() || CatchClause::cast(node).is_some() {
            return;
        }

        if let Some(function) = FunctionDecl::cast(node) {
            self.collect_structured_function_like(
                function.syntax(),
                SignatureKind::Function,
                named_decl_name(function.syntax()),
            );
        } else if let Some(closure) = ClosureExpr::cast(node) {
            self.collect_structured_function_like(closure.syntax(), SignatureKind::Closure, None);
        } else if let Some(arrow) = ArrowFunctionExpr::cast(node) {
            self.collect_structured_function_like(
                arrow.syntax(),
                SignatureKind::ArrowFunction,
                None,
            );
        } else if let Some(method) = MethodDecl::cast(node) {
            self.collect_raw_method(method);
        } else if let Some(class_decl) = ClassDecl::cast(node) {
            self.with_class_context(ClassContainerKind::Class, class_decl.syntax());
        } else if let Some(interface_decl) = InterfaceDecl::cast(node) {
            self.with_class_context(ClassContainerKind::Interface, interface_decl.syntax());
        } else if let Some(trait_decl) = TraitDecl::cast(node) {
            self.with_class_context(ClassContainerKind::Trait, trait_decl.syntax());
        } else if let Some(enum_decl) = EnumDecl::cast(node) {
            self.with_class_context(ClassContainerKind::Enum, enum_decl.syntax());
        } else {
            self.collect_children(node);
        }
    }

    fn with_class_context(&mut self, kind: ClassContainerKind, node: &SyntaxNode) {
        self.class_stack.push(ClassContext { kind });
        self.collect_children(node);
        self.class_stack.pop();
    }

    fn collect_children(&mut self, node: &SyntaxNode) {
        for child in syntax_child_nodes(node) {
            self.collect_node(child);
        }
    }

    fn collect_structured_function_like(
        &mut self,
        node: &SyntaxNode,
        kind: SignatureKind,
        name: Option<String>,
    ) {
        let mut parameters = Vec::new();
        let mut closure_use = Vec::new();
        let mut return_type = None;

        for child in syntax_child_nodes(node) {
            if let Some(param_list) = ParamList::cast(child) {
                if param_list_is_closure_use(param_list) {
                    closure_use = closure_use_names(param_list);
                } else {
                    parameters = self.collect_structured_parameters(param_list);
                }
            } else if let Some(type_view) = TypeView::cast(child) {
                let type_id = lower_type(
                    type_view,
                    self.database,
                    self.module_id,
                    self.reporter,
                    self.scope.clone(),
                    TypeContext::Return,
                    self.in_class_like_context(),
                );
                return_type =
                    type_id.map(|id| ReturnType::new(id, type_view.syntax().text_range()));
            }
        }

        self.validate_parameters(&parameters);
        self.validate_closure_use(&parameters, &closure_use);
        let body = if kind == SignatureKind::Function {
            direct_function_body_statements(self.database, self.module_id, node)
        } else {
            Vec::new()
        };

        let mut flags = FunctionLikeFlags::new(by_ref_return(node));
        flags.set_static(is_static_function_like(node));
        flags.set_generator(contains_direct_function_like_yield(node));
        flags.set_return_type_void(return_type_has_token(node, "T_VOID"));
        flags.set_return_type_never(return_type_has_token(node, "T_NEVER"));
        flags.set_tentative_or_deferred_info(flags.is_generator());
        flags.set_this_available(false);

        let signature = FunctionSignature::new(
            kind,
            name,
            parameters,
            return_type,
            by_ref_return(node),
            node.text_range(),
        )
        .with_flags(flags)
        .with_body(body)
        .with_arrow_body(
            (kind == SignatureKind::ArrowFunction)
                .then(|| arrow_body_span(node))
                .flatten(),
        );
        self.push_signature(signature);

        for child in syntax_child_nodes(node) {
            if ParamList::cast(child).is_none() && TypeView::cast(child).is_none() {
                self.collect_node(child);
            }
        }
    }

    fn collect_structured_parameters(&mut self, param_list: ParamList<'_>) -> Vec<Parameter> {
        syntax_child_nodes(param_list.syntax())
            .filter_map(Param::cast)
            .filter_map(|param| self.lower_structured_param(param))
            .collect()
    }

    fn lower_structured_param(&mut self, param: Param<'_>) -> Option<Parameter> {
        let variable = syntax_child_tokens(param.syntax())
            .find(|token| token.kind().name() == "T_VARIABLE")?;
        let name = variable.text().to_owned();
        let type_id = self.structured_param_type(param);
        let by_ref = syntax_child_tokens(param.syntax()).any(is_ampersand_token);
        let variadic = syntax_child_tokens(param.syntax()).any(|token| token.text() == "...");
        let promoted_property = promoted_property_info_from_tokens(
            direct_significant_tokens(param.syntax()).as_slice(),
        );
        let default = default_value_ref(param.syntax(), true);
        let attributes = syntax_child_nodes(param.syntax())
            .filter_map(AttributeGroup::cast)
            .map(|attribute| ParameterAttribute::new(attribute.text_range()))
            .collect();
        Some(Parameter::new(
            name,
            type_id,
            ParameterFlags::new(by_ref, variadic, promoted_property),
            default,
            attributes,
            param.text_range(),
        ))
    }

    fn structured_param_type(&mut self, param: Param<'_>) -> Option<TypeId> {
        for child in syntax_child_nodes(param.syntax()) {
            if let Some(type_view) = TypeView::cast(child) {
                return lower_type(
                    type_view,
                    self.database,
                    self.module_id,
                    self.reporter,
                    self.scope.clone(),
                    TypeContext::Parameter,
                    self.in_class_like_context(),
                );
            }
        }
        let tokens = parameter_type_tokens(param.syntax())?;
        lower_type_tokens(
            &tokens,
            self.database,
            self.module_id,
            self.reporter,
            self.scope.clone(),
            TypeContext::Parameter,
            self.in_class_like_context(),
        )
    }

    fn collect_raw_method(&mut self, method: MethodDecl<'_>) {
        let tokens = method_signature_significant_tokens(method.syntax());
        let name = raw_method_name(&tokens);
        let is_constructor = name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("__construct"));
        let is_abstract = tokens.iter().any(|token| token.kind == "T_ABSTRACT");
        let parameters = raw_method_parameter_tokens(&tokens)
            .into_iter()
            .filter_map(|param_tokens| self.lower_raw_param(&param_tokens))
            .collect::<Vec<_>>();
        let return_type = raw_return_type_tokens(&tokens).and_then(|type_tokens| {
            let span = token_range(&type_tokens)?;
            lower_type_tokens(
                &type_tokens,
                self.database,
                self.module_id,
                self.reporter,
                self.scope.clone(),
                TypeContext::Return,
                self.in_class_like_context(),
            )
            .map(|id| ReturnType::new(id, span))
        });

        self.validate_parameters(&parameters);
        self.validate_promotion_context(&parameters, is_constructor, is_abstract);

        let mut flags = FunctionLikeFlags::new(raw_by_ref_return(&tokens));
        let is_static = tokens.iter().any(|token| token.kind == "T_STATIC");
        flags.set_static(is_static);
        flags.set_generator(contains_direct_function_like_yield(method.syntax()));
        flags.set_return_type_void(raw_return_type_has_token(&tokens, "T_VOID"));
        flags.set_return_type_never(raw_return_type_has_token(&tokens, "T_NEVER"));
        flags.set_tentative_or_deferred_info(flags.is_generator());
        flags.set_this_available(!is_static);

        let signature = FunctionSignature::new(
            SignatureKind::Method,
            name,
            parameters,
            return_type,
            raw_by_ref_return(&tokens),
            method.text_range(),
        )
        .with_flags(flags);
        self.push_signature(signature);

        self.collect_children(method.syntax());
    }

    fn lower_raw_param(&mut self, tokens: &[TypeToken]) -> Option<Parameter> {
        let variable = tokens.iter().find(|token| token.kind == "T_VARIABLE")?;
        let variable_index = tokens
            .iter()
            .position(|token| token.kind == "T_VARIABLE")
            .expect("variable found above");
        let type_tokens = raw_param_type_tokens(&tokens[..variable_index]);
        let type_id = type_tokens.and_then(|type_tokens| {
            lower_type_tokens(
                &type_tokens,
                self.database,
                self.module_id,
                self.reporter,
                self.scope.clone(),
                TypeContext::Parameter,
                self.in_class_like_context(),
            )
        });
        let default = raw_default_value_ref(tokens, true);
        let promoted_property = promoted_property_info_from_tokens(&tokens[..variable_index]);
        let attributes = token_range(tokens)
            .map(ParameterAttribute::new)
            .into_iter()
            .collect();
        let span = token_range(tokens).unwrap_or(variable.range);
        Some(Parameter::new(
            variable.text.clone(),
            type_id,
            ParameterFlags::new(
                tokens[..variable_index].iter().any(is_ampersand_type_token),
                tokens[..variable_index]
                    .iter()
                    .any(|token| token.kind == "T_ELLIPSIS" || token.text == "..."),
                promoted_property,
            ),
            default,
            attributes,
            span,
        ))
    }

    fn validate_parameters(&mut self, parameters: &[Parameter]) {
        let mut seen = HashSet::<&str>::new();
        for (index, parameter) in parameters.iter().enumerate() {
            if !seen.insert(parameter.name()) {
                self.error(
                    DiagnosticId::DuplicateParameter,
                    format!("duplicate parameter `{}`", parameter.name()),
                    parameter.span(),
                );
            }
            if parameter.flags().is_variadic() {
                if parameter.default().is_some() {
                    self.error(
                        DiagnosticId::InvalidParameterDefault,
                        "variadic parameter cannot have a default value",
                        parameter.span(),
                    );
                }
                if index + 1 != parameters.len() {
                    self.error(
                        DiagnosticId::VariadicParameterNotLast,
                        "variadic parameter must be the last parameter",
                        parameter.span(),
                    );
                }
            }
        }
    }

    fn validate_closure_use(&mut self, parameters: &[Parameter], closure_use: &[ClosureUseName]) {
        if closure_use.is_empty() {
            return;
        }
        let parameter_names: HashSet<&str> = parameters.iter().map(Parameter::name).collect();
        let mut seen_captures = HashSet::<&str>::new();
        for capture in closure_use {
            if is_auto_global_name(&capture.name) {
                self.error(
                    DiagnosticId::ClosureUseAutoGlobal,
                    "Cannot use auto-global as lexical variable",
                    capture.span,
                );
            }
            if !seen_captures.insert(capture.name.as_str()) {
                self.error(
                    DiagnosticId::DuplicateClosureUseVariable,
                    format!("Cannot use variable {} twice", capture.name),
                    capture.span,
                );
            }
            if parameter_names.contains(capture.name.as_str()) {
                self.error(
                    DiagnosticId::ClosureUseDuplicatesParameter,
                    format!(
                        "Cannot use lexical variable {} as a parameter name",
                        capture.name
                    ),
                    capture.span,
                );
            }
        }
    }

    fn validate_promotion_context(
        &mut self,
        parameters: &[Parameter],
        is_constructor: bool,
        is_abstract: bool,
    ) {
        for parameter in parameters {
            if parameter.flags().promoted_property().is_none() {
                continue;
            }
            let in_interface = self
                .class_stack
                .last()
                .is_some_and(|context| context.kind == ClassContainerKind::Interface);
            if !is_constructor || is_abstract || in_interface {
                self.error(
                    DiagnosticId::InvalidPropertyPromotion,
                    "constructor property promotion is not allowed in this context",
                    parameter.span(),
                );
            }
        }
    }

    fn in_class_like_context(&self) -> bool {
        !self.class_stack.is_empty()
    }

    fn push_signature(&mut self, signature: FunctionSignature) {
        let module = self
            .database
            .module_mut(self.module_id)
            .expect("module allocated before signature lowering");
        module.push_signature(signature);
    }

    fn error(&mut self, id: DiagnosticId, message: impl Into<String>, span: TextRange) {
        self.reporter.report(SemanticDiagnostic::with_span(
            id,
            DiagnosticSeverity::Error,
            DiagnosticPhase::HirLowering,
            message,
            span,
        ));
    }
}

fn is_auto_global_name(name: &str) -> bool {
    matches!(
        name,
        "$GLOBALS"
            | "$_SERVER"
            | "$_GET"
            | "$_POST"
            | "$_FILES"
            | "$_COOKIE"
            | "$_SESSION"
            | "$_REQUEST"
            | "$_ENV"
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ClassContext {
    kind: ClassContainerKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClassContainerKind {
    Class,
    Interface,
    Trait,
    Enum,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClosureUseName {
    name: String,
    span: TextRange,
}

fn named_decl_name(node: &SyntaxNode) -> Option<String> {
    syntax_child_tokens(node)
        .find(|token| token.kind().name() == "T_STRING")
        .map(|token| token.text().to_owned())
}

fn direct_function_body_statements(
    database: &FrontendDatabase,
    module_id: ModuleId,
    node: &SyntaxNode,
) -> Vec<crate::hir::StmtId> {
    let Some(function) = FunctionDecl::cast(node) else {
        return Vec::new();
    };
    let Some(body) = function.body() else {
        return Vec::new();
    };
    let Some(module) = database.module(module_id) else {
        return Vec::new();
    };
    direct_body_statement_spans(body)
        .into_iter()
        .filter_map(|span| {
            module.statements().iter().find_map(|(stmt_id, _)| {
                database
                    .source_map()
                    .span(crate::SourceMappedId::from(stmt_id))
                    .is_some_and(|mapped| mapped == span)
                    .then_some(stmt_id)
            })
        })
        .collect()
}

fn direct_body_statement_spans(body: BlockStmt<'_>) -> Vec<TextRange> {
    syntax_child_nodes(body.syntax())
        .filter(|child| Stmt::cast(child).is_some())
        .map(|child| child.text_range())
        .collect()
}

fn param_list_is_closure_use(param_list: ParamList<'_>) -> bool {
    syntax_child_tokens(param_list.syntax())
        .find(|token| !token.kind().is_trivia())
        .is_some_and(|token| token.kind().name() == "T_USE")
}

fn closure_use_names(param_list: ParamList<'_>) -> Vec<ClosureUseName> {
    syntax_child_nodes(param_list.syntax())
        .filter_map(Param::cast)
        .filter_map(|param| {
            let variable = syntax_child_tokens(param.syntax())
                .find(|token| token.kind().name() == "T_VARIABLE")?;
            Some(ClosureUseName {
                name: variable.text().to_owned(),
                span: variable.text_range(),
            })
        })
        .collect()
}

fn by_ref_return(node: &SyntaxNode) -> bool {
    for token in syntax_child_tokens(node).filter(|token| !token.kind().is_trivia()) {
        if token.kind().name() == "T_FUNCTION" || token.kind().name() == "T_FN" {
            continue;
        }
        return is_ampersand_token(token);
    }
    false
}

fn parameter_type_tokens(param: &SyntaxNode) -> Option<Vec<TypeToken>> {
    let tokens: Vec<TypeToken> = syntax_child_tokens(param)
        .filter(|token| !token.kind().is_trivia())
        .take_while(|token| token.kind().name() != "T_VARIABLE")
        .map(type_token_from_syntax_token)
        .collect();
    non_empty_type_tokens(tokens)
}

fn default_value_ref(param: &SyntaxNode, const_expr_candidate: bool) -> Option<DefaultValueRef> {
    let equals = syntax_child_tokens(param).find(|token| token.text() == "=")?;
    let expr = widest_expr_after(param, equals.text_range().end())?;
    Some(DefaultValueRef::new(
        TextRange::new(equals.text_range().end().to_usize(), expr.end().to_usize()),
        const_expr_candidate,
    ))
}

fn widest_expr_after(node: &SyntaxNode, start: php_source::BytePos) -> Option<TextRange> {
    let mut best: Option<TextRange> = None;
    for child in syntax_child_nodes(node) {
        if child.kind().name() == "EXPR" && child.text_range().start() >= start {
            best = Some(match best {
                Some(current) if current.len() >= child.text_range().len() => current,
                _ => child.text_range(),
            });
        }
        if let Some(candidate) = widest_expr_after(child, start) {
            best = Some(match best {
                Some(current) if current.len() >= candidate.len() => current,
                _ => candidate,
            });
        }
    }
    best
}

fn direct_significant_tokens(node: &SyntaxNode) -> Vec<TypeToken> {
    syntax_child_tokens(node)
        .filter(|token| !token.kind().is_trivia())
        .map(type_token_from_syntax_token)
        .collect()
}

fn descendant_significant_tokens(node: &SyntaxNode) -> Vec<TypeToken> {
    descendant_tokens::<TokenView<'_>>(node)
        .filter(|token| !token.kind().is_trivia())
        .map(|token| TypeToken {
            text: token.text().to_owned(),
            kind: token.kind().name(),
            range: token.text_range(),
        })
        .collect()
}

fn method_signature_significant_tokens(method: &SyntaxNode) -> Vec<TypeToken> {
    let mut tokens = Vec::new();
    for child in method.children() {
        match child {
            php_syntax::SyntaxElement::Token(token) => {
                if !token.kind().is_trivia() {
                    tokens.push(type_token_from_syntax_token(token));
                }
            }
            php_syntax::SyntaxElement::Node(node) => {
                if BlockStmt::cast(node).is_some() {
                    break;
                }
                tokens.extend(descendant_significant_tokens(node));
            }
        }
    }
    tokens
}

fn raw_method_name(tokens: &[TypeToken]) -> Option<String> {
    let function = tokens.iter().position(|token| token.kind == "T_FUNCTION")?;
    tokens
        .iter()
        .skip(function + 1)
        .find(|token| token.kind == "T_STRING")
        .map(|token| token.text.clone())
}

fn raw_by_ref_return(tokens: &[TypeToken]) -> bool {
    let Some(function) = tokens.iter().position(|token| token.kind == "T_FUNCTION") else {
        return false;
    };
    for token in tokens.iter().skip(function + 1) {
        if token.kind == "T_STRING" || token.text == "(" {
            return false;
        }
        if is_ampersand_type_token(token) {
            return true;
        }
    }
    false
}

fn raw_method_parameter_tokens(tokens: &[TypeToken]) -> Vec<Vec<TypeToken>> {
    let Some(function) = tokens.iter().position(|token| token.kind == "T_FUNCTION") else {
        return Vec::new();
    };
    let Some(name) = tokens
        .iter()
        .enumerate()
        .skip(function + 1)
        .find_map(|(index, token)| (token.kind == "T_STRING").then_some(index))
    else {
        return Vec::new();
    };
    let Some(open) = tokens
        .iter()
        .enumerate()
        .skip(name + 1)
        .find_map(|(index, token)| (token.text == "(").then_some(index))
    else {
        return Vec::new();
    };
    let mut depth = 0usize;
    let mut start = open + 1;
    let mut out = Vec::new();
    for (index, token) in tokens.iter().enumerate().skip(open + 1) {
        match token.text.as_str() {
            "(" | "[" => depth += 1,
            ")" if depth == 0 => {
                if start < index {
                    out.push(tokens[start..index].to_vec());
                }
                break;
            }
            ")" | "]" => depth = depth.saturating_sub(1),
            "," if depth == 0 => {
                if start < index {
                    out.push(tokens[start..index].to_vec());
                }
                start = index + 1;
            }
            _ => {}
        }
    }
    out
}

fn raw_return_type_tokens(tokens: &[TypeToken]) -> Option<Vec<TypeToken>> {
    let colon = tokens.iter().position(|token| token.text == ":")?;
    let type_tokens = tokens
        .iter()
        .skip(colon + 1)
        .take_while(|token| !matches!(token.text.as_str(), "{" | ";" | "=>"))
        .cloned()
        .collect();
    non_empty_type_tokens(type_tokens)
}

fn return_type_has_token(node: &SyntaxNode, token_name: &str) -> bool {
    let mut after_colon = false;
    for token in descendant_tokens::<TokenView<'_>>(node).filter(|token| !token.kind().is_trivia())
    {
        match token.text() {
            ":" => {
                after_colon = true;
            }
            "{" | ";" | "=>" if after_colon => return false,
            _ if after_colon
                && token_matches_kind_or_text(&token.kind().name(), token.text(), token_name) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

fn raw_return_type_has_token(tokens: &[TypeToken], token_name: &str) -> bool {
    raw_return_type_tokens(tokens).is_some_and(|tokens| {
        tokens
            .iter()
            .any(|token| token_matches_kind_or_text(&token.kind, &token.text, token_name))
    })
}

fn token_matches_kind_or_text(kind: &str, text: &str, token_name: &str) -> bool {
    kind == token_name
        || match token_name {
            "T_VOID" => text.eq_ignore_ascii_case("void"),
            "T_NEVER" => text.eq_ignore_ascii_case("never"),
            _ => false,
        }
}

fn is_static_function_like(node: &SyntaxNode) -> bool {
    syntax_child_tokens(node)
        .filter(|token| !token.kind().is_trivia())
        .take_while(|token| !matches!(token.kind().name().as_str(), "T_FUNCTION" | "T_FN"))
        .any(|token| token.kind().name() == "T_STATIC")
}

fn contains_direct_function_like_yield(node: &SyntaxNode) -> bool {
    fn walk(node: &SyntaxNode) -> bool {
        for child in syntax_child_nodes(node) {
            if YieldExpr::cast(child).is_some() {
                return true;
            }
            if FunctionDecl::cast(child).is_some()
                || MethodDecl::cast(child).is_some()
                || ClosureExpr::cast(child).is_some()
                || ArrowFunctionExpr::cast(child).is_some()
            {
                continue;
            }
            if walk(child) {
                return true;
            }
        }
        false
    }
    walk(node)
}

fn arrow_body_span(node: &SyntaxNode) -> Option<TextRange> {
    let arrow = syntax_child_tokens(node).find(|token| token.text() == "=>")?;
    syntax_child_nodes(node)
        .find(|child| {
            child.text_range().start().to_usize() >= arrow.text_range().end().to_usize()
                && child.kind().name() == "EXPR"
        })
        .map(SyntaxNode::text_range)
        .or_else(|| {
            Some(TextRange::new(
                arrow.text_range().end().to_usize(),
                node.text_range().end().to_usize(),
            ))
        })
}

fn raw_param_type_tokens(tokens_before_variable: &[TypeToken]) -> Option<Vec<TypeToken>> {
    let type_tokens = tokens_before_variable
        .iter()
        .filter(|token| {
            is_type_atom_token(token) || matches!(token.text.as_str(), "?" | "|" | "&" | "(" | ")")
        })
        .filter(|token| !is_promotion_modifier_token(token))
        .filter(|token| token.kind != "T_ELLIPSIS")
        .cloned()
        .collect();
    non_empty_type_tokens(type_tokens)
}

fn raw_default_value_ref(
    tokens: &[TypeToken],
    const_expr_candidate: bool,
) -> Option<DefaultValueRef> {
    let equals = tokens.iter().position(|token| token.text == "=")?;
    let start = tokens.get(equals + 1)?.range.start().to_usize();
    let end = tokens.last()?.range.end().to_usize();
    Some(DefaultValueRef::new(
        TextRange::new(start, end),
        const_expr_candidate,
    ))
}

fn promoted_property_info_from_tokens(tokens: &[TypeToken]) -> Option<PromotedPropertyInfo> {
    let mut visibility = None;
    let mut readonly = false;
    let mut set_visibility = None;
    let mut first_span = None;
    let mut last_span = None;

    for token in tokens {
        match token.kind.as_str() {
            "T_PUBLIC" => visibility = Some(Visibility::Public),
            "T_PROTECTED" => visibility = Some(Visibility::Protected),
            "T_PRIVATE" => visibility = Some(Visibility::Private),
            "T_READONLY" => readonly = true,
            "T_PUBLIC_SET" => set_visibility = Some(Visibility::Public),
            "T_PROTECTED_SET" => set_visibility = Some(Visibility::Protected),
            "T_PRIVATE_SET" => set_visibility = Some(Visibility::Private),
            _ => continue,
        }
        first_span.get_or_insert(token.range);
        last_span = Some(token.range);
    }

    let visibility = visibility?;
    let first = first_span?;
    let last = last_span.unwrap_or(first);
    Some(PromotedPropertyInfo::new(
        visibility,
        readonly,
        set_visibility,
        TextRange::new(first.start().to_usize(), last.end().to_usize()),
    ))
}

fn is_promotion_modifier_token(token: &TypeToken) -> bool {
    matches!(
        token.kind.as_str(),
        "T_PUBLIC"
            | "T_PROTECTED"
            | "T_PRIVATE"
            | "T_READONLY"
            | "T_PUBLIC_SET"
            | "T_PROTECTED_SET"
            | "T_PRIVATE_SET"
    )
}

fn is_ampersand_token(token: &SyntaxToken) -> bool {
    matches!(
        token.kind().name().as_str(),
        "T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG" | "T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG"
    )
}

fn is_ampersand_type_token(token: &TypeToken) -> bool {
    matches!(
        token.kind.as_str(),
        "T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG" | "T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG"
    ) || token.text == "&"
}

#[cfg(test)]
mod tests {
    use super::collect_signatures_in_node;
    use crate::FrontendDatabase;
    use crate::diagnostics::{DiagnosticId, DiagnosticReporter};
    use crate::hir::HirModule;
    use crate::lower::types::TypeLoweringScope;
    use php_ast::{AstNode, source_file};
    use php_syntax::parse_source_file;

    #[test]
    fn lowers_basic_parameter_list() {
        let parse = parse_source_file("<?php function f(int $x, string &$y, ...$rest): void {}");
        let root = source_file(parse.root()).expect("source");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
        let mut reporter = DiagnosticReporter::new();
        collect_signatures_in_node(
            root.syntax(),
            &mut database,
            module_id,
            &mut reporter,
            TypeLoweringScope::new(None, Default::default()),
        );

        let module = database.module(module_id).expect("module");
        let signature = &module.signatures()[0];
        assert_eq!(signature.parameters().len(), 3);
        assert!(signature.parameters()[1].flags().is_by_ref());
        assert!(signature.parameters()[2].flags().is_variadic());
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn diagnoses_duplicate_and_variadic_order() {
        let parse = parse_source_file("<?php function f($x, ...$rest, $x): void {}");
        let root = source_file(parse.root()).expect("source");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
        let mut reporter = DiagnosticReporter::new();
        collect_signatures_in_node(
            root.syntax(),
            &mut database,
            module_id,
            &mut reporter,
            TypeLoweringScope::new(None, Default::default()),
        );

        let diagnostics = reporter.into_diagnostics();
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.id() == DiagnosticId::DuplicateParameter)
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.id() == DiagnosticId::VariadicParameterNotLast)
        );
    }

    #[test]
    fn diagnoses_promotion_outside_constructor() {
        let parse =
            parse_source_file("<?php class C { public function m(public string $name): void {} }");
        let root = source_file(parse.root()).expect("source");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
        let mut reporter = DiagnosticReporter::new();
        collect_signatures_in_node(
            root.syntax(),
            &mut database,
            module_id,
            &mut reporter,
            TypeLoweringScope::new(None, Default::default()),
        );

        let module = database.module(module_id).expect("module");
        let signature = &module.signatures()[0];
        assert_eq!(signature.name(), Some("m"));
        assert!(
            signature.parameters()[0]
                .flags()
                .promoted_property()
                .is_some()
        );
        assert!(
            reporter
                .into_diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.id() == DiagnosticId::InvalidPropertyPromotion)
        );
    }

    #[test]
    fn records_function_like_flags_and_arrow_body() {
        let parse = parse_source_file(
            "<?php function g(): iterable { yield 1; } $f = static fn (): never => throw new Exception(); class C { public static function m(): void {} }",
        );
        let root = source_file(parse.root()).expect("source");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
        let mut reporter = DiagnosticReporter::new();
        collect_signatures_in_node(
            root.syntax(),
            &mut database,
            module_id,
            &mut reporter,
            TypeLoweringScope::new(None, Default::default()),
        );

        let module = database.module(module_id).expect("module");
        assert!(module.signatures().iter().any(|signature| {
            signature.name() == Some("g") && signature.flags().is_generator()
        }));
        assert!(module.signatures().iter().any(|signature| {
            signature.kind() == crate::hir::SignatureKind::ArrowFunction
                && signature.flags().is_static()
                && signature.flags().has_return_type_never()
                && signature.arrow_body().is_some()
        }));
        assert!(module.signatures().iter().any(|signature| {
            signature.name() == Some("m")
                && signature.flags().is_static()
                && signature.flags().has_return_type_void()
                && !signature.flags().this_available()
        }));
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn records_nested_static_closure_without_static_method_leakage() {
        let parse = parse_source_file(
            "<?php class C { public function m() { $f = static function () { return $this; }; } }",
        );
        let root = source_file(parse.root()).expect("source");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
        let mut reporter = DiagnosticReporter::new();
        collect_signatures_in_node(
            root.syntax(),
            &mut database,
            module_id,
            &mut reporter,
            TypeLoweringScope::new(None, Default::default()),
        );

        let module = database.module(module_id).expect("module");
        assert!(module.signatures().iter().any(|signature| {
            signature.name() == Some("m")
                && !signature.flags().is_static()
                && signature.flags().this_available()
        }));
        assert!(module.signatures().iter().any(|signature| {
            signature.kind() == crate::hir::SignatureKind::Closure
                && signature.flags().is_static()
                && !signature.flags().this_available()
        }));
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn diagnoses_duplicate_closure_use_variables() {
        let parse = parse_source_file("<?php $f = function () use ($x, $x): void {};");
        let root = source_file(parse.root()).expect("source");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
        let mut reporter = DiagnosticReporter::new();
        collect_signatures_in_node(
            root.syntax(),
            &mut database,
            module_id,
            &mut reporter,
            TypeLoweringScope::new(None, Default::default()),
        );

        assert!(
            reporter
                .into_diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.id() == DiagnosticId::DuplicateClosureUseVariable)
        );
    }

    #[test]
    fn diagnoses_closure_use_auto_globals() {
        let parse = parse_source_file("<?php $f = function () use ($GLOBALS): void {};");
        let root = source_file(parse.root()).expect("source");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
        let mut reporter = DiagnosticReporter::new();
        collect_signatures_in_node(
            root.syntax(),
            &mut database,
            module_id,
            &mut reporter,
            TypeLoweringScope::new(None, Default::default()),
        );

        assert!(
            reporter
                .into_diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.id() == DiagnosticId::ClosureUseAutoGlobal)
        );
    }
}
