//! Constant-expression candidate collection and structural validation.

use crate::FrontendDatabase;
use crate::diagnostics::{
    DiagnosticId, DiagnosticPhase, DiagnosticReporter, DiagnosticSeverity, SemanticDiagnostic,
};
use crate::hir::{
    ConstExpr, ConstExprContext, ConstExprId, ConstExprKind, ConstValue, ExprId, HirExprKind,
    HirModule, ModuleId,
};
use php_ast::{
    AstNode, Attribute, ClassConstDecl, ConstDecl, EnumCase, ExprNode, Param, PropertyDecl,
    StaticStmt, syntax_child_nodes, syntax_child_tokens,
};
use php_source::TextRange;
use php_syntax::SyntaxNode;

/// Lowers one HIR expression as a constant-expression candidate.
pub fn lower_const_expr_candidate(
    database: &mut FrontendDatabase,
    module_id: ModuleId,
    expr_id: ExprId,
    context: ConstExprContext,
    span: TextRange,
    reporter: &mut DiagnosticReporter,
) -> ConstExprId {
    let validation = {
        let module = database
            .module(module_id)
            .expect("module allocated before const-expression lowering");
        validate_const_expr_allowed_forms(module, expr_id, context)
    };
    let folded_value = {
        let module = database
            .module(module_id)
            .expect("module allocated before const-expression folding");
        validation
            .allowed
            .then(|| try_fold_const_expr_pure(module, expr_id))
            .flatten()
    };
    let const_expr_id = database
        .module_mut(module_id)
        .expect("module allocated before const-expression lowering")
        .const_exprs_mut()
        .alloc(ConstExpr::new(
            context,
            validation.kind,
            expr_id,
            validation.allowed,
            folded_value,
        ));
    database.source_map_mut().insert(const_expr_id, span);

    if !validation.allowed {
        let diagnostic_id = if context == ConstExprContext::AttributeArgument {
            DiagnosticId::AttributeArgumentNotConstExpr
        } else {
            DiagnosticId::InvalidConstExpr
        };
        reporter.report(SemanticDiagnostic::with_span(
            diagnostic_id,
            DiagnosticSeverity::Error,
            DiagnosticPhase::ConstExpression,
            "expression form is not allowed in constant expression context",
            span,
        ));
    }

    const_expr_id
}

/// Conservatively folds pure constant-expression HIR.
///
/// Returns `None` whenever folding would require PHP runtime semantics, zval
/// behavior, symbol-table lookup, object construction, or imprecise numeric
/// handling.
#[must_use]
pub fn try_fold_const_expr_pure(module: &HirModule, expr_id: ExprId) -> Option<ConstValue> {
    match module.expressions()[expr_id].kind() {
        HirExprKind::Literal { text } => fold_literal(text),
        HirExprKind::Unary { operator, expr } => {
            let expr = (*expr)?;
            let ConstValue::Int(value) = try_fold_const_expr_pure(module, expr)? else {
                return None;
            };
            match operator.as_str() {
                "+" => Some(ConstValue::Int(value)),
                "-" => value.checked_neg().map(ConstValue::Int),
                _ => None,
            }
        }
        HirExprKind::Binary {
            operator,
            left,
            right,
        } if operator == "." => {
            let ConstValue::String(left) = try_fold_const_expr_pure(module, (*left)?)? else {
                return None;
            };
            let ConstValue::String(right) = try_fold_const_expr_pure(module, (*right)?)? else {
                return None;
            };
            Some(ConstValue::String(format!("{left}{right}")))
        }
        HirExprKind::Name { resolution } => {
            Some(ConstValue::UnresolvedRef(resolution.source().to_owned()))
        }
        HirExprKind::DimFetch { receiver, dim } => {
            let receiver = try_fold_const_expr_pure(module, (*receiver)?)?;
            let dim = try_fold_const_expr_pure(module, (*dim)?)?;
            match (receiver, dim) {
                (ConstValue::String(string), ConstValue::Int(index)) => {
                    let bytes = string.as_bytes();
                    let length = bytes.len() as i64;
                    let resolved = if index < 0 { index + length } else { index };
                    if resolved < 0 || resolved >= length {
                        None
                    } else {
                        Some(ConstValue::String(
                            char::from(bytes[resolved as usize]).to_string(),
                        ))
                    }
                }
                _ => None,
            }
        }
        HirExprKind::StaticAccess { .. } => None,
        HirExprKind::Array { .. }
        | HirExprKind::ArrayPair { .. }
        | HirExprKind::List { .. }
        | HirExprKind::Binary { .. }
        | HirExprKind::Ternary { .. }
        | HirExprKind::Closure { .. }
        | HirExprKind::ArrowFunction { .. }
        | HirExprKind::FirstClassCallable { .. }
        | HirExprKind::Cast { .. }
        | HirExprKind::New { .. }
        | HirExprKind::Missing
        | HirExprKind::Variable { .. }
        | HirExprKind::Assign { .. }
        | HirExprKind::Call { .. }
        | HirExprKind::BuiltinCall { .. }
        | HirExprKind::MethodCall { .. }
        | HirExprKind::PropertyFetch { .. }
        | HirExprKind::Clone { .. }
        | HirExprKind::Pipe { .. }
        | HirExprKind::CloneWith { .. }
        | HirExprKind::Match { .. }
        | HirExprKind::Yield { .. }
        | HirExprKind::YieldFrom { .. }
        | HirExprKind::Include { .. }
        | HirExprKind::Eval { .. }
        | HirExprKind::Exit { .. }
        | HirExprKind::Unlowered { .. } => None,
    }
}

