//! Shared rule-selection metadata for Region IR optimization.

use std::collections::BTreeMap;

/// Stable rule identifier within one selection report.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuleId(u32);

impl RuleId {
    /// Creates a rule identifier from a report-local index.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw report-local identifier.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Shared rule kind vocabulary.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RuleKind {
    NoRule,
    Param,
    Const,
    Move,
    BinaryInt,
    BinaryString,
    Compare,
    CompareAndBranch,
    LoadLocalEcho,
    LoadConstEcho,
    ConcatEcho,
    PackedFetchConst,
    IssetDimJump,
    PropertyShapeFetch,
    KnownBuiltinCall,
    KnownMethodDispatch,
    ReturnValue,
    Skipped,
    FusedInto(RuleId),
}

impl RuleKind {
    /// Stable report spelling.
    #[must_use]
    pub fn as_str(&self) -> String {
        match self {
            Self::NoRule => "no_rule".to_string(),
            Self::Param => "param".to_string(),
            Self::Const => "const".to_string(),
            Self::Move => "move".to_string(),
            Self::BinaryInt => "binary_int".to_string(),
            Self::BinaryString => "binary_string".to_string(),
            Self::Compare => "compare".to_string(),
            Self::CompareAndBranch => "compare_and_branch".to_string(),
            Self::LoadLocalEcho => "load_local_echo".to_string(),
            Self::LoadConstEcho => "load_const_echo".to_string(),
            Self::ConcatEcho => "concat_echo".to_string(),
            Self::PackedFetchConst => "packed_fetch_const".to_string(),
            Self::IssetDimJump => "isset_dim_jump".to_string(),
            Self::PropertyShapeFetch => "property_shape_fetch".to_string(),
            Self::KnownBuiltinCall => "known_builtin_call".to_string(),
            Self::KnownMethodDispatch => "known_method_dispatch".to_string(),
            Self::ReturnValue => "return_value".to_string(),
            Self::Skipped => "skipped".to_string(),
            Self::FusedInto(parent) => format!("fused_into_r{}", parent.raw()),
        }
    }
}

/// Operand constraints for native rule selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleOperandConstraint {
    /// Operand index within the selected rule.
    pub operand_index: u32,
    /// Stable constraint label.
    pub constraint: String,
}

/// One selected, skipped, or fused rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleSelection {
    /// Report-local rule ID.
    pub id: RuleId,
    /// Selected rule kind.
    pub kind: RuleKind,
    /// Original IR instruction or Region node indexes covered.
    pub source_indexes: Vec<u32>,
    /// Optional parent fused rule.
    pub parent: Option<RuleId>,
    /// Stable skip/reject reason.
    pub reason: Option<String>,
    /// Placeholder operand constraints.
    pub operand_constraints: Vec<RuleOperandConstraint>,
}

impl RuleSelection {
    /// Creates a selected rule.
    #[must_use]
    pub fn selected(id: RuleId, kind: RuleKind, source_indexes: Vec<u32>) -> Self {
        Self {
            id,
            kind,
            source_indexes,
            parent: None,
            reason: None,
            operand_constraints: Vec::new(),
        }
    }

    /// Creates a skipped child rule fused into a parent.
    #[must_use]
    pub fn fused_child(id: RuleId, parent: RuleId, source_indexes: Vec<u32>) -> Self {
        Self {
            id,
            kind: RuleKind::FusedInto(parent),
            source_indexes,
            parent: Some(parent),
            reason: Some("fused_child".to_string()),
            operand_constraints: Vec::new(),
        }
    }

    /// Creates an unsupported skipped rule.
    #[must_use]
    pub fn skipped(id: RuleId, source_indexes: Vec<u32>, reason: impl Into<String>) -> Self {
        Self {
            id,
            kind: RuleKind::Skipped,
            source_indexes,
            parent: None,
            reason: Some(reason.into()),
            operand_constraints: Vec::new(),
        }
    }
}

/// Rule-selection counters and selections.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuleSelectionReport {
    /// Candidate nodes/instructions considered.
    pub rule_selection_candidates: u64,
    /// Rules selected.
    pub rule_selection_selected: u64,
    /// Child instructions/nodes fused into a parent rule.
    pub rule_selection_fused: u64,
    /// Unsupported or deliberately skipped nodes/instructions.
    pub rule_selection_skipped: u64,
    /// Selected/fused/skipped rules grouped by stable kind.
    pub rule_selection_by_kind: BTreeMap<String, u64>,
    /// Stable ordered selection list.
    pub selections: Vec<RuleSelection>,
}

impl RuleSelectionReport {
    /// Adds a selection and updates counters.
    pub fn push(&mut self, selection: RuleSelection) {
        self.rule_selection_candidates += selection.source_indexes.len().max(1) as u64;
        match &selection.kind {
            RuleKind::Skipped => self.rule_selection_skipped += 1,
            RuleKind::FusedInto(_) => self.rule_selection_fused += 1,
            _ => self.rule_selection_selected += 1,
        }
        *self
            .rule_selection_by_kind
            .entry(selection.kind.as_str())
            .or_default() += 1;
        self.selections.push(selection);
    }

    /// Creates the next report-local ID.
    #[must_use]
    pub fn next_id(&self) -> RuleId {
        RuleId::new(self.selections.len() as u32)
    }

    /// Stable textual dump.
    #[must_use]
    pub fn dump_text(&self) -> String {
        let mut out = String::new();
        out.push_str("rule-selection\n");
        out.push_str(&format!(
            "candidates={}\nselected={}\nfused={}\nskipped={}\n",
            self.rule_selection_candidates,
            self.rule_selection_selected,
            self.rule_selection_fused,
            self.rule_selection_skipped
        ));
        out.push_str("by-kind:\n");
        for (kind, count) in &self.rule_selection_by_kind {
            out.push_str("  ");
            out.push_str(kind);
            out.push('=');
            out.push_str(&count.to_string());
            out.push('\n');
        }
        out.push_str("selections:\n");
        for selection in &self.selections {
            out.push_str("  r");
            out.push_str(&selection.id.raw().to_string());
            out.push(' ');
            out.push_str(&selection.kind.as_str());
            out.push_str(" sources=[");
            for (index, source) in selection.source_indexes.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&source.to_string());
            }
            out.push(']');
            if let Some(parent) = selection.parent {
                out.push_str(" parent=r");
                out.push_str(&parent.raw().to_string());
            }
            if let Some(reason) = &selection.reason {
                out.push_str(" reason=");
                out.push_str(reason);
            }
            out.push('\n');
        }
        out
    }
}
