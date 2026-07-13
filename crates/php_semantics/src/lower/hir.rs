//! Structural expression and statement HIR lowering.

use std::collections::HashMap;

use crate::FrontendDatabase;
use crate::diagnostics::{
    DiagnosticId, DiagnosticPhase, DiagnosticReporter, DiagnosticSeverity, SemanticDiagnostic,
};
use crate::hir::{
    DeferredEffects, ExprId, HirCallArg, HirCatchClause, HirExpr, HirExprKind, HirIfBranch,
    HirMatchArm, HirNameResolution, HirStaticLocal, HirStmt, HirStmtKind, HirSwitchCase, ModuleId,
    QualifiedName, StmtId,
};
use crate::lower::types::TypeLoweringScope;
use crate::symbols::resolution::{ResolveContext, ResolvedName};
use php_ast::{
    ArrowFunctionExpr, AstNode, AstToken, BlockStmt, CastKind, ClassConstDecl, ClassDecl,
    ClosureExpr, ConstructExpr, EnumCase, EnumDecl, ExprListItem, ExprNode, FunctionDecl,
    InterfaceDecl, MethodDecl, PropertyDecl, Stmt, TokenView, TraitDecl, Variable,
    descendant_tokens, syntax_child_nodes, syntax_child_tokens,
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
        expr_child_lists: HashMap::new(),
        stmt_child_lists: HashMap::new(),
        stmt_expr_child_lists: HashMap::new(),
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
        if matches!(ExprNode::cast(node), Some(ExprNode::Name(_))) {
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
    expr_child_lists: HashMap<NodeKey, Vec<ExprId>>,
    stmt_child_lists: HashMap<NodeKey, Vec<StmtId>>,
    stmt_expr_child_lists: HashMap<NodeKey, Vec<ExprId>>,
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
        if is_statement_list_item(node) {
            self.collect_node(node);
            return;
        }
        for child in syntax_child_nodes(node) {
            self.collect_statement_descendants(child);
        }
    }

    fn collect_declaration_body_statements(&mut self, node: &SyntaxNode) {
        if FunctionDecl::cast(node).is_some() || MethodDecl::cast(node).is_some() {
            for body in syntax_child_nodes(node).filter(|child| BlockStmt::cast(child).is_some()) {
                for child in syntax_child_nodes(body) {
                    self.collect_statement_descendants(child);
                }
            }
            return;
        }
        for child in syntax_child_nodes(node) {
            self.collect_declaration_body_statements(child);
        }
    }

    fn collect_runtime_declaration_constant_expressions(&mut self, node: &SyntaxNode) {
        if FunctionDecl::cast(node).is_some()
            || MethodDecl::cast(node).is_some()
            || ClosureExpr::cast(node).is_some()
            || ArrowFunctionExpr::cast(node).is_some()
        {
            for child in syntax_child_nodes(node).filter(|child| BlockStmt::cast(child).is_none()) {
                self.collect_expression_descendants(child);
            }
            return;
        }
        if ClassConstDecl::cast(node).is_some()
            || EnumCase::cast(node).is_some()
            || PropertyDecl::cast(node).is_some()
        {
            for child in syntax_child_nodes(node).filter(|child| ExprNode::cast(child).is_some()) {
                self.lower_expr(child, ResolveContext::ConstantFetch);
            }
            return;
        }
        for child in syntax_child_nodes(node) {
            self.collect_runtime_declaration_constant_expressions(child);
        }
    }

    fn collect_expression_descendants(&mut self, node: &SyntaxNode) {
        if ExprNode::cast(node).is_some() {
            self.lower_expr(node, ResolveContext::ConstantFetch);
            return;
        }
        for child in syntax_child_nodes(node) {
            self.collect_expression_descendants(child);
        }
    }

    fn lower_stmt(&mut self, node: &SyntaxNode) -> StmtId {
        let key = NodeKey::new(node);
        if let Some(id) = self.stmts.get(&key) {
            return *id;
        }

        self.collect_closure_body_statements(node);
        self.collect_declaration_body_statements(node);
        self.collect_runtime_declaration_constant_expressions(node);

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
            Some(Stmt::Echo(stmt)) => {
                let mut expressions = self.lower_typed_expr_items(stmt.expression_items());
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
            Some(Stmt::For(_)) => self.lower_for_stmt(node),
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
            Some(Stmt::Declare(stmt)) => HirStmtKind::Declare {
                expressions: self.lower_typed_expr_items(stmt.expression_items()),
                body: self.stmt_children(node),
            },
            Some(Stmt::Global(stmt)) => HirStmtKind::Global {
                variables: self.lower_typed_expr_items(stmt.expression_items()),
            },
            Some(Stmt::Static(stmt)) => HirStmtKind::Static {
                locals: stmt
                    .expression_items()
                    .map(|local| match local {
                        ExprListItem::Expression(local) => self.lower_static_local(local),
                        ExprListItem::Error(node) => HirStaticLocal {
                            variable: self.missing_expr(node, "malformed static local"),
                            initializer: None,
                        },
                    })
                    .collect(),
            },
            Some(Stmt::Unset(stmt)) => {
                let mut expressions = self.lower_typed_expr_items(stmt.expression_items());
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

    fn lower_typed_expr_items<'tree>(
        &mut self,
        items: impl Iterator<Item = ExprListItem<'tree>>,
    ) -> Vec<ExprId> {
        items
            .map(|item| match item {
                ExprListItem::Expression(expression) => {
                    self.lower_expr(expression.syntax(), ResolveContext::ConstantFetch)
                }
                ExprListItem::Error(node) => self.missing_expr(node, "malformed expression list"),
            })
            .collect()
    }

    fn lower_static_local(&mut self, local: ExprNode<'_>) -> HirStaticLocal {
        if let ExprNode::Assign(assign) = local {
            let expressions = self.expr_children(assign.syntax());
            return HirStaticLocal {
                variable: expressions
                    .first()
                    .copied()
                    .unwrap_or_else(|| self.missing_expr(assign.syntax(), "missing static local")),
                initializer: expressions.get(1).copied(),
            };
        }
        HirStaticLocal {
            variable: self.lower_expr(local.syntax(), ResolveContext::ConstantFetch),
            initializer: None,
        }
    }

    fn lower_expr_wrapper(&mut self, node: &SyntaxNode, context: ResolveContext) -> ExprId {
        if let Some(construct) = syntax_child_nodes(node)
            .find(|child| matches!(ExprNode::cast(child), Some(ExprNode::Construct(_))))
        {
            let kind = self.construct_expr_kind(construct);
            return self.alloc_expr(kind, node.text_range());
        }

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
                    let target = current
                        .map(|target| self.reclassify_static_access_target(target))
                        .or_else(|| self.static_reserved_target_name(child));
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
            let operator = first_operator_text(node).unwrap_or_else(|| "binary".to_owned());
            let right = operands
                .get(1)
                .copied()
                .map(|right| self.reclassify_binary_right_operand(&operator, right))
                .or_else(|| Some(self.missing_expr(node, "missing expression child")));
            return HirExprKind::Binary {
                operator,
                left: operands
                    .first()
                    .copied()
                    .or_else(|| Some(self.missing_expr(node, "missing expression child"))),
                right,
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
            let right = self.reclassify_binary_right_operand(&operator, right);
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
            Some(ExprNode::Variable(variable)) => HirExprKind::Variable {
                name: source_text_no_trivia(node),
                sigil_count: variable.sigil_count(),
                dynamic: variable.dynamic_expression().map(|expression| {
                    self.lower_expr(expression.syntax(), ResolveContext::ConstantFetch)
                }),
            },
            Some(ExprNode::Parenthesized(_)) => {
                let expr = self.first_expr_child(node, false);
                HirExprKind::Unary {
                    operator: "parenthesized".to_owned(),
                    expr,
                }
            }
            Some(ExprNode::Prefix(prefix)) => {
                if let Some(kind) = prefix.cast_kind() {
                    HirExprKind::Cast {
                        kind: cast_kind_name(kind).to_owned(),
                        expr: self.first_expr_child(node, true),
                    }
                } else {
                    HirExprKind::Unary {
                        operator: first_operator_text(node).unwrap_or_else(|| "unary".to_owned()),
                        expr: self.first_expr_child(node, true),
                    }
                }
            }
            Some(ExprNode::Postfix(_)) => HirExprKind::Unary {
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
                if let Some((operator, operator_start, operator_end)) =
                    first_assignment_operator(node)
                {
                    let range = node.text_range();
                    HirExprKind::Assign {
                        operator,
                        left: self.expr_chain_in_range(
                            node,
                            range.start().to_usize(),
                            operator_start,
                            ResolveContext::ConstantFetch,
                        ),
                        right: self.expr_chain_in_range(
                            node,
                            operator_end,
                            range.end().to_usize(),
                            ResolveContext::ConstantFetch,
                        ),
                    }
                } else {
                    HirExprKind::Assign {
                        operator: "=".to_owned(),
                        left: self.nth_expr_child(node, 0, true),
                        right: self.nth_expr_child(node, 1, true),
                    }
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
                    .first_expr_child_with_context(node, ResolveContext::ClassLike)
                    .or_else(|| self.static_reserved_target_name(node)),
                member: self
                    .nth_expr_child(node, 1, false)
                    .or_else(|| self.static_member_literal(node)),
            },
            Some(ExprNode::Array(_)) => {
                let elements = self.list_expr_children_preserving_holes(node);
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
        let operands = ConstructExpr::cast(node)
            .map(|construct| self.lower_typed_expr_items(construct.expression_items()))
            .unwrap_or_default();
        match keyword.to_ascii_lowercase().as_str() {
            "include" | "include_once" | "require" | "require_once" => {
                self.report_deferred_effect(
                    node,
                    "include/require effects are deferred; Semantic frontend does not load files or import symbols",
                    "include paths may be dynamic and runtime code executes in the current scope",
                );
                HirExprKind::Include {
                    kind: keyword,
                    expr: operands
                        .first()
                        .copied()
                        .or_else(|| Some(self.placeholder_expr(node))),
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
                    expr: operands
                        .first()
                        .copied()
                        .or_else(|| Some(self.placeholder_expr(node))),
                    deferred_effects: DeferredEffects::eval(),
                }
            }
            "exit" | "die" => HirExprKind::Exit {
                expr: operands.first().copied(),
            },
            "print" => HirExprKind::BuiltinCall {
                name: keyword,
                args: operands
                    .into_iter()
                    .map(|value| HirCallArg {
                        name: None,
                        value,
                        unpack: false,
                    })
                    .collect(),
            },
            "isset" | "empty" => HirExprKind::BuiltinCall {
                name: keyword,
                args: operands
                    .into_iter()
                    .map(|value| HirCallArg {
                        name: None,
                        value,
                        unpack: false,
                    })
                    .collect(),
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
        let key = NodeKey::new(node);
        if let Some(statements) = self.stmt_child_lists.get(&key) {
            return statements.clone();
        }
        let statements = syntax_child_nodes(node)
            .filter(|child| is_statement_list_item(child))
            .map(|child| self.lower_stmt(child))
            .collect::<Vec<_>>();
        self.stmt_child_lists.insert(key, statements.clone());
        statements
    }

    fn lower_if_stmt(&mut self, node: &SyntaxNode) -> HirStmtKind {
        let expr_nodes = syntax_child_nodes(node)
            .filter(|child| ExprNode::cast(child).is_some())
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
        let body =
            self.collect_statement_list_items_between(node, condition_end, first_marker_start);
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
                    self.collect_statement_list_items_between(node, body_start, next_marker_start);
                elseifs.push(HirIfBranch { condition, body });
            } else {
                else_body.extend(self.collect_statement_list_items_between(
                    node,
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
            .map(|token| self.resolve_source_name(token.text(), ResolveContext::ClassLike))
            .collect();
        HirCatchClause {
            types,
            variable,
            body: self.stmt_children(node),
        }
    }

    fn resolve_source_name(&self, source: &str, context: ResolveContext) -> HirNameResolution {
        let qualified = QualifiedName::parse(source);
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

    fn collect_statement_list_items_between(
        &mut self,
        node: &SyntaxNode,
        start: usize,
        end: usize,
    ) -> Vec<StmtId> {
        let mut statements = Vec::new();
        for child in syntax_child_nodes(node) {
            self.collect_statement_list_items_in_range(child, start, end, &mut statements);
        }
        statements
    }

    fn collect_statement_list_items_in_range(
        &mut self,
        node: &SyntaxNode,
        start: usize,
        end: usize,
        statements: &mut Vec<StmtId>,
    ) {
        let range = node.text_range();
        let node_start = range.start().to_usize();
        let node_end = range.end().to_usize();
        if node_end <= start || node_start >= end {
            return;
        }
        if is_statement_list_item(node) && node_start >= start && node_start < end {
            statements.push(self.lower_stmt(node));
            return;
        }
        for child in syntax_child_nodes(node) {
            self.collect_statement_list_items_in_range(child, start, end, statements);
        }
    }

    fn stmt_expr_children(&mut self, node: &SyntaxNode) -> Vec<ExprId> {
        let key = NodeKey::new(node);
        if let Some(expressions) = self.stmt_expr_child_lists.get(&key) {
            return expressions.clone();
        }
        let mut expressions = Vec::new();
        for child in syntax_child_nodes(node) {
            if Stmt::cast(child).is_some() {
                continue;
            }
            self.collect_expr_descendants(child, &mut expressions);
        }
        self.stmt_expr_child_lists.insert(key, expressions.clone());
        expressions
    }

    fn lower_for_stmt(&mut self, node: &SyntaxNode) -> HirStmtKind {
        let mut header_separators = descendant_tokens::<TokenView<'_>>(node)
            .filter(|token| token.text() == ";")
            .map(|token| token.text_range().start().to_usize());
        let first_separator = header_separators.next();
        let second_separator = header_separators.next();

        let (init, condition, update) = if let (Some(first_separator), Some(second_separator)) =
            (first_separator, second_separator)
        {
            (
                self.stmt_expr_children_in_range(node, 0, first_separator),
                self.stmt_expr_children_in_range(node, first_separator + 1, second_separator),
                self.stmt_expr_children_in_range(node, second_separator + 1, usize::MAX),
            )
        } else {
            (self.stmt_expr_children(node), Vec::new(), Vec::new())
        };

        HirStmtKind::For {
            init,
            condition,
            update,
            body: self.stmt_children(node),
        }
    }

    fn stmt_expr_children_in_range(
        &mut self,
        node: &SyntaxNode,
        start: usize,
        end: usize,
    ) -> Vec<ExprId> {
        let mut expressions = Vec::new();
        for child in syntax_child_nodes(node) {
            if Stmt::cast(child).is_some() {
                continue;
            }
            self.collect_expr_descendants_in_range(child, start, end, &mut expressions);
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

    fn collect_expr_descendants_in_range(
        &mut self,
        node: &SyntaxNode,
        start: usize,
        end: usize,
        expressions: &mut Vec<ExprId>,
    ) {
        let range = node.text_range();
        let node_start = range.start().to_usize();
        let node_end = range.end().to_usize();
        if node_end <= start || node_start >= end {
            return;
        }
        if ExprNode::cast(node).is_some() {
            if node_start >= start && node_end <= end {
                expressions.push(self.lower_expr(node, ResolveContext::ConstantFetch));
            }
            return;
        }
        for child in syntax_child_nodes(node) {
            if Stmt::cast(child).is_some() {
                continue;
            }
            self.collect_expr_descendants_in_range(child, start, end, expressions);
        }
    }

    fn expr_children(&mut self, node: &SyntaxNode) -> Vec<ExprId> {
        let key = NodeKey::expr(node, ResolveContext::ConstantFetch);
        if let Some(expressions) = self.expr_child_lists.get(&key) {
            return expressions.clone();
        }
        let expressions = syntax_child_nodes(node)
            .filter(|child| ExprNode::cast(child).is_some())
            .map(|child| self.lower_expr(child, ResolveContext::ConstantFetch))
            .collect::<Vec<_>>();
        self.expr_child_lists.insert(key, expressions.clone());
        expressions
    }

    fn list_expr_children_preserving_holes(&mut self, node: &SyntaxNode) -> Vec<ExprId> {
        let mut elements = Vec::new();
        let mut expecting_element = false;
        let mut saw_open = false;

        for child in node.children() {
            match child {
                SyntaxElement::Token(token) if token.kind().is_trivia() => {}
                SyntaxElement::Token(token) if token.text() == "(" || token.text() == "[" => {
                    saw_open = true;
                    expecting_element = true;
                }
                SyntaxElement::Token(token) if token.text() == "," => {
                    if saw_open && expecting_element {
                        elements.push(self.placeholder_expr(node));
                    }
                    expecting_element = true;
                }
                SyntaxElement::Token(token) if token.text() == ")" || token.text() == "]" => break,
                SyntaxElement::Token(_) => {}
                SyntaxElement::Node(child) if ExprNode::cast(child).is_some() => {
                    elements.push(self.lower_expr(child, ResolveContext::ConstantFetch));
                    expecting_element = false;
                }
                SyntaxElement::Node(_) => {}
            }
        }

        elements
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
        let direct_call = syntax_child_nodes(node).find(|child| child.kind().name() == "CALL_EXPR");
        let anonymous_call = syntax_child_nodes(node)
            .find(|child| child.kind().name() == "CLASS_DECL")
            .and_then(|class| {
                syntax_child_nodes(class).find(|child| child.kind().name() == "CALL_EXPR")
            });
        if let Some(call) = direct_call.or(anonymous_call) {
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
        let Some((question, colon)) = ternary_question_colon_offsets(node) else {
            return HirExprKind::Ternary {
                condition: self.nth_expr_child(node, 0, true),
                if_true: self.nth_expr_child(node, 1, false),
                if_false: self.nth_expr_child(node, 2, true),
            };
        };
        let range = node.text_range();
        let condition = self.expr_chain_in_range(
            node,
            range.start().to_usize(),
            question,
            ResolveContext::ConstantFetch,
        );
        let if_true =
            self.expr_chain_in_range(node, question + 1, colon, ResolveContext::ConstantFetch);
        let if_false = self.expr_chain_in_range(
            node,
            colon + 1,
            range.end().to_usize(),
            ResolveContext::ConstantFetch,
        );

        if if_true.is_none() {
            return HirExprKind::Ternary {
                condition,
                if_true: None,
                if_false: if_false
                    .or_else(|| Some(self.missing_expr(node, "missing expression child"))),
            };
        }

        HirExprKind::Ternary {
            condition,
            if_true,
            if_false: if_false
                .or_else(|| Some(self.missing_expr(node, "missing expression child"))),
        }
    }

    fn expr_chain_in_range(
        &mut self,
        node: &SyntaxNode,
        start: usize,
        end: usize,
        context: ResolveContext,
    ) -> Option<ExprId> {
        let mut operands = self.expr_operands_in_range(node, start, end, context);
        if operands.is_empty() {
            self.collect_expr_descendants_in_range_with_context(
                node,
                start,
                end,
                context,
                &mut operands,
            );
        }
        match operands.len() {
            0 => None,
            1 => operands.first().copied(),
            _ => Some(self.alloc_binary_chain_expr(
                node,
                operands,
                operator_texts_in_range(node, start, end),
            )),
        }
    }

    fn expr_operands_in_range(
        &mut self,
        node: &SyntaxNode,
        start: usize,
        end: usize,
        context: ResolveContext,
    ) -> Vec<ExprId> {
        let mut out = Vec::new();
        for child in syntax_child_nodes(node).filter(|child| ExprNode::cast(child).is_some()) {
            let range = child.text_range();
            let child_start = range.start().to_usize();
            let child_end = range.end().to_usize();
            if child_end <= start || child_start >= end {
                continue;
            }
            if child_start < start || child_end > end {
                self.collect_expr_descendants_in_range_with_context(
                    child, start, end, context, &mut out,
                );
                continue;
            }
            if self.try_push_postfix_expr(node, &mut out, child) {
                continue;
            }
            if child.kind().name() == "PROPERTY_FETCH_EXPR"
                && let Some(receiver) = out.pop()
            {
                let property = self
                    .first_expr_child(child, false)
                    .or_else(|| self.property_name_literal(child));
                let span = TextRange::new(
                    self.frontend_span(receiver)
                        .map(|span| span.start().to_usize())
                        .unwrap_or_else(|| child_start),
                    child_end,
                );
                out.push(self.alloc_expr(
                    HirExprKind::PropertyFetch {
                        receiver: Some(receiver),
                        property,
                        nullsafe: has_descendant_token_text(child, "?->"),
                    },
                    span,
                ));
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
                        .unwrap_or_else(|| child_start),
                    child_end,
                );
                out.push(self.alloc_expr(
                    HirExprKind::DimFetch {
                        receiver: Some(receiver),
                        dim,
                    },
                    span,
                ));
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
                let target = self.reclassify_static_access_target(target);
                let member = self
                    .first_expr_child(child, false)
                    .or_else(|| self.static_member_literal(child));
                let span = TextRange::new(
                    self.frontend_span(target)
                        .map(|span| span.start().to_usize())
                        .unwrap_or_else(|| child_start),
                    child_end,
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
                && let Some(previous) = out.pop()
                && self.expr_has_trailing_call(node, previous, child)
            {
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

    fn collect_expr_descendants_in_range_with_context(
        &mut self,
        node: &SyntaxNode,
        start: usize,
        end: usize,
        context: ResolveContext,
        expressions: &mut Vec<ExprId>,
    ) {
        let range = node.text_range();
        let node_start = range.start().to_usize();
        let node_end = range.end().to_usize();
        if node_end <= start || node_start >= end {
            return;
        }
        if ExprNode::cast(node).is_some() && node_start >= start && node_end <= end {
            expressions.push(self.lower_expr(node, context));
            return;
        }
        for child in syntax_child_nodes(node) {
            if Stmt::cast(child).is_some() {
                continue;
            }
            self.collect_expr_descendants_in_range_with_context(
                child,
                start,
                end,
                context,
                expressions,
            );
        }
    }

    fn alloc_binary_chain_expr(
        &mut self,
        node: &SyntaxNode,
        operands: Vec<ExprId>,
        operators: Vec<String>,
    ) -> ExprId {
        let mut left = operands[0];
        for (index, right) in operands.iter().copied().enumerate().skip(1) {
            let operator = operators
                .get(index - 1)
                .or_else(|| operators.first())
                .cloned()
                .unwrap_or_else(|| "binary".to_owned());
            let right = self.reclassify_binary_right_operand(&operator, right);
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
        left
    }

    fn reclassify_binary_right_operand(&mut self, operator: &str, right: ExprId) -> ExprId {
        if operator != "instanceof" {
            return right;
        }
        self.reclassify_name_expr(right, ResolveContext::ClassLike)
    }

    fn reclassify_name_expr(&mut self, expr: ExprId, context: ResolveContext) -> ExprId {
        let Some(span) = self.frontend_span(expr) else {
            return expr;
        };
        let Some(source) = self
            .database
            .module(self.module_id)
            .and_then(|module| module.expressions().get(expr))
            .and_then(|expr| match expr.kind() {
                HirExprKind::Name { resolution } => Some(resolution.source().to_owned()),
                _ => None,
            })
        else {
            return expr;
        };
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
        self.alloc_expr(
            HirExprKind::Name {
                resolution: HirNameResolution::new(
                    source,
                    context.as_str(),
                    result.classification(),
                    resolved,
                    fallback,
                ),
            },
            span,
        )
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
            if self.try_push_postfix_expr(node, &mut out, child) {
                continue;
            }
            if child.kind().name() == "CALL_EXPR"
                && let Some(previous) = out.pop()
                && self.expr_has_trailing_call(node, previous, child)
            {
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
                let target = self.reclassify_static_access_target(target);
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
            out.push(self.lower_expr(child, context));
        }
        out
    }

    fn reclassify_static_access_target(&mut self, target: ExprId) -> ExprId {
        let Some(span) = self.frontend_span(target) else {
            return target;
        };
        let Some(source) = self
            .database
            .module(self.module_id)
            .and_then(|module| module.expressions().get(target))
            .and_then(|expr| match expr.kind() {
                HirExprKind::Name { resolution } => Some(resolution.source().to_owned()),
                _ => None,
            })
        else {
            return target;
        };
        if source.eq_ignore_ascii_case("self")
            || source.eq_ignore_ascii_case("parent")
            || source.eq_ignore_ascii_case("static")
        {
            return target;
        }
        let qualified = QualifiedName::parse(&source);
        let result = self
            .scope
            .resolver()
            .resolve(&qualified, ResolveContext::ClassLike);
        let name_kind =
            crate::symbols::resolution::NameResolver::name_kind(ResolveContext::ClassLike);
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
        self.alloc_expr(
            HirExprKind::Name {
                resolution: HirNameResolution::new(
                    source,
                    ResolveContext::ClassLike.as_str(),
                    result.classification(),
                    resolved,
                    fallback,
                ),
            },
            span,
        )
    }

    fn try_push_postfix_expr(
        &mut self,
        node: &SyntaxNode,
        out: &mut Vec<ExprId>,
        child: &SyntaxNode,
    ) -> bool {
        if child.kind().name() != "POSTFIX_EXPR" {
            return false;
        }
        let Some(previous) = out.last().copied() else {
            return false;
        };
        if !self
            .database
            .source_map()
            .span(previous)
            .is_some_and(|span| {
                span.end() <= child.text_range().start()
                    && only_trivia_between(
                        node,
                        span.end().to_usize(),
                        child.text_range().start().to_usize(),
                    )
            })
        {
            return false;
        }
        let Some(previous) = out.pop() else {
            return false;
        };
        let span = TextRange::new(
            self.frontend_span(previous)
                .map(|span| span.start().to_usize())
                .unwrap_or_else(|| child.text_range().start().to_usize()),
            child.text_range().end().to_usize(),
        );
        out.push(self.alloc_expr(
            HirExprKind::Unary {
                operator: first_operator_text(child).unwrap_or_else(|| "postfix".to_owned()),
                expr: Some(previous),
            },
            span,
        ));
        true
    }

    fn expr_has_trailing_call(&self, node: &SyntaxNode, expr: ExprId, call: &SyntaxNode) -> bool {
        self.database.source_map().span(expr).is_some_and(|span| {
            span.end() <= call.text_range().start()
                && only_trivia_between(
                    node,
                    span.end().to_usize(),
                    call.text_range().start().to_usize(),
                )
        })
    }

    fn property_name_literal(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        let token = descendant_tokens::<TokenView<'_>>(node)
            .filter(|token| !token.kind().is_trivia())
            .filter(|token| token.text() != "->" && token.text() != "?->")
            .last()?;
        let kind = if token.kind().name() == "T_VARIABLE" {
            HirExprKind::Variable {
                name: token.text().to_owned(),
                sigil_count: 1,
                dynamic: None,
            }
        } else {
            HirExprKind::Literal {
                text: token.text().to_owned(),
            }
        };
        Some(self.alloc_expr(kind, token.text_range()))
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
                if token.kind().name() == "T_VARIABLE" {
                    return Some(self.alloc_expr(
                        HirExprKind::Variable {
                            name: token.text().to_owned(),
                            sigil_count: 1,
                            dynamic: None,
                        },
                        token.text_range(),
                    ));
                }
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

fn is_statement_list_item(node: &SyntaxNode) -> bool {
    Stmt::cast(node).is_some()
        || FunctionDecl::cast(node).is_some()
        || ClassDecl::cast(node).is_some()
        || InterfaceDecl::cast(node).is_some()
        || TraitDecl::cast(node).is_some()
        || EnumDecl::cast(node).is_some()
}

fn assignment_this_target_span(node: &SyntaxNode) -> Option<TextRange> {
    let (_, operator_start, _) = first_assignment_operator(node)?;
    let mut targets = direct_expr_nodes_for_diagnostic(node)
        .filter(|target| target.text_range().end().to_usize() <= operator_start);
    let target = targets.next()?;
    if targets.next().is_some() {
        return None;
    }
    let variable = Variable::cast(target)?;
    let mut tokens = descendant_tokens::<TokenView<'_>>(variable.syntax())
        .filter(|token| !token.kind().is_trivia());
    let token = tokens.next()?;
    (token.kind().name() == "T_VARIABLE" && token.text() == "$this" && tokens.next().is_none())
        .then_some(token.text_range())
}

fn assignment_class_constant_write_span(node: &SyntaxNode) -> Option<TextRange> {
    let (operator, operator_start, operator_end) = first_assignment_operator(node)?;
    let range = node.text_range();
    let (start, end) = if operator == "=&" {
        (operator_end, range.end().to_usize())
    } else {
        (range.start().to_usize(), operator_start)
    };
    direct_class_constant_access_in_range(node, start, end)
}

fn direct_class_constant_access_in_range(
    node: &SyntaxNode,
    start: usize,
    end: usize,
) -> Option<TextRange> {
    for target in direct_expr_nodes_for_diagnostic(node) {
        let span = target.text_range();
        if span.end().to_usize() <= start || span.start().to_usize() >= end {
            continue;
        }
        if span.start().to_usize() < start || span.end().to_usize() > end {
            continue;
        }
        if is_class_constant_access_node(target) {
            return Some(span);
        }
        if matches!(target.kind().name().as_str(), "EXPR" | "PARENTHESIZED_EXPR")
            && let Some(span) = direct_class_constant_access_in_range(target, start, end)
        {
            return Some(span);
        }
    }
    None
}

fn direct_expr_nodes_for_diagnostic(node: &SyntaxNode) -> impl Iterator<Item = &SyntaxNode> {
    syntax_child_nodes(node).filter(|child| ExprNode::cast(child).is_some())
}

fn is_class_constant_access_node(node: &SyntaxNode) -> bool {
    if node.kind().name() != "STATIC_ACCESS_EXPR" {
        return false;
    }
    descendant_tokens::<TokenView<'_>>(node)
        .filter(|token| !token.kind().is_trivia())
        .find(|token| token.text() != "::")
        .is_some_and(|member| member.kind().name() != "T_VARIABLE")
}

const fn cast_kind_name(kind: CastKind) -> &'static str {
    match kind {
        CastKind::Int => "int",
        CastKind::Float => "float",
        CastKind::String => "string",
        CastKind::Array => "array",
        CastKind::Object => "object",
        CastKind::Bool => "bool",
        CastKind::Unset => "unset",
        CastKind::Void => "void",
    }
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

fn operator_texts_in_range(node: &SyntaxNode, start: usize, end: usize) -> Vec<String> {
    syntax_child_tokens(node)
        .filter(|token| !token.kind().is_trivia())
        .filter(|token| is_operator_token(token))
        .filter(|token| {
            let range = token.text_range();
            range.start().to_usize() >= start && range.end().to_usize() <= end
        })
        .map(SyntaxToken::text)
        .map(str::to_owned)
        .collect()
}

fn ternary_question_colon_offsets(node: &SyntaxNode) -> Option<(usize, usize)> {
    let mut question = None;
    for token in syntax_child_tokens(node).filter(|token| !token.kind().is_trivia()) {
        match token.text() {
            "?" if question.is_none() => question = Some(token.text_range().start().to_usize()),
            ":" if question.is_some() => {
                return Some((question?, token.text_range().start().to_usize()));
            }
            _ => {}
        }
    }
    None
}

fn first_assignment_operator(node: &SyntaxNode) -> Option<(String, usize, usize)> {
    let mut tokens = syntax_child_tokens(node).filter(|token| !token.kind().is_trivia());
    while let Some(token) = tokens.next() {
        if !is_assignment_operator_token(&token.kind().name(), token.text()) {
            continue;
        }
        let start = token.text_range().start().to_usize();
        let end = token.text_range().end().to_usize();
        if token.text() == "="
            && let Some(next) = tokens.next()
            && (next.kind().name() == "T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG"
                || next.kind().name() == "T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG")
        {
            return Some(("=&".to_owned(), start, next.text_range().end().to_usize()));
        }
        return Some((token.text().to_owned(), start, end));
    }
    None
}

fn is_assignment_operator_token(kind: &str, text: &str) -> bool {
    text == "="
        || matches!(
            kind,
            "T_PLUS_EQUAL"
                | "T_MINUS_EQUAL"
                | "T_MUL_EQUAL"
                | "T_DIV_EQUAL"
                | "T_MOD_EQUAL"
                | "T_CONCAT_EQUAL"
                | "T_AND_EQUAL"
                | "T_OR_EQUAL"
                | "T_XOR_EQUAL"
                | "T_SL_EQUAL"
                | "T_SR_EQUAL"
                | "T_POW_EQUAL"
                | "T_COALESCE_EQUAL"
        )
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
    use crate::hir::{HirExprKind, HirModule, HirStmtKind};
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
    fn collects_method_body_statements_inside_class_declarations() {
        let parse = parse_source_file(
            "<?php class Loader { public static function register() { spl_autoload_register([self::class, 'load'], true); } public static function load($class) { return false; } }",
        );
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
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
                .any(|(_, stmt)| stmt.kind().as_str() == "expr")
        );
        assert!(
            module
                .statements()
                .iter()
                .any(|(_, stmt)| stmt.kind().as_str() == "return")
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
    fn global_recovery_preserves_missing_entry_and_exact_span() {
        let source = "<?php function f() { global /* one */ $first, $$dynamic, , $last; }";
        let parse = parse_source_file(source);
        assert!(parse.has_errors());
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
        let variables = module
            .statements()
            .iter()
            .find_map(|(_, statement)| match statement.kind() {
                HirStmtKind::Global { variables } => Some(variables),
                _ => None,
            })
            .expect("global HIR");
        assert_eq!(variables.len(), 4);
        assert!(matches!(
            module.expressions()[variables[2]].kind(),
            HirExprKind::Missing
        ));
        let missing_span = database
            .source_map()
            .span(variables[2])
            .expect("missing entry span");
        let missing_start = source.find(", ,").expect("malformed separator") + 2;
        assert_eq!(
            missing_span,
            php_source::TextRange::new(missing_start, missing_start + 1)
        );
        assert!(reporter.into_diagnostics().iter().any(|diagnostic| {
            diagnostic.id() == DiagnosticId::HirMissingChild
                && diagnostic.span() == Some(missing_span)
        }));
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
    fn lowers_isset_array_dim_arithmetic_operand() {
        let source = "<?php isset($zonen[$key - 1]);\n";
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

        assert!(has_binary_dim, "isset array dim should preserve `$key - 1`");
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn lowers_binary_condition_in_unparenthesized_ternary() {
        let source = "<?php echo $limit === PHP_INT_MAX ? \"limit\" : \"bad\";\n";
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
        let condition = module
            .expressions()
            .iter()
            .find_map(|(_, expr)| match expr.kind() {
                HirExprKind::Ternary {
                    condition: Some(condition),
                    ..
                } => Some(*condition),
                _ => None,
            })
            .expect("ternary condition");

        assert!(matches!(
            module.expressions()[condition].kind(),
            HirExprKind::Binary { operator, .. } if operator == "==="
        ));
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn lowers_method_call_condition_in_unparenthesized_ternary() {
        let source = "<?php echo $g->valid() ? \"T\" : \"F\";\n";
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
        let condition = module
            .expressions()
            .iter()
            .find_map(|(_, expr)| match expr.kind() {
                HirExprKind::Ternary {
                    condition: Some(condition),
                    ..
                } => Some(*condition),
                _ => None,
            })
            .expect("ternary condition");

        let HirExprKind::MethodCall {
            receiver: None,
            method: Some(method),
            ..
        } = module.expressions()[condition].kind()
        else {
            panic!("ternary condition should preserve method call");
        };
        assert!(matches!(
            module.expressions()[*method].kind(),
            HirExprKind::PropertyFetch {
                receiver: Some(_),
                property: Some(_),
                ..
            }
        ));
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
    fn lowers_postfix_increment_inside_binary_expression() {
        let source = "<?php if ($i++ > 10) { echo $i; }\n";
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
        let has_postfix_binary_operand = module.expressions().iter().any(|(_, expr)| {
            let HirExprKind::Binary {
                operator,
                left: Some(left),
                ..
            } = expr.kind()
            else {
                return false;
            };
            operator == ">"
                && matches!(
                    module.expressions()[*left].kind(),
                    HirExprKind::Unary {
                        operator,
                        expr: Some(_),
                    } if operator == "++"
                )
        });

        assert!(
            has_postfix_binary_operand,
            "postfix increment should remain the binary left operand"
        );
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn lowers_require_concat_as_include_operand() {
        let source = "<?php require __DIR__ . '/wp-blog-header.php';\n";
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
        let include_operand = module
            .expressions()
            .iter()
            .find_map(|(_, expr)| match expr.kind() {
                HirExprKind::Include {
                    kind,
                    expr: Some(expr),
                    ..
                } if kind == "require" => Some(*expr),
                _ => None,
            })
            .expect("require expression");

        assert!(
            matches!(
                module.expressions()[include_operand].kind(),
                HirExprKind::Binary { operator, .. } if operator == "."
            ),
            "require operand should preserve the concatenated path"
        );
        assert!(
            reporter
                .into_diagnostics()
                .iter()
                .all(|diagnostic| diagnostic.id() != DiagnosticId::HirMissingChild)
        );
    }

    #[test]
    fn lowers_eval_concat_as_evaluated_operand() {
        let source = "<?php eval('class ' . $class . '{function __construct(){}}');\n";
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
        let eval_operand = module
            .expressions()
            .iter()
            .find_map(|(_, expr)| match expr.kind() {
                HirExprKind::Eval {
                    expr: Some(expr), ..
                } => Some(*expr),
                _ => None,
            })
            .expect("eval expression");

        assert!(
            matches!(
                module.expressions()[eval_operand].kind(),
                HirExprKind::Binary { operator, .. } if operator == "."
            ),
            "eval operand should preserve the concatenated code expression"
        );
        assert!(
            reporter
                .into_diagnostics()
                .iter()
                .all(|diagnostic| diagnostic.id() != DiagnosticId::HirMissingChild)
        );
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
    fn allows_class_constant_as_array_dim_assignment_key() {
        let source = "<?php class C { const PROP_TYPE = 'type'; public function m() { $query_params = array(); $query_params[self::PROP_TYPE] = array(); } }\n";
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

    #[test]
    fn lowers_die_function_call_operand_with_numeric_argument() {
        let source = "<?php if (!$ok) { die(get_status_header_desc(501)); }\n";
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

        let HirExprKind::Call {
            callee: Some(_),
            args,
        } = module.expressions()[exit_operand].kind()
        else {
            panic!("die operand should preserve function call");
        };
        assert_eq!(args.len(), 1);
        assert!(matches!(
            module.expressions()[args[0].value].kind(),
            HirExprKind::Literal { text } if text == "501"
        ));
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn lowers_empty_method_call_operand_with_arguments() {
        let source = "<?php function check($theme) { return empty($theme->get('RequiresWP')); }\n";
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
        let empty_operand = module
            .expressions()
            .iter()
            .find_map(|(_, expr)| match expr.kind() {
                HirExprKind::BuiltinCall { name, args } if name == "empty" => {
                    args.first().map(|arg| arg.value)
                }
                _ => None,
            })
            .expect("empty operand");

        let HirExprKind::MethodCall {
            receiver: None,
            method: Some(method),
            args,
            ..
        } = module.expressions()[empty_operand].kind()
        else {
            panic!("empty operand should preserve method call");
        };
        assert!(matches!(
            module.expressions()[*method].kind(),
            HirExprKind::PropertyFetch { .. }
        ));
        assert_eq!(args.len(), 1);
        assert!(
            !matches!(
                module.expressions()[empty_operand].kind(),
                HirExprKind::Missing
            ),
            "empty operand should not lower to missing"
        );
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn lowers_empty_call_dynamic_property_operand() {
        let source = "<?php function check($kind) { return empty(get_queried_object()->$kind); }\n";
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
        let empty_operand = module
            .expressions()
            .iter()
            .find_map(|(_, expr)| match expr.kind() {
                HirExprKind::BuiltinCall { name, args } if name == "empty" => {
                    args.first().map(|arg| arg.value)
                }
                _ => None,
            })
            .expect("empty operand");

        let HirExprKind::PropertyFetch {
            receiver: Some(receiver),
            property: Some(property),
            ..
        } = module.expressions()[empty_operand].kind()
        else {
            panic!("empty operand should preserve dynamic property fetch");
        };
        assert!(matches!(
            module.expressions()[*receiver].kind(),
            HirExprKind::Call { .. }
        ));
        assert!(matches!(
            module.expressions()[*property].kind(),
            HirExprKind::Variable { name, .. } if name == "$kind"
        ));
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn lowers_empty_logical_operand_with_nested_empty() {
        let source = "<?php function check($attributes) { return ! empty( $attributes['isLink'] && ! empty( $attributes['linkTarget'] ) ); }\n";
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
        assert!(matches!(
            module
                .expressions()
                .iter()
                .find(|(_, expr)| matches!(
                    expr.kind(),
                    HirExprKind::Binary { operator, .. } if operator == "&&"
                ))
                .map(|(_, expr)| expr.kind()),
            Some(HirExprKind::Binary { .. })
        ));
        assert!(
            module
                .expressions()
                .iter()
                .all(|(_, expr)| !matches!(expr.kind(), HirExprKind::Missing)),
            "empty logical operand should not lower to missing"
        );
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn lowers_empty_static_method_call_operand_with_arguments() {
        let source = "<?php function check() { return empty(Imagick::queryFormats('WEBP')); }\n";
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
        let empty_operand = module
            .expressions()
            .iter()
            .find_map(|(_, expr)| match expr.kind() {
                HirExprKind::BuiltinCall { name, args } if name == "empty" => {
                    args.first().map(|arg| arg.value)
                }
                _ => None,
            })
            .expect("empty operand");

        let HirExprKind::Call {
            callee: Some(callee),
            args,
        } = module.expressions()[empty_operand].kind()
        else {
            panic!("empty operand should preserve static method call");
        };
        assert!(matches!(
            module.expressions()[*callee].kind(),
            HirExprKind::StaticAccess { .. }
        ));
        assert_eq!(args.len(), 1);
        assert!(
            !matches!(
                module.expressions()[empty_operand].kind(),
                HirExprKind::Missing
            ),
            "empty operand should not lower to missing"
        );
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn lowers_anonymous_class_new_constructor_arguments() {
        let source = "<?php function make($value) { return new class($value) extends Base { public function m() {} }; }\n";
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
        let args = module
            .expressions()
            .iter()
            .find_map(|(_, expr)| match expr.kind() {
                HirExprKind::New { class: None, args } => Some(args),
                _ => None,
            })
            .expect("anonymous class new expression");

        assert_eq!(args.len(), 1);
        assert!(matches!(
            module.expressions()[args[0].value].kind(),
            HirExprKind::Variable { name, .. } if name == "$value"
        ));
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn lowers_zero_arg_die_without_placeholder_operand() {
        let source = "<?php function stop_now() { die(); }\n";
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
        let exit_expr = module
            .expressions()
            .iter()
            .find_map(|(_, expr)| match expr.kind() {
                HirExprKind::Exit { expr } => Some(expr),
                _ => None,
            })
            .expect("exit expression");

        assert_eq!(*exit_expr, None);
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn lowers_die_cast_variable_operand() {
        let source = "<?php function stop_now($message) { die( (string) $message ); }\n";
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
                HirExprKind::Cast {
                    kind,
                    expr: Some(_)
                } if kind == "string"
            ),
            "exit operand should preserve string cast"
        );
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn lowers_die_concat_with_function_call_operand() {
        let source =
            "<?php die( '<h1>' . __( 'Requirements Not Met' ) . '</h1><p>' . $compat . '</p>' );\n";
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
                HirExprKind::Binary { operator, .. } if operator == "."
            ),
            "exit operand should preserve the concatenated message"
        );
        assert!(
            module
                .expressions()
                .iter()
                .any(|(_, expr)| matches!(expr.kind(), HirExprKind::Call { .. })),
            "exit operand should preserve the translation function call"
        );
        assert!(
            !module
                .expressions()
                .iter()
                .any(|(_, expr)| matches!(expr.kind(), HirExprKind::Missing)),
            "exit operand should not lower any construct part to missing"
        );
        assert!(reporter.into_diagnostics().is_empty());
    }
}
