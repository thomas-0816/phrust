//! HIR expression records.

use crate::hir::ExprId;

/// Expression record stored in the module expression arena.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirExpr {
    kind: HirExprKind,
}

impl HirExpr {
    /// Creates an expression record.
    #[must_use]
    pub const fn new(kind: HirExprKind) -> Self {
        Self { kind }
    }

    /// Creates a missing expression record for recovery-safe lowering.
    #[must_use]
    pub const fn missing() -> Self {
        Self::new(HirExprKind::Missing)
    }

    /// Returns the expression kind.
    #[must_use]
    pub const fn kind(&self) -> &HirExprKind {
        &self.kind
    }
}

/// Expression families lowered from CST without execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HirExprKind {
    /// Lowering could not produce a complete expression.
    Missing,
    /// Literal token or string-like source.
    Literal { text: String },
    /// Variable fetch.
    Variable {
        /// Exact non-trivia variable spelling retained for diagnostics and maps.
        name: String,
        /// Number of direct `$` sigils on this variable node.
        sigil_count: usize,
        /// Nested expression for variable variables such as `${$name}`.
        dynamic: Option<ExprId>,
    },
    /// Statically visible source name.
    Name { resolution: HirNameResolution },
    /// Array expression.
    Array { elements: Vec<ExprId> },
    /// Array key/value pair or append element.
    ArrayPair {
        key: Option<ExprId>,
        value: Option<ExprId>,
        unpack: bool,
        by_ref: bool,
    },
    /// List/destructuring expression.
    List { elements: Vec<ExprId> },
    /// Unary, prefix, postfix, or throw-like expression.
    Unary {
        operator: String,
        expr: Option<ExprId>,
    },
    /// Binary expression.
    Binary {
        operator: String,
        left: Option<ExprId>,
        right: Option<ExprId>,
    },
    /// Assignment expression.
    Assign {
        operator: String,
        left: Option<ExprId>,
        right: Option<ExprId>,
    },
    /// Ternary expression.
    Ternary {
        condition: Option<ExprId>,
        if_true: Option<ExprId>,
        if_false: Option<ExprId>,
    },
    /// Function-like call.
    Call {
        callee: Option<ExprId>,
        args: Vec<HirCallArg>,
    },
    /// Function-like builtin construct with no explicit source name node.
    BuiltinCall { name: String, args: Vec<HirCallArg> },
    /// Object method call.
    MethodCall {
        receiver: Option<ExprId>,
        method: Option<ExprId>,
        args: Vec<HirCallArg>,
        nullsafe: bool,
    },
    /// Object property fetch.
    PropertyFetch {
        receiver: Option<ExprId>,
        property: Option<ExprId>,
        nullsafe: bool,
    },
    /// Static access or static call target.
    StaticAccess {
        target: Option<ExprId>,
        member: Option<ExprId>,
    },
    /// Array dimension fetch.
    DimFetch {
        receiver: Option<ExprId>,
        dim: Option<ExprId>,
    },
    /// Closure expression.
    Closure { body: Vec<ExprId> },
    /// Arrow function expression.
    ArrowFunction { expr: Option<ExprId> },
    /// Object creation.
    New {
        class: Option<ExprId>,
        args: Vec<HirCallArg>,
    },
    /// Clone expression.
    Clone { expr: Option<ExprId> },
    /// PHP 8.5 pipe expression.
    Pipe {
        input: Option<ExprId>,
        callable: Option<ExprId>,
    },
    /// PHP 8.5 clone-with expression.
    CloneWith {
        expr: Option<ExprId>,
        replacements: Vec<ExprId>,
    },
    /// Match expression.
    Match {
        subject: Option<ExprId>,
        arms: Vec<HirMatchArm>,
    },
    /// Yield expression.
    Yield {
        key: Option<ExprId>,
        value: Option<ExprId>,
    },
    /// Yield from expression.
    YieldFrom { expr: Option<ExprId> },
    /// Include or require expression. Not executed.
    Include {
        kind: String,
        expr: Option<ExprId>,
        deferred_effects: DeferredEffects,
    },
    /// Eval expression. Not executed.
    Eval {
        expr: Option<ExprId>,
        deferred_effects: DeferredEffects,
    },
    /// Exit or die expression. Not executed.
    Exit { expr: Option<ExprId> },
    /// Cast expression.
    Cast { kind: String, expr: Option<ExprId> },
    /// First-class callable expression.
    FirstClassCallable { callee: Option<ExprId> },
    /// Placeholder for a CST expression that is intentionally only categorized.
    Unlowered { syntax_kind: String },
}

/// One PHP call argument after HIR lowering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirCallArg {
    /// Optional named-argument label without trailing `:`.
    pub name: Option<String>,
    /// Argument value expression.
    pub value: ExprId,
    /// True when the argument was introduced by `...`.
    pub unpack: bool,
}

