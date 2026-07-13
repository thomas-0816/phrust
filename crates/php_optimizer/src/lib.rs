//! performance conservative optimizer framework.
//!
//! The optimizer pass pipeline supports the first
//! conservative optimization pass.

use php_diagnostics::{DiagnosticEnvelope, DiagnosticLayer, DiagnosticPhase, DiagnosticSeverity};
use php_ir::instruction::{CompareOp, TerminatorKind};
use php_ir::{
    BinaryOp, BlockId, ConstId, InstrId, InstructionKind, IrConstant, IrFunction, IrUnit, Operand,
    RegId, UnaryOp, VerificationDiagnosticContext, VerificationError, verify_unit,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

/// Optimization level accepted by the performance CLI.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum OptimizationLevel {
    /// Exact legacy execution path: no optimizer pipeline is run.
    #[default]
    O0,
    /// Conservative performance optimizer pipeline.
    O1,
    /// Reserved higher conservative pipeline.
    O2,
}

impl OptimizationLevel {
    /// Stable CLI spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::O0 => "0",
            Self::O1 => "1",
            Self::O2 => "2",
        }
    }

    /// True when the optimizer pipeline should run.
    #[must_use]
    pub const fn runs_pipeline(self) -> bool {
        !matches!(self, Self::O0)
    }
}

impl fmt::Display for OptimizationLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for OptimizationLevel {
    type Err = ParseOptimizationLevelError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "0" => Ok(Self::O0),
            "1" => Ok(Self::O1),
            "2" => Ok(Self::O2),
            _ => Err(ParseOptimizationLevelError {
                value: value.to_string(),
            }),
        }
    }
}

/// Invalid optimization level.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseOptimizationLevelError {
    value: String,
}

impl fmt::Display for ParseOptimizationLevelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unsupported optimization level `{}`; expected 0, 1, or 2",
            self.value
        )
    }
}

impl std::error::Error for ParseOptimizationLevelError {}

/// Position where a pass runs relative to the verifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PassPhase {
    /// Runs before the verifier check.
    PreVerify,
    /// Runs after a verifier check.
    PostVerify,
}

impl PassPhase {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PreVerify => "pre_verify",
            Self::PostVerify => "post_verify",
        }
    }
}

/// Per-pass execution context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PassContext {
    level: OptimizationLevel,
    enabled_only: Option<BTreeSet<&'static str>>,
    disabled: BTreeSet<&'static str>,
}

impl PassContext {
    /// Creates a context for one optimizer run.
    #[must_use]
    pub fn new(level: OptimizationLevel) -> Self {
        Self {
            level,
            enabled_only: None,
            disabled: BTreeSet::new(),
        }
    }

    /// Enables only the named passes.
    #[must_use]
    pub fn with_enabled_only(mut self, pass_names: impl IntoIterator<Item = &'static str>) -> Self {
        self.enabled_only = Some(pass_names.into_iter().collect());
        self
    }

    /// Disables the named passes.
    #[must_use]
    pub fn with_disabled(mut self, pass_names: impl IntoIterator<Item = &'static str>) -> Self {
        self.disabled = pass_names.into_iter().collect();
        self
    }

    /// Optimization level for this run.
    #[must_use]
    pub const fn level(&self) -> OptimizationLevel {
        self.level
    }

    /// True when a pass should run for this context.
    #[must_use]
    pub fn is_pass_enabled(&self, pass_name: &'static str) -> bool {
        if self.disabled.contains(pass_name) {
            return false;
        }
        self.enabled_only
            .as_ref()
            .is_none_or(|enabled| enabled.contains(pass_name))
    }
}

mod pipeline;
mod reports;
mod transaction;
mod passes {
    mod analyses;
    mod branch;
    mod constant_folding;
    mod copy_propagation;
    mod literal_compaction;
    mod noop;
    mod peephole;

    pub use branch::BranchSimplify;
    pub use constant_folding::ConstantFoldingPass;
    pub use copy_propagation::CopyPropagationPass;
    pub use literal_compaction::LiteralCompactionPass;
    pub use noop::NoopPass;
    pub use peephole::PeepholeSimplify;
}

pub use passes::{
    BranchSimplify, ConstantFoldingPass, CopyPropagationPass, LiteralCompactionPass, NoopPass,
    PeepholeSimplify,
};
pub use pipeline::PassPipeline;
pub use reports::{OptimizationReport, OptimizerPass, PassError, PassReport, PassScopeReport};
pub use transaction::PassTransaction;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
