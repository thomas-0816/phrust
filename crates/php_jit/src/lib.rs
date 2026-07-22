//! Mandatory Cranelift native compiler and execution ABI.
//!
//! Every product build lowers authoritative PHP IR through executable Region IR
//! and publishes native entries. Baseline and optimizing policies share this
//! compiler; there is no alternate executor or optional-native product mode.

// This crate is the engine's sanctioned native-codegen boundary: allocating
// executable memory, W^X toggling, and calling generated code are irreducibly
// `unsafe`, each guarded by a local `// SAFETY:` contract. The
// `runtime-hardening-lints` forbids `unsafe` in the runtime and native execution
// coordinator. Since php_vm depends on php_jit, that `-D unsafe-code` propagates
// here; opt this crate out at its root so the invariant keeps protecting those
// layers without banning unsafe in the one crate whose job requires it.
#![allow(unsafe_code)]

mod abi;
mod backend;
mod code_manager;
mod cranelift_lowering;
mod dynamic_code;
mod helpers;
mod host_isa;
mod native_cache;
pub mod region_ir;

pub use abi::{
    JIT_DEOPT_LOCAL_MASK_WORDS, JIT_DEOPT_MAX_REGISTERS, JIT_DEOPT_MAX_SLOTS,
    JIT_NATIVE_ARRAY_VIEW_ABI_VERSION, JIT_NATIVE_CONSTANT_VIEW_BOOL,
    JIT_NATIVE_CONSTANT_VIEW_FLOAT, JIT_NATIVE_CONSTANT_VIEW_INT, JIT_NATIVE_CONSTANT_VIEW_NONE,
    JIT_NATIVE_CONSTANT_VIEW_NULL, JIT_NATIVE_CONSTANT_VIEW_STRING,
    JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION, JIT_NATIVE_DIRECT_ARRAY_CURSOR_NONE,
    JIT_NATIVE_DIRECT_ARRAY_CURSOR_SHIFT, JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY,
    JIT_NATIVE_DIRECT_ARRAY_FLAGS_VERSION_MASK, JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS,
    JIT_NATIVE_DIRECT_ARRAY_FREE_NONE, JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY,
    JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY, JIT_NATIVE_DIRECT_STRING_CAPACITY_SHIFT,
    JIT_NATIVE_DIRECT_STRING_FREE_BUCKETS, JIT_NATIVE_DIRECT_STRING_MIN_CAPACITY,
    JIT_NATIVE_DIRECT_VALUE_CAPACITY, JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
    JIT_NATIVE_FOREACH_VIEW_ABI_VERSION, JIT_NATIVE_INSTANCEOF_PLAN_PUBLISHED,
    JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION, JIT_NATIVE_PREPARED_CLASS_ALLOCATABLE,
    JIT_NATIVE_PREPARED_CLASS_EMPTY, JIT_NATIVE_REFERENCE_ARRAY_KEY_INT,
    JIT_NATIVE_REFERENCE_ARRAY_KEY_STRING, JIT_NATIVE_REFERENCE_ARRAY_VALUE_FALSE,
    JIT_NATIVE_REFERENCE_ARRAY_VALUE_INT, JIT_NATIVE_REFERENCE_ARRAY_VALUE_NULL,
    JIT_NATIVE_REFERENCE_ARRAY_VALUE_STRING, JIT_NATIVE_REFERENCE_ARRAY_VALUE_TRUE,
    JIT_NATIVE_REFERENCE_ARRAY_VALUE_UNINITIALIZED, JIT_NATIVE_REFERENCE_ARRAY_VALUE_UNSUPPORTED,
    JIT_NATIVE_REFERENCE_ARRAY_VIEW_ABI_VERSION, JIT_NATIVE_REFERENCE_ARRAY_VIEW_EMPTY,
    JIT_NATIVE_REFERENCE_ARRAY_VIEW_PUBLISHED, JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
    JIT_NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_FALSE, JIT_NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_INT,
    JIT_NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_NULL, JIT_NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_TRUE,
    JIT_NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_UNINITIALIZED, JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY,
    JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED, JIT_NATIVE_SHARED_ARRAY_ABI_VERSION,
    JIT_NATIVE_STATIC_PROPERTY_CAPACITY, JIT_NATIVE_STRING_VALUE_ZERO,
    JIT_NATIVE_STRING_VIEW_ABI_VERSION, JIT_NATIVE_TRUSTED_CONSTANT_EMPTY,
    JIT_NATIVE_TRUSTED_CONSTANT_PUBLISHED, JIT_NATIVE_TRUSTED_GLOBAL_REFERENCE_EMPTY,
    JIT_NATIVE_TRUSTED_GLOBAL_REFERENCE_PUBLISHED,
    JIT_NATIVE_TRUSTED_PROPERTY_SLOT_DIMENSION_WRITABLE, JIT_NATIVE_TRUSTED_PROPERTY_SLOT_EMPTY,
    JIT_NATIVE_TRUSTED_PROPERTY_SLOT_PUBLISHED, JIT_NATIVE_TRUSTED_PROPERTY_SLOT_REFERENCEABLE,
    JIT_NATIVE_TRUSTED_PROPERTY_SLOT_WRITABLE, JIT_NATIVE_TRUSTED_STATIC_LOCAL_EMPTY,
    JIT_NATIVE_TRUSTED_STATIC_LOCAL_PUBLISHED, JIT_NATIVE_TRUSTED_STATIC_PROPERTY_EMPTY,
    JIT_NATIVE_TRUSTED_STATIC_PROPERTY_READABLE, JIT_NATIVE_TRUSTED_STATIC_PROPERTY_WRITABLE,
    JIT_NATIVE_VALUE_VIEW_ARRAY, JIT_NATIVE_VALUE_VIEW_BORROWED_REFERENCE_ARRAY,
    JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY, JIT_NATIVE_VALUE_VIEW_DIRECT_FOREACH,
    JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT, JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
    JIT_NATIVE_VALUE_VIEW_FLOAT, JIT_NATIVE_VALUE_VIEW_FOREACH_DIRECT, JIT_NATIVE_VALUE_VIEW_NONE,
    JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR, JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY,
    JIT_NATIVE_VALUE_VIEW_STRING, JIT_OPTIMIZING_EXIT_ARRAY_KEY_UNSUPPORTED,
    JIT_OPTIMIZING_EXIT_ARRAY_NOT_TAGGED, JIT_OPTIMIZING_EXIT_ARRAY_VIEW_MISSING,
    JIT_RUNTIME_ABI_HASH, JIT_RUNTIME_ABI_VERSION, JitAbiSlot, JitAbiValue, JitBailout,
    JitBailoutKind, JitCExit, JitCExitTag, JitCFrameView, JitCValue, JitCValueTag, JitCallResult,
    JitCallStatus, JitDeoptState, JitExceptionMarker, JitFrameHandle, JitFrameView,
    JitNativeArgFlags, JitNativeCallArgument, JitNativeCallFrame, JitNativeCallKind,
    JitNativeCallTarget, JitNativeConstantView, JitNativeControlRecord, JitNativeControlResult,
    JitNativeDestructorPoint, JitNativeDirectArrayEntry, JitNativeDispatchTrampoline,
    JitNativeDynamicCodeKind, JitNativeDynamicCodeRequest, JitNativeDynamicCodeTrampoline,
    JitNativeExceptionHandler, JitNativeFastStateHeader, JitNativeFiberState,
    JitNativeForeachEntry, JitNativeForeachView, JitNativeFrameHeader, JitNativeGeneratorState,
    JitNativeIndirectionEntry, JitNativeInstanceOfEntry, JitNativeInstanceOfPlan,
    JitNativePcMetadata, JitNativePreparedClassPlan, JitNativeReferenceArrayEntry,
    JitNativeReferenceArrayView, JitNativeReferenceScalarView, JitNativeResumeInputKind,
    JitNativeRootEntry, JitNativeRootKind, JitNativeRuntimeView, JitNativeRuntimeViewGuard,
    JitNativeStaticPropertySlot, JitNativeSuspendKind, JitNativeSuspensionGenerationPolicy,
    JitNativeTransitionState, JitNativeTrustedConstantSlot, JitNativeTrustedGlobalReferenceSlot,
    JitNativeTrustedPropertySlot, JitNativeTrustedStaticLocalSlot,
    JitNativeTrustedStaticPropertySlot, JitNativeValueSlot, JitOpaqueHandle, JitOpaqueValueKind,
    JitRegionResult, JitRuntimeCallout, JitRuntimeCalloutResult, JitSideExit, JitVmContextHandle,
    SideExitReason, activate_native_runtime_view, jit_native_direct_array_cursor,
    jit_native_direct_array_flags, jit_native_direct_string_capacity,
    jit_native_direct_string_reserved, jit_native_instanceof_index,
};
pub use backend::{NativeCompileOutcome, NativeCompileRequest, NativeCompilerApi};
pub use code_manager::{
    CompiledRegionMetadata, CraneliftCodeCacheDisposition, CraneliftCodeKey, CraneliftCodeManager,
    CraneliftCodeManagerError, CraneliftCodeManagerEvent, CraneliftCodeManagerStats,
    ManagedJitFunction, SharedJitCodeHandle, cranelift_code_manager_stats, global_code_manager,
};
pub use cranelift_lowering::{
    CraneliftClifSmokeResult, CraneliftLoweringError, CraneliftLoweringStats,
    CraneliftNativeCompiler, NATIVE_FRAGMENT_PLAN_SCHEMA_VERSION, NativeCompilePlan,
    NativeFunctionKey, NativeFunctionTier, NativeIndirectionCell, NativeIndirectionState,
    build_trivial_add_clif_smoke, native_compiler_mode_identity, native_function_key,
};
pub use dynamic_code::{
    DynamicCodeCacheDisposition, DynamicCodeCacheKey, DynamicCodeCompileError,
    DynamicCodeCompileOnce, DynamicNativeArtifact, DynamicNativeEntry,
};
pub use helpers::{
    JIT_HELPER_REGISTRY_ABI_HASH, JIT_HELPER_SYMBOLS, JitHelperArgKind, JitHelperId,
    JitHelperReturnKind, JitHelperSymbol, helper_registry_is_stable,
    helper_registry_layout_summary, lookup_helper_by_id, lookup_helper_by_name,
    resolve_helper_address,
};
pub use host_isa::{CraneliftHostIsaError, CraneliftHostIsaIdentity, cranelift_host_isa_identity};
pub use native_cache::{
    NativeArtifactCache, NativeArtifactImage, NativeCacheConfig, NativeCacheError,
    NativeCacheEvent, NativeCacheIdentity, NativeCacheMode, NativeCacheStats,
    NativeContinuationEntry, NativeExceptionEntry, NativeFunctionAbi, NativeFunctionImage,
    NativeHelperImport, NativeLoadedArtifact, NativeRelocation, NativeRelocationKind,
    NativeRelocationTarget, NativeResumeEntry, NativeRootMap, NativeSymbol, NativeTrapEntry,
    PNA_FORMAT_VERSION, PNA_MAGIC,
};
use php_ir::{BlockId, FunctionId, InstrId, IrSpan, IrUnit, LocalId};
use std::fmt;
use std::mem;
use std::sync::Arc;

