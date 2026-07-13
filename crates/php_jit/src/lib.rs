//! Default-off performance JIT API.
//!
//! Default builds use the stub backend and never allocate executable memory.
//! Cranelift is the mandatory production compiler with
//! explicit native-execution opt-in, ABI hash checks, verifier-backed Cranelift
//! lowering, and documented unsafe call boundaries.

// This crate is the engine's sanctioned native-codegen boundary: allocating
// executable memory, W^X toggling, and calling generated code are irreducibly
// `unsafe`, each guarded by a local `// SAFETY:` contract. The
// `runtime-hardening-lints` gate forbids `unsafe` in the interpreter core
// (php_runtime, php_vm) and, since php_vm pulls php_jit as a default-feature
// path dependency, that `-D unsafe-code` propagates here; opt this crate out at
// its root so the invariant keeps protecting the core without banning unsafe in
// the one crate whose job requires it.
#![allow(unsafe_code)]

mod abi;
mod backend;
mod code_manager;
mod cranelift_lowering;
mod dynamic_code;
mod eligibility;
mod helpers;
mod host_isa;
pub mod region_ir;

pub use abi::{
    JIT_DEOPT_MAX_REGISTERS, JIT_DEOPT_MAX_SLOTS, JIT_RUNTIME_ABI_HASH, JIT_RUNTIME_ABI_VERSION,
    JitAbiSlot, JitAbiValue, JitBailout, JitBailoutKind, JitCExit, JitCExitTag, JitCFrameView,
    JitCValue, JitCValueTag, JitCallResult, JitCallStatus, JitDeoptState, JitExceptionMarker,
    JitFrameHandle, JitFrameView, JitHelperDispatch, JitNativeArgFlags, JitNativeCallArgument,
    JitNativeCallFrame, JitNativeCallKind, JitNativeCallTarget, JitNativeControlRecord,
    JitNativeDestructorPoint, JitNativeDispatchTrampoline, JitNativeDynamicCodeKind,
    JitNativeDynamicCodeRequest, JitNativeDynamicCodeTrampoline, JitNativeExceptionHandler,
    JitNativeFiberState, JitNativeFrameHeader, JitNativeGeneratorState, JitNativeIndirectionEntry,
    JitNativePcMetadata, JitNativeResumeInputKind, JitNativeRootEntry, JitNativeRootKind,
    JitNativeSuspendKind, JitNativeSuspensionGenerationPolicy, JitOpaqueHandle, JitOpaqueValueKind,
    JitRegionResult, JitRuntimeCallout, JitRuntimeCalloutResult, JitRuntimeHelperTable,
    JitSideExit, JitVmContextHandle, SideExitReason, helper_id, jit_default_helper_dispatch,
};
pub use backend::{NativeCompileOutcome, NativeCompileRequest, NativeCompilerApi};
pub use code_manager::{
    CompiledRegionMetadata, CraneliftCodeCacheDisposition, CraneliftCodeKey, CraneliftCodeManager,
    CraneliftCodeManagerError, CraneliftCodeManagerEvent, CraneliftCodeManagerStats,
    ManagedJitFunction, SharedJitCodeHandle, cranelift_code_manager_stats, global_code_manager,
};
pub use cranelift_lowering::{
    CraneliftClifSmokeResult, CraneliftLoweringError, CraneliftLoweringResult,
    CraneliftLoweringStats, CraneliftMachineCodeHandle, CraneliftNativeCompiler,
    build_trivial_add_clif_smoke, lower_function_to_cranelift,
};
pub use dynamic_code::{
    DynamicCodeCacheDisposition, DynamicCodeCacheKey, DynamicCodeCompileError,
    DynamicCodeCompileOnce, DynamicNativeArtifact, DynamicNativeEntry,
};
pub use eligibility::{
    JitCandidateKind, JitEligibility, JitEligibilityReason, JitEligibilityReport,
    JitEligibilityStats, analyze_jit_eligibility, call_args_are_jit_primitive,
};
pub use helpers::{
    JIT_HELPER_REGISTRY_ABI_HASH, JIT_HELPER_STATUS_COMPILE_REQUIRED, JIT_HELPER_STATUS_FALLBACK,
    JIT_HELPER_STATUS_FATAL, JIT_HELPER_STATUS_OK, JIT_HELPER_STATUS_OVERFLOW, JIT_HELPER_SYMBOLS,
    JitHelperArgKind, JitHelperId, JitHelperReturnKind, JitHelperSymbol, helper_registry_is_stable,
    helper_registry_layout_summary, lookup_helper_by_id, lookup_helper_by_name,
};
pub use host_isa::{CraneliftHostIsaError, CraneliftHostIsaIdentity, cranelift_host_isa_identity};
use php_ir::{BlockId, FunctionId, InstrId, IrSpan, IrUnit, LocalId};
use std::fmt;
use std::mem;
use std::sync::Arc;

