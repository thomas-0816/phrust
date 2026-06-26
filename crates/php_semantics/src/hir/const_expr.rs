//! Constant-expression candidate HIR records.

use crate::hir::ids::ExprId;

pub use crate::hir::ids::ConstExprId;

/// Constant-expression candidate record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstExpr {
    context: ConstExprContext,
    kind: ConstExprKind,
    expr_id: ExprId,
    allowed: bool,
    folded_value: Option<ConstValue>,
}

impl ConstExpr {
    /// Creates a constant-expression candidate.
    #[must_use]
    pub const fn new(
        context: ConstExprContext,
        kind: ConstExprKind,
        expr_id: ExprId,
        allowed: bool,
        folded_value: Option<ConstValue>,
    ) -> Self {
        Self {
            context,
            kind,
            expr_id,
            allowed,
            folded_value,
        }
    }

    /// Returns the source context.
    #[must_use]
    pub const fn context(&self) -> ConstExprContext {
        self.context
    }

    /// Returns the structural candidate kind.
    #[must_use]
    pub const fn kind(&self) -> ConstExprKind {
        self.kind
    }

    /// Returns the expression HIR node this candidate annotates.
    #[must_use]
    pub const fn expr_id(&self) -> ExprId {
        self.expr_id
    }

    /// Returns whether the candidate is structurally allowed.
    #[must_use]
    pub const fn is_allowed(&self) -> bool {
        self.allowed
    }

    /// Returns the conservative folded value, when one was proven pure.
    #[must_use]
    pub const fn folded_value(&self) -> Option<&ConstValue> {
        self.folded_value.as_ref()
    }
}

/// Constant-expression source context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstExprContext {
    /// Namespace-level `const` initializer.
    GlobalConstInitializer,
    /// Class constant initializer.
    ClassConstInitializer,
    /// Enum case backing value.
    EnumCaseBackingValue,
    /// Function-like parameter default.
    ParameterDefault,
    /// Attribute argument.
    AttributeArgument,
    /// Static local variable initializer.
    StaticLocalInitializer,
    /// Property default value.
    PropertyDefault,
    /// Promoted property default value sourced from a parameter default.
    PromotedPropertyDefault,
}

impl ConstExprContext {
    /// Returns stable JSON text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GlobalConstInitializer => "global_const_initializer",
            Self::ClassConstInitializer => "class_const_initializer",
            Self::EnumCaseBackingValue => "enum_case_backing_value",
            Self::ParameterDefault => "parameter_default",
            Self::AttributeArgument => "attribute_argument",
            Self::StaticLocalInitializer => "static_local_initializer",
            Self::PropertyDefault => "property_default",
            Self::PromotedPropertyDefault => "promoted_property_default",
        }
    }
}

/// Structural constant-expression classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstExprKind {
    /// Missing expression placeholder.
    Missing,
    /// Scalar literal or magic constant token.
    ScalarLiteral,
    /// Array or list-like expression.
    Array,
    /// Unary expression.
    Unary,
    /// Binary or coalesce expression.
    Binary,
    /// Ternary expression.
    Ternary,
    /// Constant name fetch.
    Name,
    /// Class constant or enum-case-like fetch.
    ClassConstFetch,
    /// Closure expression accepted as symbolic constant expression.
    Closure,
    /// Arrow function expression accepted as symbolic constant expression.
    ArrowFunction,
    /// First-class callable expression.
    FirstClassCallable,
    /// Cast expression.
    Cast,
    /// Object creation expression.
    New,
    /// Array or string dimension fetch (e.g. `[1, 2][0]` or `"ab"[0]`).
    Dim,
    /// Structurally disallowed expression family.
    Disallowed,
}

impl ConstExprKind {
    /// Returns stable JSON text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::ScalarLiteral => "scalar_literal",
            Self::Array => "array",
            Self::Unary => "unary",
            Self::Binary => "binary",
            Self::Ternary => "ternary",
            Self::Name => "name",
            Self::ClassConstFetch => "class_const_fetch",
            Self::Closure => "closure",
            Self::ArrowFunction => "arrow_function",
            Self::FirstClassCallable => "first_class_callable",
            Self::Cast => "cast",
            Self::New => "new",
            Self::Dim => "dim",
            Self::Disallowed => "disallowed",
        }
    }
}

/// Conservative symbolic value for pure constant-expression folding.
///
/// This is not a PHP zval, does not model runtime identity, and intentionally
/// omits values whenever folding would require PHP runtime semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConstValue {
    /// Literal `null`.
    Null,
    /// Literal boolean.
    Bool(bool),
    /// Safely parsed integer literal.
    Int(i64),
    /// Literal string or string concatenation proven from literals.
    String(String),
    /// Symbolic class-constant or unresolved constant reference.
    UnresolvedRef(String),
    /// Symbolic closure constant expression marker.
    ClosureConst,
    /// Symbolic first-class callable constant expression marker.
    CallableConst,
}

impl ConstValue {
    /// Returns stable JSON text for the folded value family.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::String(_) => "string",
            Self::UnresolvedRef(_) => "unresolved_ref",
            Self::ClosureConst => "closure_const",
            Self::CallableConst => "callable_const",
        }
    }
}
