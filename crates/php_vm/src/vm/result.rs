use crate::counters::VmCounters;
use crate::tiering::TieringStats;
use php_runtime::{
    ExecutionStatus, OutputBuffer, ReferenceCell, RuntimeDiagnostic, RuntimeHttpResponseState,
    UploadRegistry, Value,
};

/// Execution result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmResult {
    /// Final execution status.
    pub status: ExecutionStatus,
    /// Captured stdout bytes.
    pub output: OutputBuffer,
    /// Structured runtime diagnostics emitted during execution.
    pub diagnostics: Vec<RuntimeDiagnostic>,
    /// Request-local HTTP response state accumulated by web-response builtins.
    pub http_response: RuntimeHttpResponseState,
    /// Request-local upload registry state after PHP code has executed.
    pub upload_registry: UploadRegistry,
    /// Return value when execution returned successfully.
    pub return_value: Option<Value>,
    pub(super) yielded: Option<super::GeneratorYield>,
    pub(super) fiber_suspension: Option<super::FiberSuspension>,
    pub(super) return_ref: Option<ReferenceCell>,
    /// Deterministic trace events captured when `VmOptions::trace` is enabled.
    pub trace: Vec<String>,
    /// Optional performance VM/runtime counters.
    pub counters: Option<VmCounters>,
    /// Optional performance tiering stats.
    pub tiering_stats: Option<TieringStats>,
}

/// VM control-flow signal, kept separate from runtime diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VmControlFlow {
    /// Function return.
    Return(Option<Value>),
    /// Future exception throw signal.
    Throw(Value),
    /// Loop break signal.
    Break,
    /// Loop continue signal.
    Continue,
}