/// Structural validation result for a constant-expression candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConstExprValidation {
    /// Top-level candidate kind.
    pub kind: ConstExprKind,
    /// Whether the complete candidate tree is structurally allowed.
    pub allowed: bool,
}

/// Validates allowed constant-expression forms without evaluating values.
#[must_use]
pub fn validate_const_expr_allowed_forms(
    module: &HirModule,
    expr_id: ExprId,
    context: ConstExprContext,
) -> ConstExprValidation {
    let kind = module.expressions()[expr_id].kind();
    match kind {
        HirExprKind::Literal { .. } => allowed(ConstExprKind::ScalarLiteral),
        HirExprKind::Name { .. } => allowed(ConstExprKind::Name),
        HirExprKind::Array { elements } | HirExprKind::List { elements } => aggregate(
            module,
            ConstExprKind::Array,
            elements.iter().copied(),
            context,
        ),
        HirExprKind::ArrayPair { key, value, .. } => aggregate(
            module,
            ConstExprKind::Array,
            [*key, *value].into_iter().flatten(),
            context,
        ),
        HirExprKind::Unary { expr, .. } => {
            aggregate(module, ConstExprKind::Unary, expr.iter().copied(), context)
        }
        HirExprKind::Binary { left, right, .. } => aggregate(
            module,
            ConstExprKind::Binary,
            [*left, *right].into_iter().flatten(),
            context,
        ),
        HirExprKind::Ternary {
            condition,
            if_true,
            if_false,
        } => aggregate(
            module,
            ConstExprKind::Ternary,
            [*condition, *if_true, *if_false].into_iter().flatten(),
            context,
        ),
        HirExprKind::StaticAccess { target, member } => aggregate(
            module,
            ConstExprKind::ClassConstFetch,
            [*target, *member].into_iter().flatten(),
            context,
        ),
        HirExprKind::Closure { .. } => allowed(ConstExprKind::Closure),
        HirExprKind::ArrowFunction { .. } => allowed(ConstExprKind::ArrowFunction),
        HirExprKind::FirstClassCallable { .. } => allowed(ConstExprKind::FirstClassCallable),
        HirExprKind::Cast { expr, .. } => {
            aggregate(module, ConstExprKind::Cast, expr.iter().copied(), context)
        }
        HirExprKind::New { class, args } => {
            let class_allowed = class
                .iter()
                .copied()
                .all(|child| validate_const_expr_allowed_forms(module, child, context).allowed);
            let args_allowed = args
                .iter()
                .all(|arg| validate_new_argument_allowed(module, arg.value, context));
            ConstExprValidation {
                kind: ConstExprKind::New,
                allowed: class_allowed && args_allowed,
            }
        }
        HirExprKind::DimFetch { receiver, dim } => aggregate(
            module,
            ConstExprKind::Dim,
            [*receiver, *dim].into_iter().flatten(),
            context,
        ),
        HirExprKind::Missing => disallowed(ConstExprKind::Missing),
        HirExprKind::Variable { .. }
        | HirExprKind::Assign { .. }
        | HirExprKind::Call { .. }
        | HirExprKind::BuiltinCall { .. }
        | HirExprKind::MethodCall { .. }
        | HirExprKind::PropertyFetch { .. }
        | HirExprKind::Clone { .. }
        | HirExprKind::Pipe { .. }
        | HirExprKind::CloneWith { .. }
        | HirExprKind::Match { .. }
        | HirExprKind::Yield { .. }
        | HirExprKind::YieldFrom { .. }
        | HirExprKind::Include { .. }
        | HirExprKind::Eval { .. }
        | HirExprKind::Exit { .. }
        | HirExprKind::Unlowered { .. } => disallowed(ConstExprKind::Disallowed),
    }
}

