//! Default-off performance JIT API.
//!
//! Default builds use the stub backend and never allocate executable memory.
//! The `jit-cranelift` feature enables a constrained performance experiment with
//! explicit native-execution opt-in, ABI hash checks, verifier-backed Cranelift
//! lowering, and documented unsafe call boundaries.

pub mod aarch64;
mod abi;
mod backend;
pub mod code_memory;
pub mod copy_patch;
#[cfg(feature = "jit-cranelift")]
mod cranelift_lowering;
mod eligibility;
mod helpers;
pub mod region_ir;

pub use abi::{
    JIT_RUNTIME_ABI_HASH, JIT_RUNTIME_ABI_VERSION, JitAbiValue, JitBailout, JitBailoutKind,
    JitCExit, JitCExitTag, JitCFrameView, JitCValue, JitCValueTag, JitExceptionMarker,
    JitFrameHandle, JitFrameView, JitOpaqueHandle, JitOpaqueValueKind, JitRegionResult,
    JitRuntimeCallout, JitRuntimeCalloutResult, JitSideExit, JitVmContextHandle, SideExitReason,
};
pub use backend::{
    CurrentJitBackend, JitBackendApi, JitBackendCompileOutcome, JitBackendCompileRequest,
    NoopJitBackend,
};
#[cfg(feature = "jit-cranelift")]
pub use cranelift_lowering::{
    CraneliftClifSmokeResult, CraneliftLoweringError, CraneliftLoweringResult,
    CraneliftLoweringStats, CraneliftMachineCodeHandle, CraneliftNoExecBackend,
    build_trivial_add_clif_smoke, lower_function_to_cranelift,
};
pub use eligibility::{
    JitCandidateKind, JitEligibility, JitEligibilityReason, JitEligibilityReport,
    JitEligibilityStats, analyze_jit_eligibility, call_args_are_jit_primitive,
};
pub use helpers::{
    JIT_HELPER_REGISTRY_ABI_HASH, JIT_HELPER_STATUS_FALLBACK, JIT_HELPER_STATUS_OK,
    JIT_HELPER_STATUS_OVERFLOW, JIT_HELPER_SYMBOLS, JitHelperArgKind, JitHelperId,
    JitHelperReturnKind, JitHelperSymbol, helper_registry_is_stable,
    helper_registry_layout_summary, lookup_helper_by_id, lookup_helper_by_name,
};
use php_ir::{FunctionId, IrUnit};
use std::fmt;
use std::mem;

/// Stable backend selected for the current build.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JitBackend {
    /// No native backend is compiled in.
    Stub,
    /// The Cranelift experiment feature is compiled, but execution is still off.
    CraneliftExperiment,
}

impl JitBackend {
    /// Returns the backend for this build.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(feature = "jit-cranelift") {
            Self::CraneliftExperiment
        } else {
            Self::Stub
        }
    }

    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stub => "stub",
            Self::CraneliftExperiment => "cranelift-experiment",
        }
    }
}

/// Options for constructing a JIT engine.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JitOptions {
    /// Runtime switch. Defaults off even when a backend feature is compiled.
    pub enabled: bool,
    /// Whether native execution is allowed for this process.
    pub allow_native_execution: bool,
}

/// Runtime-owned helper addresses the backend may call from generated code.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitRuntimeHelperAddresses {
    /// ABI wrapper for `php_jit_array_len`.
    pub packed_array_len: usize,
    /// ABI wrapper for `php_jit_array_fetch_int_slow`.
    pub packed_array_fetch_int_slow: usize,
    /// ABI wrapper for guarded `strlen($value)`.
    pub known_strlen: usize,
    /// ABI wrapper for guarded `count($value)`.
    pub known_count: usize,
    /// ABI wrapper for guarded string/string concatenation.
    pub string_concat: usize,
    /// ABI wrapper for guarded monomorphic property loads.
    pub property_load: usize,
    /// ABI wrapper for record-shape symbol-guarded array lookups.
    pub record_array_lookup: usize,
}

/// Request to compile one future JIT region.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitCompileRequest {
    /// Stable region identifier chosen by the caller.
    pub region_id: String,
    /// Optional PHP function or method name for reports.
    pub function_name: Option<String>,
    /// Optional stable IR fingerprint when available.
    pub ir_fingerprint: Option<String>,
    /// Optimization level active when the request was made.
    pub opt_level: u8,
}

impl JitCompileRequest {
    /// Creates a compile request for a region.
    #[must_use]
    pub fn new(region_id: impl Into<String>) -> Self {
        Self {
            region_id: region_id.into(),
            function_name: None,
            ir_fingerprint: None,
            opt_level: 0,
        }
    }

    /// Adds a function name.
    #[must_use]
    pub fn with_function_name(mut self, function_name: impl Into<String>) -> Self {
        self.function_name = Some(function_name.into());
        self
    }

    /// Adds an IR fingerprint.
    #[must_use]
    pub fn with_ir_fingerprint(mut self, ir_fingerprint: impl Into<String>) -> Self {
        self.ir_fingerprint = Some(ir_fingerprint.into());
        self
    }

    /// Adds the active optimization level.
    #[must_use]
    pub const fn with_opt_level(mut self, opt_level: u8) -> Self {
        self.opt_level = opt_level;
        self
    }
}

/// Opaque handle for a compiled function.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitFunctionHandle {
    /// Stable handle id.
    pub id: u64,
    /// Region that produced this handle.
    pub region_id: String,
    /// Backend that produced this handle.
    pub backend: JitBackend,
    native_entry: Option<JitNativeEntry>,
    specialization: JitNativeSpecialization,
    code_bytes: u64,
    helper_calls_per_invocation: u64,
    fast_path_hits_per_invocation: u64,
    property_load_metadata: Option<JitPropertyLoadMetadata>,
}

/// Compile-time metadata for a monomorphic property-load fast path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitPropertyLoadMetadata {
    /// Normalized receiver class name expected by the guard.
    pub receiver_class: String,
    /// Stable class table ID expected by the guard.
    pub class_id: u32,
    /// Declared property name without `$`.
    pub property: String,
    /// Runtime storage name used by the safe helper ABI.
    pub storage_name: String,
    /// Declared property slot/index in the class metadata.
    pub property_slot_index: usize,
    /// Lookup/layout epoch captured when the handle was compiled.
    pub layout_version: u64,
}