const JIT_NATIVE_HANDLER_RESUME_TAG: u32 = 0x8000_0000;
const JIT_NATIVE_SUSPENSION_RESUME_TAG: u32 = 0x4000_0000;
pub const JIT_NATIVE_TRANSITION_RESUME_TAG: u32 = 0x2000_0000;
pub const JIT_NATIVE_OPTIMIZING_BLOCK_RESUME_TAG: u32 = 0x1000_0000;
/// Cranelift release included in restart-persistent native cache identity.
pub const CRANELIFT_VERSION: &str = "0.133.1";

const fn native_handler_resume_id(block: BlockId) -> i32 {
    (JIT_NATIVE_HANDLER_RESUME_TAG | block.raw()) as i32
}

const fn native_suspension_resume_id(continuation_id: u32) -> i32 {
    (JIT_NATIVE_SUSPENSION_RESUME_TAG | continuation_id) as i32
}

const fn native_transition_resume_id(continuation_id: u32) -> i32 {
    (JIT_NATIVE_TRANSITION_RESUME_TAG | continuation_id) as i32
}

const fn native_optimizing_continuation_resume_id(continuation_id: u32) -> i32 {
    (JIT_NATIVE_OPTIMIZING_BLOCK_RESUME_TAG | continuation_id) as i32
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
    /// Typed dynamic-call resolver/invoker; never an interpreter dispatcher.
    pub native_call_dispatch: usize,
    /// Direct statically identified builtin invocation over packed i64 arguments.
    pub native_builtin_dispatch: usize,
    /// Exact prepared symbol/reflection query handlers. Each address names one
    /// fixed builtin; optimizing code never supplies an operation or registry ID.
    pub native_defined: usize,
    pub native_function_exists: usize,
    pub native_class_exists: usize,
    pub native_interface_exists: usize,
    pub native_trait_exists: usize,
    pub native_enum_exists: usize,
    pub native_method_exists: usize,
    pub native_property_exists: usize,
    /// Exact prepared PCRE handlers. Each address names one fixed builtin;
    /// optimizing code never supplies an operation or registry ID.
    pub native_preg_match: usize,
    pub native_preg_match_all: usize,
    pub native_preg_replace: usize,
    pub native_preg_filter: usize,
    pub native_preg_split: usize,
    pub native_preg_grep: usize,
    pub native_preg_quote: usize,
    pub native_preg_last_error: usize,
    pub native_preg_last_error_msg: usize,
    /// Exact prepared JSON handlers.
    pub native_json_encode: usize,
    pub native_json_decode: usize,
    pub native_json_validate: usize,
    pub native_json_last_error: usize,
    pub native_json_last_error_msg: usize,
    /// Exact prepared formatting handlers.
    pub native_sprintf: usize,
    pub native_printf: usize,
    pub native_vsprintf: usize,
    pub native_vprintf: usize,
    /// Exact prepared path/filesystem handlers. Pure path handlers use only
    /// native values; filesystem queries receive the request's narrow
    /// cwd/capability view through FastState.
    pub native_basename: usize,
    pub native_dirname: usize,
    pub native_realpath: usize,
    pub native_file_exists: usize,
    /// Direct typed PHP semantic operation over packed i64 operands.
    pub native_semantic_dispatch: usize,
    /// Resolves or compiles one statically known PHP callee without invoking it.
    pub native_function_resolve: usize,
    /// Allocates bounded request-local native call-frame storage.
    pub native_frame_alloc: usize,
    /// Releases the most recent request-local call-frame allocation.
    pub native_frame_release: usize,
    /// Dynamic include/eval/declaration compiler and native-entry invoker.
    pub native_dynamic_code: usize,
    /// Typed PHP unary operation over native value handles.
    pub native_unary: usize,
    /// Typed PHP binary operation over native value handles.
    pub native_binary: usize,
    /// Typed PHP comparison over native value handles.
    pub native_compare: usize,
    /// Typed PHP cast over native value handles.
    pub native_cast: usize,
    /// Typed PHP echo operation.
    pub native_echo: usize,
    /// Exact direct-string output append without Rust `Value` materialization.
    pub native_echo_bytes: usize,
    /// Exact SSA-integer output append without Rust `Value` materialization.
    pub native_echo_int: usize,
    /// Exact SSA-float output append without Rust `Value` materialization.
    pub native_echo_float: usize,
    /// Exact SSA-float to direct-string conversion without `Value` materialization.
    pub native_float_to_string: usize,
    /// Exact exceptional SSA-float to integer conversion and warning path.
    pub native_float_to_int: usize,
    /// Exact direct-object class-name read without generic property dispatch.
    pub native_object_class_name: usize,
    /// Exact allocation from a request-published immutable class plan.
    pub native_prepared_object_new: usize,
    /// Exact shallow clone for an SSA-exact class without `__clone`.
    pub native_plain_object_clone: usize,
    /// Resolves one local load, including superglobal seeding and warnings.
    pub native_local_fetch: usize,
    /// Stores through a PHP reference cell or replaces a plain local value.
    pub native_local_store: usize,
    /// Performs the cold final release of one request-owned native value.
    pub native_value_release: usize,
    /// Creates or propagates one PHP reference cell.
    pub native_reference_bind: usize,
    /// Enforces one declared PHP function parameter type at a direct call site.
    pub native_argument_check: usize,
    /// Enforces one declared PHP function return type.
    pub native_return_check: usize,
    /// Materializes one throwable value for explicit native throw flow.
    pub native_exception_new: usize,
    /// Creates one request-owned PHP array value.
    pub native_array_new: usize,
    /// Creates one request-owned PHP object value.
    pub native_object_new: usize,
    /// Reads one named PHP object property.
    pub native_property_fetch: usize,
    /// Writes one named PHP object property.
    pub native_property_assign: usize,
    /// Clones one PHP object into a distinct identity.
    pub native_object_clone: usize,
    /// Clones one PHP object and applies replacement properties.
    pub native_object_clone_with: usize,
    /// Inserts or appends one PHP array element.
    pub native_array_insert: usize,
    /// Consumes a local array owner, inserts, and returns its replacement.
    pub native_array_insert_local: usize,
    /// Fetches one PHP array dimension.
    pub native_array_fetch: usize,
    /// Removes one PHP array dimension.
    pub native_array_unset: usize,
    /// Spreads one PHP array into another.
    pub native_array_spread: usize,
    /// Creates one by-value PHP foreach iterator.
    pub native_foreach_init: usize,
    /// Advances one by-value PHP foreach iterator.
    pub native_foreach_next: usize,
    /// Releases one PHP foreach iterator.
    pub native_foreach_cleanup: usize,
    /// Resolves one named PHP constant into a native value handle.
    pub native_constant_fetch: usize,
    /// Typed PHP truthiness operation used by native branches.
    pub native_truthy: usize,
    /// Direct, non-allocating PHP type-predicate operation.
    pub native_type_predicate: usize,
    /// Typed `strlen`/array `count` slow path for stable value views.
    pub native_stable_length: usize,
    /// Direct string contains/starts-with/ends-with predicate over native handles.
    pub native_string_predicate: usize,
    /// Typed runtime-fatal publication operation.
    pub native_runtime_fatal: usize,
    /// Cooperative execution-deadline poll emitted at native loop headers.
    pub native_execution_poll: usize,
}

/// Type predicates that generated baseline code may invoke without building a
/// generic PHP call frame. Numeric values are part of the runtime-helper ABI.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JitNativeTypePredicate {
    Int = 1,
    Float = 2,
    String = 3,
    Bool = 4,
    Null = 5,
    Array = 6,
    Object = 7,
    Resource = 8,
    Scalar = 9,
    Iterable = 10,
}

/// Stable high-bit namespace for immutable IR constant handles.
pub const JIT_VALUE_CONSTANT_TAG: u64 = 0x7ff1_0000_0000_0000;
/// Stable high-bit namespace for request-owned runtime value handles.
pub const JIT_VALUE_RUNTIME_TAG: u64 = 0x7ff2_0000_0000_0000;
/// Stable high-bit namespace for request-owned reference handles.
pub const JIT_VALUE_RUNTIME_REFERENCE_TAG: u64 = 0x7ff2_0001_0000_0000;
/// Stable high-bit namespace for request-owned array handles.
pub const JIT_VALUE_RUNTIME_ARRAY_TAG: u64 = 0x7ff2_0002_0000_0000;
/// Stable high-bit namespace for request-owned object handles.
pub const JIT_VALUE_RUNTIME_OBJECT_TAG: u64 = 0x7ff2_0003_0000_0000;
/// Stable high-bit namespace for request-owned string handles.
pub const JIT_VALUE_RUNTIME_STRING_TAG: u64 = 0x7ff2_0004_0000_0000;
/// Stable high-bit namespace for request-owned float handles.
pub const JIT_VALUE_RUNTIME_FLOAT_TAG: u64 = 0x7ff2_0005_0000_0000;
/// Stable high-bit namespace for request-owned resource handles.
pub const JIT_VALUE_RUNTIME_RESOURCE_TAG: u64 = 0x7ff2_0006_0000_0000;
/// Stable high-bit namespace for request-owned callable handles.
pub const JIT_VALUE_RUNTIME_CALLABLE_TAG: u64 = 0x7ff2_0007_0000_0000;
/// Stable high-bit namespace for request-owned generator handles.
pub const JIT_VALUE_RUNTIME_GENERATOR_TAG: u64 = 0x7ff2_0008_0000_0000;
/// Stable high-bit namespace for request-owned fiber handles.
pub const JIT_VALUE_RUNTIME_FIBER_TAG: u64 = 0x7ff2_0009_0000_0000;
/// Stable high-bit namespace for request-owned internal iterator handles.
pub const JIT_VALUE_RUNTIME_ITERATOR_TAG: u64 = 0x7ff2_000a_0000_0000;
pub const JIT_VALUE_TAG_MASK: u64 = 0xffff_0000_0000_0000;
/// Includes the runtime namespace and its stable subtype, while excluding the
/// low 32-bit request-owned table index.
pub const JIT_VALUE_RUNTIME_KIND_MASK: u64 = 0xffff_ffff_0000_0000;
/// Reserved constant handle used for a local that has not been initialized.
pub const JIT_VALUE_UNINITIALIZED: u32 = u32::MAX - 1;
/// Reserved immutable handle for PHP `false`.
pub const JIT_VALUE_FALSE: u32 = u32::MAX - 2;
/// Reserved immutable handle for PHP `true`.
pub const JIT_VALUE_TRUE: u32 = u32::MAX - 3;
/// Local-fetch helper flag for an ordinary non-global slot whose immediate
/// values can bypass request-context decoding.
pub const JIT_LOCAL_FETCH_PLAIN_LOCAL: u32 = 1 << 1;
/// Local-store helper flag for an ordinary non-global slot. The runtime still
/// checks both handles for PHP references before using the plain replacement
/// path.
pub const JIT_LOCAL_STORE_PLAIN_LOCAL: u32 = 1;
pub const JIT_LOCAL_STORE_MOVE_INPUT: u32 = 2;

/// Encodes one IR constant identity in an i64 native slot.
#[must_use]
pub const fn jit_encode_constant(constant: u32) -> i64 {
    (JIT_VALUE_CONSTANT_TAG | constant as u64) as i64
}

/// Decodes one IR constant identity from an i64 native slot.
#[must_use]
pub const fn jit_decode_constant(value: i64) -> Option<u32> {
    if (value as u64) & JIT_VALUE_TAG_MASK == JIT_VALUE_CONSTANT_TAG {
        Some(value as u32)
    } else {
        None
    }
}