/// Collects constant-expression candidates visible inside one top-level node.
pub fn collect_const_expr_in_node(
    node: &SyntaxNode,
    database: &mut FrontendDatabase,
    module_id: ModuleId,
    reporter: &mut DiagnosticReporter,
) {
    let mut collector = ConstExprCollector {
        database,
        module_id,
        reporter,
    };
    collector.collect_node(node);
}

struct ConstExprCollector<'a> {
    database: &'a mut FrontendDatabase,
    module_id: ModuleId,
    reporter: &'a mut DiagnosticReporter,
}

impl ConstExprCollector<'_> {
    fn collect_node(&mut self, node: &SyntaxNode) {
        if ConstDecl::cast(node).is_some() {
            self.collect_direct_expressions(node, ConstExprContext::GlobalConstInitializer);
        } else if ClassConstDecl::cast(node).is_some() {
            self.collect_direct_expressions(node, ConstExprContext::ClassConstInitializer);
        } else if EnumCase::cast(node).is_some() {
            self.collect_direct_expressions(node, ConstExprContext::EnumCaseBackingValue);
        } else if PropertyDecl::cast(node).is_some() {
            self.collect_direct_expressions(node, ConstExprContext::PropertyDefault);
        } else if StaticStmt::cast(node).is_some() {
            self.collect_direct_expressions(node, ConstExprContext::StaticLocalInitializer);
        } else if Param::cast(node).is_some() {
            let context = if is_promoted_parameter(node) {
                ConstExprContext::PromotedPropertyDefault
            } else {
                ConstExprContext::ParameterDefault
            };
            self.collect_direct_expressions(node, context);
        } else if Attribute::cast(node).is_some() {
            self.collect_attribute_arguments(node);
        }

        for child in syntax_child_nodes(node) {
            self.collect_node(child);
        }
    }

    fn collect_direct_expressions(&mut self, node: &SyntaxNode, context: ConstExprContext) {
        for expr_node in syntax_child_nodes(node).filter(|child| ExprNode::cast(child).is_some()) {
            if let Some(expr_id) = self.expr_id_for_node(expr_node) {
                lower_const_expr_candidate(
                    self.database,
                    self.module_id,
                    expr_id,
                    context,
                    expr_node.text_range(),
                    self.reporter,
                );
            }
        }
    }

    fn collect_attribute_arguments(&mut self, node: &SyntaxNode) {
        let Some(argument_start) = syntax_child_tokens(node)
            .find(|token| token.text() == "(")
            .map(|token| token.text_range().end().to_usize())
        else {
            return;
        };

        for expr_node in syntax_child_nodes(node).filter(|child| {
            ExprNode::cast(child).is_some()
                && child.text_range().start().to_usize() >= argument_start
        }) {
            if let Some(expr_id) = self.expr_id_for_node(expr_node) {
                lower_const_expr_candidate(
                    self.database,
                    self.module_id,
                    expr_id,
                    ConstExprContext::AttributeArgument,
                    expr_node.text_range(),
                    self.reporter,
                );
            }
        }
    }

    fn expr_id_for_node(&self, node: &SyntaxNode) -> Option<ExprId> {
        self.expr_id_for_span(node.text_range()).or_else(|| {
            if node.kind().name() != "EXPR" {
                return None;
            }
            syntax_child_nodes(node)
                .filter(|child| ExprNode::cast(child).is_some())
                .last()
                .and_then(|child| self.expr_id_for_node(child))
        })
    }

    fn expr_id_for_span(&self, span: TextRange) -> Option<ExprId> {
        let module = self
            .database
            .module(self.module_id)
            .expect("module allocated before const-expression lowering");
        module
            .expressions()
            .iter()
            .find_map(|(id, _)| (self.database.source_map().span(id) == Some(span)).then_some(id))
    }
}