/// Native specialization kind used by the VM to attribute guarded exits.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum JitNativeSpecialization {
    /// Generic native entry without specialization-specific counters.
    #[default]
    Generic,
    /// Packed-array foreach integer sum loop.
    PackedForeachIntSum,
    /// Guarded known `strlen($value)` call.
    KnownCallStrlen,
    /// Guarded known `count($value)` call.
    KnownCallCount,
    /// Guarded two-string concatenation.
    StringConcat,
    /// Guarded monomorphic property load.
    PropertyLoad,
    /// Guarded record-shape symbol-keyed array lookup.
    RecordArrayLookup,
}

impl JitFunctionHandle {
    /// Creates a non-executable handle for tests and future metadata-only paths.
    #[must_use]
    pub const fn metadata_only(id: u64, region_id: String, backend: JitBackend) -> Self {
        Self {
            id,
            region_id,
            backend,
            native_entry: None,
            specialization: JitNativeSpecialization::Generic,
            code_bytes: 0,
            helper_calls_per_invocation: 0,
            fast_path_hits_per_invocation: 0,
            property_load_metadata: None,
        }
    }

    /// Creates a scalar integer native-entry handle.
    #[must_use]
    #[cfg(feature = "jit-cranelift")]
    pub(crate) const fn i64_native(
        id: u64,
        region_id: String,
        backend: JitBackend,
        address: usize,
        arity: u8,
        code_bytes: u64,
    ) -> Self {
        Self {
            id,
            region_id,
            backend,
            native_entry: Some(JitNativeEntry {
                address,
                arity,
                abi_hash: JIT_RUNTIME_ABI_HASH,
                kind: JitNativeEntryKind::I64Return,
            }),
            specialization: JitNativeSpecialization::Generic,
            code_bytes,
            helper_calls_per_invocation: 0,
            fast_path_hits_per_invocation: 0,
            property_load_metadata: None,
        }
    }

    /// Creates a status/out-pointer integer native-entry handle.
    #[must_use]
    #[cfg(feature = "jit-cranelift")]
    pub(crate) const fn i64_status_out_native(
        id: u64,
        region_id: String,
        backend: JitBackend,
        address: usize,
        arity: u8,
        code_bytes: u64,
        helper_calls_per_invocation: u64,
        fast_path_hits_per_invocation: u64,
    ) -> Self {
        Self {
            id,
            region_id,
            backend,
            native_entry: Some(JitNativeEntry {
                address,
                arity,
                abi_hash: JIT_RUNTIME_ABI_HASH,
                kind: JitNativeEntryKind::I64StatusOut,
            }),
            specialization: JitNativeSpecialization::Generic,
            code_bytes,
            helper_calls_per_invocation,
            fast_path_hits_per_invocation,
            property_load_metadata: None,
        }
    }

    /// Creates a status/out-pointer native entry for one opaque value and int index.
    #[must_use]
    #[cfg(feature = "jit-cranelift")]
    pub(crate) const fn value_i64_status_out_native(
        id: u64,
        region_id: String,
        backend: JitBackend,
        address: usize,
        code_bytes: u64,
        helper_calls_per_invocation: u64,
        fast_path_hits_per_invocation: u64,
    ) -> Self {
        Self {
            id,
            region_id,
            backend,
            native_entry: Some(JitNativeEntry {
                address,
                arity: 2,
                abi_hash: JIT_RUNTIME_ABI_HASH,
                kind: JitNativeEntryKind::ValueI64StatusOut,
            }),
            specialization: JitNativeSpecialization::Generic,
            code_bytes,
            helper_calls_per_invocation,
            fast_path_hits_per_invocation,
            property_load_metadata: None,
        }
    }

    /// Creates a status/out-pointer native entry for one opaque value.
    #[must_use]
    #[cfg(feature = "jit-cranelift")]
    pub(crate) const fn value_status_out_native(
        id: u64,
        region_id: String,
        backend: JitBackend,
        address: usize,
        code_bytes: u64,
        helper_calls_per_invocation: u64,
        fast_path_hits_per_invocation: u64,
        specialization: JitNativeSpecialization,
    ) -> Self {
        Self {
            id,
            region_id,
            backend,
            native_entry: Some(JitNativeEntry {
                address,
                arity: 1,
                abi_hash: JIT_RUNTIME_ABI_HASH,
                kind: JitNativeEntryKind::ValueStatusOut,
            }),
            specialization,
            code_bytes,
            helper_calls_per_invocation,
            fast_path_hits_per_invocation,
            property_load_metadata: None,
        }
    }

    /// Creates a status/out-pointer native entry for one opaque value plus metadata.
    #[must_use]
    #[cfg(feature = "jit-cranelift")]
    pub(crate) fn value_metadata_status_out_native(
        id: u64,
        region_id: String,
        backend: JitBackend,
        address: usize,
        code_bytes: u64,
        helper_calls_per_invocation: u64,
        fast_path_hits_per_invocation: u64,
        property_load_metadata: JitPropertyLoadMetadata,
    ) -> Self {
        Self {
            id,
            region_id,
            backend,
            native_entry: Some(JitNativeEntry {
                address,
                arity: 2,
                abi_hash: JIT_RUNTIME_ABI_HASH,
                kind: JitNativeEntryKind::ValueMetadataStatusOut,
            }),
            specialization: JitNativeSpecialization::PropertyLoad,
            code_bytes,
            helper_calls_per_invocation,
            fast_path_hits_per_invocation,
            property_load_metadata: Some(property_load_metadata),
        }
    }

    /// Creates a status/out-pointer native entry for two opaque values.
    #[must_use]
    #[cfg(feature = "jit-cranelift")]
    pub(crate) const fn value_value_status_out_native(
        id: u64,
        region_id: String,
        backend: JitBackend,
        address: usize,
        code_bytes: u64,
        helper_calls_per_invocation: u64,
        fast_path_hits_per_invocation: u64,
        specialization: JitNativeSpecialization,
    ) -> Self {
        Self {
            id,
            region_id,
            backend,
            native_entry: Some(JitNativeEntry {
                address,
                arity: 2,
                abi_hash: JIT_RUNTIME_ABI_HASH,
                kind: JitNativeEntryKind::ValueValueStatusOut,
            }),
            specialization,
            code_bytes,
            helper_calls_per_invocation,
            fast_path_hits_per_invocation,
            property_load_metadata: None,
        }
    }

    /// Returns specialization metadata for VM counter attribution.
    #[must_use]
    pub const fn specialization(&self) -> JitNativeSpecialization {
        self.specialization
    }

