//! HIR statement records.

use crate::hir::{ExprId, StmtId};

/// Statement record stored in the module statement arena.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirStmt {
    kind: HirStmtKind,
}

impl HirStmt {
    /// Creates a statement record.
    #[must_use]
    pub const fn new(kind: HirStmtKind) -> Self {
        Self { kind }
    }

    /// Returns the statement kind.
    #[must_use]
    pub const fn kind(&self) -> &HirStmtKind {
        &self.kind
    }
}

/// Statement families lowered from CST without execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HirStmtKind {
    /// Lowering could not produce a complete statement.
    Missing,
    /// Expression statement.
    Expr { expr: Option<ExprId> },
    /// Block statement.
    Block { statements: Vec<StmtId> },
    /// If statement.
    If {
        condition: Option<ExprId>,
        body: Vec<StmtId>,
        elseifs: Vec<HirIfBranch>,
        else_body: Vec<StmtId>,
    },
    /// While statement.
    While {
        condition: Option<ExprId>,
        body: Vec<StmtId>,
    },
    /// Do/while statement.
    DoWhile {
        condition: Option<ExprId>,
        body: Vec<StmtId>,
    },
    /// For statement.
    For {
        init: Vec<ExprId>,
        condition: Vec<ExprId>,
        update: Vec<ExprId>,
        body: Vec<StmtId>,
    },
    /// Foreach statement.
    Foreach {
        /// Iterable expression.
        source: Option<ExprId>,
        /// Optional key target for `key => value` foreach.
        key_target: Option<ExprId>,
        /// Value target.
        value_target: Option<ExprId>,
        /// Whether the value target is by-reference.
        by_ref: bool,
        body: Vec<StmtId>,
    },
    /// Switch statement.
    Switch {
        condition: Option<ExprId>,
        body: Vec<StmtId>,
        cases: Vec<HirSwitchCase>,
    },
    /// Try statement.
    Try {
        body: Vec<StmtId>,
        catches: Vec<HirCatchClause>,
        finally_body: Vec<StmtId>,
    },
    /// Return statement.
    Return { expr: Option<ExprId> },
    /// Throw statement.
    Throw { expr: Option<ExprId> },
    /// Break statement.
    Break { expr: Option<ExprId> },
    /// Continue statement.
    Continue { expr: Option<ExprId> },
    /// Declare statement.
    Declare {
        expressions: Vec<ExprId>,
        body: Vec<StmtId>,
    },
    /// Global statement.
    Global { variables: Vec<ExprId> },
    /// Static local statement.
    Static { variables: Vec<ExprId> },
    /// Unset statement.
    Unset { expressions: Vec<ExprId> },
    /// Echo statement.
    Echo { expressions: Vec<ExprId> },
    /// Inline HTML statement.
    InlineHtml { text: String },
    /// Label statement.
    Label { name: Option<String> },
    /// Goto statement.
    Goto { label: Option<String> },
    /// Placeholder for a CST statement that is intentionally only categorized.
    Unlowered { syntax_kind: String },
}

/// One `elseif` branch in an if statement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirIfBranch {
    /// Branch condition.
    pub condition: Option<ExprId>,
    /// Statements executed when the condition is truthy.
    pub body: Vec<StmtId>,
}

/// One `case` or `default` section in a switch statement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirSwitchCase {
    /// Case expression, or `None` for `default`.
    pub condition: Option<ExprId>,
    /// Statements in the section before the next label.
    pub body: Vec<StmtId>,
    /// Whether this section came from `default`.
    pub is_default: bool,
}

/// One MVP catch clause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirCatchClause {
    /// Static catch type names in source order.
    pub types: Vec<String>,
    /// Optional caught-exception variable name without `$`.
    pub variable: Option<String>,
    /// Catch body.
    pub body: Vec<StmtId>,
}

impl HirStmtKind {
    /// Returns stable JSON text for the statement family.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Expr { .. } => "expr",
            Self::Block { .. } => "block",
            Self::If { .. } => "if",
            Self::While { .. } => "while",
            Self::DoWhile { .. } => "do_while",
            Self::For { .. } => "for",
            Self::Foreach { .. } => "foreach",
            Self::Switch { .. } => "switch",
            Self::Try { .. } => "try",
            Self::Return { .. } => "return",
            Self::Throw { .. } => "throw",
            Self::Break { .. } => "break",
            Self::Continue { .. } => "continue",
            Self::Declare { .. } => "declare",
            Self::Global { .. } => "global",
            Self::Static { .. } => "static",
            Self::Unset { .. } => "unset",
            Self::Echo { .. } => "echo",
            Self::InlineHtml { .. } => "inline_html",
            Self::Label { .. } => "label",
            Self::Goto { .. } => "goto",
            Self::Unlowered { .. } => "unlowered",
        }
    }
}
