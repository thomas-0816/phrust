use crate::counters::VmCounters;
use crate::tiering::TieringStats;
#[cfg(test)]
use php_diagnostics::{
    DiagnosticEnvelope, DiagnosticLayer, DiagnosticPhase, DiagnosticSeverity, DiagnosticSuggestion,
};
use php_runtime::api::{
    ExecutionStatus, OutputBuffer, ReferenceCell, RuntimeDiagnostic, RuntimeHttpResponseState,
    SessionState, UploadRegistry, Value,
};
#[cfg(test)]
use std::collections::BTreeMap;

/// Execution result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmResult {
    /// Final execution status.
    pub status: ExecutionStatus,
    /// Captured stdout bytes.
    pub output: OutputBuffer,
    /// Structured runtime diagnostics emitted during execution.
    pub diagnostics: Vec<RuntimeDiagnostic>,
    /// Request-local HTTP response state accumulated by web-response
    /// builtins. Boxed and optional: only the outermost `execute` boundary
    /// attaches it, while every nested function return moves a `VmResult`
    /// by value.
    pub http_response: Option<Box<RuntimeHttpResponseState>>,
    /// Request-local upload registry state after PHP code has executed.
    /// Boxed for the same reason.
    pub upload_registry: Option<Box<UploadRegistry>>,
    /// Request-local session state after PHP code has executed. Boxed and
    /// optional: only the outermost `execute` boundary attaches it, while
    /// every nested function return moves a `VmResult` by value.
    pub session: Option<Box<SessionState>>,
    /// Return value when execution returned successfully.
    pub return_value: Option<Value>,
    /// True when the return value came from an explicit PHP `return`.
    pub(super) returned_explicitly: bool,
    /// Process exit code when PHP `exit`/`die` terminated the script.
    pub process_exit_code: Option<i32>,
    /// Whether the process must terminate directly instead of returning to the caller.
    pub process_exit_terminates_process: bool,
    /// Boxed: generator/fiber suspensions are rare relative to plain
    /// returns, and `VmResult` moves by value on every function return.
    pub(super) yielded: Option<Box<super::GeneratorYield>>,
    pub(super) fiber_suspension: Option<Box<super::FiberSuspension>>,
    pub(super) return_ref: Option<ReferenceCell>,
    /// Deterministic trace events captured when `VmOptions::trace` is enabled.
    pub trace: Vec<String>,
    /// Optional performance VM/runtime counters. Boxed: the flat counter
    /// struct is multiple kilobytes and only the outermost `execute`
    /// boundary attaches it, while every nested function return moves a
    /// `VmResult` by value.
    pub counters: Option<Box<VmCounters>>,
    /// Optional performance tiering stats. Boxed for the same reason.
    pub tiering_stats: Option<Box<TieringStats>>,
}

/// Structured VM max-step diagnostic context.
#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmStepLimitDiagnostic {
    /// Configured maximum VM steps.
    pub max_steps: u64,
    /// Current function ID, when available.
    pub function_id: Option<u32>,
    /// Current block ID, when available.
    pub block_id: Option<u32>,
    /// Current instruction ID, when available.
    pub instruction_id: Option<u32>,
    /// Current opcode, when available.
    pub opcode: Option<String>,
}