    /// Returns native code bytes associated with the handle.
    #[must_use]
    pub const fn code_bytes(&self) -> u64 {
        self.code_bytes
    }

    /// Returns statically known helper calls per successful native invocation.
    #[must_use]
    pub const fn helper_calls_per_invocation(&self) -> u64 {
        self.helper_calls_per_invocation
    }

    /// Returns statically known inline fast-path hits per successful native invocation.
    #[must_use]
    pub const fn fast_path_hits_per_invocation(&self) -> u64 {
        self.fast_path_hits_per_invocation
    }

    /// Returns monomorphic property-load metadata when this handle carries it.
    #[must_use]
    pub const fn property_load_metadata(&self) -> Option<&JitPropertyLoadMetadata> {
        self.property_load_metadata.as_ref()
    }

    /// Returns true when this handle expects one opaque runtime value and one integer.
    #[must_use]
    pub const fn expects_value_i64(&self) -> bool {
        matches!(
            self.native_entry,
            Some(JitNativeEntry {
                kind: JitNativeEntryKind::ValueI64StatusOut,
                ..
            })
        )
    }

    /// Returns true when this handle expects one opaque runtime value.
    #[must_use]
    pub const fn expects_value(&self) -> bool {
        matches!(
            self.native_entry,
            Some(JitNativeEntry {
                kind: JitNativeEntryKind::ValueStatusOut,
                ..
            })
        )
    }

    /// Returns true when this handle expects two opaque runtime values.
    #[must_use]
    pub const fn expects_value_value(&self) -> bool {
        matches!(
            self.native_entry,
            Some(JitNativeEntry {
                kind: JitNativeEntryKind::ValueValueStatusOut,
                ..
            })
        )
    }

    /// Returns true when this handle expects one value plus metadata.
    #[must_use]
    pub const fn expects_value_metadata(&self) -> bool {
        matches!(
            self.native_entry,
            Some(JitNativeEntry {
                kind: JitNativeEntryKind::ValueMetadataStatusOut,
                ..
            })
        )
    }

    /// Invokes a compiled `i64` return function after checking the runtime ABI.
    pub fn invoke_i64(&self, args: &[i64], runtime_abi_hash: u64) -> Result<i64, JitInvokeError> {
        let Some(entry) = self.native_entry else {
            return Err(JitInvokeError::MissingNativeEntry);
        };
        if runtime_abi_hash != JIT_RUNTIME_ABI_HASH || entry.abi_hash != JIT_RUNTIME_ABI_HASH {
            return Err(JitInvokeError::AbiHashMismatch {
                expected: JIT_RUNTIME_ABI_HASH,
                actual: runtime_abi_hash,
            });
        }
        if args.len() != usize::from(entry.arity) {
            return Err(JitInvokeError::ArityMismatch {
                expected: entry.arity,
                actual: args.len() as u8,
            });
        }
        entry.invoke_i64(args)
    }

    /// Invokes a native entry specialized for `Value` plus integer index.
    pub fn invoke_value_i64(
        &self,
        value_ptr: usize,
        index: i64,
        runtime_abi_hash: u64,
    ) -> Result<i64, JitInvokeError> {
        let Some(entry) = self.native_entry else {
            return Err(JitInvokeError::MissingNativeEntry);
        };
        if runtime_abi_hash != JIT_RUNTIME_ABI_HASH || entry.abi_hash != JIT_RUNTIME_ABI_HASH {
            return Err(JitInvokeError::AbiHashMismatch {
                expected: JIT_RUNTIME_ABI_HASH,
                actual: runtime_abi_hash,
            });
        }
        entry.invoke_value_i64(value_ptr, index)
    }

    /// Invokes a native entry specialized for one `Value` argument.
    pub fn invoke_value(
        &self,
        value_ptr: usize,
        runtime_abi_hash: u64,
    ) -> Result<i64, JitInvokeError> {
        let Some(entry) = self.native_entry else {
            return Err(JitInvokeError::MissingNativeEntry);
        };
        if runtime_abi_hash != JIT_RUNTIME_ABI_HASH || entry.abi_hash != JIT_RUNTIME_ABI_HASH {
            return Err(JitInvokeError::AbiHashMismatch {
                expected: JIT_RUNTIME_ABI_HASH,
                actual: runtime_abi_hash,
            });
        }
        entry.invoke_value(value_ptr)
    }

    /// Invokes a native entry specialized for two `Value` arguments.
    pub fn invoke_value_value(
        &self,
        lhs_ptr: usize,
        rhs_ptr: usize,
        runtime_abi_hash: u64,
    ) -> Result<usize, JitInvokeError> {
        let Some(entry) = self.native_entry else {
            return Err(JitInvokeError::MissingNativeEntry);
        };
        if runtime_abi_hash != JIT_RUNTIME_ABI_HASH || entry.abi_hash != JIT_RUNTIME_ABI_HASH {
            return Err(JitInvokeError::AbiHashMismatch {
                expected: JIT_RUNTIME_ABI_HASH,
                actual: runtime_abi_hash,
            });
        }
        entry.invoke_value_value(lhs_ptr, rhs_ptr)
    }