const JIT_NATIVE_HANDLER_RESUME_TAG: u32 = 0x8000_0000;
const JIT_NATIVE_SUSPENSION_RESUME_TAG: u32 = 0x4000_0000;

const fn native_handler_resume_id(block: BlockId) -> i32 {
    (JIT_NATIVE_HANDLER_RESUME_TAG | block.raw()) as i32
}

const fn native_suspension_resume_id(continuation_id: u32) -> i32 {
    (JIT_NATIVE_SUSPENSION_RESUME_TAG | continuation_id) as i32
}

/// Stable native compiler identity embedded in code/cache metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CraneliftCompilerIdentity;

impl CraneliftCompilerIdentity {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        "cranelift"
    }
}

/// Runtime-owned helper addresses the backend may call from generated code.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitRuntimeHelperAddresses {
    /// Versioned `repr(C)` helper table for broad Region IR callouts.
    pub helper_table: usize,
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
    /// Typed dynamic-call resolver/invoker; never an interpreter dispatcher.
    pub native_call_dispatch: usize,
    /// Dynamic include/eval/declaration compiler and native-entry invoker.
    pub native_dynamic_code: usize,
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
    /// Effective runtime/compiler configuration hash for process-cache identity.
    pub config_hash: u64,
    /// Runtime invalidation generation (for example a class-layout epoch).
    pub invalidation_generation: u64,
    /// Stable identity of linked source/dependency inputs.
    pub dependency_identity: Option<String>,
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
            config_hash: 0,
            invalidation_generation: 0,
            dependency_identity: None,
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

    /// Adds the effective runtime/compiler configuration identity.
    #[must_use]
    pub const fn with_config_hash(mut self, config_hash: u64) -> Self {
        self.config_hash = config_hash;
        self
    }

    /// Adds the runtime generation that invalidates layout-sensitive code.
    #[must_use]
    pub const fn with_invalidation_generation(mut self, generation: u64) -> Self {
        self.invalidation_generation = generation;
        self
    }

    /// Adds the linked dependency identity used by native code caches.
    #[must_use]
    pub fn with_dependency_identity(mut self, identity: impl Into<String>) -> Self {
        self.dependency_identity = Some(identity.into());
        self
    }
}

/// Opaque handle for a compiled function.
#[derive(Clone, Debug)]
pub struct JitFunctionHandle {
    /// Stable handle id.
    pub id: u64,
    /// Region that produced this handle.
    pub region_id: String,
    /// Backend that produced this handle.
    pub compiler: CraneliftCompilerIdentity,
    native_entry: Option<JitNativeEntry>,
    specialization: JitNativeSpecialization,
    code_bytes: u64,
    helper_calls_per_invocation: u64,
    fast_path_hits_per_invocation: u64,
    property_load_metadata: Option<JitPropertyLoadMetadata>,
    region_state_metadata: Option<Arc<JitRegionStateMetadata>>,
    code_lifetime: Option<SharedJitCodeHandle>,
    code_manager_event: Option<CraneliftCodeManagerEvent>,
}

impl PartialEq for JitFunctionHandle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.region_id == other.region_id
            && self.compiler == other.compiler
            && self.native_entry == other.native_entry
            && self.specialization == other.specialization
            && self.code_bytes == other.code_bytes
            && self.helper_calls_per_invocation == other.helper_calls_per_invocation
            && self.fast_path_hits_per_invocation == other.fast_path_hits_per_invocation
            && self.property_load_metadata == other.property_load_metadata
            && self.region_state_metadata == other.region_state_metadata
            && self.code_lifetime == other.code_lifetime
    }
}

/// One precise native continuation in an executable region.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitContinuationMetadata {
    pub id: u32,
    pub function: FunctionId,
    pub block: BlockId,
    pub instruction: Option<InstrId>,
    pub span: IrSpan,
    pub live_locals: Vec<LocalId>,
}

/// Native code range attributed to one precise region continuation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitNativePcRange {
    pub function: FunctionId,
    pub start: u32,
    pub end: u32,
    pub continuation_id: u32,
}

/// Exception-handler table row published for explicit native unwind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitExceptionHandlerMetadata {
    pub function: FunctionId,
    pub enter_continuation: u32,
    pub catch: Option<BlockId>,
    pub catch_types: Vec<String>,
    pub finally: Option<BlockId>,
    pub after: BlockId,
    pub exception_local: Option<LocalId>,
}

/// GC roots live at one native safepoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitNativeSafepointMetadata {
    pub function: FunctionId,
    pub continuation_id: u32,
    /// Baseline frames publish tagged handles through these stable slots.
    pub baseline_frame_slots: Vec<LocalId>,
    /// Optimized code must provide stack-map or shadow-slot entries.
    pub optimized_roots_required: bool,
}