#[cfg(test)]
impl VmStepLimitDiagnostic {
    /// Converts this step-limit failure to the shared diagnostic envelope.
    #[must_use]
    pub fn to_diagnostic_envelope(&self) -> DiagnosticEnvelope {
        let mut context = BTreeMap::new();
        context.insert("max_steps".to_string(), self.max_steps.to_string());
        if let Some(function_id) = self.function_id {
            context.insert("function_id".to_string(), function_id.to_string());
        }
        if let Some(block_id) = self.block_id {
            context.insert("block_id".to_string(), block_id.to_string());
        }
        if let Some(instruction_id) = self.instruction_id {
            context.insert("instruction_id".to_string(), instruction_id.to_string());
        }
        if let Some(opcode) = &self.opcode {
            context.insert("opcode".to_string(), opcode.clone());
        }

        let mut envelope = DiagnosticEnvelope::new(
            "E_PHP_VM_STEP_LIMIT",
            DiagnosticLayer::vm(),
            DiagnosticPhase::new("execute"),
            DiagnosticSeverity::FatalError,
            "VM step limit exceeded",
        )
        .with_context(context);
        envelope.suggestion = Some(DiagnosticSuggestion::new(
            "enable debug mode or reduce the reproducer around the reported instruction",
        ));
        envelope.php_visible = false;
        envelope
    }
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
            returned_explicitly: false,
            process_exit_code: None,
            process_exit_terminates_process: false,
            yielded: None,
            fiber_suspension: None,
            return_ref: None,
            trace: Vec::new(),
            counters: None,
            tiering_stats: None,
        }
    }

    pub(crate) fn success_no_output(return_value: Option<Value>) -> Self {
        Self::success(OutputBuffer::new(), return_value)
    }

    pub(crate) fn success_with_diagnostics(
        output: OutputBuffer,
        return_value: Option<Value>,
        diagnostics: Vec<RuntimeDiagnostic>,
    ) -> Self {
        Self {
            status: ExecutionStatus::success(),
            output,
            diagnostics,
            http_response: None,
            upload_registry: None,
            session: None,
            return_value,
            returned_explicitly: false,
            process_exit_code: None,
            process_exit_terminates_process: false,
            yielded: None,
            fiber_suspension: None,
            return_ref: None,
            trace: Vec::new(),
            counters: None,
            tiering_stats: None,
        }
    }

    pub(crate) fn success_with_diagnostics_no_output(
        return_value: Option<Value>,
        diagnostics: Vec<RuntimeDiagnostic>,
    ) -> Self {
        Self::success_with_diagnostics(OutputBuffer::new(), return_value, diagnostics)
    }

    pub(super) fn script_exit(output: OutputBuffer, code: i32, terminates_process: bool) -> Self {
        let mut result = Self::success(output, None);
        result.process_exit_code = Some(code);
        result.process_exit_terminates_process = terminates_process;
        result
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn runtime_error_with_diagnostic(
        output: OutputBuffer,
        message: impl Into<String>,
        diagnostic: RuntimeDiagnostic,
    ) -> Self {
        Self {
            status: ExecutionStatus::runtime_error(message),
            output,
            diagnostics: vec![diagnostic],
            http_response: None,
            upload_registry: None,
            session: None,
            return_value: None,
            returned_explicitly: false,
            process_exit_code: None,
            process_exit_terminates_process: false,
            yielded: None,
            fiber_suspension: None,
            return_ref: None,
            trace: Vec::new(),
            counters: None,
            tiering_stats: None,
        }
    }

    #[cold]
    #[inline(never)]
    pub(super) fn compile_error(output: OutputBuffer, message: impl Into<String>) -> Self {
        Self {
            status: ExecutionStatus::compile_error(message),
            output,
            diagnostics: Vec::new(),
            http_response: None,
            upload_registry: None,
            session: None,
            return_value: None,
            returned_explicitly: false,
            process_exit_code: None,
            process_exit_terminates_process: false,
            yielded: None,
            fiber_suspension: None,
            return_ref: None,
            trace: Vec::new(),
            counters: None,
            tiering_stats: None,
        }
    }

    pub(super) fn unsupported(output: OutputBuffer, message: impl Into<String>) -> Self {
        Self {
            status: ExecutionStatus::unsupported(message),
            output,
            diagnostics: Vec::new(),
            http_response: None,
            upload_registry: None,
            session: None,
            return_value: None,
            returned_explicitly: false,
            process_exit_code: None,
            process_exit_terminates_process: false,
            yielded: None,
            fiber_suspension: None,
            return_ref: None,
            trace: Vec::new(),
            counters: None,
            tiering_stats: None,
        }
    }

    /// Non-success result marking that a throwable is unwinding the call stack.
    ///
    /// The throwable itself travels in `ExecutionState::pending_throw`; this
    /// result only signals callers (via `!is_success()`) to consult it.
    pub(super) fn propagating_exception(output: OutputBuffer) -> Self {
        Self {
            status: ExecutionStatus::runtime_error(
                "E_PHP_VM_PENDING_EXCEPTION: exception unwinding call stack",
            ),
            output,
            diagnostics: Vec::new(),
            http_response: None,
            upload_registry: None,
            session: None,
            return_value: None,
            returned_explicitly: false,
            process_exit_code: None,
            process_exit_terminates_process: false,
            yielded: None,
            fiber_suspension: None,
            return_ref: None,
            trace: Vec::new(),
            counters: None,
            tiering_stats: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vm_step_limit_has_shared_envelope_context() {
        let diagnostic = VmStepLimitDiagnostic {
            max_steps: 100,
            function_id: Some(1),
            block_id: Some(2),
            instruction_id: Some(3),
            opcode: Some("jump".to_string()),
        };

        let envelope = diagnostic.to_diagnostic_envelope();
        let json: serde_json::Value =
            serde_json::from_str(&envelope.compact_json().expect("json")).expect("parse json");

        assert_eq!(json["code"], "E_PHP_VM_STEP_LIMIT");
        assert_eq!(json["layer"], "vm");
        assert_eq!(json["phase"], "execute");
        assert_eq!(json["context"]["max_steps"], "100");
        assert_eq!(json["context"]["function_id"], "1");
        assert_eq!(json["context"]["block_id"], "2");
        assert_eq!(json["context"]["instruction_id"], "3");
        assert_eq!(json["context"]["opcode"], "jump");
    }
}

#[cfg(test)]
mod size_tests {

    /// `VmResult` is returned by value from every nested function call, so
    /// inline payload growth multiplies across the whole interpreter. The
    /// unboxed counters struct alone once pushed this to ~5.7 KB per return.
    #[test]
    fn vm_result_stays_call_sized() {
        let size = std::mem::size_of::<super::VmResult>();
        assert!(
            size <= 320,
            "VmResult grew to {size} bytes; it is moved by value on every function return — box new large fields"
        );
    }
}