    /// Invokes a native entry specialized for one `Value` plus metadata.
    pub fn invoke_value_metadata(
        &self,
        value_ptr: usize,
        metadata_ptr: usize,
        runtime_abi_hash: u64,
    ) -> Result<usize, JitInvokeError> {
        let Some(entry) = self.native_entry else {
            return Err(JitInvokeError::MissingNativeEntry);
        };
        if runtime_abi_hash != JIT_RUNTIME_ABI_HASH || entry.abi_hash != JIT_RUNTIME_ABI_HASH {
            return Err(JitInvokeError::AbiHashMismatch {
                expected: JIT_RUNTIME_ABI_HASH,
                actual: runtime_abi_hash,
            });
        }
        entry.invoke_value_metadata(value_ptr, metadata_ptr)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct JitNativeEntry {
    address: usize,
    arity: u8,
    abi_hash: u64,
    kind: JitNativeEntryKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
enum JitNativeEntryKind {
    I64Return,
    I64StatusOut,
    ValueI64StatusOut,
    ValueStatusOut,
    ValueMetadataStatusOut,
    ValueValueStatusOut,
}

impl JitNativeEntry {
    fn invoke_i64(self, args: &[i64]) -> Result<i64, JitInvokeError> {
        match self.kind {
            JitNativeEntryKind::I64Return => self.invoke_i64_return(args),
            JitNativeEntryKind::I64StatusOut => self.invoke_i64_status_out(args),
            JitNativeEntryKind::ValueI64StatusOut
            | JitNativeEntryKind::ValueStatusOut
            | JitNativeEntryKind::ValueMetadataStatusOut
            | JitNativeEntryKind::ValueValueStatusOut => Err(JitInvokeError::ArityMismatch {
                expected: self.arity,
                actual: args.len() as u8,
            }),
        }
    }

    fn invoke_i64_return(self, args: &[i64]) -> Result<i64, JitInvokeError> {
        // SAFETY: Handles are created only after Cranelift defines the matching
        // `extern "C" fn(...i64) -> i64` signature. The public method checks
        // ABI hash and exact arity before reaching this call.
        let value = unsafe {
            match args {
                [] => {
                    let function: extern "C" fn() -> i64 = mem::transmute(self.address);
                    function()
                }
                [a] => {
                    let function: extern "C" fn(i64) -> i64 = mem::transmute(self.address);
                    function(*a)
                }
                [a, b] => {
                    let function: extern "C" fn(i64, i64) -> i64 = mem::transmute(self.address);
                    function(*a, *b)
                }
                [a, b, c] => {
                    let function: extern "C" fn(i64, i64, i64) -> i64 =
                        mem::transmute(self.address);
                    function(*a, *b, *c)
                }
                [a, b, c, d] => {
                    let function: extern "C" fn(i64, i64, i64, i64) -> i64 =
                        mem::transmute(self.address);
                    function(*a, *b, *c, *d)
                }
                _ => return Err(JitInvokeError::UnsupportedArity(args.len() as u8)),
            }
        };
        Ok(value)
    }

    fn invoke_i64_status_out(self, args: &[i64]) -> Result<i64, JitInvokeError> {
        let mut out = 0_i64;
        let out_ptr = &mut out as *mut i64;
        // SAFETY: Handles are created only after Cranelift defines the matching
        // `extern "C" fn(...i64, *mut i64) -> i32` signature. The public method
        // checks ABI hash and exact arity before reaching this call.
        let status = unsafe {
            match args {
                [] => {
                    let function: extern "C" fn(*mut i64) -> i32 = mem::transmute(self.address);
                    function(out_ptr)
                }
                [a] => {
                    let function: extern "C" fn(i64, *mut i64) -> i32 =
                        mem::transmute(self.address);
                    function(*a, out_ptr)
                }
                [a, b] => {
                    let function: extern "C" fn(i64, i64, *mut i64) -> i32 =
                        mem::transmute(self.address);
                    function(*a, *b, out_ptr)
                }
                [a, b, c] => {
                    let function: extern "C" fn(i64, i64, i64, *mut i64) -> i32 =
                        mem::transmute(self.address);
                    function(*a, *b, *c, out_ptr)
                }
                [a, b, c, d] => {
                    let function: extern "C" fn(i64, i64, i64, i64, *mut i64) -> i32 =
                        mem::transmute(self.address);
                    function(*a, *b, *c, *d, out_ptr)
                }
                _ => return Err(JitInvokeError::UnsupportedArity(args.len() as u8)),
            }
        };
        if status == JIT_HELPER_STATUS_OK {
            Ok(out)
        } else {
            Err(JitInvokeError::NativeStatus(status))
        }
    }

    fn invoke_value_i64(self, value_ptr: usize, index: i64) -> Result<i64, JitInvokeError> {
        if self.kind != JitNativeEntryKind::ValueI64StatusOut {
            return Err(JitInvokeError::ArityMismatch {
                expected: self.arity,
                actual: 2,
            });
        }
        let mut out = 0_i64;
        let out_ptr = &mut out as *mut i64;
        // SAFETY: Handles are created only after Cranelift defines the matching
        // `extern "C" fn(usize, i64, *mut i64) -> i32` signature. The public
        // method checks ABI hashes before reaching this call.
        let status = unsafe {
            let function: extern "C" fn(usize, i64, *mut i64) -> i32 = mem::transmute(self.address);
            function(value_ptr, index, out_ptr)
        };
        if status == JIT_HELPER_STATUS_OK {
            Ok(out)
        } else {
            Err(JitInvokeError::NativeStatus(status))
        }
    }

    fn invoke_value(self, value_ptr: usize) -> Result<i64, JitInvokeError> {
        if self.kind != JitNativeEntryKind::ValueStatusOut {
            return Err(JitInvokeError::ArityMismatch {
                expected: self.arity,
                actual: 1,
            });
        }
        let mut out = 0_i64;
        let out_ptr = &mut out as *mut i64;
        // SAFETY: Handles are created only after Cranelift defines the matching
        // `extern "C" fn(usize, *mut i64) -> i32` signature. The public method
        // checks ABI hashes before reaching this call.
        let status = unsafe {
            let function: extern "C" fn(usize, *mut i64) -> i32 = mem::transmute(self.address);
            function(value_ptr, out_ptr)
        };
        if status == JIT_HELPER_STATUS_OK {
            Ok(out)
        } else {
            Err(JitInvokeError::NativeStatus(status))
        }
    }

    fn invoke_value_value(self, lhs_ptr: usize, rhs_ptr: usize) -> Result<usize, JitInvokeError> {
        if self.kind != JitNativeEntryKind::ValueValueStatusOut {
            return Err(JitInvokeError::ArityMismatch {
                expected: self.arity,
                actual: 2,
            });
        }
        let mut out = 0_usize;
        let out_ptr = &mut out as *mut usize;
        // SAFETY: Handles are created only after Cranelift defines the matching
        // `extern "C" fn(usize, usize, *mut usize) -> i32` signature. The public
        // method checks ABI hashes before reaching this call.
        let status = unsafe {
            let function: extern "C" fn(usize, usize, *mut usize) -> i32 =
                mem::transmute(self.address);
            function(lhs_ptr, rhs_ptr, out_ptr)
        };
        if status == JIT_HELPER_STATUS_OK {
            Ok(out)
        } else {
            Err(JitInvokeError::NativeStatus(status))
        }
    }

    fn invoke_value_metadata(
        self,
        value_ptr: usize,
        metadata_ptr: usize,
    ) -> Result<usize, JitInvokeError> {
        if self.kind != JitNativeEntryKind::ValueMetadataStatusOut {
            return Err(JitInvokeError::ArityMismatch {
                expected: self.arity,
                actual: 2,
            });
        }
        let mut out = 0_usize;
        let out_ptr = &mut out as *mut usize;
        // SAFETY: Handles are created only after Cranelift defines the matching
        // `extern "C" fn(usize, usize, *mut usize) -> i32` signature. The
        // public method checks ABI hashes before reaching this call.
        let status = unsafe {
            let function: extern "C" fn(usize, usize, *mut usize) -> i32 =
                mem::transmute(self.address);
            function(value_ptr, metadata_ptr, out_ptr)
        };
        if status == JIT_HELPER_STATUS_OK {
            Ok(out)
        } else {
            Err(JitInvokeError::NativeStatus(status))
        }
    }
}

/// Invocation failures reported before interpreter fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JitInvokeError {
    /// Handle does not contain an executable entry.
    MissingNativeEntry,
    /// VM/runtime ABI hash does not match the compiled entry.
    AbiHashMismatch { expected: u64, actual: u64 },
    /// Call arguments do not match the compiled signature.
    ArityMismatch { expected: u8, actual: u8 },
    /// This module currently exposes a tiny fixed-arity trampoline.
    UnsupportedArity(u8),
    /// Native code returned a non-zero helper status.
    NativeStatus(i32),
}

impl JitInvokeError {
    /// Native helper status when available.
    #[must_use]
    pub const fn native_status(&self) -> Option<i32> {
        match self {
            Self::NativeStatus(status) => Some(*status),
            _ => None,
        }
    }

    /// Returns structured side-exit metadata for failures that should resume
    /// through the interpreter instead of trusting native output.
    #[must_use]
    pub const fn side_exit(&self) -> JitSideExit {
        match self {
            Self::MissingNativeEntry | Self::UnsupportedArity(_) => {
                JitSideExit::new(SideExitReason::UnsupportedValue)
            }
            Self::AbiHashMismatch { .. } => JitSideExit::new(SideExitReason::AbiMismatch),
            Self::ArityMismatch { .. } => JitSideExit::new(SideExitReason::TypeMismatch),
            Self::NativeStatus(status) if *status == JIT_HELPER_STATUS_OVERFLOW => {
                JitSideExit::new(SideExitReason::Overflow).with_status(*status)
            }
            Self::NativeStatus(status) => {
                JitSideExit::new(SideExitReason::HelperStatus).with_status(*status)
            }
        }
    }
}

/// Machine-readable compile status.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JitCompileStatus {
    /// Runtime JIT flag is disabled.
    Disabled,
    /// Backend feature is not compiled in.
    BackendUnavailable,
    /// Backend feature is present, but native execution is blocked.
    NativeExecutionDisabled,
    /// Region was rejected before code generation.
    Rejected { reason: String },
    /// Region compiled to native code.
    Compiled,
}

impl JitCompileStatus {
    /// Stable report spelling.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::BackendUnavailable => "backend_unavailable",
            Self::NativeExecutionDisabled => "native_execution_disabled",
            Self::Rejected { .. } => "rejected",
            Self::Compiled => "compiled",
        }
    }
}

