use crate::counters::VmCounters;
use crate::tiering::TieringStats;
use php_jit::NativeCacheStats;
use php_runtime::api::{
    ExecutionStatus, OutputBuffer, RuntimeDiagnostic, RuntimeHttpResponseState, SessionState,
    UploadRegistry, Value,
};

/// Result assembled at the outer native execution boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmResult {
    pub status: ExecutionStatus,
    pub output: OutputBuffer,
    pub diagnostics: Vec<RuntimeDiagnostic>,
    pub http_response: Option<Box<RuntimeHttpResponseState>>,
    pub upload_registry: Option<Box<UploadRegistry>>,
    pub session: Option<Box<SessionState>>,
    pub return_value: Option<Value>,
    pub process_exit_code: Option<i32>,
    pub process_exit_terminates_process: bool,
    pub trace: Vec<String>,
    pub counters: Option<Box<VmCounters>>,
    pub tiering_stats: Option<Box<TieringStats>>,
    pub native_cache_stats: Option<Box<NativeCacheStats>>,
    pub native_cache_load_nanos: u64,
    pub native_compile_nanos: u64,
}

impl VmResult {
    pub(crate) fn success(output: OutputBuffer, return_value: Option<Value>) -> Self {
        Self {
            status: ExecutionStatus::success(),
            output,
            diagnostics: Vec::new(),
            http_response: None,
            upload_registry: None,
            session: None,
            return_value,
            process_exit_code: None,
            process_exit_terminates_process: false,
            trace: Vec::new(),
            counters: None,
            tiering_stats: None,
            native_cache_stats: None,
            native_cache_load_nanos: 0,
            native_compile_nanos: 0,
        }
    }

    pub(crate) fn compile_error(output: OutputBuffer, message: impl Into<String>) -> Self {
        Self {
            status: ExecutionStatus::compile_error(message),
            output,
            diagnostics: Vec::new(),
            http_response: None,
            upload_registry: None,
            session: None,
            return_value: None,
            process_exit_code: None,
            process_exit_terminates_process: false,
            trace: Vec::new(),
            counters: None,
            tiering_stats: None,
            native_cache_stats: None,
            native_cache_load_nanos: 0,
            native_compile_nanos: 0,
        }
    }

    pub(crate) fn runtime_error(
        output: OutputBuffer,
        diagnostic: Option<RuntimeDiagnostic>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            status: ExecutionStatus::runtime_error(message),
            output,
            diagnostics: diagnostic.into_iter().collect(),
            http_response: None,
            upload_registry: None,
            session: None,
            return_value: None,
            process_exit_code: None,
            process_exit_terminates_process: false,
            trace: Vec::new(),
            counters: None,
            tiering_stats: None,
            native_cache_stats: None,
            native_cache_load_nanos: 0,
            native_compile_nanos: 0,
        }
    }

    pub(crate) fn fatal(
        output: OutputBuffer,
        diagnostic: Option<RuntimeDiagnostic>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            status: ExecutionStatus::fatal(message),
            output,
            diagnostics: diagnostic.into_iter().collect(),
            http_response: None,
            upload_registry: None,
            session: None,
            return_value: None,
            process_exit_code: None,
            process_exit_terminates_process: false,
            trace: Vec::new(),
            counters: None,
            tiering_stats: None,
            native_cache_stats: None,
            native_cache_load_nanos: 0,
            native_compile_nanos: 0,
        }
    }

    pub(crate) fn success_exit(output: OutputBuffer, exit_code: i32) -> Self {
        let mut result = Self::success(output, None);
        result.process_exit_code = Some(exit_code);
        result
    }
}
