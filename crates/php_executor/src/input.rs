use php_diagnostics::DiagnosticEnvelope;
use php_optimizer::OptimizationLevel;
use php_runtime::api::{
    ExitStatus, RuntimeContext, RuntimeDiagnostic, RuntimeHttpResponseState, SessionState,
    UploadRegistry, Value,
};
use php_vm::api::{QuickeningSiteSnapshot, TieringStats, VmCounters, VmOptions};
use std::path::PathBuf;

/// Executor-wide defaults.
#[derive(Clone, Debug)]
pub struct PhpExecutorOptions {
    pub optimization_level: OptimizationLevel,
    pub vm_options: VmOptions,
}

impl Default for PhpExecutorOptions {
    fn default() -> Self {
        Self::managed_fast_runtime()
    }
}

/// Source compilation input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhpCompileInput {
    pub source: String,
    pub source_path: String,
    pub optimization_level: Option<OptimizationLevel>,
}

/// One-shot compile and execute input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhpExecutionInput {
    pub source: String,
    pub source_path: String,
    pub real_path: Option<PathBuf>,
    pub cwd: PathBuf,
    pub include_roots: Vec<PathBuf>,
    pub runtime_context: RuntimeContext,
    pub optimization_level: Option<OptimizationLevel>,
    pub collect_counters: bool,
}

/// Per-request execution input for a compiled script.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhpRequestExecutionInput {
    pub real_path: Option<PathBuf>,
    pub cwd: PathBuf,
    pub include_roots: Vec<PathBuf>,
    pub runtime_context: RuntimeContext,
    pub collect_counters: bool,
}

/// Owned PHP execution output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhpExecutionOutput {
    pub stdout: Vec<u8>,
    pub diagnostics_text: String,
    pub diagnostics: Vec<DiagnosticEnvelope>,
    pub status: PhpExecutionStatus,
    pub runtime_diagnostics: Vec<RuntimeDiagnostic>,
    pub http_response: RuntimeHttpResponseState,
    pub upload_registry: UploadRegistry,
    pub session: SessionState,
    pub return_value: Option<Value>,
    pub trace: Vec<String>,
    pub counters: Option<VmCounters>,
    pub tiering_stats: Option<TieringStats>,
    pub quickening_feedback: Vec<QuickeningSiteSnapshot>,
}

impl PhpExecutionOutput {
    pub(crate) fn engine_error(error: String) -> Self {
        Self {
            stdout: Vec::new(),
            diagnostics_text: error,
            diagnostics: Vec::new(),
            status: PhpExecutionStatus::Fatal,
            runtime_diagnostics: Vec::new(),
            http_response: RuntimeHttpResponseState::default(),
            upload_registry: UploadRegistry::default(),
            session: SessionState::default(),
            return_value: None,
            trace: Vec::new(),
            counters: None,
            tiering_stats: None,
            quickening_feedback: Vec::new(),
        }
    }
}

/// Stable status classification for transport layers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhpExecutionStatus {
    Success,
    CompileError,
    RuntimeError,
    Unsupported,
    Fatal,
}

impl From<ExitStatus> for PhpExecutionStatus {
    fn from(status: ExitStatus) -> Self {
        match status {
            ExitStatus::Success => Self::Success,
            ExitStatus::CompileError => Self::CompileError,
            ExitStatus::RuntimeError => Self::RuntimeError,
            ExitStatus::Unsupported => Self::Unsupported,
            ExitStatus::Fatal => Self::Fatal,
        }
    }
}

/// Executor failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PhpExecutionError {
    Compile(Box<PhpExecutionOutput>),
    Engine(String),
}