/// Result of a compile attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitCompileResult {
    /// Compile status.
    pub status: JitCompileStatus,
    /// Future compiled function handle. Always `None` until native compilation is enabled.
    pub handle: Option<JitFunctionHandle>,
    /// Diagnostics suitable for logs and smoke reports.
    pub diagnostics: Vec<String>,
    /// Snapshot of engine stats after the request.
    pub stats: JitStats,
}

/// JIT error type for invalid API use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JitError {
    /// Region id was empty.
    EmptyRegionId,
}

impl fmt::Display for JitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRegionId => f.write_str("JIT region id must not be empty"),
        }
    }
}

impl std::error::Error for JitError {}

/// Accumulated JIT counters.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JitStats {
    /// Compile requests observed.
    pub compile_requests: u64,
    /// Requests skipped because runtime JIT was disabled.
    pub disabled_requests: u64,
    /// Requests skipped because no backend feature was compiled in.
    pub backend_unavailable: u64,
    /// Requests blocked because native execution is not enabled.
    pub native_execution_disabled: u64,
    /// Regions rejected by the skeleton before code generation.
    pub rejected: u64,
    /// Native compile successes. Zero until native compilation work enable it.
    pub native_compiles: u64,
    /// Executable memory allocations. Zero until native compilation work enable it.
    pub executable_memory_allocations: u64,
    /// Native code bytes emitted by successful compiles.
    pub native_code_bytes: u64,
    /// Cumulative native compile latency in nanoseconds.
    pub native_compile_time_nanos: u64,
    /// Eligibility analyses observed.
    pub eligibility_analyses: u64,
    /// Functions accepted by the conservative eligibility analysis.
    pub eligibility_eligible: u64,
    /// Functions rejected by the conservative eligibility analysis.
    pub eligibility_rejected: u64,
    /// Functions the conservative eligibility analysis could not classify.
    pub eligibility_unknown: u64,
    /// Blocks inspected by eligibility analysis.
    pub eligibility_blocks_analyzed: u64,
    /// Instructions inspected by eligibility analysis.
    pub eligibility_instructions_analyzed: u64,
}

/// Default-off JIT engine skeleton.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitEngine {
    options: JitOptions,
    backend: JitBackend,
    stats: JitStats,
}

impl JitEngine {
    /// Creates a default-off engine.
    #[must_use]
    pub fn new() -> Self {
        Self::with_options(JitOptions::default())
    }

    /// Creates an engine with explicit options.
    #[must_use]
    pub fn with_options(options: JitOptions) -> Self {
        Self {
            options,
            backend: JitBackend::current(),
            stats: JitStats::default(),
        }
    }

    /// Returns the selected backend for this build.
    #[must_use]
    pub const fn backend(&self) -> JitBackend {
        self.backend
    }

    /// Returns accumulated stats.
    #[must_use]
    pub const fn stats(&self) -> &JitStats {
        &self.stats
    }

    /// Analyzes one IR function for the future JIT subset.
    ///
    /// This does not compile or execute native code. It only records whether a
    /// function is eligible for later experimental lowering.
    pub fn analyze_eligibility(
        &mut self,
        unit: &IrUnit,
        function: FunctionId,
    ) -> JitEligibilityReport {
        let report = analyze_jit_eligibility(unit, function);
        self.stats.eligibility_analyses += 1;
        self.stats.eligibility_blocks_analyzed += report.stats.blocks_analyzed;
        self.stats.eligibility_instructions_analyzed += report.stats.instructions_analyzed;
        match &report.eligibility {
            JitEligibility::Eligible => self.stats.eligibility_eligible += 1,
            JitEligibility::Rejected { .. } => self.stats.eligibility_rejected += 1,
            JitEligibility::Unknown { .. } => self.stats.eligibility_unknown += 1,
        }
        report
    }

