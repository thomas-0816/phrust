//! Backend-neutral JIT API.
//!
//! This module defines the boundary the VM can use without depending on a
//! concrete Cranelift implementation.

#[cfg(feature = "jit-cranelift")]
use crate::CraneliftNoExecBackend;
use crate::{
    JitBackend, JitCompileRequest, JitCompileStatus, JitFunctionHandle, JitRuntimeHelperAddresses,
};
use php_ir::{FunctionId, IrUnit};

/// Backend compile request shared by all native-code experiments.
#[derive(Debug)]
pub struct JitBackendCompileRequest<'a> {
    /// Stable region metadata chosen by the caller.
    pub compile: &'a JitCompileRequest,
    /// Optional IR unit when the backend needs to inspect or lower code.
    pub unit: Option<&'a IrUnit>,
    /// Optional function inside the IR unit.
    pub function: Option<FunctionId>,
    /// Runtime permission for native execution.
    pub allow_native_execution: bool,
    /// Runtime-owned helper function addresses available to native code.
    pub runtime_helpers: JitRuntimeHelperAddresses,
}

/// Backend compile outcome without VM-specific counter updates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitBackendCompileOutcome {
    /// Backend-specific compile status.
    pub status: JitCompileStatus,
    /// Opaque compiled function handle when native code exists.
    pub handle: Option<JitFunctionHandle>,
    /// Stable diagnostics for reports and smoke gates.
    pub diagnostics: Vec<String>,
    /// Native code bytes produced by the backend for this region.
    pub code_bytes: u64,
    /// Compile latency in nanoseconds for successful native compiles.
    pub compile_time_nanos: u64,
}

impl JitBackendCompileOutcome {
    /// Creates an outcome without a compiled handle.
    #[must_use]
    pub fn skipped(status: JitCompileStatus, diagnostic: impl Into<String>) -> Self {
        Self {
            status,
            handle: None,
            diagnostics: vec![diagnostic.into()],
            code_bytes: 0,
            compile_time_nanos: 0,
        }
    }

    /// Creates a successful native compile outcome.
    #[must_use]
    pub fn compiled(
        handle: JitFunctionHandle,
        diagnostic: impl Into<String>,
        code_bytes: u64,
        compile_time_nanos: u64,
    ) -> Self {
        Self {
            status: JitCompileStatus::Compiled,
            handle: Some(handle),
            diagnostics: vec![diagnostic.into()],
            code_bytes,
            compile_time_nanos,
        }
    }
}

/// Backend-neutral compile interface.
pub trait JitBackendApi {
    /// Stable backend kind.
    fn backend(&self) -> JitBackend;

    /// Attempts to compile one region.
    fn compile_region(
        &mut self,
        request: &JitBackendCompileRequest<'_>,
    ) -> JitBackendCompileOutcome;
}

/// No-op backend used by default-off builds.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NoopJitBackend;

impl JitBackendApi for NoopJitBackend {
    fn backend(&self) -> JitBackend {
        JitBackend::Stub
    }

    fn compile_region(
        &mut self,
        request: &JitBackendCompileRequest<'_>,
    ) -> JitBackendCompileOutcome {
        let function_detail = request
            .function
            .map(|function| format!(" function={}", function.raw()))
            .unwrap_or_default();
        JitBackendCompileOutcome::skipped(
            JitCompileStatus::BackendUnavailable,
            format!(
                "jit backend unavailable for region `{}`{}",
                request.compile.region_id, function_detail
            ),
        )
    }
}

/// Build-selected backend adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentJitBackend {
    backend: JitBackend,
}

impl CurrentJitBackend {
    /// Creates the current backend adapter for the build.
    #[must_use]
    pub const fn new(backend: JitBackend) -> Self {
        Self { backend }
    }
}

impl Default for CurrentJitBackend {
    fn default() -> Self {
        Self {
            backend: JitBackend::current(),
        }
    }
}

impl JitBackendApi for CurrentJitBackend {
    fn backend(&self) -> JitBackend {
        self.backend
    }

    fn compile_region(
        &mut self,
        request: &JitBackendCompileRequest<'_>,
    ) -> JitBackendCompileOutcome {
        match self.backend {
            JitBackend::Stub => NoopJitBackend.compile_region(request),
            JitBackend::CraneliftExperiment => {
                #[cfg(feature = "jit-cranelift")]
                {
                    let mut backend = CraneliftNoExecBackend;
                    backend.compile_region(request)
                }
                #[cfg(not(feature = "jit-cranelift"))]
                {
                    NoopJitBackend.compile_region(request)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CurrentJitBackend, JitBackendApi, JitBackendCompileRequest, JitCompileStatus,
        NoopJitBackend,
    };
    use crate::{JitBackend, JitCompileRequest, JitRuntimeHelperAddresses};

    #[test]
    fn noop_backend_reports_backend_unavailable() {
        let mut backend = NoopJitBackend;
        let request = JitCompileRequest::new("region.noop");
        let outcome = backend.compile_region(&JitBackendCompileRequest {
            compile: &request,
            unit: None,
            function: None,
            allow_native_execution: false,
            runtime_helpers: JitRuntimeHelperAddresses::default(),
        });

        assert_eq!(backend.backend(), JitBackend::Stub);
        assert_eq!(outcome.status, JitCompileStatus::BackendUnavailable);
        assert!(outcome.handle.is_none());
        assert!(outcome.diagnostics[0].contains("region.noop"));
    }

    #[test]
    fn current_backend_is_feature_sensitive_and_default_safe() {
        let mut backend = CurrentJitBackend::default();
        let request = JitCompileRequest::new("region.current");
        let outcome = backend.compile_region(&JitBackendCompileRequest {
            compile: &request,
            unit: None,
            function: None,
            allow_native_execution: false,
            runtime_helpers: JitRuntimeHelperAddresses::default(),
        });

        if cfg!(feature = "jit-cranelift") {
            assert_eq!(backend.backend(), JitBackend::CraneliftExperiment);
            assert_eq!(outcome.status, JitCompileStatus::NativeExecutionDisabled);
        } else {
            assert_eq!(backend.backend(), JitBackend::Stub);
            assert_eq!(outcome.status, JitCompileStatus::BackendUnavailable);
        }
        assert!(outcome.handle.is_none());
    }
}