fn allowed(kind: ConstExprKind) -> ConstExprValidation {
    ConstExprValidation {
        kind,
        allowed: true,
    }
}

fn disallowed(kind: ConstExprKind) -> ConstExprValidation {
    ConstExprValidation {
        kind,
        allowed: false,
    }
}

fn aggregate(
    module: &HirModule,
    kind: ConstExprKind,
    children: impl IntoIterator<Item = ExprId>,
    context: ConstExprContext,
) -> ConstExprValidation {
    let allowed = children
        .into_iter()
        .all(|child| validate_const_expr_allowed_forms(module, child, context).allowed);
    ConstExprValidation { kind, allowed }
}

fn validate_new_argument_allowed(
    module: &HirModule,
    expr_id: ExprId,
    context: ConstExprContext,
) -> bool {
    match module.expressions()[expr_id].kind() {
        HirExprKind::Call { callee: None, args } => args
            .iter()
            .all(|arg| validate_const_expr_allowed_forms(module, arg.value, context).allowed),
        HirExprKind::BuiltinCall { args, .. } => args
            .iter()
            .all(|arg| validate_const_expr_allowed_forms(module, arg.value, context).allowed),
        _ => validate_const_expr_allowed_forms(module, expr_id, context).allowed,
    }
}

fn fold_literal(text: &str) -> Option<ConstValue> {
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("null") {
        return Some(ConstValue::Null);
    }
    if trimmed.eq_ignore_ascii_case("true") {
        return Some(ConstValue::Bool(true));
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return Some(ConstValue::Bool(false));
    }
    if is_magic_constant_like(trimmed) {
        return None;
    }
    if let Some(value) = parse_single_quoted_string(trimmed)
        .or_else(|| parse_double_quoted_string_without_interpolation(trimmed))
    {
        return Some(ConstValue::String(value));
    }
    parse_decimal_int(trimmed).map(ConstValue::Int)
}

fn is_magic_constant_like(text: &str) -> bool {
    matches!(
        text.to_ascii_lowercase().as_str(),
        "__line__"
            | "__file__"
            | "__dir__"
            | "__function__"
            | "__class__"
            | "__trait__"
            | "__method__"
            | "__namespace__"
    )
}

fn parse_decimal_int(text: &str) -> Option<i64> {
    let digits = text.replace('_', "");
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn parse_single_quoted_string(text: &str) -> Option<String> {
    let text = strip_binary_string_prefix(text);
    let body = text.strip_prefix('\'')?.strip_suffix('\'')?;
    let mut out = String::new();
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('\\') => out.push('\\'),
                Some('\'') => out.push('\''),
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    Some(out)
}

fn parse_double_quoted_string_without_interpolation(text: &str) -> Option<String> {
    let text = strip_binary_string_prefix(text);
    let body = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        if ch == '$' || ch == '{' {
            return None;
        }
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('v') => out.push('\u{0b}'),
                Some('e') => out.push('\u{1b}'),
                Some('f') => out.push('\u{0c}'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('$') => out.push('$'),
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    Some(out)
}

fn strip_binary_string_prefix(text: &str) -> &str {
    let bytes = text.as_bytes();
    if matches!(bytes, [b'b' | b'B', b'\'' | b'"', ..]) {
        &text[1..]
    } else {
        text
    }
}

fn is_promoted_parameter(node: &SyntaxNode) -> bool {
    syntax_child_tokens(node).any(|token| {
        matches!(
            token.kind().name().as_str(),
            "T_PUBLIC"
                | "T_PROTECTED"
                | "T_PRIVATE"
                | "T_READONLY"
                | "T_PUBLIC_SET"
                | "T_PROTECTED_SET"
                | "T_PRIVATE_SET"
        )
    })
}