    /// Attempts to compile a region with the build-selected backend.
    pub fn compile(&mut self, request: JitCompileRequest) -> Result<JitCompileResult, JitError> {
        let mut backend = CurrentJitBackend::new(self.backend);
        self.compile_with_backend(request, &mut backend)
    }

    /// Attempts to compile one IR function with the build-selected backend.
    pub fn compile_function(
        &mut self,
        unit: &IrUnit,
        function: FunctionId,
        request: JitCompileRequest,
    ) -> Result<JitCompileResult, JitError> {
        let mut backend = CurrentJitBackend::new(self.backend);
        self.compile_function_with_backend(unit, function, request, &mut backend)
    }

    /// Attempts to compile one IR function with runtime helper addresses.
    pub fn compile_function_with_runtime_helpers(
        &mut self,
        unit: &IrUnit,
        function: FunctionId,
        request: JitCompileRequest,
        runtime_helpers: JitRuntimeHelperAddresses,
    ) -> Result<JitCompileResult, JitError> {
        let mut backend = CurrentJitBackend::new(self.backend);
        self.compile_inner(
            request,
            Some(unit),
            Some(function),
            runtime_helpers,
            &mut backend,
        )
    }

    /// Attempts to compile a region through a backend-neutral implementation.
    ///
    /// This is the API boundary the VM can exercise without taking a concrete
    /// dependency on Cranelift internals.
    pub fn compile_with_backend<B: JitBackendApi>(
        &mut self,
        request: JitCompileRequest,
        backend: &mut B,
    ) -> Result<JitCompileResult, JitError> {
        self.compile_inner(
            request,
            None,
            None,
            JitRuntimeHelperAddresses::default(),
            backend,
        )
    }

    /// Attempts to compile one IR function through a backend-neutral implementation.
    pub fn compile_function_with_backend<B: JitBackendApi>(
        &mut self,
        unit: &IrUnit,
        function: FunctionId,
        request: JitCompileRequest,
        backend: &mut B,
    ) -> Result<JitCompileResult, JitError> {
        self.compile_inner(
            request,
            Some(unit),
            Some(function),
            JitRuntimeHelperAddresses::default(),
            backend,
        )
    }

    fn compile_inner<B: JitBackendApi>(
        &mut self,
        request: JitCompileRequest,
        unit: Option<&IrUnit>,
        function: Option<FunctionId>,
        runtime_helpers: JitRuntimeHelperAddresses,
        backend: &mut B,
    ) -> Result<JitCompileResult, JitError> {
        if request.region_id.is_empty() {
            return Err(JitError::EmptyRegionId);
        }

        self.stats.compile_requests += 1;
        let (status, handle, diagnostics) = if !self.options.enabled {
            self.stats.disabled_requests += 1;
            (
                JitCompileStatus::Disabled,
                None,
                vec![format!(
                    "jit region `{}` skipped with status `disabled`",
                    request.region_id
                )],
            )
        } else {
            let backend_request = JitBackendCompileRequest {
                compile: &request,
                unit,
                function,
                allow_native_execution: self.options.allow_native_execution,
                runtime_helpers,
            };
            let outcome = backend.compile_region(&backend_request);
            self.record_compile_status(&outcome.status);
            self.stats.native_code_bytes += outcome.code_bytes;
            self.stats.native_compile_time_nanos += outcome.compile_time_nanos;
            if outcome.handle.is_some() {
                self.stats.executable_memory_allocations += 1;
            }
            (outcome.status, outcome.handle, outcome.diagnostics)
        };

        Ok(JitCompileResult {
            status,
            handle,
            diagnostics,
            stats: self.stats.clone(),
        })
    }

    fn record_compile_status(&mut self, status: &JitCompileStatus) {
        match status {
            JitCompileStatus::Disabled => self.stats.disabled_requests += 1,
            JitCompileStatus::BackendUnavailable => self.stats.backend_unavailable += 1,
            JitCompileStatus::NativeExecutionDisabled => self.stats.native_execution_disabled += 1,
            JitCompileStatus::Rejected { .. } => self.stats.rejected += 1,
            JitCompileStatus::Compiled => self.stats.native_compiles += 1,
        }
    }
}