/// One arm in a PHP `match` expression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirMatchArm {
    /// Arm conditions. Empty for `default`.
    pub conditions: Vec<ExprId>,
    /// Result expression for the arm.
    pub result: Option<ExprId>,
    /// Whether this arm came from `default`.
    pub is_default: bool,
}

impl HirExprKind {
    /// Returns stable JSON text for the expression family.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Literal { .. } => "literal",
            Self::Variable { .. } => "variable",
            Self::Name { .. } => "name",
            Self::Array { .. } => "array",
            Self::ArrayPair { .. } => "array_pair",
            Self::List { .. } => "list",
            Self::Unary { .. } => "unary",
            Self::Binary { .. } => "binary",
            Self::Assign { .. } => "assign",
            Self::Ternary { .. } => "ternary",
            Self::Call { .. } => "call",
            Self::BuiltinCall { .. } => "builtin_call",
            Self::MethodCall { .. } => "method_call",
            Self::PropertyFetch { .. } => "property_fetch",
            Self::StaticAccess { .. } => "static_access",
            Self::DimFetch { .. } => "dim_fetch",
            Self::Closure { .. } => "closure",
            Self::ArrowFunction { .. } => "arrow_function",
            Self::New { .. } => "new",
            Self::Clone { .. } => "clone",
            Self::CloneWith { .. } => "clone_with",
            Self::Match { .. } => "match",
            Self::Yield { .. } => "yield",
            Self::YieldFrom { .. } => "yield_from",
            Self::Include { .. } => "include",
            Self::Eval { .. } => "eval",
            Self::Exit { .. } => "exit",
            Self::Cast { .. } => "cast",
            Self::Pipe { .. } => "pipe",
            Self::FirstClassCallable { .. } => "first_class_callable",
            Self::Unlowered { .. } => "unlowered",
        }
    }
}

/// Runtime effects that Semantic frontend records but does not execute.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeferredEffects {
    may_load_file: bool,
    may_define_symbols: bool,
    may_execute_code: bool,
    scope_effect_deferred: bool,
}

impl DeferredEffects {
    /// Effects for include/require constructs.
    #[must_use]
    pub const fn include_like() -> Self {
        Self {
            may_load_file: true,
            may_define_symbols: true,
            may_execute_code: true,
            scope_effect_deferred: true,
        }
    }

    /// Effects for eval constructs.
    #[must_use]
    pub const fn eval() -> Self {
        Self {
            may_load_file: false,
            may_define_symbols: true,
            may_execute_code: true,
            scope_effect_deferred: true,
        }
    }

    /// Whether runtime execution may load a file.
    #[must_use]
    pub const fn may_load_file(self) -> bool {
        self.may_load_file
    }

    /// Whether runtime execution may define functions, classes, or constants.
    #[must_use]
    pub const fn may_define_symbols(self) -> bool {
        self.may_define_symbols
    }

    /// Whether runtime execution may execute code.
    #[must_use]
    pub const fn may_execute_code(self) -> bool {
        self.may_execute_code
    }

    /// Whether current-scope effects are deferred to runtime-aware layers.
    #[must_use]
    pub const fn scope_effect_deferred(self) -> bool {
        self.scope_effect_deferred
    }
}

/// Source-name resolution attached to `HirExprKind::Name`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirNameResolution {
    source: String,
    context: String,
    classification: String,
    resolved: Option<String>,
    resolved_display: Option<String>,
    fallback: Option<String>,
}

impl HirNameResolution {
    /// Creates a source-name resolution record.
    #[must_use]
    pub fn new(
        source: impl Into<String>,
        context: impl Into<String>,
        classification: impl Into<String>,
        resolved: Option<String>,
        fallback: Option<String>,
    ) -> Self {
        Self::new_with_display(source, context, classification, resolved, None, fallback)
    }

    /// Creates a resolution with separate canonical and source-case FQNs.
    #[must_use]
    pub fn new_with_display(
        source: impl Into<String>,
        context: impl Into<String>,
        classification: impl Into<String>,
        resolved: Option<String>,
        resolved_display: Option<String>,
        fallback: Option<String>,
    ) -> Self {
        Self {
            source: source.into(),
            context: context.into(),
            classification: classification.into(),
            resolved,
            resolved_display,
            fallback,
        }
    }

    /// Returns the source spelling.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the resolution context.
    #[must_use]
    pub fn context(&self) -> &str {
        &self.context
    }

    /// Returns the resolution classification.
    #[must_use]
    pub fn classification(&self) -> &str {
        &self.classification
    }

    /// Returns the resolved canonical name, when statically known.
    #[must_use]
    pub fn resolved(&self) -> Option<&str> {
        self.resolved.as_deref()
    }

    /// Returns the resolved FQN preserving source segment casing.
    #[must_use]
    pub fn resolved_display(&self) -> Option<&str> {
        self.resolved_display.as_deref()
    }

    /// Returns the runtime fallback canonical name, when PHP may fall back.
    #[must_use]
    pub fn fallback(&self) -> Option<&str> {
        self.fallback.as_deref()
    }
}
