//! Structural expression and statement HIR lowering.

use std::collections::HashMap;

use crate::FrontendDatabase;
use crate::diagnostics::{
    DiagnosticId, DiagnosticPhase, DiagnosticReporter, DiagnosticSeverity, SemanticDiagnostic,
};
use crate::hir::{
    DeferredEffects, ExprId, HirCallArg, HirCatchClause, HirExpr, HirExprKind, HirIfBranch,
    HirMatchArm, HirNameResolution, HirStmt, HirStmtKind, HirSwitchCase, ModuleId, QualifiedName,
    StmtId,
};
use crate::lower::types::TypeLoweringScope;
use crate::symbols::resolution::{ResolveContext, ResolvedName};
use php_ast::{
    AstToken, ExprNode, Stmt, TokenView, descendant_tokens, syntax_child_nodes, syntax_child_tokens,
};
use php_source::TextRange;
use php_syntax::{SyntaxElement, SyntaxNode, SyntaxToken};

/// Lowers statements and expressions inside one top-level node.
pub fn collect_hir_in_node(
    node: &SyntaxNode,
    database: &mut FrontendDatabase,
    module_id: ModuleId,
    reporter: &mut DiagnosticReporter,
    scope: TypeLoweringScope,
) {
    let mut lowerer = HirLowerer {
        database,
        module_id,
        reporter,
        scope,
        exprs: HashMap::new(),
        stmts: HashMap::new(),
    };
    lowerer.collect_node(node);
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct NodeKey {
    kind: String,
    start: usize,
    end: usize,
}

impl NodeKey {
    fn new(node: &SyntaxNode) -> Self {
        Self {
            kind: node.kind().name(),
            start: node.text_range().start().to_usize(),
            end: node.text_range().end().to_usize(),
        }
    }

    fn expr(node: &SyntaxNode, context: ResolveContext) -> Self {
        let mut key = Self::new(node);
        if node.kind().name() == "NAME" {
            key.kind.push('@');
            key.kind.push_str(context.as_str());
        }
        key
    }
}

struct HirLowerer<'a> {
    database: &'a mut FrontendDatabase,
    module_id: ModuleId,
    reporter: &'a mut DiagnosticReporter,
    scope: TypeLoweringScope,
    exprs: HashMap<NodeKey, ExprId>,
    stmts: HashMap<NodeKey, StmtId>,
}