impl Default for JitEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{JitBackend, JitCompileRequest, JitCompileStatus, JitEngine, JitError, JitOptions};
    use crate::{
        JitAbiValue, JitBailout, JitBailoutKind, JitEligibility, JitExceptionMarker,
        JitFrameHandle, JitFrameView, JitOpaqueHandle, JitOpaqueValueKind, JitRegionResult,
        JitRuntimeCallout, JitRuntimeCalloutResult, JitVmContextHandle, analyze_jit_eligibility,
    };
    use php_ir::{
        BinaryOp, BlockId, FunctionFlags, FunctionId, InstrId, InstructionKind, IrBuilder,
        IrConstant, IrReturnType, IrSpan, LocalId, Operand, RegId, UnitId,
    };

    #[test]
    fn backend_reflects_feature_flag_without_requiring_native_execution() {
        let engine = JitEngine::new();
        if cfg!(feature = "jit-cranelift") {
            assert_eq!(engine.backend(), JitBackend::CraneliftExperiment);
        } else {
            assert_eq!(engine.backend(), JitBackend::Stub);
        }
        assert_eq!(engine.stats().executable_memory_allocations, 0);
    }

    #[test]
    fn default_engine_skips_compile_when_disabled() {
        let mut engine = JitEngine::new();
        let result = engine
            .compile(JitCompileRequest::new("main"))
            .expect("compile request is valid");

        assert_eq!(result.status, JitCompileStatus::Disabled);
        assert!(result.handle.is_none());
        assert_eq!(result.stats.compile_requests, 1);
        assert_eq!(result.stats.disabled_requests, 1);
        assert_eq!(result.stats.native_compiles, 0);
        assert_eq!(result.stats.executable_memory_allocations, 0);
    }

    #[test]
    fn enabled_stub_reports_backend_unavailable_without_native_code() {
        let mut engine = JitEngine::with_options(JitOptions {
            enabled: true,
            allow_native_execution: false,
        });
        let result = engine
            .compile(
                JitCompileRequest::new("loop")
                    .with_function_name("loop")
                    .with_ir_fingerprint("abc123")
                    .with_opt_level(1),
            )
            .expect("compile request is valid");

        if cfg!(feature = "jit-cranelift") {
            assert_eq!(result.status, JitCompileStatus::NativeExecutionDisabled);
            assert_eq!(result.stats.native_execution_disabled, 1);
        } else {
            assert_eq!(result.status, JitCompileStatus::BackendUnavailable);
            assert_eq!(result.stats.backend_unavailable, 1);
        }
        assert!(result.handle.is_none());
        assert_eq!(result.stats.native_compiles, 0);
        assert_eq!(result.stats.executable_memory_allocations, 0);
    }

    #[test]
    fn empty_region_id_is_an_api_error() {
        let mut engine = JitEngine::new();
        let error = engine
            .compile(JitCompileRequest::new(""))
            .expect_err("empty ids are rejected");

        assert_eq!(error, JitError::EmptyRegionId);
        assert_eq!(engine.stats().compile_requests, 0);
    }

    #[test]
    fn eligibility_accepts_primitive_int_bool_leaf_fixture() {
        let (unit, function) = eligible_int_add_fixture();
        let report = analyze_jit_eligibility(&unit, function);

        assert_eq!(report.eligibility, JitEligibility::Eligible);
        assert_eq!(
            report.candidate_kind,
            Some(super::JitCandidateKind::IntLeafCandidate)
        );
        assert!(report.reasons.is_empty());
        assert_eq!(report.stats.blocks_analyzed, 1);
        assert_eq!(report.stats.instructions_analyzed, 3);
        assert!(
            report
                .debug_output()
                .contains("jit-eligibility function=eligible_int_add status=eligible")
        );

        let mut engine = JitEngine::new();
        let report = engine.analyze_eligibility(&unit, function);
        assert_eq!(report.eligibility, JitEligibility::Eligible);
        assert_eq!(engine.stats().eligibility_analyses, 1);
        assert_eq!(engine.stats().eligibility_eligible, 1);
        assert_eq!(engine.stats().eligibility_rejected, 0);
        assert_eq!(engine.stats().eligibility_instructions_analyzed, 3);
        assert_eq!(engine.stats().executable_memory_allocations, 0);
    }

    #[test]
    fn eligibility_marks_typed_int_leaf_candidate_and_serializes_json() {
        let (unit, function) = typed_int_leaf_fixture();
        let report = analyze_jit_eligibility(&unit, function);

        assert_eq!(report.eligibility, JitEligibility::Eligible);
        assert_eq!(
            report.candidate_kind,
            Some(super::JitCandidateKind::IntLeafCandidate)
        );
        let json = report.to_json();
        assert!(json.contains("\"status\":\"eligible\""));
        assert!(json.contains("\"candidate_kind\":\"IntLeafCandidate\""));
        assert!(json.contains("\"function_name\":\"typed_int_leaf\""));
    }

    #[test]
    fn eligibility_rejects_calls_arrays_and_nonprimitive_constants() {
        let (unit, function) = rejected_dynamic_fixture();
        let report = analyze_jit_eligibility(&unit, function);

        assert_eq!(report.eligibility.as_str(), "rejected");
        let codes: Vec<_> = report.reasons.iter().map(|reason| reason.code).collect();
        assert!(codes.contains(&"JIT_ELIGIBILITY_REJECT_CALL_OPCODE"));
        assert!(codes.contains(&"JIT_ELIGIBILITY_REJECT_ARRAY_OPCODE"));
        assert!(codes.contains(&"JIT_ELIGIBILITY_REJECT_NON_PRIMITIVE_CONSTANT"));
        assert!(report.debug_output().contains("status=rejected"));
    }

    #[test]
    fn eligibility_rejects_property_reference_opcodes() {
        let (unit, function) = rejected_property_references_fixture();
        let report = analyze_jit_eligibility(&unit, function);

        assert_eq!(report.eligibility.as_str(), "rejected");
        let reference_rejections = report
            .reasons
            .iter()
            .filter(|reason| reason.code == "JIT_ELIGIBILITY_REJECT_REFERENCE_OPCODE")
            .count();
        assert_eq!(reference_rejections, 3);
    }

    #[test]
    fn eligibility_rejects_untyped_parameters_without_profile() {
        let (unit, function) = untyped_param_fixture();
        let report = analyze_jit_eligibility(&unit, function);

        assert_eq!(report.eligibility.as_str(), "rejected");
        assert!(report.candidate_kind.is_none());
        assert_eq!(
            report.reasons[0].code,
            "JIT_ELIGIBILITY_REJECT_UNTYPED_PARAM"
        );
        assert!(report.to_json().contains("no stable int profile"));
    }

    #[test]
    fn eligibility_reports_unknown_for_missing_ir_function() {
        let (unit, _) = eligible_int_add_fixture();
        let report = analyze_jit_eligibility(&unit, FunctionId::new(99));

        assert_eq!(report.eligibility.as_str(), "unknown");
        assert_eq!(report.reasons[0].code, "JIT_ELIGIBILITY_UNKNOWN_FUNCTION");
    }

    #[test]
    fn abi_handles_are_opaque_and_non_zero() {
        assert!(JitOpaqueHandle::new(0).is_none());
        assert!(JitVmContextHandle::new(0).is_none());
        assert!(JitFrameHandle::new(0).is_none());

        let context = JitVmContextHandle::new(1).expect("non-zero context");
        let frame = JitFrameHandle::new(2).expect("non-zero frame");
        let view = JitFrameView::new(context, frame, FunctionId::new(7), 3, 2);

        assert_eq!(view.context.raw(), 1);
        assert_eq!(view.frame.raw(), 2);
        assert!(view.contains_register(RegId::new(2)));
        assert!(!view.contains_register(RegId::new(3)));
        assert!(view.contains_local(LocalId::new(1)));
        assert!(!view.contains_local(LocalId::new(2)));
    }

    #[test]
    fn abi_value_boundary_uses_by_value_or_opaque_values() {
        let string_handle = JitOpaqueHandle::new(44).expect("non-zero handle");
        let value = JitAbiValue::Opaque {
            kind: JitOpaqueValueKind::String,
            handle: string_handle,
        };

        assert!(value.is_opaque());
        assert_eq!(JitOpaqueValueKind::String.as_str(), "string");
        assert_eq!(
            JitAbiValue::float(1.5),
            JitAbiValue::FloatBits(1.5f64.to_bits())
        );

        let callout = JitRuntimeCallout::new("strlen", vec![value], true);
        assert_eq!(callout.name, "strlen");
        assert!(callout.can_throw);
        assert_eq!(
            JitRuntimeCalloutResult::Returned(JitAbiValue::Int(3)),
            JitRuntimeCalloutResult::Returned(JitAbiValue::Int(3))
        );
    }

    #[test]
    fn abi_models_bailout_deopt_and_exception_markers() {
        let bailout = JitBailout::new(JitBailoutKind::GuardFailed, "type guard failed")
            .with_resume(BlockId::new(1), InstrId::new(2));
        assert_eq!(bailout.kind.as_str(), "guard_failed");
        assert_eq!(bailout.resume_block, Some(BlockId::new(1)));
        assert_eq!(bailout.resume_instruction, Some(InstrId::new(2)));

        let exception = JitExceptionMarker::named("TypeError", "bad argument");
        assert_eq!(exception.class_name.as_deref(), Some("TypeError"));

        let opaque_exception =
            JitExceptionMarker::opaque(JitOpaqueHandle::new(99).expect("non-zero handle"));
        assert_eq!(opaque_exception.exception.expect("handle").raw(), 99);

        assert_eq!(
            JitRegionResult::Bailout(bailout.clone()),
            JitRegionResult::Bailout(bailout)
        );
        assert_eq!(
            JitRuntimeCalloutResult::Exception(exception.clone()),
            JitRuntimeCalloutResult::Exception(exception)
        );
    }

    fn eligible_int_add_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("tests/fixtures/performance/jit/eligible-int-add.php");
        let span = IrSpan::new(file, 0, 0);
        let function = builder.start_function("eligible_int_add", FunctionFlags::default(), span);
        builder.set_entry(function);
        let block = builder.append_block(function);
        let one = builder.add_constant(IrConstant::Int(1));
        let two = builder.add_constant(IrConstant::Int(2));
        let r0 = builder.alloc_register(function);
        let r1 = builder.alloc_register(function);
        let r2 = builder.alloc_register(function);
        builder.emit_load_const(function, block, r0, one, span);
        builder.emit_load_const(function, block, r1, two, span);
        builder.emit(
            function,
            block,
            InstructionKind::Binary {
                dst: r2,
                op: BinaryOp::Add,
                lhs: Operand::Register(r0),
                rhs: Operand::Register(r1),
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(r2)), span);
        (builder.finish(), function)
    }

    fn typed_int_leaf_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder
            .add_file("tests/fixtures/performance/cranelift/eligibility/eligible-int-leaf.php");
        let span = IrSpan::new(file, 0, 0);
        let function = builder.start_function("typed_int_leaf", FunctionFlags::default(), span);
        builder.set_entry(function);
        let local_a = builder.intern_local(function, "a");
        let local_b = builder.intern_local(function, "b");
        builder.push_param(
            function,
            php_ir::IrParam {
                name: "a".to_owned(),
                local: local_a,
                required: true,
                default: None,
                type_: Some(IrReturnType::Int),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            },
        );
        builder.push_param(
            function,
            php_ir::IrParam {
                name: "b".to_owned(),
                local: local_b,
                required: true,
                default: None,
                type_: Some(IrReturnType::Int),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            },
        );
        builder.set_return_type(function, Some(IrReturnType::Int));
        let block = builder.append_block(function);
        let r0 = builder.alloc_register(function);
        let r1 = builder.alloc_register(function);
        let r2 = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal {
                dst: r0,
                local: local_a,
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal {
                dst: r1,
                local: local_b,
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::Binary {
                dst: r2,
                op: BinaryOp::Add,
                lhs: Operand::Register(r0),
                rhs: Operand::Register(r1),
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(r2)), span);
        (builder.finish(), function)
    }

    fn untyped_param_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file(
            "tests/fixtures/performance/cranelift/eligibility/rejected-untyped-param.php",
        );
        let span = IrSpan::new(file, 0, 0);
        let function =
            builder.start_function("rejected_untyped_param", FunctionFlags::default(), span);
        builder.set_entry(function);
        let local = builder.intern_local(function, "value");
        builder.push_required_param(function, "value", local);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let block = builder.append_block(function);
        let r0 = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal { dst: r0, local },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(r0)), span);
        (builder.finish(), function)
    }

    fn rejected_dynamic_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("tests/fixtures/performance/jit/rejected-dynamic.php");
        let span = IrSpan::new(file, 0, 0);
        let function = builder.start_function("rejected_dynamic", FunctionFlags::default(), span);
        builder.set_entry(function);
        let block = builder.append_block(function);
        let text = builder.add_constant(IrConstant::String("not primitive".to_owned()));
        let r0 = builder.alloc_register(function);
        let r1 = builder.alloc_register(function);
        builder.emit_load_const(function, block, r0, text, span);
        builder.emit(
            function,
            block,
            InstructionKind::CallFunction {
                dst: r1,
                name: "strlen".to_owned(),
                args: Vec::new(),
            },
            span,
        );
        builder.emit(function, block, InstructionKind::NewArray { dst: r0 }, span);
        builder.terminate_return(function, block, Some(Operand::Register(r1)), span);
        (builder.finish(), function)
    }

    fn rejected_property_references_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("tests/fixtures/performance/jit/rejected-property-ref.php");
        let span = IrSpan::new(file, 0, 0);
        let function =
            builder.start_function("rejected_property_ref", FunctionFlags::default(), span);
        builder.set_entry(function);
        let block = builder.append_block(function);
        let source = builder.intern_local(function, "source");
        let target = builder.intern_local(function, "target");
        let object = builder.alloc_register(function);
        let offset = builder.add_constant(IrConstant::Int(0));

        builder.emit(
            function,
            block,
            InstructionKind::BindReferenceProperty {
                object: Operand::Register(object),
                property: "value".to_owned(),
                source,
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::BindReferencePropertyDim {
                object: Operand::Register(object),
                property: "value".to_owned(),
                dims: vec![Operand::Constant(offset)],
                append: false,
                source,
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::BindReferenceDimFromProperty {
                local: target,
                dims: vec![Operand::Constant(offset)],
                append: false,
                object: Operand::Register(object),
                property: "value".to_owned(),
            },
            span,
        );
        builder.terminate_return(function, block, None, span);
        (builder.finish(), function)
    }
}