/// Stable native entry published for one generator/fiber suspension.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitNativeSuspensionMetadata {
    pub function: FunctionId,
    pub continuation_id: u32,
    pub resume_id: i32,
    pub kind: JitNativeSuspendKind,
    pub span: IrSpan,
    pub live_locals: Vec<LocalId>,
    pub owning_generation_required: bool,
}

/// Dynamic source/declaration site compiled and invoked through the native
/// runtime compiler boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitNativeDynamicCodeMetadata {
    pub function: FunctionId,
    pub continuation_id: u32,
    pub kind: JitNativeDynamicCodeKind,
    pub declared_function: Option<FunctionId>,
    pub span: IrSpan,
    pub process_cache: bool,
    pub restart_cache: bool,
}

/// Source-level frame resolved from a native PC without interpreter frames.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitNativeBacktraceFrame {
    pub function: FunctionId,
    pub continuation_id: u32,
    pub span: IrSpan,
}

/// Runtime/native action selected while unwinding one compiled PHP frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JitNativeUnwindTarget {
    Catch {
        block: BlockId,
        exception_local: Option<LocalId>,
        handler_index: usize,
    },
    Finally {
        block: BlockId,
        pending: JitCallStatus,
        handler_index: usize,
    },
    Propagate(JitCallStatus),
}

/// One native loop-entry point addressable by a stable OSR ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitOsrEntryMetadata {
    pub id: u32,
    pub function: FunctionId,
    pub block: BlockId,
    pub continuation_id: u32,
    pub live_locals: Vec<LocalId>,
}

/// Immutable state metadata attached to one compiled region handle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitRegionStateMetadata {
    pub local_count: u32,
    /// Statically linked native call sites in this compiled call graph.
    pub compiled_to_compiled_call_sites: u64,
    pub continuations: Vec<JitContinuationMetadata>,
    pub native_pc_ranges: Vec<JitNativePcRange>,
    pub osr_entries: Vec<JitOsrEntryMetadata>,
    pub exception_handlers: Vec<JitExceptionHandlerMetadata>,
    pub safepoints: Vec<JitNativeSafepointMetadata>,
    pub suspensions: Vec<JitNativeSuspensionMetadata>,
    pub dynamic_code: Vec<JitNativeDynamicCodeMetadata>,
}

impl JitRegionStateMetadata {
    /// Resolves a generated PC offset to precise PHP source metadata.
    #[must_use]
    pub fn resolve_native_pc(&self, pc: u32) -> Option<JitNativeBacktraceFrame> {
        let range = self
            .native_pc_ranges
            .iter()
            .find(|range| range.start <= pc && pc < range.end)?;
        let continuation = self.continuations.iter().find(|continuation| {
            continuation.function == range.function && continuation.id == range.continuation_id
        })?;
        Some(JitNativeBacktraceFrame {
            function: continuation.function,
            continuation_id: continuation.id,
            span: continuation.span,
        })
    }

    /// Selects catch/finally control without entering an interpreter dispatch
    /// loop. The runtime supplies PHP class matching for a thrown object.
    #[must_use]
    pub fn select_native_unwind(
        &self,
        function: FunctionId,
        active_handler_depth: usize,
        status: JitCallStatus,
        mut catch_matches: impl FnMut(&[String]) -> bool,
    ) -> JitNativeUnwindTarget {
        let handlers = self
            .exception_handlers
            .iter()
            .enumerate()
            .filter(|(_, handler)| handler.function == function)
            .take(active_handler_depth)
            .collect::<Vec<_>>();
        for (index, handler) in handlers.into_iter().rev() {
            if status == JitCallStatus::THROW
                && let Some(catch) = handler.catch
                && catch_matches(&handler.catch_types)
            {
                return JitNativeUnwindTarget::Catch {
                    block: catch,
                    exception_local: handler.exception_local,
                    handler_index: index,
                };
            }
            if let Some(finally) = handler.finally {
                return JitNativeUnwindTarget::Finally {
                    block: finally,
                    pending: status,
                    handler_index: index,
                };
            }
        }
        JitNativeUnwindTarget::Propagate(status)
    }
}

impl Eq for JitFunctionHandle {}

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
    /// `JitCValueTag` (as `u16`) the loading function's declared return type
    /// requires the property value to already have, or `0` for no expectation.
    ///
    /// A leaf's native result bypasses the interpreter's return-site coercion,
    /// so a scalar of a *different* type than the declared return type must
    /// side-exit (the interpreter then coerces `bool` → `int(1)`, `int` →
    /// `float`, or throws the exact `TypeError`) instead of committing the raw
    /// property value. Recognizers set this from the return type (`int` →
    /// `Int`, `float` → `FloatBits`, `bool` → `Bool`, `mixed` → `0`) and
    /// reject return types with a richer coercion matrix.
    pub expected_result_tag: u16,
}