impl HirLowerer<'_> {
    fn collect_node(&mut self, node: &SyntaxNode) {
        if Stmt::cast(node).is_some() {
            self.lower_stmt(node);
            return;
        }
        if ExprNode::cast(node).is_some() {
            self.lower_expr(node, ResolveContext::ConstantFetch);
            self.collect_closure_body_statements(node);
            return;
        }
        for child in syntax_child_nodes(node) {
            self.collect_node(child);
        }
    }

    fn collect_closure_body_statements(&mut self, node: &SyntaxNode) {
        if node.kind().name() == "CLOSURE_EXPR" {
            for child in syntax_child_nodes(node) {
                self.collect_statement_descendants(child);
            }
            return;
        }
        for child in syntax_child_nodes(node) {
            self.collect_closure_body_statements(child);
        }
    }

    fn collect_statement_descendants(&mut self, node: &SyntaxNode) {
        if Stmt::cast(node).is_some() {
            self.collect_node(node);
            return;
        }
        for child in syntax_child_nodes(node) {
            self.collect_statement_descendants(child);
        }
    }

    fn lower_stmt(&mut self, node: &SyntaxNode) -> StmtId {
        let key = NodeKey::new(node);
        if let Some(id) = self.stmts.get(&key) {
            return *id;
        }

        self.collect_closure_body_statements(node);

        let kind = match Stmt::cast(node) {
            Some(Stmt::InlineHtml(_)) => HirStmtKind::InlineHtml {
                text: source_text_no_trivia(node),
            },
            Some(Stmt::Empty(_)) => HirStmtKind::Block {
                statements: Vec::new(),
            },
            Some(Stmt::Expr(_)) => HirStmtKind::Expr {
                expr: self.first_stmt_expr_child(node, false),
            },
            Some(Stmt::Echo(_)) => {
                let mut expressions = self.stmt_expr_children(node);
                if expressions.is_empty() {
                    expressions.push(self.missing_expr(node, "missing echo expression"));
                }
                HirStmtKind::Echo { expressions }
            }
            Some(Stmt::Return(_)) => HirStmtKind::Return {
                expr: self.first_stmt_expr_child(node, false),
            },
            Some(Stmt::Throw(_)) => HirStmtKind::Throw {
                expr: self.first_stmt_expr_child(node, true),
            },
            Some(Stmt::Break(_)) => HirStmtKind::Break {
                expr: self.first_stmt_expr_child(node, false),
            },
            Some(Stmt::Continue(_)) => HirStmtKind::Continue {
                expr: self.first_stmt_expr_child(node, false),
            },
            Some(Stmt::Block(_)) => HirStmtKind::Block {
                statements: self.stmt_children(node),
            },
            Some(Stmt::If(_)) => self.lower_if_stmt(node),
            Some(Stmt::While(_)) => HirStmtKind::While {
                condition: self.first_stmt_expr_child_or_placeholder(node),
                body: self.stmt_children(node),
            },
            Some(Stmt::DoWhile(_)) => HirStmtKind::DoWhile {
                condition: self.first_stmt_expr_child_or_placeholder(node),
                body: self.stmt_children(node),
            },
            Some(Stmt::For(_)) => HirStmtKind::For {
                expressions: self.stmt_expr_children(node),
                body: self.stmt_children(node),
            },
            Some(Stmt::Foreach(_)) => self.lower_foreach_stmt(node),
            Some(Stmt::Switch(_)) => HirStmtKind::Switch {
                condition: self.first_switch_condition_or_placeholder(node),
                body: self.stmt_children(node),
                cases: self.switch_cases(node),
            },
            Some(Stmt::Try(_)) => self.lower_try_stmt(node),
            Some(Stmt::Catch(_)) | Some(Stmt::Finally(_)) => HirStmtKind::Block {
                statements: self.stmt_children(node),
            },
            Some(Stmt::Declare(_)) => HirStmtKind::Declare {
                expressions: self.stmt_expr_children(node),
                body: self.stmt_children(node),
            },
            Some(Stmt::Global(_)) => HirStmtKind::Global {
                variables: self.stmt_expr_children(node),
            },
            Some(Stmt::Static(_)) => HirStmtKind::Static {
                variables: self.stmt_expr_children(node),
            },
            Some(Stmt::Unset(_)) => {
                let mut expressions = self.construct_operand_exprs(node);
                if expressions.is_empty() {
                    expressions.push(self.placeholder_expr(node));
                }
                HirStmtKind::Unset { expressions }
            }
            Some(Stmt::Goto(_)) => HirStmtKind::Goto {
                label: first_string_token_after_keyword(node),
            },
            Some(Stmt::Label(_)) => HirStmtKind::Label {
                name: first_string_token_after_keyword(node),
            },
            None => HirStmtKind::Unlowered {
                syntax_kind: node.kind().name(),
            },
        };

        let id = self.alloc_stmt(kind, node.text_range());
        self.stmts.insert(key, id);
        id
    }

    fn lower_expr(&mut self, node: &SyntaxNode, context: ResolveContext) -> ExprId {
        let key = NodeKey::expr(node, context);
        if let Some(id) = self.exprs.get(&key) {
            return *id;
        }

        let id = if node.kind().name() == "EXPR" {
            self.lower_expr_wrapper(node, context)
        } else {
            let kind = self.expr_kind(node, context);
            self.alloc_expr(kind, node.text_range())
        };
        self.exprs.insert(key, id);
        id
    }

    fn lower_expr_wrapper(&mut self, node: &SyntaxNode, context: ResolveContext) -> ExprId {
        let mut current = None;
        let mut current_node_kind = None::<String>;
        let mut previous_child = None::<&SyntaxNode>;
        for child in syntax_child_nodes(node) {
            if ExprNode::cast(child).is_none() {
                continue;
            }
            match child.kind().name().as_str() {
                "CALL_EXPR" => {
                    if previous_child.is_some_and(|node| node.kind().name() == "NAME") {
                        current = previous_child
                            .map(|node| self.lower_expr(node, ResolveContext::FunctionCall));
                    }
                    let args = self.call_args(child);
                    let is_first_class = is_first_class_call_expr(child);
                    let kind = if is_first_class {
                        HirExprKind::FirstClassCallable { callee: current }
                    } else if current_node_kind.as_deref() == Some("PROPERTY_FETCH_EXPR") {
                        HirExprKind::MethodCall {
                            receiver: None,
                            method: current,
                            args,
                            nullsafe: has_descendant_token_text(node, "?->"),
                        }
                    } else {
                        HirExprKind::Call {
                            callee: current,
                            args,
                        }
                    };
                    current = Some(self.alloc_expr(kind, cover_child_span(node, child)));
                    current_node_kind = Some("CALL_EXPR".to_owned());
                    previous_child = Some(child);
                }
                "ARRAY_DIM_FETCH_EXPR" => {
                    let dim = self
                        .first_expr_child(child, false)
                        .or_else(|| self.array_dim_literal(child));
                    current = Some(self.alloc_expr(
                        HirExprKind::DimFetch {
                            receiver: current,
                            dim,
                        },
                        cover_child_span(node, child),
                    ));
                    current_node_kind = Some("ARRAY_DIM_FETCH_EXPR".to_owned());
                    previous_child = Some(child);
                }
                "PROPERTY_FETCH_EXPR" => {
                    let property = self
                        .first_expr_child(child, false)
                        .or_else(|| self.property_name_literal(child));
                    current = Some(self.alloc_expr(
                        HirExprKind::PropertyFetch {
                            receiver: current,
                            property,
                            nullsafe: has_descendant_token_text(child, "?->"),
                        },
                        cover_child_span(node, child),
                    ));
                    current_node_kind = Some("PROPERTY_FETCH_EXPR".to_owned());
                    previous_child = Some(child);
                }
                "STATIC_ACCESS_EXPR" => {
                    let member = self
                        .first_expr_child(child, false)
                        .or_else(|| self.static_member_literal(child));
                    let target = current.or_else(|| self.static_reserved_target_name(child));
                    current = Some(self.alloc_expr(
                        HirExprKind::StaticAccess { target, member },
                        cover_child_span(node, child),
                    ));
                    current_node_kind = Some("STATIC_ACCESS_EXPR".to_owned());
                    previous_child = Some(child);
                }
                "POSTFIX_EXPR" => {
                    let kind = HirExprKind::Unary {
                        operator: first_operator_text(child)
                            .unwrap_or_else(|| "postfix".to_owned()),
                        expr: current,
                    };
                    current = Some(self.alloc_expr(kind, cover_child_span(node, child)));
                    current_node_kind = Some("POSTFIX_EXPR".to_owned());
                    previous_child = Some(child);
                }
                _ => {
                    current = Some(self.lower_expr(child, context));
                    current_node_kind = Some(child.kind().name());
                    previous_child = Some(child);
                }
            }
        }
        current.unwrap_or_else(|| self.missing_expr(node, "missing expression child"))
    }

    fn pipe_input_expr(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        let expressions = self.expr_children(node);
        let (first, rest) = expressions.split_first()?;
        let Some((_last, callables)) = rest.split_last() else {
            return Some(*first);
        };
        let mut input = *first;
        for callable in callables {
            input = self.alloc_expr(
                HirExprKind::Pipe {
                    input: Some(input),
                    callable: Some(*callable),
                },
                node.text_range(),
            );
        }
        Some(input)
    }

    fn pipe_last_callable_expr(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        self.expr_children(node).into_iter().last()
    }

    fn binary_expr_kind(&mut self, node: &SyntaxNode) -> HirExprKind {
        let operands = self.expr_operands(node, ResolveContext::ConstantFetch);
        if operands.len() <= 2 {
            return HirExprKind::Binary {
                operator: first_operator_text(node).unwrap_or_else(|| "binary".to_owned()),
                left: operands
                    .first()
                    .copied()
                    .or_else(|| Some(self.missing_expr(node, "missing expression child"))),
                right: operands
                    .get(1)
                    .copied()
                    .or_else(|| Some(self.missing_expr(node, "missing expression child"))),
            };
        }

        let operators = operator_texts(node);
        let mut left = operands[0];
        for (index, right) in operands.iter().copied().enumerate().skip(1) {
            let operator = operators
                .get(index - 1)
                .or_else(|| operators.first())
                .cloned()
                .unwrap_or_else(|| "binary".to_owned());
            let span = TextRange::new(
                self.frontend_span(left)
                    .map(|span| span.start().to_usize())
                    .unwrap_or_else(|| node.text_range().start().to_usize()),
                self.frontend_span(right)
                    .map(|span| span.end().to_usize())
                    .unwrap_or_else(|| node.text_range().end().to_usize()),
            );
            left = self.alloc_expr(
                HirExprKind::Binary {
                    operator,
                    left: Some(left),
                    right: Some(right),
                },
                span,
            );
        }

        self.database
            .module(self.module_id)
            .and_then(|module| module.expressions().get(left))
            .map(|expr| expr.kind().clone())
            .unwrap_or(HirExprKind::Binary {
                operator: first_operator_text(node).unwrap_or_else(|| "binary".to_owned()),
                left: operands.first().copied(),
                right: operands.get(1).copied(),
            })
    }

    fn expr_kind(&mut self, node: &SyntaxNode, context: ResolveContext) -> HirExprKind {
        match ExprNode::cast(node) {
            Some(ExprNode::Literal(_))
            | Some(ExprNode::String(_))
            | Some(ExprNode::Encapsed(_))
            | Some(ExprNode::Heredoc(_)) => HirExprKind::Literal {
                text: source_text_no_trivia(node),
            },
            Some(ExprNode::Name(_)) => HirExprKind::Name {
                resolution: self.resolve_name(node, context),
            },
            Some(ExprNode::Variable(_)) => HirExprKind::Variable {
                name: source_text_no_trivia(node),
            },
            Some(ExprNode::Parenthesized(_)) => {
                let expr = self.first_expr_child(node, false);
                HirExprKind::Unary {
                    operator: "parenthesized".to_owned(),
                    expr,
                }
            }
            Some(ExprNode::Prefix(_)) | Some(ExprNode::Postfix(_)) => HirExprKind::Unary {
                operator: first_operator_text(node).unwrap_or_else(|| "unary".to_owned()),
                expr: self.first_expr_child(node, true),
            },
            Some(ExprNode::VoidCast(_)) => {
                self.reporter.report(SemanticDiagnostic::with_span(
                    DiagnosticId::InvalidVoidCast,
                    DiagnosticSeverity::Error,
                    DiagnosticPhase::HirLowering,
                    "`(void)` cast is rejected by the pinned PHP reference",
                    node.text_range(),
                ));
                self.report_deferred_effect(
                    node,
                    "(void) cast discards its operand at runtime; Semantic frontend records the cast without executing it",
                    "runtime value-discard behavior belongs to runtime execution semantics",
                );
                HirExprKind::Cast {
                    kind: "void".to_owned(),
                    expr: self.first_expr_child(node, true),
                }
            }
            Some(ExprNode::Binary(_)) => self.binary_expr_kind(node),
            Some(ExprNode::Assign(_)) => {
                if let Some(span) = assignment_class_constant_write_span(node) {
                    self.reporter.report(SemanticDiagnostic::with_span(
                        DiagnosticId::InvalidClassConstantWrite,
                        DiagnosticSeverity::Error,
                        DiagnosticPhase::HirLowering,
                        "syntax error, unexpected token",
                        span,
                    ));
                }
                if let Some(span) = assignment_this_target_span(node) {
                    self.reporter.report(SemanticDiagnostic::with_span(
                        DiagnosticId::ThisReassignment,
                        DiagnosticSeverity::Error,
                        DiagnosticPhase::HirLowering,
                        "Cannot re-assign $this",
                        span,
                    ));
                }
                HirExprKind::Assign {
                    operator: first_assignment_operator_text(node)
                        .unwrap_or_else(|| "=".to_owned()),
                    left: self.nth_expr_child(node, 0, true),
                    right: self.nth_expr_child(node, 1, true),
                }
            }
            Some(ExprNode::Ternary(_)) => self.ternary_expr_kind(node),
            Some(ExprNode::Call(_)) => {
                if is_first_class_call_expr(node) {
                    HirExprKind::FirstClassCallable { callee: None }
                } else {
                    HirExprKind::Call {
                        callee: None,
                        args: self.call_args(node),
                    }
                }
            }
            Some(ExprNode::ArrayDimFetch(_)) => HirExprKind::DimFetch {
                receiver: None,
                dim: self
                    .first_expr_child(node, false)
                    .or_else(|| self.array_dim_literal(node)),
            },
            Some(ExprNode::PropertyFetch(_)) => HirExprKind::PropertyFetch {
                receiver: None,
                property: self
                    .first_expr_child(node, false)
                    .or_else(|| self.property_name_literal(node)),
                nullsafe: has_descendant_token_text(node, "?->"),
            },
            Some(ExprNode::StaticAccess(_)) => HirExprKind::StaticAccess {
                target: self
                    .first_expr_child(node, false)
                    .or_else(|| self.static_reserved_target_name(node)),
                member: self
                    .nth_expr_child(node, 1, false)
                    .or_else(|| self.static_member_literal(node)),
            },
            Some(ExprNode::Array(_)) => {
                let elements = self.expr_children(node);
                if first_significant_token_text(node)
                    .is_some_and(|text| text.eq_ignore_ascii_case("list"))
                {
                    HirExprKind::List { elements }
                } else {
                    HirExprKind::Array { elements }
                }
            }
            Some(ExprNode::ArrayPair(_)) => {
                let exprs = self.expr_children(node);
                let has_key = has_direct_token_text(node, "=>");
                let unpack = has_direct_token_text(node, "...");
                HirExprKind::ArrayPair {
                    key: has_key.then(|| exprs.first().copied()).flatten(),
                    value: if has_key {
                        exprs.get(1).copied()
                    } else {
                        exprs.first().copied()
                    },
                    unpack,
                    by_ref: array_pair_is_by_ref(node),
                }
            }
            Some(ExprNode::Match(_)) => HirExprKind::Match {
                subject: self.first_expr_child(node, true),
                arms: self.match_arms(node),
            },
            Some(ExprNode::Throw(_)) => HirExprKind::Unary {
                operator: "throw".to_owned(),
                expr: self.first_expr_child(node, true),
            },
            Some(ExprNode::Construct(_)) => self.construct_expr_kind(node),
            Some(ExprNode::Yield(_)) => {
                if has_descendant_token_kind(node, "T_YIELD_FROM")
                    || has_descendant_token_text(node, "from")
                    || has_descendant_token_kind(node, "T_FROM")
                {
                    HirExprKind::YieldFrom {
                        expr: self.first_expr_child(node, true),
                    }
                } else {
                    HirExprKind::Yield {
                        key: self.nth_expr_child(node, 0, false),
                        value: self.nth_expr_child(node, 1, false),
                    }
                }
            }
            Some(ExprNode::Closure(_)) => HirExprKind::Closure {
                body: self.expr_children(node),
            },
            Some(ExprNode::ArrowFunction(_)) => HirExprKind::ArrowFunction {
                expr: self.first_expr_child(node, true),
            },
            Some(ExprNode::New(_)) => HirExprKind::New {
                class: self.first_expr_child_with_context(node, ResolveContext::ClassLike),
                args: self.new_call_args(node),
            },
            Some(ExprNode::Clone(_)) => HirExprKind::Clone {
                expr: self.first_expr_child(node, true),
            },
            Some(ExprNode::CloneWith(_)) => HirExprKind::CloneWith {
                expr: self.first_expr_child(node, true),
                replacements: self.expr_children(node).into_iter().skip(1).collect(),
            },
            Some(ExprNode::Pipe(_)) => HirExprKind::Pipe {
                input: self.pipe_input_expr(node),
                callable: self.pipe_last_callable_expr(node),
            },
            Some(ExprNode::Expr(_)) => HirExprKind::Unlowered {
                syntax_kind: node.kind().name(),
            },
            None => HirExprKind::Unlowered {
                syntax_kind: node.kind().name(),
            },
        }
    }

    fn construct_expr_kind(&mut self, node: &SyntaxNode) -> HirExprKind {
        let keyword = first_significant_token_text(node).unwrap_or_default();
        match keyword.to_ascii_lowercase().as_str() {
            "include" | "include_once" | "require" | "require_once" => {
                self.report_deferred_effect(
                    node,
                    "include/require effects are deferred; Semantic frontend does not load files or import symbols",
                    "include paths may be dynamic and runtime code executes in the current scope",
                );
                HirExprKind::Include {
                    kind: keyword,
                    expr: self.first_expr_child_or_placeholder(node),
                    deferred_effects: DeferredEffects::include_like(),
                }
            }
            "eval" => {
                self.report_deferred_effect(
                    node,
                    "eval effects are deferred; Semantic frontend does not parse or execute runtime code",
                    "eval may define symbols and mutate the current runtime scope",
                );
                HirExprKind::Eval {
                    expr: Some(self.construct_operand_expr(node)),
                    deferred_effects: DeferredEffects::eval(),
                }
            }
            "exit" | "die" => HirExprKind::Exit {
                expr: self.optional_construct_operand_expr(node),
            },
            "print" => HirExprKind::BuiltinCall {
                name: keyword,
                args: self.call_args(node),
            },
            "isset" | "empty" => HirExprKind::BuiltinCall {
                name: keyword,
                args: vec![HirCallArg {
                    name: None,
                    value: self.construct_operand_expr(node),
                    unpack: false,
                }],
            },
            _ => HirExprKind::Unlowered {
                syntax_kind: node.kind().name(),
            },
        }
    }

    fn resolve_name(&self, node: &SyntaxNode, context: ResolveContext) -> HirNameResolution {
        let source = source_text_no_trivia(node);
        let qualified = QualifiedName::parse(&source);
        let result = self.scope.resolver().resolve(&qualified, context);
        let name_kind = crate::symbols::resolution::NameResolver::name_kind(context);
        let (resolved, fallback) = match &result {
            ResolvedName::FullyQualified(name) => (Some(name.canonical(name_kind)), None),
            ResolvedName::MaybeRuntimeFallback {
                namespaced,
                fallback,
            } => (
                Some(namespaced.canonical(name_kind)),
                Some(fallback.canonical(name_kind)),
            ),
            ResolvedName::Dynamic | ResolvedName::Unresolved => (None, None),
        };
        HirNameResolution::new(
            source,
            context.as_str(),
            result.classification(),
            resolved,
            fallback,
        )
    }

    fn stmt_children(&mut self, node: &SyntaxNode) -> Vec<StmtId> {
        syntax_child_nodes(node)
            .filter(|child| Stmt::cast(child).is_some())
            .map(|child| self.lower_stmt(child))
            .collect()
    }

    fn lower_if_stmt(&mut self, node: &SyntaxNode) -> HirStmtKind {
        let expr_nodes = syntax_child_nodes(node)
            .filter(|child| ExprNode::cast(child).is_some())
            .collect::<Vec<_>>();
        let stmt_nodes = syntax_child_nodes(node)
            .filter(|child| Stmt::cast(child).is_some())
            .collect::<Vec<_>>();
        let mut markers = syntax_child_tokens(node)
            .filter(|token| matches!(token.kind().name().as_str(), "T_ELSEIF" | "T_ELSE"))
            .map(|token| {
                (
                    token.text_range().start().to_usize(),
                    token.kind().name().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        markers.sort_by_key(|(start, _)| *start);

        let condition = expr_nodes
            .first()
            .map(|child| self.lower_expr(child, ResolveContext::ConstantFetch))
            .or_else(|| Some(self.placeholder_expr(node)));
        let first_marker_start = markers
            .first()
            .map(|(start, _)| *start)
            .unwrap_or_else(|| node.text_range().end().to_usize());
        let condition_end = expr_nodes
            .first()
            .map(|child| child.text_range().end().to_usize())
            .unwrap_or_else(|| node.text_range().start().to_usize());
        let body = self.collect_stmt_nodes_between(&stmt_nodes, condition_end, first_marker_start);
        let mut elseifs = Vec::new();
        let mut else_body = Vec::new();

        for (index, (marker_start, marker_kind)) in markers.iter().enumerate() {
            let next_marker_start = markers
                .get(index + 1)
                .map(|(start, _)| *start)
                .unwrap_or_else(|| node.text_range().end().to_usize());
            if marker_kind == "T_ELSEIF" {
                let condition_node = expr_nodes.iter().copied().find(|expr| {
                    let start = expr.text_range().start().to_usize();
                    start > *marker_start && start < next_marker_start
                });
                let condition = condition_node
                    .map(|child| self.lower_expr(child, ResolveContext::ConstantFetch));
                let body_start = condition_node
                    .map(|child| child.text_range().end().to_usize())
                    .unwrap_or(*marker_start);
                let body =
                    self.collect_stmt_nodes_between(&stmt_nodes, body_start, next_marker_start);
                elseifs.push(HirIfBranch { condition, body });
            } else {
                else_body.extend(self.collect_stmt_nodes_between(
                    &stmt_nodes,
                    *marker_start,
                    next_marker_start,
                ));
            }
        }

        HirStmtKind::If {
            condition,
            body,
            elseifs,
            else_body,
        }
    }

    fn first_switch_condition_or_placeholder(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        let header_end = syntax_child_tokens(node)
            .find(|token| matches!(token.text(), "{" | ":"))
            .map(|token| token.text_range().start().to_usize())
            .unwrap_or_else(|| node.text_range().end().to_usize());
        syntax_child_nodes(node)
            .find(|child| {
                ExprNode::cast(child).is_some()
                    && child.text_range().start().to_usize() < header_end
            })
            .map(|child| self.lower_expr(child, ResolveContext::ConstantFetch))
            .or_else(|| Some(self.placeholder_expr(node)))
    }

    fn switch_cases(&mut self, node: &SyntaxNode) -> Vec<HirSwitchCase> {
        let expr_nodes = syntax_child_nodes(node)
            .filter(|child| ExprNode::cast(child).is_some())
            .collect::<Vec<_>>();
        let stmt_nodes = syntax_child_nodes(node)
            .filter(|child| Stmt::cast(child).is_some())
            .collect::<Vec<_>>();
        let mut markers = syntax_child_tokens(node)
            .filter(|token| matches!(token.kind().name().as_str(), "T_CASE" | "T_DEFAULT"))
            .map(|token| {
                (
                    token.text_range().start().to_usize(),
                    token.kind().name().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        markers.sort_by_key(|(start, _)| *start);

        let mut cases = Vec::new();
        for (index, (marker_start, marker_kind)) in markers.iter().enumerate() {
            let next_marker_start = markers
                .get(index + 1)
                .map(|(start, _)| *start)
                .unwrap_or_else(|| node.text_range().end().to_usize());
            let is_default = marker_kind == "T_DEFAULT";
            let condition_node = (!is_default)
                .then(|| {
                    expr_nodes.iter().copied().find(|expr| {
                        let start = expr.text_range().start().to_usize();
                        start > *marker_start && start < next_marker_start
                    })
                })
                .flatten();
            let body_start = condition_node
                .map(|child| child.text_range().end().to_usize())
                .unwrap_or(*marker_start);
            cases.push(HirSwitchCase {
                condition: condition_node
                    .map(|child| self.lower_expr(child, ResolveContext::ConstantFetch)),
                body: self.collect_stmt_nodes_between(&stmt_nodes, body_start, next_marker_start),
                is_default,
            });
        }
        cases
    }

    fn lower_try_stmt(&mut self, node: &SyntaxNode) -> HirStmtKind {
        let stmt_nodes = syntax_child_nodes(node)
            .filter(|child| Stmt::cast(child).is_some())
            .collect::<Vec<_>>();
        let mut body = Vec::new();
        let mut catches = Vec::new();
        let mut finally_body = Vec::new();

        for child in stmt_nodes {
            match Stmt::cast(child) {
                Some(Stmt::Catch(_)) => catches.push(self.lower_catch_clause(child)),
                Some(Stmt::Finally(_)) => finally_body = self.stmt_children(child),
                Some(_) if catches.is_empty() && finally_body.is_empty() => {
                    body.extend(self.stmt_children(child));
                }
                _ => {}
            }
        }

        HirStmtKind::Try {
            body,
            catches,
            finally_body,
        }
    }

    fn lower_catch_clause(&mut self, node: &SyntaxNode) -> HirCatchClause {
        let variable = descendant_tokens::<TokenView<'_>>(node)
            .find(|token| token.text().starts_with('$'))
            .map(|token| token.text().trim_start_matches('$').to_owned());
        let variable_start = descendant_tokens::<TokenView<'_>>(node)
            .find(|token| token.text().starts_with('$'))
            .map(|token| token.text_range().start().to_usize())
            .unwrap_or_else(|| node.text_range().end().to_usize());
        let types = descendant_tokens::<TokenView<'_>>(node)
            .filter(|token| {
                !token.kind().is_trivia()
                    && token.text_range().start().to_usize() < variable_start
                    && matches!(
                        token.kind().name().as_str(),
                        "T_STRING" | "T_NAME_FULLY_QUALIFIED" | "T_NAME_QUALIFIED"
                    )
            })
            .map(|token| token.text().to_owned())
            .collect();
        HirCatchClause {
            types,
            variable,
            body: self.stmt_children(node),
        }
    }

    fn match_arms(&mut self, node: &SyntaxNode) -> Vec<HirMatchArm> {
        let subject_end = syntax_child_tokens(node)
            .find(|token| token.text() == "{")
            .map(|token| token.text_range().end().to_usize())
            .unwrap_or_else(|| node.text_range().start().to_usize());
        let expr_nodes = syntax_child_nodes(node)
            .filter(|child| {
                ExprNode::cast(child).is_some()
                    && child.text_range().start().to_usize() >= subject_end
            })
            .collect::<Vec<_>>();
        let mut arrows = syntax_child_tokens(node)
            .filter(|token| token.kind().name() == "T_DOUBLE_ARROW")
            .map(|token| token.text_range().start().to_usize())
            .collect::<Vec<_>>();
        arrows.sort_unstable();

        let mut arms = Vec::new();
        let mut arm_start = subject_end;
        for (index, arrow_start) in arrows.iter().enumerate() {
            let next_arrow_start = arrows
                .get(index + 1)
                .copied()
                .unwrap_or_else(|| node.text_range().end().to_usize());
            let is_default = syntax_child_tokens(node).any(|token| {
                token.kind().name() == "T_DEFAULT"
                    && token.text_range().start().to_usize() >= arm_start
                    && token.text_range().start().to_usize() < *arrow_start
            });
            let conditions = if is_default {
                Vec::new()
            } else {
                expr_nodes
                    .iter()
                    .copied()
                    .filter(|expr| {
                        let start = expr.text_range().start().to_usize();
                        start >= arm_start && start < *arrow_start
                    })
                    .map(|expr| self.lower_expr(expr, ResolveContext::ConstantFetch))
                    .collect()
            };
            let result = expr_nodes.iter().copied().find(|expr| {
                let start = expr.text_range().start().to_usize();
                start > *arrow_start && start < next_arrow_start
            });
            arms.push(HirMatchArm {
                conditions,
                result: result.map(|expr| self.lower_expr(expr, ResolveContext::ConstantFetch)),
                is_default,
            });
            arm_start = syntax_child_tokens(node)
                .find(|token| {
                    token.text() == ","
                        && token.text_range().start().to_usize() > *arrow_start
                        && token.text_range().start().to_usize() < next_arrow_start
                })
                .map(|token| token.text_range().end().to_usize())
                .unwrap_or(*arrow_start);
        }
        arms
    }

    fn collect_stmt_nodes_between(
        &mut self,
        stmt_nodes: &[&SyntaxNode],
        start: usize,
        end: usize,
    ) -> Vec<StmtId> {
        stmt_nodes
            .iter()
            .copied()
            .filter(|child| {
                let child_start = child.text_range().start().to_usize();
                child_start >= start && child_start < end
            })
            .map(|child| self.lower_stmt(child))
            .collect()
    }

    fn stmt_expr_children(&mut self, node: &SyntaxNode) -> Vec<ExprId> {
        let mut expressions = Vec::new();
        for child in syntax_child_nodes(node) {
            if Stmt::cast(child).is_some() {
                continue;
            }
            self.collect_expr_descendants(child, &mut expressions);
        }
        expressions
    }

    fn lower_foreach_stmt(&mut self, node: &SyntaxNode) -> HirStmtKind {
        let expressions = self.stmt_expr_children(node);
        let (source, key_target, value_target) = match expressions.as_slice() {
            [source, value] => (Some(*source), None, Some(*value)),
            [source, key, value, ..] => (Some(*source), Some(*key), Some(*value)),
            [source] => (Some(*source), None, None),
            [] => (None, None, None),
        };
        HirStmtKind::Foreach {
            source,
            key_target,
            value_target,
            by_ref: foreach_header_is_by_ref(node),
            body: self.stmt_children(node),
        }
    }

    fn construct_operand_expr(&mut self, node: &SyntaxNode) -> ExprId {
        if let Some(expr) = self.simple_construct_operand_expr(node) {
            return expr;
        }
        let mut current = None;
        for child in syntax_child_nodes(node) {
            self.collect_construct_operand_chain(child, &mut current);
        }
        current.unwrap_or_else(|| self.placeholder_expr(node))
    }

    fn construct_operand_exprs(&mut self, node: &SyntaxNode) -> Vec<ExprId> {
        let source = source_text_no_trivia(node);
        let Some(open) = source.find('(') else {
            return vec![self.construct_operand_expr(node)];
        };
        let Some(close) = source.rfind(')') else {
            return vec![self.construct_operand_expr(node)];
        };
        let inner = &source[open + 1..close];
        let parts = split_construct_args(inner);
        if parts.len() <= 1 {
            return vec![self.construct_operand_expr(node)];
        }
        let mut expressions = Vec::with_capacity(parts.len());
        for part in parts {
            let Some(expr) = self.simple_construct_operand_source(part.trim(), node.text_range())
            else {
                return vec![self.construct_operand_expr(node)];
            };
            expressions.push(expr);
        }
        expressions
    }

    fn optional_construct_operand_expr(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        let source = source_text_no_trivia(node);
        let rest = source
            .strip_prefix("exit")
            .or_else(|| source.strip_prefix("die"))
            .unwrap_or(&source)
            .trim();
        if rest.is_empty() || rest == "()" {
            None
        } else {
            Some(self.construct_operand_expr(node))
        }
    }

    fn simple_construct_operand_expr(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        let source = source_text_no_trivia(node);
        let open = source.find('(')?;
        let close = source.rfind(')')?;
        let rest = source[open + 1..close].trim();
        if is_quoted_construct_operand(rest) {
            return Some(self.alloc_expr(
                HirExprKind::Literal {
                    text: rest.to_owned(),
                },
                node.text_range(),
            ));
        }
        self.simple_construct_operand_source(rest, node.text_range())
    }

    fn simple_construct_operand_source(
        &mut self,
        mut rest: &str,
        range: TextRange,
    ) -> Option<ExprId> {
        if let Some(expr) = self.simple_static_property_construct_operand_source(rest, range) {
            return Some(expr);
        }
        if !rest.starts_with('$') {
            return None;
        }
        let variable_len = rest
            .char_indices()
            .find_map(|(index, ch)| {
                (index > 0 && !(ch == '_' || ch.is_ascii_alphanumeric())).then_some(index)
            })
            .unwrap_or(rest.len());
        let variable = &rest[..variable_len];
        let mut current = self.alloc_expr(
            HirExprKind::Variable {
                name: variable.to_owned(),
            },
            range,
        );
        rest = rest[variable_len..].trim();
        loop {
            if let Some(after_open) = rest.strip_prefix('[') {
                let close = after_open.find(']')?;
                let dim_text = after_open[..close].trim();
                let dim = (!dim_text.is_empty()).then(|| {
                    self.alloc_expr(
                        HirExprKind::Literal {
                            text: dim_text.to_owned(),
                        },
                        range,
                    )
                });
                current = self.alloc_expr(
                    HirExprKind::DimFetch {
                        receiver: Some(current),
                        dim,
                    },
                    range,
                );
                rest = after_open[close + 1..].trim();
                continue;
            }
            if let Some(after_arrow) = rest.strip_prefix("->") {
                let property_len = after_arrow
                    .char_indices()
                    .find_map(|(index, ch)| {
                        (index > 0 && !(ch == '_' || ch.is_ascii_alphanumeric())).then_some(index)
                    })
                    .unwrap_or(after_arrow.len());
                let property = after_arrow[..property_len].trim();
                if property.is_empty() {
                    return None;
                }
                let property = self.alloc_expr(
                    HirExprKind::Literal {
                        text: property.to_owned(),
                    },
                    range,
                );
                current = self.alloc_expr(
                    HirExprKind::PropertyFetch {
                        receiver: Some(current),
                        property: Some(property),
                        nullsafe: false,
                    },
                    range,
                );
                rest = after_arrow[property_len..].trim();
                if let Some(after_open) = rest.strip_prefix('(')
                    && let Some(after_close) = after_open.strip_prefix(')')
                {
                    current = self.alloc_expr(
                        HirExprKind::MethodCall {
                            receiver: None,
                            method: Some(current),
                            args: Vec::new(),
                            nullsafe: false,
                        },
                        range,
                    );
                    rest = after_close.trim();
                }
                continue;
            }
            break;
        }
        rest.is_empty().then_some(current)
    }

    fn simple_static_property_construct_operand_source(
        &mut self,
        rest: &str,
        range: TextRange,
    ) -> Option<ExprId> {
        let (class_name, property_name) = rest.split_once("::$")?;
        if class_name.is_empty() || property_name.is_empty() {
            return None;
        }
        if !is_simple_static_construct_class_name(class_name)
            || !is_simple_construct_identifier(property_name)
        {
            return None;
        }

        let resolved = if class_name.eq_ignore_ascii_case("self")
            || class_name.eq_ignore_ascii_case("parent")
            || class_name.eq_ignore_ascii_case("static")
        {
            class_name.to_ascii_lowercase()
        } else {
            class_name.to_owned()
        };
        let target = self.alloc_expr(
            HirExprKind::Name {
                resolution: HirNameResolution::new(
                    class_name,
                    ResolveContext::ConstantFetch.as_str(),
                    "fully_qualified",
                    Some(resolved),
                    None,
                ),
            },
            range,
        );
        let member = self.alloc_expr(
            HirExprKind::Literal {
                text: format!("${property_name}"),
            },
            range,
        );
        Some(self.alloc_expr(
            HirExprKind::StaticAccess {
                target: Some(target),
                member: Some(member),
            },
            range,
        ))
    }

    fn collect_construct_operand_chain(&mut self, node: &SyntaxNode, current: &mut Option<ExprId>) {
        match node.kind().name().as_str() {
            "VARIABLE" => {
                *current = Some(self.lower_expr(node, ResolveContext::ConstantFetch));
                return;
            }
            "ARRAY_DIM_FETCH_EXPR" => {
                let dim = self
                    .first_expr_child(node, false)
                    .or_else(|| self.array_dim_literal(node));
                *current = Some(self.alloc_expr(
                    HirExprKind::DimFetch {
                        receiver: *current,
                        dim,
                    },
                    node.text_range(),
                ));
                return;
            }
            "PROPERTY_FETCH_EXPR" => {
                let property = self
                    .first_expr_child(node, false)
                    .or_else(|| self.property_name_literal(node));
                *current = Some(self.alloc_expr(
                    HirExprKind::PropertyFetch {
                        receiver: *current,
                        property,
                        nullsafe: has_descendant_token_text(node, "?->"),
                    },
                    node.text_range(),
                ));
                return;
            }
            "STATIC_ACCESS_EXPR" => {
                *current = Some(self.lower_expr(node, ResolveContext::ConstantFetch));
                return;
            }
            "CALL_EXPR" => {
                let Some(previous) = *current else {
                    *current = Some(self.missing_expr(node, "missing construct call target"));
                    return;
                };
                let args = self.call_args(node);
                let previous_kind = self
                    .database
                    .module(self.module_id)
                    .and_then(|module| module.expressions().get(previous))
                    .map(|expr| expr.kind().as_str())
                    .unwrap_or_default();
                let kind = if previous_kind == "property_fetch" {
                    HirExprKind::MethodCall {
                        receiver: None,
                        method: Some(previous),
                        args,
                        nullsafe: has_descendant_token_text(node, "?->"),
                    }
                } else {
                    HirExprKind::Call {
                        callee: Some(previous),
                        args,
                    }
                };
                let start = self
                    .frontend_span(previous)
                    .map(|span| span.start().to_usize())
                    .unwrap_or_else(|| node.text_range().start().to_usize());
                *current = Some(self.alloc_expr(
                    kind,
                    TextRange::new(start, node.text_range().end().to_usize()),
                ));
                return;
            }
            _ => {}
        }
        for child in syntax_child_nodes(node) {
            self.collect_construct_operand_chain(child, current);
        }
    }

    fn collect_expr_descendants(&mut self, node: &SyntaxNode, expressions: &mut Vec<ExprId>) {
        if ExprNode::cast(node).is_some() {
            expressions.push(self.lower_expr(node, ResolveContext::ConstantFetch));
            return;
        }
        for child in syntax_child_nodes(node) {
            if Stmt::cast(child).is_some() {
                continue;
            }
            self.collect_expr_descendants(child, expressions);
        }
    }

    fn expr_children(&mut self, node: &SyntaxNode) -> Vec<ExprId> {
        syntax_child_nodes(node)
            .filter(|child| ExprNode::cast(child).is_some())
            .map(|child| self.lower_expr(child, ResolveContext::ConstantFetch))
            .collect()
    }

    fn call_args(&mut self, node: &SyntaxNode) -> Vec<HirCallArg> {
        let children = node.children();
        let mut args = Vec::new();
        let mut pending_name = None;
        let mut pending_unpack = false;

        for (index, child) in children.iter().enumerate() {
            match child {
                SyntaxElement::Token(token) if token.kind().is_trivia() => {}
                SyntaxElement::Token(token) if token.text() == "..." => pending_unpack = true,
                SyntaxElement::Token(token)
                    if token.kind().name() == "T_STRING"
                        && next_significant_direct_token_text(children, index + 1).as_deref()
                            == Some(":") =>
                {
                    pending_name = Some(token.text().to_owned());
                }
                SyntaxElement::Token(_) => {}
                SyntaxElement::Node(child) if ExprNode::cast(child).is_some() => {
                    args.push(HirCallArg {
                        name: pending_name.take(),
                        value: self.lower_expr(child, ResolveContext::ConstantFetch),
                        unpack: pending_unpack,
                    });
                    pending_unpack = false;
                }
                SyntaxElement::Node(_) => {}
            }
        }

        args
    }

    fn new_call_args(&mut self, node: &SyntaxNode) -> Vec<HirCallArg> {
        if let Some(call) =
            syntax_child_nodes(node).find(|child| child.kind().name() == "CALL_EXPR")
        {
            return self.call_args(call);
        }
        self.expr_children(node)
            .into_iter()
            .skip(1)
            .map(|value| HirCallArg {
                name: None,
                value,
                unpack: false,
            })
            .collect()
    }

    fn first_expr_child(&mut self, node: &SyntaxNode, required: bool) -> Option<ExprId> {
        self.nth_expr_child(node, 0, required)
    }

    fn ternary_expr_kind(&mut self, node: &SyntaxNode) -> HirExprKind {
        let children = syntax_child_nodes(node)
            .filter(|child| ExprNode::cast(child).is_some())
            .collect::<Vec<_>>();
        if children.len() == 2 {
            return HirExprKind::Ternary {
                condition: Some(self.lower_expr(children[0], ResolveContext::ConstantFetch)),
                if_true: None,
                if_false: Some(self.lower_expr(children[1], ResolveContext::ConstantFetch)),
            };
        }

        HirExprKind::Ternary {
            condition: self.nth_expr_child(node, 0, true),
            if_true: self.nth_expr_child(node, 1, false),
            if_false: self.nth_expr_child(node, 2, true),
        }
    }

    fn first_expr_child_or_placeholder(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        self.first_expr_child(node, false)
            .or_else(|| Some(self.placeholder_expr(node)))
    }

    fn first_stmt_expr_child(&mut self, node: &SyntaxNode, required: bool) -> Option<ExprId> {
        let expr = self.stmt_expr_children(node).into_iter().next();
        match expr {
            Some(expr) => Some(expr),
            None if required => Some(self.missing_expr(node, "missing expression child")),
            None => None,
        }
    }

    fn first_stmt_expr_child_or_placeholder(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        self.stmt_expr_children(node)
            .into_iter()
            .next()
            .or_else(|| Some(self.placeholder_expr(node)))
    }

    fn first_expr_child_with_context(
        &mut self,
        node: &SyntaxNode,
        context: ResolveContext,
    ) -> Option<ExprId> {
        syntax_child_nodes(node)
            .find(|child| ExprNode::cast(child).is_some())
            .map(|child| self.lower_expr(child, context))
    }

    fn nth_expr_child(
        &mut self,
        node: &SyntaxNode,
        target: usize,
        required: bool,
    ) -> Option<ExprId> {
        let children = self.expr_operands(node, ResolveContext::ConstantFetch);
        match children.get(target).copied() {
            Some(child) => Some(child),
            None if required => Some(self.missing_expr(node, "missing expression child")),
            None => None,
        }
    }

    fn expr_operands(&mut self, node: &SyntaxNode, context: ResolveContext) -> Vec<ExprId> {
        let mut out = Vec::new();
        for child in syntax_child_nodes(node).filter(|child| ExprNode::cast(child).is_some()) {
            if child.kind().name() == "PROPERTY_FETCH_EXPR"
                && let Some(receiver) = out.pop()
            {
                let property = self
                    .first_expr_child(child, false)
                    .or_else(|| self.property_name_literal(child));
                let span = TextRange::new(
                    self.frontend_span(receiver)
                        .map(|span| span.start().to_usize())
                        .unwrap_or_else(|| child.text_range().start().to_usize()),
                    child.text_range().end().to_usize(),
                );
                let id = self.alloc_expr(
                    HirExprKind::PropertyFetch {
                        receiver: Some(receiver),
                        property,
                        nullsafe: has_descendant_token_text(child, "?->"),
                    },
                    span,
                );
                out.push(id);
                continue;
            }
            if child.kind().name() == "ARRAY_DIM_FETCH_EXPR"
                && let Some(receiver) = out.pop()
            {
                let dim = self
                    .first_expr_child(child, false)
                    .or_else(|| self.array_dim_literal(child));
                let span = TextRange::new(
                    self.frontend_span(receiver)
                        .map(|span| span.start().to_usize())
                        .unwrap_or_else(|| child.text_range().start().to_usize()),
                    child.text_range().end().to_usize(),
                );
                let id = self.alloc_expr(
                    HirExprKind::DimFetch {
                        receiver: Some(receiver),
                        dim,
                    },
                    span,
                );
                out.push(id);
                continue;
            }
            if child.kind().name() == "STATIC_ACCESS_EXPR"
                && let Some(target) = out.last().copied()
                && self
                    .database
                    .source_map()
                    .span(target)
                    .is_some_and(|span| span.end() == child.text_range().start())
            {
                let Some(target) = out.pop() else {
                    continue;
                };
                let member = self
                    .first_expr_child(child, false)
                    .or_else(|| self.static_member_literal(child));
                let span = TextRange::new(
                    self.frontend_span(target)
                        .map(|span| span.start().to_usize())
                        .unwrap_or_else(|| child.text_range().start().to_usize()),
                    child.text_range().end().to_usize(),
                );
                out.push(self.alloc_expr(
                    HirExprKind::StaticAccess {
                        target: Some(target),
                        member,
                    },
                    span,
                ));
                continue;
            }
            if child.kind().name() == "CALL_EXPR"
                && let Some(callee) = out.last().copied()
                && self.database.source_map().span(callee).is_some_and(|span| {
                    only_trivia_between(
                        node,
                        span.end().to_usize(),
                        child.text_range().start().to_usize(),
                    )
                })
            {
                let Some(previous) = out.pop() else {
                    continue;
                };
                let args = self.call_args(child);
                let current_kind = self
                    .database
                    .module(self.module_id)
                    .and_then(|module| module.expressions().get(previous))
                    .map(|expr| expr.kind().as_str())
                    .unwrap_or_default();
                let kind = if current_kind == "property_fetch" {
                    HirExprKind::MethodCall {
                        receiver: None,
                        method: Some(previous),
                        args,
                        nullsafe: has_descendant_token_text(child, "?->"),
                    }
                } else {
                    HirExprKind::Call {
                        callee: Some(previous),
                        args,
                    }
                };
                let span = TextRange::new(
                    self.frontend_span(previous)
                        .map(|span| span.start().to_usize())
                        .unwrap_or_else(|| child.text_range().start().to_usize()),
                    child.text_range().end().to_usize(),
                );
                out.push(self.alloc_expr(kind, span));
                continue;
            }
            out.push(self.lower_expr(child, context));
        }
        out
    }

    fn property_name_literal(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        let token = descendant_tokens::<TokenView<'_>>(node)
            .filter(|token| !token.kind().is_trivia())
            .filter(|token| token.text() != "->" && token.text() != "?->")
            .last()?;
        Some(self.alloc_expr(
            HirExprKind::Literal {
                text: token.text().to_owned(),
            },
            token.text_range(),
        ))
    }

    fn static_member_literal(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        let mut after_double_colon = false;
        for token in
            descendant_tokens::<TokenView<'_>>(node).filter(|token| !token.kind().is_trivia())
        {
            if token.text() == "::" {
                after_double_colon = true;
                continue;
            }
            if after_double_colon {
                return Some(self.alloc_expr(
                    HirExprKind::Literal {
                        text: token.text().to_owned(),
                    },
                    token.text_range(),
                ));
            }
        }
        None
    }

    fn static_reserved_target_name(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        let mut previous = None::<TokenView<'_>>;
        for token in
            descendant_tokens::<TokenView<'_>>(node).filter(|token| !token.kind().is_trivia())
        {
            if token.text() == "::" {
                let token = previous?;
                let text = token.text();
                if text.eq_ignore_ascii_case("self")
                    || text.eq_ignore_ascii_case("parent")
                    || text.eq_ignore_ascii_case("static")
                {
                    return Some(self.alloc_expr(
                        HirExprKind::Name {
                            resolution: HirNameResolution::new(
                                text,
                                ResolveContext::ConstantFetch.as_str(),
                                "fully_qualified",
                                Some(text.to_ascii_lowercase()),
                                None,
                            ),
                        },
                        token.text_range(),
                    ));
                }
                return None;
            }
            previous = Some(token);
        }
        None
    }

    fn array_dim_literal(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        let token = descendant_tokens::<TokenView<'_>>(node).find(|token| {
            !token.kind().is_trivia() && token.text() != "[" && token.text() != "]"
        })?;
        Some(self.alloc_expr(
            HirExprKind::Literal {
                text: token.text().to_owned(),
            },
            token.text_range(),
        ))
    }

    fn frontend_span(&self, expr: ExprId) -> Option<TextRange> {
        self.database.source_map().span(expr)
    }

    fn missing_expr(&mut self, node: &SyntaxNode, message: &'static str) -> ExprId {
        self.reporter.report(SemanticDiagnostic::with_span(
            DiagnosticId::HirMissingChild,
            DiagnosticSeverity::Error,
            DiagnosticPhase::HirLowering,
            message,
            node.text_range(),
        ));
        self.alloc_expr(HirExprKind::Missing, node.text_range())
    }

    fn placeholder_expr(&mut self, node: &SyntaxNode) -> ExprId {
        self.alloc_expr(HirExprKind::Missing, node.text_range())
    }

    fn report_deferred_effect(
        &mut self,
        node: &SyntaxNode,
        message: &'static str,
        note: &'static str,
    ) {
        self.reporter.report(
            SemanticDiagnostic::with_span(
                DiagnosticId::RuntimeCheckDeferred,
                DiagnosticSeverity::Note,
                DiagnosticPhase::HirLowering,
                message,
                node.text_range(),
            )
            .with_note(note),
        );
    }

    fn alloc_stmt(&mut self, kind: HirStmtKind, span: TextRange) -> StmtId {
        let id = self
            .database
            .module_mut(self.module_id)
            .expect("module allocated before statement lowering")
            .statements_mut()
            .alloc(HirStmt::new(kind));
        self.database.source_map_mut().insert(id, span);
        id
    }

    fn alloc_expr(&mut self, kind: HirExprKind, span: TextRange) -> ExprId {
        let id = self
            .database
            .module_mut(self.module_id)
            .expect("module allocated before expression lowering")
            .expressions_mut()
            .alloc(HirExpr::new(kind));
        self.database.source_map_mut().insert(id, span);
        id
    }
}

fn cover_child_span(parent: &SyntaxNode, child: &SyntaxNode) -> TextRange {
    TextRange::new(
        parent.text_range().start().to_usize(),
        child.text_range().end().to_usize(),
    )
}

fn source_text_no_trivia(node: &SyntaxNode) -> String {
    descendant_tokens::<TokenView<'_>>(node)
        .filter(|token| !token.kind().is_trivia())
        .map(|token| token.text())
        .collect::<Vec<_>>()
        .join("")
}

fn assignment_this_target_span(node: &SyntaxNode) -> Option<TextRange> {
    let mut left = String::new();
    let mut this_span = None;
    for token in descendant_tokens::<TokenView<'_>>(node).filter(|token| !token.kind().is_trivia())
    {
        if token.text().ends_with('=') {
            break;
        }
        if token.kind().name() == "T_VARIABLE" && token.text() == "$this" {
            this_span.get_or_insert(token.text_range());
        }
        left.push_str(token.text());
    }
    (left == "$this").then_some(this_span).flatten()
}

fn assignment_class_constant_write_span(node: &SyntaxNode) -> Option<TextRange> {
    let source = source_text_no_trivia(node);
    let operator = first_assignment_operator_text(node)?;
    if operator == "=&" {
        let rhs = source.split_once("=&")?.1;
        return is_class_constant_access_text(rhs).then_some(node.text_range());
    }
    let lhs = source.split_once(operator.as_str())?.0;
    is_class_constant_access_text(lhs).then_some(node.text_range())
}

fn is_class_constant_access_text(text: &str) -> bool {
    let text = text.strip_prefix('&').unwrap_or(text);
    let Some((_class, member)) = text.split_once("::") else {
        return false;
    };
    !member.is_empty() && !member.starts_with('$') && !member.starts_with('{')
}

fn split_construct_args(source: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut start = 0usize;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut quote = None::<u8>;
    let mut escaped = false;
    for (index, byte) in source.bytes().enumerate() {
        if let Some(quoted) = quote {
            if escaped {
                escaped = false;
                continue;
            }
            if byte == b'\\' {
                escaped = true;
                continue;
            }
            if byte == quoted {
                quote = None;
            }
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'[' => bracket_depth = bracket_depth.saturating_add(1),
            b']' => bracket_depth = bracket_depth.saturating_sub(1),
            b'(' => paren_depth = paren_depth.saturating_add(1),
            b')' => paren_depth = paren_depth.saturating_sub(1),
            b',' if bracket_depth == 0 && paren_depth == 0 => {
                args.push(source[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    args.push(source[start..].trim());
    args
}

fn only_trivia_between(node: &SyntaxNode, left_end: usize, right_start: usize) -> bool {
    if left_end > right_start {
        return false;
    }
    descendant_tokens::<TokenView<'_>>(node)
        .filter(|token| {
            let range = token.text_range();
            range.start().to_usize() >= left_end && range.end().to_usize() <= right_start
        })
        .all(|token| token.kind().is_trivia())
}

fn has_direct_token_text(node: &SyntaxNode, expected: &str) -> bool {
    syntax_child_tokens(node)
        .filter(|token| !token.kind().is_trivia())
        .any(|token| token.text() == expected)
}

fn array_pair_is_by_ref(node: &SyntaxNode) -> bool {
    syntax_child_tokens(node)
        .filter(|token| !token.kind().is_trivia())
        .any(|token| {
            let name = token.kind().name();
            token.text() == "&"
                || name == "T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG"
                || name == "T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG"
        })
}

fn is_quoted_construct_operand(text: &str) -> bool {
    let Some(first) = text.chars().next() else {
        return false;
    };
    if first != '\'' && first != '"' {
        return false;
    }
    text.len() >= 2 && text.ends_with(first)
}

fn is_simple_construct_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first != '_' && !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_simple_static_construct_class_name(text: &str) -> bool {
    let text = text.strip_prefix('\\').unwrap_or(text);
    if text.is_empty() {
        return false;
    }
    for segment in text.split('\\') {
        if segment.is_empty() {
            return false;
        }
        if !is_simple_construct_identifier(segment) {
            return false;
        }
    }
    true
}

fn foreach_header_is_by_ref(node: &SyntaxNode) -> bool {
    let mut after_as = false;
    for token in syntax_child_tokens(node).filter(|token| !token.kind().is_trivia()) {
        let text = token.text();
        if text == "as" {
            after_as = true;
            continue;
        }
        if !after_as {
            continue;
        }
        let name = token.kind().name();
        if name == "T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG"
            || name == "T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG"
        {
            return true;
        }
        if text == ")" {
            break;
        }
    }
    false
}

fn first_significant_token_text(node: &SyntaxNode) -> Option<String> {
    descendant_tokens::<TokenView<'_>>(node)
        .find(|token| !token.kind().is_trivia())
        .map(|token| token.text().to_owned())
}

fn next_significant_direct_token_text(children: &[SyntaxElement], start: usize) -> Option<String> {
    children.iter().skip(start).find_map(|child| match child {
        SyntaxElement::Token(token) if token.kind().is_trivia() => None,
        SyntaxElement::Token(token) => Some(token.text().to_owned()),
        SyntaxElement::Node(_) => None,
    })
}

fn is_first_class_call_expr(node: &SyntaxNode) -> bool {
    let has_argument_expr = syntax_child_nodes(node).any(|child| ExprNode::cast(child).is_some());
    !has_argument_expr && syntax_child_tokens(node).any(|token| token.text() == "...")
}

fn first_string_token_after_keyword(node: &SyntaxNode) -> Option<String> {
    syntax_child_tokens(node)
        .filter(|token| !token.kind().is_trivia())
        .find(|token| token.kind().name() == "T_STRING")
        .map(SyntaxToken::text)
        .map(str::to_owned)
}

fn first_operator_text(node: &SyntaxNode) -> Option<String> {
    syntax_child_tokens(node)
        .filter(|token| !token.kind().is_trivia())
        .find(|token| is_operator_token(token))
        .map(SyntaxToken::text)
        .map(str::to_owned)
}

fn operator_texts(node: &SyntaxNode) -> Vec<String> {
    syntax_child_tokens(node)
        .filter(|token| !token.kind().is_trivia())
        .filter(|token| is_operator_token(token))
        .map(SyntaxToken::text)
        .map(str::to_owned)
        .collect()
}

fn first_assignment_operator_text(node: &SyntaxNode) -> Option<String> {
    let mut tokens = syntax_child_tokens(node).filter(|token| !token.kind().is_trivia());
    while let Some(token) = tokens.next() {
        if !token.text().ends_with('=') {
            continue;
        }
        if token.text() == "="
            && let Some(next) = tokens.next()
            && (next.kind().name() == "T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG"
                || next.kind().name() == "T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG")
        {
            return Some("=&".to_owned());
        }
        return Some(token.text().to_owned());
    }
    None
}

fn is_operator_token_text(text: &str) -> bool {
    matches!(
        text,
        "+" | "-"
            | "*"
            | "**"
            | "/"
            | "%"
            | "."
            | "&"
            | "|"
            | "^"
            | "<<"
            | ">>"
            | "&&"
            | "||"
            | "and"
            | "or"
            | "xor"
            | "instanceof"
            | "!"
            | "@"
            | "~"
            | "=="
            | "==="
            | "!="
            | "!=="
            | "<>"
            | "<"
            | "<="
            | ">"
            | ">="
            | "<=>"
            | "??"
            | "|>"
            | "++"
            | "--"
    )
}

fn is_operator_token(token: &SyntaxToken) -> bool {
    is_operator_token_text(token.text())
        || token.kind().name().ends_with("_CAST")
        || token.kind().name() == "T_INSTANCEOF"
}

fn has_descendant_token_text(node: &SyntaxNode, text: &str) -> bool {
    descendant_tokens::<TokenView<'_>>(node).any(|token| token.text() == text)
}

fn has_descendant_token_kind(node: &SyntaxNode, kind: &str) -> bool {
    descendant_tokens::<TokenView<'_>>(node).any(|token| token.kind().name() == kind)
}

#[cfg(test)]
mod tests {
    use super::collect_hir_in_node;
    use crate::FrontendDatabase;
    use crate::diagnostics::{DiagnosticId, DiagnosticReporter};
    use crate::hir::{HirExprKind, HirModule};
    use crate::lower::types::TypeLoweringScope;
    use php_ast::{AstNode, source_file};
    use php_syntax::parse_source_file;

    #[test]
    fn lowers_statement_and_expression_shapes() {
        let parse = parse_source_file("<?php echo $value |> trim(...);\n");
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 31));
        let mut reporter = DiagnosticReporter::new();

        collect_hir_in_node(
            root.syntax(),
            &mut database,
            module_id,
            &mut reporter,
            TypeLoweringScope::new(None, Default::default()),
        );

        let module = database.module(module_id).expect("module");
        assert!(
            module
                .statements()
                .iter()
                .any(|(_, stmt)| stmt.kind().as_str() == "echo")
        );
        assert!(
            module
                .expressions()
                .iter()
                .any(|(_, expr)| matches!(expr.kind(), HirExprKind::Pipe { .. }))
        );
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn recovery_missing_expression_does_not_panic() {
        let parse = parse_source_file("<?php echo ;\n");
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 12));
        let mut reporter = DiagnosticReporter::new();

        collect_hir_in_node(
            root.syntax(),
            &mut database,
            module_id,
            &mut reporter,
            TypeLoweringScope::new(None, Default::default()),
        );

        let module = database.module(module_id).expect("module");
        assert!(
            module
                .statements()
                .iter()
                .any(|(_, stmt)| stmt.kind().as_str() == "echo")
        );
        assert!(
            module
                .expressions()
                .iter()
                .any(|(_, expr)| matches!(expr.kind(), HirExprKind::Missing))
        );
        assert!(
            reporter
                .into_diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.id().as_str() == "E_PHP_HIR_MISSING_CHILD")
        );
    }

    #[test]
    fn lowers_array_dim_fetch_as_single_binary_operand() {
        let source = "<?php function sum(...$xs) { return $xs[0] + $xs[1]; }\n";
        let parse = parse_source_file(source);
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", source.len()));
        let mut reporter = DiagnosticReporter::new();

        collect_hir_in_node(
            root.syntax(),
            &mut database,
            module_id,
            &mut reporter,
            TypeLoweringScope::new(None, Default::default()),
        );

        let module = database.module(module_id).expect("module");
        let binary = module
            .expressions()
            .iter()
            .find_map(|(_, expr)| match expr.kind() {
                HirExprKind::Binary {
                    operator,
                    left: Some(left),
                    right: Some(right),
                } if operator == "+" => Some((*left, *right)),
                _ => None,
            })
            .expect("binary expression");

        assert!(matches!(
            module.expressions()[binary.0].kind(),
            HirExprKind::DimFetch {
                receiver: Some(_),
                dim: Some(_)
            }
        ));
        assert!(matches!(
            module.expressions()[binary.1].kind(),
            HirExprKind::DimFetch {
                receiver: Some(_),
                dim: Some(_)
            }
        ));
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn lowers_array_dim_fetch_expression_operand() {
        let source = "<?php $args_array[$counter - 1];\n";
        let parse = parse_source_file(source);
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", source.len()));
        let mut reporter = DiagnosticReporter::new();

        collect_hir_in_node(
            root.syntax(),
            &mut database,
            module_id,
            &mut reporter,
            TypeLoweringScope::new(None, Default::default()),
        );

        let module = database.module(module_id).expect("module");
        let has_binary_dim = module.expressions().iter().any(|(_, expr)| {
            let HirExprKind::DimFetch { dim: Some(dim), .. } = expr.kind() else {
                return false;
            };
            matches!(
                module.expressions()[*dim].kind(),
                HirExprKind::Binary { operator, .. } if operator == "-"
            )
        });

        assert!(has_binary_dim, "array dim should preserve `$counter - 1`");
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn lowers_logical_and_assignment_with_php_precedence() {
        let source = "<?php function cmp($a) { is_array($a) and $a = count($a); }\n";
        let parse = parse_source_file(source);
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", source.len()));
        let mut reporter = DiagnosticReporter::new();

        collect_hir_in_node(
            root.syntax(),
            &mut database,
            module_id,
            &mut reporter,
            TypeLoweringScope::new(None, Default::default()),
        );

        let module = database.module(module_id).expect("module");
        let has_logical_and_assignment = module.expressions().iter().any(|(_, expr)| {
            let HirExprKind::Binary {
                operator,
                left: Some(left),
                right: Some(right),
                ..
            } = expr.kind()
            else {
                return false;
            };
            operator == "and"
                && matches!(
                    module.expressions()[*left].kind(),
                    HirExprKind::Call {
                        callee: Some(_),
                        ..
                    }
                )
                && matches!(
                    module.expressions()[*right].kind(),
                    HirExprKind::Assign { operator, .. } if operator == "="
                )
        });

        assert!(
            has_logical_and_assignment,
            "`and` should bind looser than assignment"
        );
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn diagnoses_this_reassignment() {
        let source = "<?php class C { function m($other) { $result = $this = $other; } }\n";
        let parse = parse_source_file(source);
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", source.len()));
        let mut reporter = DiagnosticReporter::new();

        collect_hir_in_node(
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
                .any(|diagnostic| diagnostic.id() == DiagnosticId::ThisReassignment)
        );
    }

    #[test]
    fn allows_this_property_assignment() {
        let source = "<?php class C { function m($other) { $this->prop = $other; } }\n";
        let parse = parse_source_file(source);
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", source.len()));
        let mut reporter = DiagnosticReporter::new();

        collect_hir_in_node(
            root.syntax(),
            &mut database,
            module_id,
            &mut reporter,
            TypeLoweringScope::new(None, Default::default()),
        );

        assert!(
            !reporter
                .into_diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.id() == DiagnosticId::ThisReassignment)
        );
    }

    #[test]
    fn diagnoses_class_constant_write_positions() {
        let source = "<?php C::NAME = 1; $ref = &C::NAME;\n";
        let parse = parse_source_file(source);
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", source.len()));
        let mut reporter = DiagnosticReporter::new();

        collect_hir_in_node(
            root.syntax(),
            &mut database,
            module_id,
            &mut reporter,
            TypeLoweringScope::new(None, Default::default()),
        );

        assert_eq!(
            reporter
                .into_diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.id() == DiagnosticId::InvalidClassConstantWrite)
                .count(),
            2
        );
    }

    #[test]
    fn allows_static_property_write_positions() {
        let source = "<?php C::$name = 1; $ref = &C::$name;\n";
        let parse = parse_source_file(source);
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", source.len()));
        let mut reporter = DiagnosticReporter::new();

        collect_hir_in_node(
            root.syntax(),
            &mut database,
            module_id,
            &mut reporter,
            TypeLoweringScope::new(None, Default::default()),
        );

        assert!(
            !reporter
                .into_diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.id() == DiagnosticId::InvalidClassConstantWrite)
        );
    }

    #[test]
    fn lowers_exit_method_call_operand() {
        let source = "<?php try {} catch (Exception $e) { exit($e->getMessage()); }\n";
        let parse = parse_source_file(source);
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", source.len()));
        let mut reporter = DiagnosticReporter::new();

        collect_hir_in_node(
            root.syntax(),
            &mut database,
            module_id,
            &mut reporter,
            TypeLoweringScope::new(None, Default::default()),
        );

        let module = database.module(module_id).expect("module");
        let exit_operand = module
            .expressions()
            .iter()
            .find_map(|(_, expr)| match expr.kind() {
                HirExprKind::Exit { expr: Some(expr) } => Some(*expr),
                _ => None,
            })
            .expect("exit operand");

        assert!(
            matches!(
                module.expressions()[exit_operand].kind(),
                HirExprKind::MethodCall { .. }
            ),
            "exit operand should preserve method calls"
        );
        assert!(
            !matches!(
                module.expressions()[exit_operand].kind(),
                HirExprKind::Missing
            ),
            "exit operand should not lower to missing"
        );
        assert!(reporter.into_diagnostics().is_empty());
    }
}
