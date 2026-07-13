//! Mandatory Cranelift compiler boundary.

use crate::{JitCompileRequest, JitCompileStatus, JitFunctionHandle, JitRuntimeHelperAddresses};
use php_ir::{FunctionId, IrUnit};

/// Backend compile request shared by all native-code experiments.
#[derive(Debug)]
pub struct NativeCompileRequest<'a> {
    /// Stable region metadata chosen by the caller.
    pub compile: &'a JitCompileRequest,
    /// Optional IR unit when the backend needs to inspect or lower code.
    pub unit: Option<&'a IrUnit>,
    /// Optional function inside the IR unit.
    pub function: Option<FunctionId>,
    /// Runtime-owned helper function addresses available to native code.
    pub runtime_helpers: JitRuntimeHelperAddresses,
}

/// Backend compile outcome without VM-specific counter updates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeCompileOutcome {
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

impl NativeCompileOutcome {
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

/// Native compile interface implemented by the mandatory Cranelift compiler.
pub trait NativeCompilerApi {
    /// Attempts to compile one region.
    fn compile_region(&mut self, request: &NativeCompileRequest<'_>) -> NativeCompileOutcome;
}