/// Encodes one request-owned runtime value index in an i64 native slot.
#[must_use]
pub const fn jit_encode_runtime_value(index: u32) -> i64 {
    (JIT_VALUE_RUNTIME_TAG | index as u64) as i64
}

/// Encodes one request-owned runtime value index with its stable value class.
#[must_use]
pub const fn jit_encode_typed_runtime_value(index: u32, tag: u64) -> i64 {
    (tag | index as u64) as i64
}

/// Returns the stable runtime-handle subtype tag, when present.
#[must_use]
pub const fn jit_runtime_value_tag(value: i64) -> Option<u64> {
    if (value as u64) & JIT_VALUE_TAG_MASK == JIT_VALUE_RUNTIME_TAG {
        Some((value as u64) & JIT_VALUE_RUNTIME_KIND_MASK)
    } else {
        None
    }
}

/// Decodes one request-owned runtime value index from an i64 native slot.
#[must_use]
pub const fn jit_decode_runtime_value(value: i64) -> Option<u32> {
    if jit_runtime_value_tag(value).is_some() {
        Some(value as u32)
    } else {
        None
    }
}

/// Request to compile one future JIT region.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitExternalParameterSignature {
    /// Parameter name without `$`.
    pub name: String,
    /// True when the parameter aliases its caller argument.
    pub by_ref: bool,
    /// True when the parameter collects remaining arguments.
    pub variadic: bool,
}

/// Runtime-visible userland function signature from another compiled unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitExternalFunctionSignature {
    /// Case-insensitive PHP function name.
    pub name: String,
    /// Parameters in declaration order.
    pub params: Vec<JitExternalParameterSignature>,
}

/// Request to compile one future JIT region.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitCompileRequest {
    /// Stable region identifier chosen by the caller.
    pub region_id: String,
    /// Optional PHP function or method name for reports.
    pub function_name: Option<String>,
    /// Optional stable fingerprint of the requested function IR.
    pub ir_fingerprint: Option<String>,
    /// Stable identity of the source unit used to address linkage cells.
    ///
    /// This is deliberately separate from `ir_fingerprint`: changing an
    /// unrelated function must invalidate neither this function's code cache
    /// nor its single-flight compile key, while every declaration in one unit
    /// must still agree on the same indirection-cell namespace.
    pub deployment_identity: Option<String>,
    /// Process-local numeric identity shared with the active runtime unit.
    /// It is used only for request-cache addressing and is never persisted as
    /// an address-bearing artifact identity.
    pub deployment_runtime_identity: u64,
    /// Optimization level active when the request was made.
    pub opt_level: u8,
    /// Effective runtime/compiler configuration hash for process-cache identity.
    pub config_hash: u64,
    /// Runtime invalidation generation (for example a class-layout epoch).
    pub invalidation_generation: u64,
    /// Stable identity of linked source/dependency inputs.
    pub dependency_identity: Option<String>,
    /// Signatures already visible from other loaded PHP units.
    pub external_function_signatures: Vec<JitExternalFunctionSignature>,
}

impl JitCompileRequest {
    /// Creates a compile request for a region.
    #[must_use]
    pub fn new(region_id: impl Into<String>) -> Self {
        Self {
            region_id: region_id.into(),
            function_name: None,
            ir_fingerprint: None,
            deployment_identity: None,
            deployment_runtime_identity: 0,
            opt_level: 0,
            config_hash: 0,
            invalidation_generation: 0,
            dependency_identity: None,
            external_function_signatures: Vec::new(),
        }
    }

    /// Adds runtime-visible function signatures from other source units.
    #[must_use]
    pub fn with_external_function_signatures(
        mut self,
        signatures: Vec<JitExternalFunctionSignature>,
    ) -> Self {
        self.external_function_signatures = signatures;
        self
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

    /// Adds the source-unit identity used by native linkage cells.
    #[must_use]
    pub fn with_deployment_identity(mut self, identity: impl Into<String>) -> Self {
        self.deployment_identity = Some(identity.into());
        self
    }

    /// Adds the numeric identity used by the request-local native entry cache.
    #[must_use]
    pub const fn with_deployment_runtime_identity(mut self, identity: u64) -> Self {
        self.deployment_runtime_identity = identity;
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
    code_bytes: u64,
    helper_calls_per_invocation: u64,
    fast_path_hits_per_invocation: u64,
    ssa_promoted_locals: u64,
    ssa_promoted_registers: u64,
    ownership_moves: u64,
    region_state_metadata: Option<Arc<JitRegionStateMetadata>>,
    relocatable_code: Option<Arc<JitRelocatableCode>>,
    code_lifetime: Option<SharedJitCodeHandle>,
    cached_code_lifetime: Option<Arc<NativeLoadedArtifact>>,
    code_manager_event: Option<CraneliftCodeManagerEvent>,
}

impl PartialEq for JitFunctionHandle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.region_id == other.region_id
            && self.compiler == other.compiler
            && self.native_entry == other.native_entry
            && self.code_bytes == other.code_bytes
            && self.helper_calls_per_invocation == other.helper_calls_per_invocation
            && self.fast_path_hits_per_invocation == other.fast_path_hits_per_invocation
            && self.ssa_promoted_locals == other.ssa_promoted_locals
            && self.ssa_promoted_registers == other.ssa_promoted_registers
            && self.ownership_moves == other.ownership_moves
            && self.region_state_metadata == other.region_state_metadata
            && self.relocatable_code == other.relocatable_code
            && self.code_lifetime == other.code_lifetime
            && match (&self.cached_code_lifetime, &other.cached_code_lifetime) {
                (Some(left), Some(right)) => Arc::ptr_eq(left, right),
                (None, None) => true,
                _ => false,
            }
    }
}

/// Relocation kind emitted by the production Cranelift lowering path.
///
/// This deliberately contains only the relocation forms accepted by the PNA1
/// loader. An unsupported Cranelift relocation makes the compiled handle
/// ineligible for restart-persistent cache emission instead of persisting an
/// unchecked linker contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JitRelocatableKind {
    Abs64,
    X86PcRel4,
    X86CallPcRel4,
    Arm64Call,
}

/// Symbolic relocation target captured before `JITModule` finalization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JitRelocatableTarget {
    /// A function in the same generated graph.
    InternalFunction(FunctionId),
    /// A runtime helper imported by its stable link name.
    Helper(String),
}

/// One relocation relative to the beginning of [`JitRelocatableCode::code`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitRelocatableRelocation {
    pub offset: u64,
    pub kind: JitRelocatableKind,
    pub target: JitRelocatableTarget,
    pub addend: i64,
}

/// One function body inside a relocatable generated graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitRelocatableFunction {
    pub function: FunctionId,
    pub code_offset: u64,
    pub code_len: u64,
    pub arity: u8,
    pub local_count: u32,
}

/// Actual machine code and symbolic relocations produced by the same lowering
/// invocation that publishes the in-process executable entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitRelocatableCode {
    pub root: FunctionId,
    pub code: Vec<u8>,
    pub functions: Vec<JitRelocatableFunction>,
    pub relocations: Vec<JitRelocatableRelocation>,
}

/// One precise native continuation in an executable region.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct JitContinuationMetadata {
    pub id: u32,
    pub function: FunctionId,
    pub block: BlockId,
    pub instruction: Option<InstrId>,
    pub span: IrSpan,
    pub live_locals: Vec<LocalId>,
}

/// Native code range attributed to one precise region continuation.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct JitNativePcRange {
    pub function: FunctionId,
    pub start: u32,
    pub end: u32,
    pub continuation_id: u32,
}

/// Exception-handler table row published for explicit native unwind.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct JitExceptionHandlerMetadata {
    pub function: FunctionId,
    pub enter_continuation: u32,
    pub protected_blocks: Vec<BlockId>,
    pub catch: Option<BlockId>,
    pub catch_types: Vec<String>,
    pub finally: Option<BlockId>,
    pub after: BlockId,
    pub exception_local: Option<LocalId>,
}

/// GC roots live at one native safepoint.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct JitNativeSafepointMetadata {
    pub function: FunctionId,
    pub continuation_id: u32,
    /// Baseline frames publish tagged handles through these stable slots.
    pub baseline_frame_slots: Vec<LocalId>,
    /// Optimized code must provide stack-map or shadow-slot entries.
    pub optimized_roots_required: bool,
}

/// Stable native entry published for one generator/fiber suspension.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
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
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct JitNativeDynamicCodeMetadata {
    pub function: FunctionId,
    pub continuation_id: u32,
    pub kind: JitNativeDynamicCodeKind,
    pub declared_function: Option<FunctionId>,
    pub span: IrSpan,
    pub process_cache: bool,
    pub restart_cache: bool,
}

/// Exact baseline-native continuation available to optimized guard exits.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct JitNativeTransitionMetadata {
    pub function: FunctionId,
    pub native_version: u32,
    pub continuation_id: u32,
    pub resume_id: i32,
    pub span: IrSpan,
    pub live_locals: Vec<LocalId>,
    pub live_registers: Vec<php_ir::RegId>,
    pub result_register: Option<php_ir::RegId>,
}

/// Production code shape selected for one optimizing instruction.
///
/// This is emitted with the artifact rather than reconstructed from source
/// tests.  In particular, a helper-free optimizing relocation table is not
/// sufficient evidence when an instruction can immediately return
/// `RECOMPILE_REQUESTED` and execute the old baseline helper instead.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum JitProductionLoweringClass {
    DirectClif,
    DirectNativeData,
    CompiledNativeCall,
    BaselineFragmentTransition,
}

/// Emitted-code contract row for one optimizing instruction.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct JitProductionLoweringMetadata {
    pub function: FunctionId,
    pub continuation_id: u32,
    /// Stable authoritative `InstructionKind` variant name.
    pub operation: String,
    pub class: JitProductionLoweringClass,
    /// True only when otherwise-direct optimizing code embeds its own exit to
    /// the generic baseline implementation.  These sites are removed family
    /// by family by the native hot-path replacement contract.
    pub operation_local_transition: bool,
}

/// Process-local generated entry for one function in a compiled unit graph.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct JitNativeFunctionEntryMetadata {
    pub function: FunctionId,
    pub address: usize,
    pub arity: u8,
    /// Exact machine-code bytes emitted for this function body.
    #[serde(default)]
    pub code_bytes: u64,
    /// Cranelift's fixed native spill/temporary frame size. PHP locals and
    /// call frames live in the request arena and are not included here.
    #[serde(default)]
    pub native_stack_bytes: u32,
    /// Local slots required when this function is entered as a graph root.
    pub local_count: u32,
    /// Statically linked calls reached by one straight-line invocation.
    #[serde(default)]
    pub direct_call_sites: u64,
    /// Monomorphic instance-method subset of `direct_call_sites`.
    #[serde(default)]
    pub direct_method_call_sites: u64,
    /// Calls removed by the bounded scalar-wrapper inliner.
    #[serde(default)]
    pub inlined_call_sites: u64,
    /// Conservative emitted bytes attributed to bounded inlining.
    #[serde(default)]
    pub inline_bytes_added: u64,
    /// Tail calls emitted for this function body.
    #[serde(default)]
    pub tail_call_sites: u64,
    /// Stable rejection categories for direct calls considered by the inliner.
    #[serde(default)]
    pub inline_rejected_by_reason: std::collections::BTreeMap<String, u64>,
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
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct JitOsrEntryMetadata {
    pub id: u32,
    pub function: FunctionId,
    pub block: BlockId,
    pub continuation_id: u32,
    pub live_locals: Vec<LocalId>,
}