/// Compile-time metadata for a monomorphic property-store fast path.
///
/// Same guard shape as [`JitPropertyLoadMetadata`] — the store helper performs
/// the identical receiver-class layout guard before committing the write — but
/// kept a distinct type because the store's recognition-time contract is
/// stricter (declared *untyped*, non-readonly, hook-free, symmetric-visibility
/// public slot), and conflating the two would let a load-eligible property leak
/// into the write path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitPropertyStoreMetadata {
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
    pub const fn metadata_only(
        id: u64,
        region_id: String,
        compiler: CraneliftCompilerIdentity,
    ) -> Self {
        Self {
            id,
            region_id,
            compiler,
            native_entry: None,
            specialization: JitNativeSpecialization::Generic,
            code_bytes: 0,
            helper_calls_per_invocation: 0,
            fast_path_hits_per_invocation: 0,
            property_load_metadata: None,
            region_state_metadata: None,
            code_lifetime: None,
            code_manager_event: None,
        }
    }

    /// Creates a scalar integer native-entry handle.
    #[must_use]
    pub(crate) const fn i64_native(
        id: u64,
        region_id: String,
        compiler: CraneliftCompilerIdentity,
        address: usize,
        arity: u8,
        code_bytes: u64,
    ) -> Self {
        Self {
            id,
            region_id,
            compiler,
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
            region_state_metadata: None,
            code_lifetime: None,
            code_manager_event: None,
        }
    }

    /// Creates a status/out-pointer integer native-entry handle.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn i64_status_out_native(
        id: u64,
        region_id: String,
        compiler: CraneliftCompilerIdentity,
        address: usize,
        arity: u8,
        code_bytes: u64,
        helper_calls_per_invocation: u64,
        fast_path_hits_per_invocation: u64,
        region_state_metadata: JitRegionStateMetadata,
    ) -> Self {
        Self {
            id,
            region_id,
            compiler,
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
            region_state_metadata: Some(Arc::new(region_state_metadata)),
            code_lifetime: None,
            code_manager_event: None,
        }
    }

    /// Creates a status/out-pointer native entry for one opaque value and int index.
    #[must_use]
    pub(crate) const fn value_i64_status_out_native(
        id: u64,
        region_id: String,
        compiler: CraneliftCompilerIdentity,
        address: usize,
        code_bytes: u64,
        helper_calls_per_invocation: u64,
        fast_path_hits_per_invocation: u64,
    ) -> Self {
        Self {
            id,
            region_id,
            compiler,
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
            region_state_metadata: None,
            code_lifetime: None,
            code_manager_event: None,
        }
    }

    /// Creates a status/out-pointer native entry for one opaque value.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn value_status_out_native(
        id: u64,
        region_id: String,
        compiler: CraneliftCompilerIdentity,
        address: usize,
        code_bytes: u64,
        helper_calls_per_invocation: u64,
        fast_path_hits_per_invocation: u64,
        specialization: JitNativeSpecialization,
    ) -> Self {
        Self {
            id,
            region_id,
            compiler,
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
            region_state_metadata: None,
            code_lifetime: None,
            code_manager_event: None,
        }
    }

    /// Creates a status/out-pointer native entry for one opaque value plus metadata.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn value_metadata_status_out_native(
        id: u64,
        region_id: String,
        compiler: CraneliftCompilerIdentity,
        address: usize,
        code_bytes: u64,
        helper_calls_per_invocation: u64,
        fast_path_hits_per_invocation: u64,
        property_load_metadata: JitPropertyLoadMetadata,
    ) -> Self {
        Self {
            id,
            region_id,
            compiler,
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
            region_state_metadata: None,
            code_lifetime: None,
            code_manager_event: None,
        }
    }

    /// Creates a status/out-pointer native entry for two opaque values.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn value_value_status_out_native(
        id: u64,
        region_id: String,
        compiler: CraneliftCompilerIdentity,
        address: usize,
        code_bytes: u64,
        helper_calls_per_invocation: u64,
        fast_path_hits_per_invocation: u64,
        specialization: JitNativeSpecialization,
    ) -> Self {
        Self {
            id,
            region_id,
            compiler,
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
            region_state_metadata: None,
            code_lifetime: None,
            code_manager_event: None,
        }
    }

    pub(crate) fn bind_code_lifetime(&mut self, lifetime: SharedJitCodeHandle) {
        debug_assert_eq!(self.native_entry_address(), Some(lifetime.entry_address()));
        self.code_lifetime = Some(lifetime);
    }

    pub(crate) fn bind_code_manager_event(&mut self, event: CraneliftCodeManagerEvent) {
        self.code_manager_event = Some(event);
    }

    /// Exact process-cache event associated with publication of this handle.
    #[must_use]
    pub fn code_manager_event(&self) -> Option<CraneliftCodeManagerEvent> {
        self.code_manager_event
    }

    /// Returns the owning code generation for native handles.
    #[must_use]
    pub fn code_generation_id(&self) -> Option<u64> {
        self.code_lifetime
            .as_ref()
            .map(SharedJitCodeHandle::generation_id)
    }

    pub(crate) fn native_entry_address(&self) -> Option<usize> {
        self.native_entry.map(|entry| entry.address)
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

    /// Native call sites executed by one successful straight-line invocation.
    #[must_use]
    pub fn compiled_to_compiled_calls_per_invocation(&self) -> u64 {
        self.region_state_metadata
            .as_ref()
            .map(|metadata| metadata.compiled_to_compiled_call_sites)
            .unwrap_or(0)
    }

    /// Returns monomorphic property-load metadata when this handle carries it.
    #[must_use]
    pub const fn property_load_metadata(&self) -> Option<&JitPropertyLoadMetadata> {
        self.property_load_metadata.as_ref()
    }

    /// Returns precise continuation and native-PC metadata for executable regions.
    #[must_use]
    pub fn region_state_metadata(&self) -> Option<&JitRegionStateMetadata> {
        self.region_state_metadata.as_deref()
    }

    /// Binds layout-sensitive metadata to the runtime epoch at compilation.
    pub fn bind_runtime_layout_version(&mut self, layout_version: u64) {
        if let Some(metadata) = self.property_load_metadata.as_mut() {
            metadata.layout_version = layout_version;
        }
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

    /// Invokes a scalar region while retaining precise state on a native exit.
    pub fn invoke_i64_with_deopt(
        &self,
        args: &[i64],
        runtime_abi_hash: u64,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
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
        entry.invoke_i64_with_deopt(args)
    }

    /// Runs catch/finally continuations through native block entries until the
    /// status is handled or must propagate to the caller. No interpreter frame
    /// is constructed during this loop.
    pub fn invoke_i64_with_native_unwind(
        &self,
        args: &[i64],
        runtime_abi_hash: u64,
        mut catch_matches: impl FnMut(&[String], i64) -> bool,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        let Some(entry) = self.native_entry else {
            return Err(JitInvokeError::MissingNativeEntry);
        };
        let Some(metadata) = self.region_state_metadata() else {
            return self.invoke_i64_with_deopt(args, runtime_abi_hash);
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
        let function = metadata
            .continuations
            .first()
            .map(|continuation| continuation.function)
            .ok_or(JitInvokeError::MissingNativeEntry)?;
        let mut outcome = entry.invoke_i64_with_deopt(args)?;
        let mut handler_depth = metadata
            .exception_handlers
            .iter()
            .filter(|handler| handler.function == function)
            .count();
        loop {
            let JitI64InvokeOutcome::SideExit {
                status,
                value,
                state,
            } = outcome
            else {
                return Ok(outcome);
            };
            let control = JitCallStatus(status as u32);
            if !matches!(
                control,
                JitCallStatus::THROW
                    | JitCallStatus::RETURN
                    | JitCallStatus::RETURN_REFERENCE
                    | JitCallStatus::EXIT
            ) {
                return Ok(JitI64InvokeOutcome::SideExit {
                    status,
                    value,
                    state,
                });
            }
            match metadata.select_native_unwind(function, handler_depth, control, |types| {
                catch_matches(types, value)
            }) {
                JitNativeUnwindTarget::Catch {
                    block,
                    exception_local,
                    handler_index,
                } => {
                    handler_depth = handler_index;
                    let mut resume_state = state;
                    if let Some(local) = exception_local
                        && local.index() < JIT_DEOPT_MAX_SLOTS
                    {
                        resume_state.slots[local.index()] = value;
                        resume_state.initialized_mask |= 1_u64 << local.raw();
                    }
                    outcome = entry.invoke_i64_handler_resume(
                        args,
                        block,
                        JitCallStatus::CONTINUE,
                        value,
                        resume_state,
                    )?;
                }
                JitNativeUnwindTarget::Finally {
                    block,
                    pending,
                    handler_index,
                } => {
                    handler_depth = handler_index;
                    outcome =
                        entry.invoke_i64_handler_resume(args, block, pending, value, state)?;
                }
                JitNativeUnwindTarget::Propagate(_) => {
                    return Ok(JitI64InvokeOutcome::SideExit {
                        status,
                        value,
                        state,
                    });
                }
            }
        }
    }

    /// Resumes exactly one generated generator/fiber continuation. Scheduling,
    /// delegated iteration, and heap-state ownership remain runtime concerns;
    /// PHP control after the suspension executes in generated code.
    pub fn invoke_i64_suspension_resume(
        &self,
        args: &[i64],
        state: &JitDeoptState,
        input: JitNativeResumeInputKind,
        value: i64,
        runtime_abi_hash: u64,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        let Some(metadata) = self.region_state_metadata() else {
            return Err(JitInvokeError::MissingSuspensionEntry(
                state.continuation_id,
            ));
        };
        if !metadata
            .suspensions
            .iter()
            .any(|entry| entry.continuation_id == state.continuation_id)
        {
            return Err(JitInvokeError::MissingSuspensionEntry(
                state.continuation_id,
            ));
        }
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
        entry.invoke_i64_suspension_resume(args, state, input, value)
    }

    /// Enters a compiled loop through a stable native OSR entry.
    pub fn invoke_i64_osr(
        &self,
        args: &[i64],
        entry_id: u32,
        state: &JitDeoptState,
        runtime_abi_hash: u64,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        let Some(metadata) = self.region_state_metadata() else {
            return Err(JitInvokeError::MissingOsrEntry(entry_id));
        };
        let Some(entry_metadata) = metadata
            .osr_entries
            .iter()
            .find(|entry| entry.id == entry_id)
        else {
            return Err(JitInvokeError::MissingOsrEntry(entry_id));
        };
        if entry_metadata
            .live_locals
            .iter()
            .any(|local| state.initialized_mask & (1_u64 << local.raw()) == 0)
        {
            return Err(JitInvokeError::IncompleteOsrState(entry_id));
        }
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
        entry.invoke_i64_osr(args, entry_id, state)
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

    fn invoke_i64_with_deopt(self, args: &[i64]) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        match self.kind {
            JitNativeEntryKind::I64StatusOut => self.invoke_i64_status_out_with_deopt(args),
            JitNativeEntryKind::I64Return => self
                .invoke_i64_return(args)
                .map(JitI64InvokeOutcome::Returned),
            JitNativeEntryKind::ValueI64StatusOut
            | JitNativeEntryKind::ValueStatusOut
            | JitNativeEntryKind::ValueMetadataStatusOut
            | JitNativeEntryKind::ValueValueStatusOut => Err(JitInvokeError::ArityMismatch {
                expected: self.arity,
                actual: args.len() as u8,
            }),
        }
    }

    fn invoke_i64_osr(
        self,
        args: &[i64],
        entry_id: u32,
        state: &JitDeoptState,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        if self.kind != JitNativeEntryKind::I64StatusOut {
            return Err(JitInvokeError::MissingOsrEntry(entry_id));
        }
        self.invoke_i64_status_out_with_resume(args, entry_id as i32, state as *const _)
    }

    fn invoke_i64_handler_resume(
        self,
        args: &[i64],
        block: BlockId,
        status: JitCallStatus,
        value: i64,
        mut state: JitDeoptState,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        if self.kind != JitNativeEntryKind::I64StatusOut {
            return Err(JitInvokeError::MissingNativeEntry);
        }
        state.control_status = status;
        state.control_value = value;
        self.invoke_i64_status_out_with_resume(
            args,
            native_handler_resume_id(block),
            &state as *const _,
        )
    }

    fn invoke_i64_suspension_resume(
        self,
        args: &[i64],
        state: &JitDeoptState,
        input: JitNativeResumeInputKind,
        value: i64,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        if self.kind != JitNativeEntryKind::I64StatusOut {
            return Err(JitInvokeError::MissingSuspensionEntry(
                state.continuation_id,
            ));
        }
        let mut resumed = state.clone();
        resumed.control_status = if input == JitNativeResumeInputKind::THROW {
            JitCallStatus::THROW
        } else {
            JitCallStatus::CONTINUE
        };
        resumed.control_value = value;
        self.invoke_i64_status_out_with_resume(
            args,
            native_suspension_resume_id(state.continuation_id),
            &resumed as *const _,
        )
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
        match self.invoke_i64_status_out_with_deopt(args)? {
            JitI64InvokeOutcome::Returned(value) => Ok(value),
            JitI64InvokeOutcome::SideExit { status, .. } => {
                Err(JitInvokeError::NativeStatus(status))
            }
        }
    }

    fn invoke_i64_status_out_with_deopt(
        self,
        args: &[i64],
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        self.invoke_i64_status_out_with_resume(args, -1, std::ptr::null())
    }

    fn invoke_i64_status_out_with_resume(
        self,
        args: &[i64],
        resume_id: i32,
        resume_state: *const JitDeoptState,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        let mut out = 0_i64;
        let out_ptr = &mut out as *mut i64;
        let mut deopt = JitDeoptState::default();
        let deopt_ptr = &mut deopt as *mut JitDeoptState;
        // SAFETY: Handles are created only after Cranelift defines the matching
        // `extern "C" fn(...i64, *mut i64) -> i32` signature. The public method
        // checks ABI hash and exact arity before reaching this call.
        let status = unsafe {
            match args {
                [] => {
                    let function: extern "C" fn(
                        *mut i64,
                        *mut JitDeoptState,
                        i32,
                        *const JitDeoptState,
                    ) -> i32 = mem::transmute(self.address);
                    function(out_ptr, deopt_ptr, resume_id, resume_state)
                }
                [a] => {
                    let function: extern "C" fn(
                        i64,
                        *mut i64,
                        *mut JitDeoptState,
                        i32,
                        *const JitDeoptState,
                    ) -> i32 = mem::transmute(self.address);
                    function(*a, out_ptr, deopt_ptr, resume_id, resume_state)
                }
                [a, b] => {
                    let function: extern "C" fn(
                        i64,
                        i64,
                        *mut i64,
                        *mut JitDeoptState,
                        i32,
                        *const JitDeoptState,
                    ) -> i32 = mem::transmute(self.address);
                    function(*a, *b, out_ptr, deopt_ptr, resume_id, resume_state)
                }
                [a, b, c] => {
                    let function: extern "C" fn(
                        i64,
                        i64,
                        i64,
                        *mut i64,
                        *mut JitDeoptState,
                        i32,
                        *const JitDeoptState,
                    ) -> i32 = mem::transmute(self.address);
                    function(*a, *b, *c, out_ptr, deopt_ptr, resume_id, resume_state)
                }
                [a, b, c, d] => {
                    let function: extern "C" fn(
                        i64,
                        i64,
                        i64,
                        i64,
                        *mut i64,
                        *mut JitDeoptState,
                        i32,
                        *const JitDeoptState,
                    ) -> i32 = mem::transmute(self.address);
                    function(*a, *b, *c, *d, out_ptr, deopt_ptr, resume_id, resume_state)
                }
                _ => return Err(JitInvokeError::UnsupportedArity(args.len() as u8)),
            }
        };
        if status == JitCallStatus::RETURN.0 as i32 {
            Ok(JitI64InvokeOutcome::Returned(out))
        } else {
            Ok(JitI64InvokeOutcome::SideExit {
                status,
                value: out,
                state: deopt,
            })
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

/// Result of a scalar native invocation with precise side-exit state retained.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JitI64InvokeOutcome {
    Returned(i64),
    SideExit {
        status: i32,
        value: i64,
        state: JitDeoptState,
    },
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
    /// Requested OSR entry is not published by this handle.
    MissingOsrEntry(u32),
    /// Requested generator/fiber continuation is not part of this artifact.
    MissingSuspensionEntry(u32),
    /// Caller did not materialize every local required by the OSR entry.
    IncompleteOsrState(u32),
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
            Self::MissingNativeEntry
            | Self::UnsupportedArity(_)
            | Self::MissingOsrEntry(_)
            | Self::MissingSuspensionEntry(_)
            | Self::IncompleteOsrState(_) => JitSideExit::new(SideExitReason::UnsupportedValue),
            Self::AbiHashMismatch { .. } => JitSideExit::new(SideExitReason::AbiMismatch),
            Self::ArityMismatch { .. } => JitSideExit::new(SideExitReason::TypeMismatch),
            Self::NativeStatus(status)
                if *status == JIT_HELPER_STATUS_OVERFLOW
                    || *status == JitCallStatus::RECOMPILE_REQUESTED.0 as i32 =>
            {
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

/// Baseline compilation record for one function in an IR unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitUnitCompileRecord {
    pub function: FunctionId,
    pub result: JitCompileResult,
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

/// Mandatory native compiler engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitEngine {
    stats: JitStats,
}

impl JitEngine {
    /// Creates a Cranelift compiler engine.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stats: JitStats::default(),
        }
    }

    /// Returns the selected backend for this build.
    #[must_use]
    pub const fn compiler_kind(&self) -> CraneliftCompilerIdentity {
        CraneliftCompilerIdentity
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
        let mut backend = CraneliftNativeCompiler;
        self.compile_with_backend(request, &mut backend)
    }

    /// Attempts to compile one IR function with the build-selected backend.
    pub fn compile_function(
        &mut self,
        unit: &IrUnit,
        function: FunctionId,
        request: JitCompileRequest,
    ) -> Result<JitCompileResult, JitError> {
        let mut backend = CraneliftNativeCompiler;
        self.compile_function_with_backend(unit, function, request, &mut backend)
    }

    /// Compiles every known function body before an IR unit is executable.
    pub fn compile_unit(
        &mut self,
        unit: &IrUnit,
        request: JitCompileRequest,
    ) -> Result<Vec<JitUnitCompileRecord>, JitError> {
        self.compile_unit_with_runtime_helpers(unit, request, JitRuntimeHelperAddresses::default())
    }

    /// Compiles every body with the runtime helper table used by native code.
    pub fn compile_unit_with_runtime_helpers(
        &mut self,
        unit: &IrUnit,
        mut request: JitCompileRequest,
        runtime_helpers: JitRuntimeHelperAddresses,
    ) -> Result<Vec<JitUnitCompileRecord>, JitError> {
        if request.region_id.is_empty() {
            return Err(JitError::EmptyRegionId);
        }
        if request.ir_fingerprint.is_none() {
            request.ir_fingerprint = Some(stable_ir_fingerprint(unit));
        }
        if request.dependency_identity.is_none() {
            request.dependency_identity = Some(stable_dependency_identity(unit));
        }
        let base_region = request.region_id.clone();
        let mut records = Vec::with_capacity(unit.functions.len());
        for (index, function) in unit.functions.iter().enumerate() {
            let function_id = FunctionId::new(index as u32);
            let mut function_request = request.clone();
            function_request.region_id = format!("{base_region}.function.{index}");
            function_request.function_name = Some(function.name.clone());
            let result = self.compile_function_with_runtime_helpers(
                unit,
                function_id,
                function_request,
                runtime_helpers,
            )?;
            records.push(JitUnitCompileRecord {
                function: function_id,
                result,
            });
        }
        Ok(records)
    }

    /// Attempts to compile one IR function with runtime helper addresses.
    pub fn compile_function_with_runtime_helpers(
        &mut self,
        unit: &IrUnit,
        function: FunctionId,
        request: JitCompileRequest,
        runtime_helpers: JitRuntimeHelperAddresses,
    ) -> Result<JitCompileResult, JitError> {
        let mut backend = CraneliftNativeCompiler;
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
    pub fn compile_with_backend<B: NativeCompilerApi>(
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
    pub fn compile_function_with_backend<B: NativeCompilerApi>(
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

    fn compile_inner<B: NativeCompilerApi>(
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
        let backend_request = NativeCompileRequest {
            compile: &request,
            unit,
            function,
            runtime_helpers,
        };
        let outcome = backend.compile_region(&backend_request);
        self.record_compile_status(&outcome.status);
        self.stats.native_code_bytes += outcome.code_bytes;
        self.stats.native_compile_time_nanos += outcome.compile_time_nanos;
        if outcome.handle.is_some() {
            self.stats.executable_memory_allocations += 1;
        }

        Ok(JitCompileResult {
            status: outcome.status,
            handle: outcome.handle,
            diagnostics: outcome.diagnostics,
            stats: self.stats.clone(),
        })
    }

    fn record_compile_status(&mut self, status: &JitCompileStatus) {
        match status {
            JitCompileStatus::Rejected { .. } => self.stats.rejected += 1,
            JitCompileStatus::Compiled => self.stats.native_compiles += 1,
        }
    }
}

fn stable_ir_fingerprint(unit: &IrUnit) -> String {
    format!(
        "php-ir-v{}-{:016x}",
        unit.version,
        stable_text_hash(&format!("{unit:?}"))
    )
}

fn stable_dependency_identity(unit: &IrUnit) -> String {
    let dependencies = format!(
        "{:?}:{:?}:{:?}",
        unit.files, unit.linked_file_entries, unit.linked_entry_autoload_declarations
    );
    format!(
        "php-dependencies-v1-{:016x}",
        stable_text_hash(&dependencies)
    )
}

fn stable_text_hash(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

impl Default for JitEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{JitCompileRequest, JitEngine, JitError};
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
    fn empty_region_id_is_an_api_error() {
        let mut engine = JitEngine::new();
        let error = engine
            .compile(JitCompileRequest::new(""))
            .expect_err("empty ids are rejected");

        assert_eq!(error, JitError::EmptyRegionId);
        assert_eq!(engine.stats().compile_requests, 0);
    }

    #[test]
    fn compile_unit_records_every_known_function_before_publication() {
        let mut builder = IrBuilder::new(UnitId::new(44));
        let file = builder.add_file("unit.php");
        let span = IrSpan::new(file, 0, 8);
        let constant = builder.intern_constant(IrConstant::Int(4));
        let mut functions = Vec::new();
        for name in ["entry", "declared_later"] {
            let function = builder.start_function(name, FunctionFlags::default(), span);
            builder.set_return_type(function, Some(IrReturnType::Int));
            let block = builder.append_block(function);
            let value = builder.alloc_register(function);
            builder.emit(
                function,
                block,
                InstructionKind::LoadConst {
                    dst: value,
                    constant,
                },
                span,
            );
            builder.terminate_return(function, block, Some(Operand::Register(value)), span);
            functions.push(function);
        }
        builder.set_entry(functions[0]);
        let unit = builder.finish();
        let mut engine = JitEngine::new();
        let records = engine
            .compile_unit(&unit, JitCompileRequest::new("whole-unit"))
            .expect("unit compile records");

        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|record| {
            matches!(record.result.status, crate::JitCompileStatus::Compiled)
                && record.result.handle.is_some()
        }));
        assert_eq!(engine.stats().compile_requests, 2);
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