/// Immutable state metadata attached to one compiled region handle.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct JitRegionStateMetadata {
    pub local_count: u32,
    pub compiler_tier: region_ir::NativeCompilerTier,
    pub native_version: u32,
    /// Statically linked native call sites in this compiled call graph.
    pub compiled_to_compiled_call_sites: u64,
    /// Monomorphic instance-method subset of compiled native calls.
    #[serde(default)]
    pub compiled_to_compiled_method_call_sites: u64,
    #[serde(default)]
    pub inlined_call_sites: u64,
    #[serde(default)]
    pub inline_bytes_added: u64,
    #[serde(default)]
    pub tail_call_sites: u64,
    #[serde(default)]
    pub inline_rejected_by_reason: std::collections::BTreeMap<String, u64>,
    pub continuations: Vec<JitContinuationMetadata>,
    pub native_pc_ranges: Vec<JitNativePcRange>,
    pub osr_entries: Vec<JitOsrEntryMetadata>,
    pub exception_handlers: Vec<JitExceptionHandlerMetadata>,
    pub safepoints: Vec<JitNativeSafepointMetadata>,
    pub suspensions: Vec<JitNativeSuspensionMetadata>,
    pub dynamic_code: Vec<JitNativeDynamicCodeMetadata>,
    pub native_transitions: Vec<JitNativeTransitionMetadata>,
    /// Authoritative optimizing code-shape manifest. Baseline artifacts leave
    /// this empty; old cache entries deserialize with an empty manifest.
    #[serde(default)]
    pub production_lowering: Vec<JitProductionLoweringMetadata>,
    pub function_entries: Vec<JitNativeFunctionEntryMetadata>,
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
        active_block: Option<BlockId>,
        status: JitCallStatus,
        mut catch_matches: impl FnMut(&[String]) -> bool,
    ) -> JitNativeUnwindTarget {
        let handlers = self
            .exception_handlers
            .iter()
            .enumerate()
            .filter(|(_, handler)| handler.function == function)
            .filter(|(_, handler)| {
                active_block.is_none_or(|block| handler.protected_blocks.contains(&block))
            })
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

impl JitFunctionHandle {
    /// Publishes one validated PNA1 function entry while retaining the RX
    /// mapping for the complete lifetime of the handle.
    pub fn from_cached_artifact(
        artifact: Arc<NativeLoadedArtifact>,
        function: FunctionId,
        region_state_metadata: Option<JitRegionStateMetadata>,
    ) -> Result<Self, NativeCacheError> {
        let function_image = artifact
            .image()
            .functions
            .iter()
            .find(|entry| entry.function_id == function.raw())
            .ok_or(NativeCacheError::UnknownInternalSymbol(function.raw()))?;
        let address = artifact.entry_address(function.raw())?;
        Ok(Self {
            id: u64::from(function.raw()) + 1,
            region_id: format!("pna1.{}", function.raw()),
            compiler: CraneliftCompilerIdentity,
            native_entry: Some(JitNativeEntry {
                address,
                arity: function_image.arity,
                abi_hash: JIT_RUNTIME_ABI_HASH,
                kind: JitNativeEntryKind::PackedI64StatusOut,
            }),
            code_bytes: function_image.code_len,
            helper_calls_per_invocation: 0,
            fast_path_hits_per_invocation: 0,
            ssa_promoted_locals: 0,
            ssa_promoted_registers: 0,
            ownership_moves: 0,
            region_state_metadata: region_state_metadata.map(Arc::new),
            relocatable_code: None,
            code_lifetime: None,
            cached_code_lifetime: Some(artifact),
            code_manager_event: None,
        })
    }

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
            code_bytes: 0,
            helper_calls_per_invocation: 0,
            fast_path_hits_per_invocation: 0,
            ssa_promoted_locals: 0,
            ssa_promoted_registers: 0,
            ownership_moves: 0,
            region_state_metadata: None,
            relocatable_code: None,
            code_lifetime: None,
            cached_code_lifetime: None,
            code_manager_event: None,
        }
    }

    /// Creates a scalar integer native-entry handle.
    #[must_use]
    #[allow(dead_code)]
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
            code_bytes,
            helper_calls_per_invocation: 0,
            fast_path_hits_per_invocation: 0,
            ssa_promoted_locals: 0,
            ssa_promoted_registers: 0,
            ownership_moves: 0,
            region_state_metadata: None,
            relocatable_code: None,
            code_lifetime: None,
            cached_code_lifetime: None,
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
                kind: JitNativeEntryKind::PackedI64StatusOut,
            }),
            code_bytes,
            helper_calls_per_invocation,
            fast_path_hits_per_invocation,
            ssa_promoted_locals: 0,
            ssa_promoted_registers: 0,
            ownership_moves: 0,
            region_state_metadata: Some(Arc::new(region_state_metadata)),
            relocatable_code: None,
            code_lifetime: None,
            cached_code_lifetime: None,
            code_manager_event: None,
        }
    }

    pub(crate) fn bind_code_lifetime(&mut self, lifetime: SharedJitCodeHandle) {
        debug_assert_eq!(self.native_entry_address(), Some(lifetime.entry_address()));
        self.code_lifetime = Some(lifetime);
    }

    pub(crate) fn bind_ssa_metrics(
        &mut self,
        promoted_locals: u64,
        promoted_registers: u64,
        ownership_moves: u64,
    ) {
        self.ssa_promoted_locals = promoted_locals;
        self.ssa_promoted_registers = promoted_registers;
        self.ownership_moves = ownership_moves;
    }

    pub(crate) fn bind_code_manager_event(&mut self, event: CraneliftCodeManagerEvent) {
        self.code_manager_event = Some(event);
    }

    pub(crate) fn bind_relocatable_code(&mut self, code: JitRelocatableCode) {
        self.relocatable_code = Some(Arc::new(code));
    }

    /// Returns the pre-finalization machine-code image used for PNA1 emission.
    #[must_use]
    pub fn relocatable_code(&self) -> Option<&JitRelocatableCode> {
        self.relocatable_code.as_deref()
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

    /// Returns the generation-owned executable entry for native-to-native
    /// indirection. Callers must retain this handle (or its published cell)
    /// for the complete invocation lifetime.
    #[must_use]
    pub fn native_entry_address(&self) -> Option<usize> {
        self.native_entry.map(|entry| entry.address)
    }

    /// Returns native code bytes associated with the handle.
    #[must_use]
    pub const fn code_bytes(&self) -> u64 {
        self.code_bytes
    }

    /// Copies relocation-free machine code for restart-cache emission.
    ///
    /// Callers must restrict this to a single-function Region graph with no
    /// helper or external call relocations. General graphs must use a
    /// relocation-aware artifact emitter instead.
    pub fn copy_relocation_free_machine_code(&self) -> Option<Vec<u8>> {
        let address = self.native_entry_address()?;
        let length = usize::try_from(self.code_bytes).ok()?;
        if length == 0
            || self.helper_calls_per_invocation != 0
            || self
                .region_state_metadata()
                .is_some_and(|metadata| metadata.function_entries.len() != 1)
        {
            return None;
        }
        // SAFETY: a native handle owns an executable allocation of at least
        // `code_bytes` for its published entry for the lifetime of `self`.
        Some(unsafe { std::slice::from_raw_parts(address as *const u8, length) }.to_vec())
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

    /// Static executable SSA metrics for this compiled region graph.
    #[must_use]
    pub const fn ssa_metrics(&self) -> (u64, u64, u64) {
        (
            self.ssa_promoted_locals,
            self.ssa_promoted_registers,
            self.ownership_moves,
        )
    }

    /// Native call sites executed by one successful straight-line invocation.
    #[must_use]
    pub fn compiled_to_compiled_calls_per_invocation(&self) -> u64 {
        self.region_state_metadata
            .as_ref()
            .map(|metadata| metadata.compiled_to_compiled_call_sites)
            .unwrap_or(0)
    }

    /// Monomorphic method calls executed by one successful invocation.
    #[must_use]
    pub fn compiled_method_calls_per_invocation(&self) -> u64 {
        self.region_state_metadata
            .as_ref()
            .map(|metadata| metadata.compiled_to_compiled_method_call_sites)
            .unwrap_or(0)
    }

    /// Calls removed by bounded native inlining per invocation.
    #[must_use]
    pub fn inlined_calls_per_invocation(&self) -> u64 {
        self.region_state_metadata
            .as_ref()
            .map_or(0, |metadata| metadata.inlined_call_sites)
    }

    /// Conservative code bytes added by bounded native inlining.
    #[must_use]
    pub fn inline_bytes_added_per_invocation(&self) -> u64 {
        self.region_state_metadata
            .as_ref()
            .map_or(0, |metadata| metadata.inline_bytes_added)
    }

    /// Tail calls emitted per invocation.
    #[must_use]
    pub fn tail_calls_per_invocation(&self) -> u64 {
        self.region_state_metadata
            .as_ref()
            .map_or(0, |metadata| metadata.tail_call_sites)
    }

    /// Returns precise continuation and native-PC metadata for executable regions.
    #[must_use]
    pub fn region_state_metadata(&self) -> Option<&JitRegionStateMetadata> {
        self.region_state_metadata.as_deref()
    }

    /// Clones this graph handle with another function entry as its root.
    ///
    /// A production function artifact has one PHP-function root. This helper
    /// also supports validated offline or fragment bundles with multiple
    /// internal entries while retaining their owning generation.
    #[must_use]
    pub fn clone_for_function_entry(&self, function: FunctionId) -> Option<Self> {
        let metadata = self.region_state_metadata.as_ref()?;
        let function_entry = metadata
            .function_entries
            .iter()
            .find(|entry| entry.function == function)?;
        let mut native_entry = self.native_entry?;
        if native_entry.kind != JitNativeEntryKind::PackedI64StatusOut {
            return None;
        }
        native_entry.address = function_entry.address;
        native_entry.arity = function_entry.arity;

        let mut root_metadata = metadata.as_ref().clone();
        root_metadata.local_count = function_entry.local_count;
        root_metadata.compiled_to_compiled_call_sites = function_entry.direct_call_sites;
        root_metadata.compiled_to_compiled_method_call_sites =
            function_entry.direct_method_call_sites;
        root_metadata.inlined_call_sites = function_entry.inlined_call_sites;
        root_metadata.inline_bytes_added = function_entry.inline_bytes_added;
        root_metadata.tail_call_sites = function_entry.tail_call_sites;
        root_metadata.inline_rejected_by_reason = function_entry.inline_rejected_by_reason.clone();
        let mut handle = self.clone();
        handle.id = u64::from(function.raw()) + 1;
        handle.region_id = format!("{}.entry.{}", self.region_id, function.raw());
        handle.native_entry = Some(native_entry);
        handle.region_state_metadata = Some(Arc::new(root_metadata));
        // The graph's compile/cache event and emitted bytes belong to its
        // original root record. Alias records must not multiply telemetry.
        handle.code_bytes = 0;
        handle.code_manager_event = None;
        Some(handle)
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

    /// Invokes a validated native entry with the request fast-state pointer
    /// that generated code passes unchanged to callees and runtime helpers.
    pub fn invoke_i64_with_deopt_runtime(
        &self,
        args: &[i64],
        runtime_abi_hash: u64,
        runtime: *mut std::ffi::c_void,
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
        entry.invoke_i64_with_deopt_runtime(args, runtime)
    }

    /// Enters the exact baseline-native continuation described by `state`.
    /// Live locals/registers are reconstructed from the shared native state;
    /// instructions before the continuation are not executed again.
    pub fn invoke_i64_native_transition(
        &self,
        state: &JitNativeTransitionState,
        runtime_abi_hash: u64,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        self.invoke_i64_transition_impl(state, runtime_abi_hash, true, std::ptr::null_mut())
    }

    /// Enters a baseline continuation and keeps PHP catch/finally unwinding
    /// inside native code. This is the only production boundary used when an
    /// optimizing guard rejects its direct native representation.
    pub fn invoke_i64_native_transition_with_unwind_runtime(
        &self,
        state: &JitNativeTransitionState,
        runtime_abi_hash: u64,
        runtime: *mut std::ffi::c_void,
        catch_matches: impl FnMut(&[String], i64) -> bool,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        let outcome = self.invoke_i64_transition_impl(state, runtime_abi_hash, true, runtime)?;
        let arity = self.native_entry.map_or(0, |entry| entry.arity);
        let args = vec![0_i64; usize::from(arity)];
        self.continue_i64_with_native_unwind_runtime(
            &args,
            runtime_abi_hash,
            runtime,
            outcome,
            catch_matches,
        )
    }

    /// Re-enters a continuation published by this same native artifact. This
    /// is used for suspended nested calls whose caller and saved state share
    /// one generation, including optimizing-tier artifacts.
    pub fn invoke_i64_same_artifact_transition(
        &self,
        state: &JitNativeTransitionState,
        runtime_abi_hash: u64,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        self.invoke_i64_transition_impl(state, runtime_abi_hash, false, std::ptr::null_mut())
    }

    /// Re-enters a continuation while preserving the request fast-state
    /// pointer used by the suspended native call.
    pub fn invoke_i64_same_artifact_transition_runtime(
        &self,
        state: &JitNativeTransitionState,
        runtime_abi_hash: u64,
        runtime: *mut std::ffi::c_void,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        self.invoke_i64_transition_impl(state, runtime_abi_hash, false, runtime)
    }

    fn invoke_i64_transition_impl(
        &self,
        state: &JitNativeTransitionState,
        runtime_abi_hash: u64,
        require_baseline: bool,
        runtime: *mut std::ffi::c_void,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        if runtime_abi_hash != JIT_RUNTIME_ABI_HASH {
            return Err(JitInvokeError::AbiHashMismatch {
                expected: JIT_RUNTIME_ABI_HASH,
                actual: runtime_abi_hash,
            });
        }
        let metadata =
            self.region_state_metadata()
                .ok_or(JitInvokeError::MissingNativeTransition {
                    function: state.function_id,
                    continuation: state.continuation_id,
                })?;
        if require_baseline && metadata.compiler_tier != region_ir::NativeCompilerTier::Baseline {
            return Err(JitInvokeError::NativeTransitionRequiresBaseline);
        }
        let transition = metadata
            .native_transitions
            .iter()
            .find(|entry| {
                entry.function.raw() == state.function_id
                    && entry.continuation_id == state.continuation_id
            })
            .ok_or(JitInvokeError::MissingNativeTransition {
                function: state.function_id,
                continuation: state.continuation_id,
            })?;
        let locals_complete = transition
            .live_locals
            .iter()
            .all(|local| state.local_initialized(*local));
        let register_source_slots = transition
            .live_registers
            .iter()
            .map(|register| {
                (0..JIT_DEOPT_MAX_REGISTERS).find(|snapshot_slot| {
                    let initialized = state.initialized_register_mask
                        & (1_u64
                            .checked_shl(u32::try_from(*snapshot_slot).unwrap_or(u32::MAX))
                            .unwrap_or(0))
                        != 0;
                    initialized && state.register_ids[*snapshot_slot] == register.raw()
                })
            })
            .collect::<Vec<_>>();
        let registers_complete = register_source_slots.iter().all(Option::is_some);
        if !locals_complete || !registers_complete {
            return Err(JitInvokeError::IncompleteNativeTransition(
                state.continuation_id,
            ));
        }
        let mut remapped_state = state.clone();
        remapped_state.initialized_register_mask =
            if transition.live_registers.len() >= u64::BITS as usize {
                u64::MAX
            } else {
                (1_u64 << transition.live_registers.len()).saturating_sub(1)
            };
        remapped_state.register_ids.fill(u32::MAX);
        remapped_state.registers.fill(0);
        for (target_slot, (register, source_slot)) in transition
            .live_registers
            .iter()
            .zip(register_source_slots.into_iter())
            .enumerate()
        {
            let source_slot = source_slot.expect("transition completeness was checked");
            remapped_state.register_ids[target_slot] = register.raw();
            remapped_state.registers[target_slot] = state.registers[source_slot];
        }
        let function_entry = metadata
            .function_entries
            .iter()
            .find(|entry| entry.function.raw() == state.function_id)
            .ok_or(JitInvokeError::MissingNativeTransition {
                function: state.function_id,
                continuation: state.continuation_id,
            })?;
        let Some(mut entry) = self.native_entry else {
            return Err(JitInvokeError::MissingNativeEntry);
        };
        entry.address = function_entry.address;
        entry.arity = function_entry.arity;
        let args = vec![0_i64; usize::from(function_entry.arity)];
        entry.invoke_i64_status_out_with_resume(
            &args,
            transition.resume_id,
            std::ptr::from_ref(&remapped_state),
            runtime,
        )
    }

    /// Runs optimized native code and transfers a guard exit directly into an
    /// already-published baseline-native continuation.
    pub fn invoke_i64_with_native_transition(
        &self,
        baseline: &Self,
        args: &[i64],
        runtime_abi_hash: u64,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        let outcome = self.invoke_i64_with_deopt(args, runtime_abi_hash)?;
        let JitI64InvokeOutcome::SideExit { status, state, .. } = &outcome else {
            return Ok(outcome);
        };
        if *status != JitCallStatus::RECOMPILE_REQUESTED.0 as i32 {
            return Ok(outcome);
        }
        baseline.invoke_i64_native_transition(state, runtime_abi_hash)
    }

    /// Runs catch/finally continuations through native block entries until the
    /// status is handled or must propagate to the caller. No interpreter frame
    /// is constructed during this loop.
    pub fn invoke_i64_with_native_unwind(
        &self,
        args: &[i64],
        runtime_abi_hash: u64,
        catch_matches: impl FnMut(&[String], i64) -> bool,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        self.invoke_i64_with_native_unwind_runtime(
            args,
            runtime_abi_hash,
            std::ptr::null_mut(),
            catch_matches,
        )
    }

    /// Runs native unwind with a request fast-state pointer passed directly
    /// through every generated continuation entry.
    pub fn invoke_i64_with_native_unwind_runtime(
        &self,
        args: &[i64],
        runtime_abi_hash: u64,
        runtime: *mut std::ffi::c_void,
        catch_matches: impl FnMut(&[String], i64) -> bool,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        let Some(entry) = self.native_entry else {
            return Err(JitInvokeError::MissingNativeEntry);
        };
        if self.region_state_metadata().is_none() {
            return self.invoke_i64_with_deopt_runtime(args, runtime_abi_hash, runtime);
        }
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
        let outcome = entry.invoke_i64_with_deopt_runtime(args, runtime)?;
        self.continue_i64_with_native_unwind_runtime(
            args,
            runtime_abi_hash,
            runtime,
            outcome,
            catch_matches,
        )
    }

    fn continue_i64_with_native_unwind_runtime(
        &self,
        args: &[i64],
        runtime_abi_hash: u64,
        runtime: *mut std::ffi::c_void,
        mut outcome: JitI64InvokeOutcome,
        mut catch_matches: impl FnMut(&[String], i64) -> bool,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        let Some(entry) = self.native_entry else {
            return Err(JitInvokeError::MissingNativeEntry);
        };
        let Some(metadata) = self.region_state_metadata() else {
            return Ok(outcome);
        };
        if runtime_abi_hash != JIT_RUNTIME_ABI_HASH || entry.abi_hash != JIT_RUNTIME_ABI_HASH {
            return Err(JitInvokeError::AbiHashMismatch {
                expected: JIT_RUNTIME_ABI_HASH,
                actual: runtime_abi_hash,
            });
        }
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
            let function = FunctionId::new(state.function_id);
            let active_block = metadata
                .continuations
                .iter()
                .find(|continuation| {
                    continuation.function == function && continuation.id == state.continuation_id
                })
                .map(|continuation| continuation.block);
            let handler_depth = metadata
                .exception_handlers
                .iter()
                .filter(|handler| handler.function == function)
                .filter(|handler| {
                    active_block.is_none_or(|block| handler.protected_blocks.contains(&block))
                })
                .count();
            match metadata.select_native_unwind(
                function,
                handler_depth,
                active_block,
                control,
                |types| catch_matches(types, value),
            ) {
                JitNativeUnwindTarget::Catch {
                    block,
                    exception_local,
                    handler_index: _,
                } => {
                    let mut resume_state = state;
                    if let Some(local) = exception_local
                        && local.index() < JIT_DEOPT_MAX_SLOTS
                    {
                        resume_state.slots[local.index()] = value;
                        resume_state.mark_local_initialized(local);
                    }
                    outcome = entry.invoke_i64_handler_resume(
                        args,
                        block,
                        JitCallStatus::CONTINUE,
                        value,
                        resume_state,
                        runtime,
                    )?;
                }
                JitNativeUnwindTarget::Finally {
                    block,
                    pending,
                    handler_index: _,
                } => {
                    outcome = entry
                        .invoke_i64_handler_resume(args, block, pending, value, state, runtime)?;
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
        self.invoke_i64_suspension_resume_runtime(
            args,
            state,
            input,
            value,
            runtime_abi_hash,
            std::ptr::null_mut(),
        )
    }

    /// Resumes a generated suspension with the original request fast state.
    pub fn invoke_i64_suspension_resume_runtime(
        &self,
        args: &[i64],
        state: &JitDeoptState,
        input: JitNativeResumeInputKind,
        value: i64,
        runtime_abi_hash: u64,
        runtime: *mut std::ffi::c_void,
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
        entry.invoke_i64_suspension_resume(args, state, input, value, runtime)
    }

    /// Resumes a generator/fiber continuation and executes any generated
    /// catch/finally entries selected by the resumed control flow.
    pub fn invoke_i64_suspension_resume_with_native_unwind(
        &self,
        args: &[i64],
        state: &JitDeoptState,
        input: JitNativeResumeInputKind,
        value: i64,
        runtime_abi_hash: u64,
        catch_matches: impl FnMut(&[String], i64) -> bool,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        self.invoke_i64_suspension_resume_with_native_unwind_runtime(
            args,
            state,
            input,
            value,
            runtime_abi_hash,
            std::ptr::null_mut(),
            catch_matches,
        )
    }

    /// Resumes a suspension and generated unwind with direct request state.
    pub fn invoke_i64_suspension_resume_with_native_unwind_runtime(
        &self,
        args: &[i64],
        state: &JitDeoptState,
        input: JitNativeResumeInputKind,
        value: i64,
        runtime_abi_hash: u64,
        runtime: *mut std::ffi::c_void,
        mut catch_matches: impl FnMut(&[String], i64) -> bool,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        let Some(metadata) = self.region_state_metadata() else {
            return self.invoke_i64_suspension_resume_runtime(
                args,
                state,
                input,
                value,
                runtime_abi_hash,
                runtime,
            );
        };
        let Some(entry) = self.native_entry else {
            return Err(JitInvokeError::MissingNativeEntry);
        };
        let function = FunctionId::new(state.function_id);
        let mut outcome = self.invoke_i64_suspension_resume_runtime(
            args,
            state,
            input,
            value,
            runtime_abi_hash,
            runtime,
        )?;
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
            let active_block = metadata
                .continuations
                .iter()
                .find(|continuation| {
                    continuation.function == function && continuation.id == state.continuation_id
                })
                .map(|continuation| continuation.block);
            let handler_depth = metadata
                .exception_handlers
                .iter()
                .filter(|handler| handler.function == function)
                .filter(|handler| {
                    active_block.is_none_or(|block| handler.protected_blocks.contains(&block))
                })
                .count();
            match metadata.select_native_unwind(
                function,
                handler_depth,
                active_block,
                control,
                |types| catch_matches(types, value),
            ) {
                JitNativeUnwindTarget::Catch {
                    block,
                    exception_local,
                    handler_index: _,
                } => {
                    let mut resume_state = state;
                    if let Some(local) = exception_local
                        && local.index() < JIT_DEOPT_MAX_SLOTS
                    {
                        resume_state.slots[local.index()] = value;
                        resume_state.mark_local_initialized(local);
                    }
                    outcome = entry.invoke_i64_handler_resume(
                        args,
                        block,
                        JitCallStatus::CONTINUE,
                        value,
                        resume_state,
                        runtime,
                    )?;
                }
                JitNativeUnwindTarget::Finally {
                    block,
                    pending,
                    handler_index: _,
                } => {
                    outcome = entry
                        .invoke_i64_handler_resume(args, block, pending, value, state, runtime)?;
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
            .any(|local| !state.local_initialized(*local))
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
    #[allow(dead_code)]
    I64Return,
    PackedI64StatusOut,
}

macro_rules! invoke_i64_return_entry {
    ($address:expr; $($argument:ident),+ $(,)?) => {{
        let function: extern "C" fn($(invoke_i64_return_entry!(@type $argument)),+) -> i64 =
            mem::transmute($address);
        function($(*$argument),+)
    }};
    (@type $argument:ident) => { i64 };
}

impl JitNativeEntry {
    fn invoke_i64(self, args: &[i64]) -> Result<i64, JitInvokeError> {
        match self.kind {
            JitNativeEntryKind::I64Return => self.invoke_i64_return(args),
            JitNativeEntryKind::PackedI64StatusOut => self.invoke_i64_status_out(args),
        }
    }

    fn invoke_i64_with_deopt(self, args: &[i64]) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        match self.kind {
            JitNativeEntryKind::PackedI64StatusOut => self.invoke_i64_status_out_with_deopt(args),
            JitNativeEntryKind::I64Return => self
                .invoke_i64_return(args)
                .map(JitI64InvokeOutcome::Returned),
        }
    }

    fn invoke_i64_osr(
        self,
        args: &[i64],
        entry_id: u32,
        state: &JitDeoptState,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        if self.kind != JitNativeEntryKind::PackedI64StatusOut {
            return Err(JitInvokeError::MissingOsrEntry(entry_id));
        }
        self.invoke_i64_status_out_with_resume(
            args,
            entry_id as i32,
            state as *const _,
            std::ptr::null_mut(),
        )
    }

    fn invoke_i64_handler_resume(
        self,
        args: &[i64],
        block: BlockId,
        status: JitCallStatus,
        value: i64,
        mut state: JitDeoptState,
        runtime: *mut std::ffi::c_void,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        if self.kind != JitNativeEntryKind::PackedI64StatusOut {
            return Err(JitInvokeError::MissingNativeEntry);
        }
        state.control_status = status;
        state.control_value = value;
        self.invoke_i64_status_out_with_resume(
            args,
            native_handler_resume_id(block),
            &state as *const _,
            runtime,
        )
    }

    fn invoke_i64_suspension_resume(
        self,
        args: &[i64],
        state: &JitDeoptState,
        input: JitNativeResumeInputKind,
        value: i64,
        runtime: *mut std::ffi::c_void,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        if self.kind != JitNativeEntryKind::PackedI64StatusOut {
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
            runtime,
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
                [a, b, c, d, e] => invoke_i64_return_entry!(self.address; a, b, c, d, e),
                [a, b, c, d, e, f] => invoke_i64_return_entry!(self.address; a, b, c, d, e, f),
                [a, b, c, d, e, f, g] => {
                    invoke_i64_return_entry!(self.address; a, b, c, d, e, f, g)
                }
                [a, b, c, d, e, f, g, h] => {
                    invoke_i64_return_entry!(self.address; a, b, c, d, e, f, g, h)
                }
                [a, b, c, d, e, f, g, h, i] => {
                    invoke_i64_return_entry!(self.address; a, b, c, d, e, f, g, h, i)
                }
                [a, b, c, d, e, f, g, h, i, j] => {
                    invoke_i64_return_entry!(self.address; a, b, c, d, e, f, g, h, i, j)
                }
                [a, b, c, d, e, f, g, h, i, j, k] => {
                    invoke_i64_return_entry!(self.address; a, b, c, d, e, f, g, h, i, j, k)
                }
                [a, b, c, d, e, f, g, h, i, j, k, l] => {
                    invoke_i64_return_entry!(self.address; a, b, c, d, e, f, g, h, i, j, k, l)
                }
                [a, b, c, d, e, f, g, h, i, j, k, l, m] => {
                    invoke_i64_return_entry!(self.address; a, b, c, d, e, f, g, h, i, j, k, l, m)
                }
                [a, b, c, d, e, f, g, h, i, j, k, l, m, n] => {
                    invoke_i64_return_entry!(self.address; a, b, c, d, e, f, g, h, i, j, k, l, m, n)
                }
                [a, b, c, d, e, f, g, h, i, j, k, l, m, n, o] => {
                    invoke_i64_return_entry!(self.address; a, b, c, d, e, f, g, h, i, j, k, l, m, n, o)
                }
                [a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p] => {
                    invoke_i64_return_entry!(self.address; a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p)
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
        self.invoke_i64_status_out_with_resume(args, -1, std::ptr::null(), std::ptr::null_mut())
    }

    fn invoke_i64_with_deopt_runtime(
        self,
        args: &[i64],
        runtime: *mut std::ffi::c_void,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        self.invoke_i64_status_out_with_resume(args, -1, std::ptr::null(), runtime)
    }

    fn invoke_i64_status_out_with_resume(
        self,
        args: &[i64],
        resume_id: i32,
        resume_state: *const JitDeoptState,
        runtime: *mut std::ffi::c_void,
    ) -> Result<JitI64InvokeOutcome, JitInvokeError> {
        let mut out = 0_i64;
        let out_ptr = &mut out as *mut i64;
        // Normal native returns only use the request runtime view inside this
        // buffer. Do not clear the 256 local and 64 register slots for every
        // warm call; complete the sparse state only when native code actually
        // reports a side exit.
        let mut deopt = prepare_native_deopt_out(runtime);
        let deopt_ptr = deopt.as_mut_ptr();
        // SAFETY: Handles are created only after Cranelift defines the matching
        // packed status/out signature. `args.as_ptr()` remains valid for the
        // synchronous native call, including the zero-length case where native
        // code performs no argument loads. The public method checks the ABI hash
        // and exact arity before reaching this call.
        let status = unsafe {
            let function: extern "C" fn(
                *mut std::ffi::c_void,
                *const i64,
                *mut i64,
                *mut JitDeoptState,
                i32,
                *const JitDeoptState,
            ) -> i32 = mem::transmute(self.address);
            function(
                runtime,
                args.as_ptr(),
                out_ptr,
                deopt_ptr,
                resume_id,
                resume_state,
            )
        };
        if status == JitCallStatus::RETURN.0 as i32 {
            Ok(JitI64InvokeOutcome::Returned(out))
        } else {
            // SAFETY: the call initializes every slot selected by the masks.
            // The preparation initialized all scalar metadata; this fills the
            // complementary sparse slots before constructing the Rust value.
            let deopt = unsafe { complete_native_deopt_out(deopt) };
            Ok(JitI64InvokeOutcome::SideExit {
                status,
                value: out,
                state: deopt,
            })
        }
    }
}

fn prepare_native_deopt_out(
    runtime: *mut std::ffi::c_void,
) -> std::mem::MaybeUninit<JitDeoptState> {
    let mut state = std::mem::MaybeUninit::<JitDeoptState>::uninit();
    let pointer = state.as_mut_ptr();
    // SAFETY: each write targets a distinct scalar/metadata field. The large
    // value arrays deliberately remain uninitialized until a side exit.
    unsafe {
        std::ptr::addr_of_mut!((*pointer).function_id).write(u32::MAX);
        std::ptr::addr_of_mut!((*pointer).continuation_id).write(u32::MAX);
        std::ptr::addr_of_mut!((*pointer).slot_count).write(0);
        std::ptr::addr_of_mut!((*pointer).native_version).write(0);
        std::ptr::addr_of_mut!((*pointer).initialized_mask).write(0);
        std::ptr::addr_of_mut!((*pointer).initialized_masks_high)
            .write([0; JIT_DEOPT_LOCAL_MASK_WORDS - 1]);
        std::ptr::addr_of_mut!((*pointer).control_status).write(JitCallStatus::CONTINUE);
        std::ptr::addr_of_mut!((*pointer).control_reserved).write(0);
        std::ptr::addr_of_mut!((*pointer).control_value).write(0);
        std::ptr::addr_of_mut!((*pointer).suspend_kind).write(0);
        std::ptr::addr_of_mut!((*pointer).suspend_flags).write(0);
        std::ptr::addr_of_mut!((*pointer).yielded_key).write(0);
        std::ptr::addr_of_mut!((*pointer).delegation_handle).write(0);
        std::ptr::addr_of_mut!((*pointer).initialized_register_mask).write(0);
        std::ptr::addr_of_mut!((*pointer).register_ids).write([u32::MAX; JIT_DEOPT_MAX_REGISTERS]);
        std::ptr::addr_of_mut!((*pointer).runtime_view).write(abi::native_runtime_view(runtime));
    }
    state
}

/// Completes the inactive portion of a sparse native side-exit state.
///
/// # Safety
///
/// `state` must have been produced by [`prepare_native_deopt_out`], and native
/// code must have initialized every local/register selected by its masks.
unsafe fn complete_native_deopt_out(
    mut state: std::mem::MaybeUninit<JitDeoptState>,
) -> JitDeoptState {
    let pointer = state.as_mut_ptr();
    // SAFETY: scalar masks were initialized before the native call. Selected
    // elements were written by native code; the loops initialize every other
    // element without reading it.
    unsafe {
        let low_mask = std::ptr::addr_of!((*pointer).initialized_mask).read();
        let high_masks = std::ptr::addr_of!((*pointer).initialized_masks_high).read();
        let slots = std::ptr::addr_of_mut!((*pointer).slots).cast::<i64>();
        for index in 0..JIT_DEOPT_MAX_SLOTS {
            let mask = if index < u64::BITS as usize {
                low_mask
            } else {
                high_masks[index / u64::BITS as usize - 1]
            };
            if mask & (1_u64 << (index % u64::BITS as usize)) == 0 {
                slots.add(index).write(0);
            }
        }
        let register_mask = std::ptr::addr_of!((*pointer).initialized_register_mask).read();
        let registers = std::ptr::addr_of_mut!((*pointer).registers).cast::<i64>();
        for index in 0..JIT_DEOPT_MAX_REGISTERS {
            if register_mask & (1_u64 << index) == 0 {
                registers.add(index).write(0);
            }
        }
        state.assume_init()
    }
}

/// Result of a scalar native invocation with precise side-exit state retained.
#[derive(Clone, Debug, Eq, PartialEq)]
// The fixed-size ABI state intentionally stays inline so a side exit cannot
// introduce allocation failure or allocator work on a runtime control path.
#[allow(clippy::large_enum_variant)]
pub enum JitI64InvokeOutcome {
    Returned(i64),
    SideExit {
        status: i32,
        value: i64,
        state: JitDeoptState,
    },
}

/// Invocation failures reported before a native entry can be selected.
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
    /// Requested baseline-native continuation is absent.
    MissingNativeTransition { function: u32, continuation: u32 },
    /// The selected target is not a non-speculative baseline artifact.
    NativeTransitionRequiresBaseline,
    /// Guard state omits a local/register required by the continuation.
    IncompleteNativeTransition(u32),
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
    /// through a generic or less-specialized native continuation.
    #[must_use]
    pub const fn side_exit(&self) -> JitSideExit {
        match self {
            Self::MissingNativeEntry
            | Self::UnsupportedArity(_)
            | Self::MissingOsrEntry(_)
            | Self::MissingSuspensionEntry(_)
            | Self::IncompleteOsrState(_)
            | Self::MissingNativeTransition { .. }
            | Self::NativeTransitionRequiresBaseline
            | Self::IncompleteNativeTransition(_) => {
                JitSideExit::new(SideExitReason::UnsupportedValue)
            }
            Self::AbiHashMismatch { .. } => JitSideExit::new(SideExitReason::AbiMismatch),
            Self::ArityMismatch { .. } => JitSideExit::new(SideExitReason::TypeMismatch),
            Self::NativeStatus(status)
                if *status == JitCallStatus::RECOMPILE_REQUESTED.0 as i32 =>
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

    /// Offline/precompile API that requests every known body separately.
    /// Production execution must use [`Self::compile_function`] on demand.
    pub fn compile_unit(
        &mut self,
        unit: &IrUnit,
        request: JitCompileRequest,
    ) -> Result<Vec<JitUnitCompileRecord>, JitError> {
        self.compile_unit_with_runtime_helpers(unit, request, JitRuntimeHelperAddresses::default())
    }

    /// Offline/precompile API that emits one independent compile group per body.
    pub fn compile_unit_with_runtime_helpers(
        &mut self,
        unit: &IrUnit,
        mut request: JitCompileRequest,
        runtime_helpers: JitRuntimeHelperAddresses,
    ) -> Result<Vec<JitUnitCompileRecord>, JitError> {
        if request.region_id.is_empty() {
            return Err(JitError::EmptyRegionId);
        }
        let deployment_identity = request
            .deployment_identity
            .get_or_insert_with(|| stable_ir_fingerprint(unit))
            .clone();
        if request.dependency_identity.is_none() {
            request.dependency_identity = Some(stable_dependency_identity(unit));
        }
        let base_region = request.region_id.clone();
        let mut records = vec![None; unit.functions.len()];
        for (index, function) in unit.functions.iter().enumerate() {
            if records[index].is_some() {
                continue;
            }
            let function_id = FunctionId::new(index as u32);
            let mut function_request = request.clone();
            function_request.region_id = format!("{base_region}.function.{index}");
            function_request.function_name = Some(function.name.clone());
            function_request.ir_fingerprint =
                Some(stable_function_ir_fingerprint(unit, function_id));
            function_request.deployment_identity = Some(deployment_identity.clone());
            let result = self.compile_function_with_runtime_helpers(
                unit,
                function_id,
                function_request,
                runtime_helpers,
            )?;
            records[index] = Some(JitUnitCompileRecord {
                function: function_id,
                result: result.clone(),
            });
            let Some(handle) = result.handle.as_ref() else {
                continue;
            };
            let Some(metadata) = handle.region_state_metadata() else {
                continue;
            };
            for entry in &metadata.function_entries {
                let entry_index = entry.function.index();
                if entry_index >= records.len() || records[entry_index].is_some() {
                    continue;
                }
                let Some(alias) = handle.clone_for_function_entry(entry.function) else {
                    continue;
                };
                let mut alias_result = result.clone();
                alias_result.handle = Some(alias);
                records[entry_index] = Some(JitUnitCompileRecord {
                    function: entry.function,
                    result: alias_result,
                });
            }
        }
        Ok(records
            .into_iter()
            .enumerate()
            .map(|(index, record)| {
                record.unwrap_or_else(|| JitUnitCompileRecord {
                    function: FunctionId::new(index as u32),
                    result: JitCompileResult {
                        status: JitCompileStatus::Rejected {
                            reason: "native unit publication did not produce a function entry"
                                .to_owned(),
                        },
                        handle: None,
                        diagnostics: vec![
                            "compiled Region graph omitted a required function entry".to_owned(),
                        ],
                        stats: self.stats.clone(),
                    },
                })
            })
            .collect())
    }

    /// Attempts to compile one IR function with runtime helper addresses.
    pub fn compile_function_with_runtime_helpers(
        &mut self,
        unit: &IrUnit,
        function: FunctionId,
        mut request: JitCompileRequest,
        runtime_helpers: JitRuntimeHelperAddresses,
    ) -> Result<JitCompileResult, JitError> {
        if request.ir_fingerprint.is_none() {
            request.ir_fingerprint = Some(stable_function_ir_fingerprint(unit, function));
        }
        if request.deployment_identity.is_none() {
            request.deployment_identity = Some(stable_ir_fingerprint(unit));
        }
        if request.dependency_identity.is_none() {
            request.dependency_identity = Some(stable_dependency_identity(unit));
        }
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
        mut request: JitCompileRequest,
        backend: &mut B,
    ) -> Result<JitCompileResult, JitError> {
        if request.ir_fingerprint.is_none() {
            request.ir_fingerprint = Some(stable_function_ir_fingerprint(unit, function));
        }
        if request.deployment_identity.is_none() {
            request.deployment_identity = Some(stable_ir_fingerprint(unit));
        }
        if request.dependency_identity.is_none() {
            request.dependency_identity = Some(stable_dependency_identity(unit));
        }
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

/// Stable full-IR fingerprint used by process and restart-persistent caches.
#[must_use]
pub fn stable_ir_fingerprint(unit: &IrUnit) -> String {
    format!(
        "php-ir-v{}-{:016x}",
        unit.version,
        stable_format_hash(format_args!("{unit:?}"))
    )
}

struct FunctionDeclarations<'unit>(&'unit [php_ir::IrFunction]);

impl fmt::Debug for FunctionDeclarations<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_list()
            .entries(self.0.iter().map(|function| {
                (
                    &function.name,
                    &function.params,
                    function.flags,
                    &function.return_type,
                    function.returns_by_ref,
                    &function.captures,
                    &function.attributes,
                )
            }))
            .finish()
    }
}

/// Stable identity for one requested PHP function and its semantic inputs.
/// Unrelated function bodies are deliberately excluded so a function-on-demand
/// artifact survives edits elsewhere in the source unit. Declaration shapes,
/// class metadata, and the declaring file's strict-types mode remain included
/// because they can change call lowering without changing the requested body.
#[must_use]
pub fn stable_function_ir_fingerprint(unit: &IrUnit, function: FunctionId) -> String {
    let Some(body) = unit.functions.get(function.index()) else {
        return format!("php-function-ir-v2-missing-{}", function.raw());
    };
    stable_function_ir_fingerprint_with_context(
        unit,
        function,
        body,
        stable_function_ir_fingerprint_context(unit),
    )
}

/// Shared semantic identity used by function-scoped IR fingerprints.
///
/// Preparing this once lets function-on-demand callers hash only the requested
/// body instead of formatting every dormant body in the source unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StableFunctionIrFingerprintContext(u64);

/// Hashes the unit-wide semantic tables shared by function-scoped identities.
#[must_use]
pub fn stable_function_ir_fingerprint_context(unit: &IrUnit) -> StableFunctionIrFingerprintContext {
    StableFunctionIrFingerprintContext(stable_function_semantic_context_hash(unit))
}

/// Computes one function identity with an already prepared semantic context.
#[must_use]
pub fn stable_function_ir_fingerprint_in_context(
    unit: &IrUnit,
    function: FunctionId,
    context: StableFunctionIrFingerprintContext,
) -> String {
    let Some(body) = unit.functions.get(function.index()) else {
        return format!("php-function-ir-v2-missing-{}", function.raw());
    };
    stable_function_ir_fingerprint_with_context(unit, function, body, context)
}

/// Computes every function-scoped cache identity while hashing shared semantic
/// tables only once. This is the production path for a prepared source unit.
#[must_use]
pub fn stable_function_ir_fingerprints(unit: &IrUnit) -> Vec<String> {
    let semantic_context = stable_function_ir_fingerprint_context(unit);
    unit.functions
        .iter()
        .enumerate()
        .map(|(index, body)| {
            let function = FunctionId::new(u32::try_from(index).unwrap_or(u32::MAX));
            stable_function_ir_fingerprint_with_context(unit, function, body, semantic_context)
        })
        .collect()
}

fn stable_function_semantic_context_hash(unit: &IrUnit) -> u64 {
    // Constant IDs are unit-relative and are embedded in the function body.
    // Include the immutable table until php_ir exposes a canonical reachable-
    // constant walk. Calls may be specialized from declaration and class
    // metadata, so hash those tables while projecting every foreign function
    // down to its body-free declaration shape.
    let declarations = FunctionDeclarations(&unit.functions);
    stable_format_hash(format_args!(
        "constants={:?};declarations={declarations:?};function_table={:?};\
         constant_table={:?};classes={:?}",
        unit.constants, unit.function_table, unit.constant_table, unit.classes,
    ))
}

fn stable_function_ir_fingerprint_with_context(
    unit: &IrUnit,
    function: FunctionId,
    body: &php_ir::IrFunction,
    semantic_context: StableFunctionIrFingerprintContext,
) -> String {
    let semantic_context = semantic_context.0;
    let identity = stable_format_hash(format_args!(
        "function={body:?};semantic_context={semantic_context:016x};strict_types={}",
        unit.strict_types_for_function(function),
    ));
    format!("php-function-ir-v2-{identity:016x}")
}

/// Stable dependency-graph fingerprint used by native cache identities.
#[must_use]
pub fn stable_dependency_identity(unit: &IrUnit) -> String {
    let dependencies = stable_format_hash(format_args!(
        "{:?}:{:?}:{:?}",
        unit.files, unit.linked_file_entries, unit.linked_entry_autoload_declarations
    ));
    format!("php-dependencies-v1-{dependencies:016x}")
}

struct StableFormatHasher(u64);

impl StableFormatHasher {
    const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325_u64)
    }
}

impl fmt::Write for StableFormatHasher {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes() {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Ok(())
    }
}

fn stable_format_hash(arguments: fmt::Arguments<'_>) -> u64 {
    let mut hasher = StableFormatHasher::new();
    fmt::Write::write_fmt(&mut hasher, arguments).expect("stable hash formatter cannot fail");
    hasher.0
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
        JitAbiValue, JitBailout, JitBailoutKind, JitExceptionMarker, JitFrameHandle, JitFrameView,
        JitOpaqueHandle, JitOpaqueValueKind, JitRegionResult, JitRuntimeCallout,
        JitRuntimeCalloutResult, JitVmContextHandle,
    };
    use php_ir::{
        FunctionFlags, FunctionId, InstructionKind, IrBuilder, IrConstant, IrReturnType, IrSpan,
        LocalId, Operand, RegId, UnitId,
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
    fn function_fingerprint_excludes_unrelated_function_bodies() {
        fn unit_with_foreign_result(foreign_constant: usize) -> php_ir::IrUnit {
            let mut builder = IrBuilder::new(UnitId::new(45));
            let file = builder.add_file("function-cache-identity.php");
            let span = IrSpan::new(file, 0, 8);
            let constants = [
                builder.intern_constant(IrConstant::Int(1)),
                builder.intern_constant(IrConstant::Int(2)),
            ];

            for (name, constant) in [
                ("requested", constants[0]),
                ("unrelated", constants[foreign_constant]),
            ] {
                let function = builder.start_function(name, FunctionFlags::default(), span);
                builder.set_return_type(function, Some(IrReturnType::Int));
                let block = builder.append_block(function);
                let result = builder.alloc_register(function);
                builder.emit(
                    function,
                    block,
                    InstructionKind::LoadConst {
                        dst: result,
                        constant,
                    },
                    span,
                );
                builder.terminate_return(function, block, Some(Operand::Register(result)), span);
            }
            builder.set_entry(FunctionId::new(0));
            builder.finish()
        }

        let first = unit_with_foreign_result(0);
        let changed = unit_with_foreign_result(1);
        let batched = crate::stable_function_ir_fingerprints(&first);
        let context = crate::stable_function_ir_fingerprint_context(&first);

        assert_eq!(batched.len(), first.functions.len());
        assert_eq!(
            batched[0],
            crate::stable_function_ir_fingerprint(&first, FunctionId::new(0))
        );
        assert_eq!(
            batched[1],
            crate::stable_function_ir_fingerprint(&first, FunctionId::new(1))
        );
        assert_eq!(
            batched[0],
            crate::stable_function_ir_fingerprint_in_context(&first, FunctionId::new(0), context,)
        );

        assert_ne!(
            crate::stable_ir_fingerprint(&first),
            crate::stable_ir_fingerprint(&changed)
        );
        assert_eq!(
            crate::stable_function_ir_fingerprint(&first, FunctionId::new(0)),
            crate::stable_function_ir_fingerprint(&changed, FunctionId::new(0))
        );
        assert_ne!(
            crate::stable_function_ir_fingerprint(&first, FunctionId::new(1)),
            crate::stable_function_ir_fingerprint(&changed, FunctionId::new(1))
        );

        let mut strict = first.clone();
        strict.strict_types = true;
        strict.file_strict_types[0] = true;
        assert_ne!(
            crate::stable_function_ir_fingerprint(&first, FunctionId::new(0)),
            crate::stable_function_ir_fingerprint(&strict, FunctionId::new(0))
        );
    }

    #[test]
    fn compile_function_does_not_publish_same_unit_callee_body() {
        extern "C" fn trampoline(
            _runtime: *mut std::ffi::c_void,
            _vm_context: u64,
            _frame: *mut crate::JitNativeCallFrame,
            out: *mut crate::JitCallResult,
        ) -> i32 {
            // SAFETY: The generated call owns the synchronous result record.
            unsafe {
                out.write(crate::JitCallResult {
                    status: crate::JitCallStatus::RETURN,
                    detail: 0,
                    value: crate::JitAbiSlot {
                        tag: 3,
                        flags: 0,
                        payload: 4,
                    },
                });
            }
            crate::JitCallStatus::RETURN.0 as i32
        }

        let mut builder = IrBuilder::new(UnitId::new(44));
        let file = builder.add_file("unit.php");
        let span = IrSpan::new(file, 0, 8);
        let constant = builder.intern_constant(IrConstant::Int(4));
        let entry = builder.start_function("entry", FunctionFlags::default(), span);
        builder.set_return_type(entry, Some(IrReturnType::Int));
        let entry_block = builder.append_block(entry);
        let result = builder.alloc_register(entry);
        builder.emit(
            entry,
            entry_block,
            InstructionKind::CallFunction {
                dst: result,
                name: "declared_later".to_owned(),
                args: Vec::new(),
            },
            span,
        );
        builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);

        let declared = builder.start_function("declared_later", FunctionFlags::default(), span);
        builder.set_return_type(declared, Some(IrReturnType::Int));
        // Give the callee a distinct local layout so the alias must select the
        // function-entry metadata rather than inheriting the caller's layout.
        let local = builder.intern_local(declared, "value");
        let declared_block = builder.append_block(declared);
        let value = builder.alloc_register(declared);
        builder.emit(
            declared,
            declared_block,
            InstructionKind::LoadConst {
                dst: value,
                constant,
            },
            span,
        );
        builder.emit(
            declared,
            declared_block,
            InstructionKind::StoreLocal {
                local,
                src: Operand::Register(value),
            },
            span,
        );
        builder.terminate_return(
            declared,
            declared_block,
            Some(Operand::Register(value)),
            span,
        );
        builder.register_function_name("declared_later", declared);
        builder.set_entry(entry);
        let unit = builder.finish();
        let mut failed_engine = JitEngine::new();
        let failed = failed_engine
            .compile_function(
                &unit,
                entry,
                JitCompileRequest::new("function-on-demand-failure"),
            )
            .expect("rejected compile result");
        assert!(matches!(
            failed.status,
            crate::JitCompileStatus::Rejected { .. }
        ));
        let deployment = crate::stable_ir_fingerprint(&unit);
        let entry_function = &unit.functions[entry.index()];
        let entry_key = crate::native_function_key(
            deployment,
            entry.raw(),
            entry_function.params.len(),
            entry_function.local_count,
            false,
            0,
        );
        let manager = crate::global_code_manager().expect("code manager");
        let cell = manager
            .function_cell(&entry_key)
            .expect("declared entry cell");
        assert_eq!(cell.state(), crate::NativeIndirectionState::Declared);
        assert!(manager.published_function(&entry_key).is_none());

        let mut engine = JitEngine::new();
        let result = engine
            .compile_function_with_runtime_helpers(
                &unit,
                entry,
                JitCompileRequest::new("function-on-demand"),
                crate::JitRuntimeHelperAddresses {
                    native_call_dispatch: trampoline as *const () as usize,
                    ..crate::JitRuntimeHelperAddresses::default()
                },
            )
            .expect("entry compile result");

        assert!(matches!(result.status, crate::JitCompileStatus::Compiled));
        assert_eq!(engine.stats().compile_requests, 1);
        let handle = result.handle.as_ref().expect("entry handle");
        assert_eq!(cell.state(), crate::NativeIndirectionState::Published);
        let metadata = handle.region_state_metadata().expect("entry metadata");
        assert_eq!(metadata.function_entries.len(), 1);
        assert_eq!(metadata.function_entries[0].function, entry);
        let artifact_functions = &handle.relocatable_code().unwrap().functions;
        assert_eq!(
            artifact_functions
                .iter()
                .filter(|function| function.function.index() < unit.functions.len())
                .count(),
            1,
            "internal native fragments must not be counted as foreign PHP function bodies"
        );
        assert!(
            artifact_functions
                .iter()
                .filter(|function| function.function.index() < unit.functions.len())
                .all(|function| function.function == entry)
        );
        assert_eq!(
            handle.invoke_i64(&[], crate::JIT_RUNTIME_ABI_HASH).unwrap(),
            4
        );
    }

    #[test]
    fn offline_compile_unit_keeps_every_compile_group_function_scoped() {
        let mut builder = IrBuilder::new(UnitId::new(45));
        let file = builder.add_file("large-unit.php");
        let span = IrSpan::new(file, 0, 8);
        let constant = builder.intern_constant(IrConstant::Int(7));
        for index in 0..24 {
            let function =
                builder.start_function(format!("function_{index}"), FunctionFlags::default(), span);
            builder.set_return_type(function, Some(IrReturnType::Int));
            let block = builder.append_block(function);
            let result = builder.alloc_register(function);
            builder.emit(
                function,
                block,
                InstructionKind::LoadConst {
                    dst: result,
                    constant,
                },
                span,
            );
            builder.terminate_return(function, block, Some(Operand::Register(result)), span);
            builder.register_function_name(format!("function_{index}"), function);
            if index == 0 {
                builder.set_entry(function);
            }
        }
        let unit = builder.finish();
        let mut engine = JitEngine::new();
        let records = engine
            .compile_unit(&unit, JitCompileRequest::new("large-whole-unit"))
            .expect("unit compile records");

        assert_eq!(records.len(), 24);
        assert_eq!(engine.stats().compile_requests, 24);
        assert!(records.iter().all(|record| record.result.handle.is_some()));
        assert!(records.iter().all(|record| {
            let handle = record.result.handle.as_ref().unwrap();
            handle.code_bytes() > 0
                && handle.region_state_metadata().is_some_and(|metadata| {
                    metadata.function_entries.len() == 1
                        && metadata.function_entries[0].function == record.function
                })
        }));
    }

    #[test]
    fn single_function_compile_identity_separates_units_with_matching_ids() {
        fn constant_unit(value: i64) -> php_ir::IrUnit {
            let mut builder = IrBuilder::new(UnitId::new(0));
            let file = builder.add_file("same-path.php");
            let span = IrSpan::new(file, 0, 8);
            let function = builder.start_function("main", FunctionFlags::default(), span);
            builder.set_return_type(function, Some(IrReturnType::Int));
            let block = builder.append_block(function);
            let constant = builder.intern_constant(IrConstant::Int(value));
            let result = builder.alloc_register(function);
            builder.emit(
                function,
                block,
                InstructionKind::LoadConst {
                    dst: result,
                    constant,
                },
                span,
            );
            builder.terminate_return(function, block, Some(Operand::Register(result)), span);
            builder.set_entry(function);
            builder.finish()
        }

        let first_unit = constant_unit(41);
        let second_unit = constant_unit(42);
        let mut engine = JitEngine::new();
        let first = engine
            .compile_function_with_runtime_helpers(
                &first_unit,
                first_unit.entry,
                JitCompileRequest::new("same-region"),
                crate::JitRuntimeHelperAddresses::default(),
            )
            .unwrap()
            .handle
            .unwrap();
        let second = engine
            .compile_function_with_runtime_helpers(
                &second_unit,
                second_unit.entry,
                JitCompileRequest::new("same-region"),
                crate::JitRuntimeHelperAddresses::default(),
            )
            .unwrap()
            .handle
            .unwrap();

        assert_eq!(
            first.invoke_i64(&[], crate::JIT_RUNTIME_ABI_HASH).unwrap(),
            41
        );
        assert_eq!(
            second.invoke_i64(&[], crate::JIT_RUNTIME_ABI_HASH).unwrap(),
            42
        );
        assert_ne!(
            first
                .code_lifetime
                .as_ref()
                .unwrap()
                .metadata()
                .key
                .compiled_unit,
            second
                .code_lifetime
                .as_ref()
                .unwrap()
                .metadata()
                .key
                .compiled_unit
        );
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
            .with_native_continuation(1, 2);
        assert_eq!(bailout.kind.as_str(), "guard_failed");
        assert_eq!(bailout.continuation_id, Some(1));
        assert_eq!(bailout.source_position, Some(2));

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
}
