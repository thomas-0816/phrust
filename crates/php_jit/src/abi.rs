//! Safe VM/JIT ABI boundary types.
//!
//! These types are intentionally handle-based. They do not expose raw pointers,
//! Rust references, frame internals, GC cells, refcount state, or COW storage to
//! future native code.

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use php_ir::{BlockId, FunctionId, InstrId, LocalId, RegId};

/// Version for the C-compatible runtime ABI records.
pub const JIT_RUNTIME_ABI_VERSION: u32 = 6;

/// Stable ABI fingerprint for Cranelift ABI.
///
/// This is updated only when a `repr(C)` boundary type changes layout or tag
/// meaning. It is intentionally independent from Rust type names.
pub const JIT_RUNTIME_ABI_HASH: u64 = 0x07c1_a817_0000_0006;

/// Maximum number of scalar VM locals materialized by one native side exit.
pub const JIT_DEOPT_MAX_SLOTS: usize = 64;

/// Caller-owned state buffer populated before a native side exit returns.
#[repr(C)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitDeoptState {
    /// Stable IR function ID that owns `continuation_id`.
    pub function_id: u32,
    /// Stable continuation ID in the compiled region metadata.
    pub continuation_id: u32,
    /// Number of addressable local slots in the compiled region.
    pub slot_count: u32,
    /// Reserved for append-only ABI growth; native writers must store zero.
    pub reserved: u32,
    /// Bit `n` is set when `slots[n]` contains a materialized value.
    pub initialized_mask: u64,
    /// Materialized scalar locals indexed by their VM local ID.
    pub slots: [i64; JIT_DEOPT_MAX_SLOTS],
    /// Explicit PHP control resumed at a catch/finally native entry.
    pub control_status: JitCallStatus,
    pub control_reserved: u32,
    pub control_value: i64,
}

impl Default for JitDeoptState {
    fn default() -> Self {
        Self {
            function_id: u32::MAX,
            continuation_id: u32::MAX,
            slot_count: 0,
            reserved: 0,
            initialized_mask: 0,
            slots: [0; JIT_DEOPT_MAX_SLOTS],
            control_status: JitCallStatus::CONTINUE,
            control_reserved: 0,
            control_value: 0,
        }
    }
}

/// Stable status returned by native calls and runtime helpers.
///
/// Native code must compare the numeric constants below. It must never depend
/// on a Rust enum discriminant.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct JitCallStatus(pub u32);

impl Default for JitCallStatus {
    fn default() -> Self {
        Self::CONTINUE
    }
}

impl JitCallStatus {
    pub const CONTINUE: Self = Self(0);
    pub const RETURN: Self = Self(1);
    pub const RETURN_REFERENCE: Self = Self(2);
    pub const THROW: Self = Self(3);
    pub const EXIT: Self = Self(4);
    pub const SUSPEND_GENERATOR: Self = Self(5);
    pub const SUSPEND_FIBER: Self = Self(6);
    pub const RUNTIME_ERROR: Self = Self(7);
    pub const COMPILE_REQUIRED: Self = Self(8);
    pub const RECOMPILE_REQUESTED: Self = Self(9);
    /// Boundary validation failed before generated code was entered.
    pub const ABI_MISMATCH: Self = Self(10);

    /// Compatibility spellings retained for callers while all native entry
    /// points migrate to the explicit control-status vocabulary.
    pub const YIELD: Self = Self::SUSPEND_GENERATOR;
    pub const FIBER_SUSPEND: Self = Self::SUSPEND_FIBER;
    pub const DEOPT: Self = Self::RECOMPILE_REQUESTED;

    #[must_use]
    pub const fn is_terminal_return(self) -> bool {
        self.0 == Self::RETURN.0 || self.0 == Self::RETURN_REFERENCE.0
    }
}

/// Stable tagged value passed across the generic helper boundary.
///
/// `payload` is either an immediate bit pattern or an opaque VM-owned handle,
/// as selected by `tag`. No Rust `Value` layout crosses the boundary.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitAbiSlot {
    pub tag: u32,
    pub flags: u32,
    pub payload: u64,
}

/// Compact result record shared by native entries and helper dispatch.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JitCallResult {
    pub status: JitCallStatus,
    pub detail: u32,
    pub value: JitAbiSlot,
}

impl Default for JitCallResult {
    fn default() -> Self {
        Self {
            status: JitCallStatus::RETURN,
            detail: 0,
            value: JitAbiSlot::default(),
        }
    }
}

/// ABI-visible reason why native PHP control left the current frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeControlRecord {
    pub status: JitCallStatus,
    /// Source continuation at which the status originated.
    pub continuation_id: u32,
    /// Return value, reference cell, throwable, exit code, or suspend value.
    pub value: JitAbiSlot,
    /// Opaque VM-owned throwable handle when `status` is `THROW`.
    pub exception_handle: u64,
    /// Resume target selected by explicit native unwind, or `u32::MAX`.
    pub resume_block: u32,
    pub handler_depth: u32,
}

/// One exception region published by a native PHP frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeExceptionHandler {
    pub enter_continuation: u32,
    pub catch_block: u32,
    pub finally_block: u32,
    pub after_block: u32,
    pub exception_local: u32,
    pub catch_type_start: u32,
    pub catch_type_count: u32,
}

/// GC root representation for a live value at a native safepoint.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct JitNativeRootKind(pub u32);

impl JitNativeRootKind {
    /// Baseline frame slot in the frame's published tagged-slot table.
    pub const FRAME_SLOT: Self = Self(1);
    /// Optimized-frame root in a compiler stack map.
    pub const STACK_MAP: Self = Self(2);
    /// Optimized-frame root mirrored into a shadow slot.
    pub const SHADOW_SLOT: Self = Self(3);
}

/// One heap handle visible to GC at a native safepoint.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeRootEntry {
    pub kind: JitNativeRootKind,
    pub slot: u32,
    pub stack_offset: i32,
    pub value_tag: u32,
}

/// PHP-visible points at which native code may release the last object root
/// and must invoke `__destruct` through a native call entry.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct JitNativeDestructorPoint(pub u32);

impl JitNativeDestructorPoint {
    pub const LOCAL_OVERWRITE: Self = Self(1);
    pub const DISCARD: Self = Self(2);
    pub const FRAME_RETURN: Self = Self(3);
    pub const EXCEPTION_UNWIND: Self = Self(4);
    pub const REQUEST_SHUTDOWN: Self = Self(5);
}

/// Precise source/backtrace record associated with a generated PC range.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativePcMetadata {
    pub function_id: u32,
    pub continuation_id: u32,
    pub native_start: u32,
    pub native_end: u32,
    pub file_id: u32,
    pub source_start: u32,
    pub source_end: u32,
    pub handler_depth: u32,
    pub root_map_start: u32,
    pub root_map_count: u32,
}

/// Published native frame header. Generated code and runtime helpers exchange
/// only pointers/counts; Rust containers never cross this boundary.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeFrameHeader {
    pub abi_version: u32,
    pub struct_size: u32,
    pub function_id: u32,
    pub generation: u32,
    pub caller_frame: u64,
    pub slots: u64,
    pub slot_count: u32,
    pub active_handler_depth: u32,
    pub handlers: u64,
    pub handler_count: u32,
    pub roots: u64,
    pub root_count: u32,
    pub pc_metadata: u64,
    pub pc_metadata_count: u32,
    pub flags: u32,
}

/// Stable native-call target family. Numeric values are ABI-visible.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct JitNativeCallKind(pub u32);

impl JitNativeCallKind {
    pub const FUNCTION: Self = Self(1);
    pub const METHOD: Self = Self(2);
    pub const STATIC_METHOD: Self = Self(3);
    pub const CLOSURE: Self = Self(4);
    pub const CALLABLE: Self = Self(5);
    pub const PIPE: Self = Self(6);
    pub const CONSTRUCTOR: Self = Self(7);
    pub const DYNAMIC_CONSTRUCTOR: Self = Self(8);
    pub const MAGIC_METHOD: Self = Self(9);
    pub const PROPERTY_HOOK: Self = Self(10);
    pub const AUTOLOAD_CALLBACK: Self = Self(11);
    pub const ERROR_HANDLER: Self = Self(12);
    pub const SHUTDOWN_FUNCTION: Self = Self(13);
    pub const DESTRUCTOR: Self = Self(14);
    pub const BUILTIN_CALLBACK: Self = Self(15);
}

/// ABI-visible flags for one prepared native argument.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct JitNativeArgFlags(pub u32);

impl JitNativeArgFlags {
    pub const NAMED: Self = Self(1 << 0);
    pub const UNPACK: Self = Self(1 << 1);
    pub const BY_REFERENCE: Self = Self(1 << 2);
    pub const INDIRECT_TEMPORARY: Self = Self(1 << 3);
    pub const BY_REF_RETURN_DESTINATION: Self = Self(1 << 4);

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// One argument slot written directly into a native callee-frame buffer.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeCallArgument {
    pub value: JitAbiSlot,
    /// Stable symbol hash for a named argument, zero for positional arguments.
    pub name_hash: u64,
    pub flags: JitNativeArgFlags,
    /// Caller local/lvalue index for by-reference binding, or `u32::MAX`.
    pub source_slot: u32,
}

/// Stable target descriptor resolved through generation-safe indirection.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeCallTarget {
    pub kind: JitNativeCallKind,
    /// Known `FunctionId`, or `u32::MAX` for dynamic resolution.
    pub function_id: u32,
    /// Stable deployment generation expected by the caller.
    pub generation: u64,
    /// Function/method/callable symbol hash; no persisted absolute address.
    pub symbol_hash: u64,
    /// Class/receiver-context symbol hash when applicable.
    pub class_hash: u64,
}

/// One ABI-stable PHP native frame shared by direct and dynamic calls.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JitNativeCallFrame {
    pub abi_version: u32,
    pub struct_size: u32,
    pub function_id: u32,
    pub region_id: u32,
    pub continuation_id: u32,
    pub result_slot: u32,
    pub local_count: u32,
    pub temporary_count: u32,
    pub argument_count: u32,
    pub flags: u32,
    /// Caller-owned `JitAbiSlot` table.
    pub local_slots: u64,
    /// Caller-owned `JitAbiSlot` table.
    pub temporary_slots: u64,
    /// Caller-owned `JitNativeCallArgument` table.
    pub arguments: u64,
    pub caller_frame: u64,
    pub receiver_handle: u64,
    pub class_context: u64,
    pub exception_metadata: u64,
    pub trace_metadata: u64,
    pub generator_handle: u64,
    pub fiber_handle: u64,
    pub target: JitNativeCallTarget,
}

impl Default for JitNativeCallFrame {
    fn default() -> Self {
        Self {
            abi_version: JIT_RUNTIME_ABI_VERSION,
            struct_size: std::mem::size_of::<Self>() as u32,
            function_id: u32::MAX,
            region_id: u32::MAX,
            continuation_id: u32::MAX,
            result_slot: u32::MAX,
            local_count: 0,
            temporary_count: 0,
            argument_count: 0,
            flags: 0,
            local_slots: 0,
            temporary_slots: 0,
            arguments: 0,
            caller_frame: 0,
            receiver_handle: 0,
            class_context: 0,
            exception_metadata: 0,
            trace_metadata: 0,
            generator_handle: 0,
            fiber_handle: 0,
            target: JitNativeCallTarget::default(),
        }
    }
}

/// Dynamic call resolver/invoker. It may compile and retry a native entry, but
/// it must never invoke a bytecode or IR interpreter.
pub type JitNativeDispatchTrampoline = unsafe extern "C" fn(
    vm_context: u64,
    frame: *mut JitNativeCallFrame,
    out: *mut JitCallResult,
) -> i32;

/// Generation-safe process-local indirection entry. Persisted code stores only
/// `function_id` and generation; absolute addresses remain in this live table.
#[derive(Debug)]
pub struct JitNativeIndirectionEntry {
    function_id: u32,
    generation: AtomicU64,
    address: AtomicUsize,
}

impl JitNativeIndirectionEntry {
    #[must_use]
    pub const fn new(function_id: u32) -> Self {
        Self {
            function_id,
            generation: AtomicU64::new(0),
            address: AtomicUsize::new(0),
        }
    }

    #[must_use]
    pub const fn function_id(&self) -> u32 {
        self.function_id
    }

    /// Publishes a new generation after the native entry address is visible.
    pub fn publish(&self, generation: u64, address: usize) {
        self.address.store(address, Ordering::Release);
        self.generation.store(generation, Ordering::Release);
    }

    /// Resolves only the exact generation expected by compiled code.
    #[must_use]
    pub fn resolve(&self, expected_generation: u64) -> Option<usize> {
        (self.generation.load(Ordering::Acquire) == expected_generation)
            .then(|| self.address.load(Ordering::Acquire))
            .filter(|address| *address != 0)
    }
}

/// Stable helper IDs. Append-only within an ABI version.
pub mod helper_id {
    pub const ARRAY_LEN: u32 = 1;
    pub const ARRAY_FETCH_INT: u32 = 2;
    pub const STRLEN: u32 = 3;
    pub const COUNT: u32 = 4;
    pub const CONCAT: u32 = 5;
    pub const RECORD_LOOKUP: u32 = 6;
    pub const PROPERTY_LOAD: u32 = 7;
    pub const BUILTIN_DISPATCH: u32 = 8;
}

/// One versioned helper entry point. The dispatcher validates `helper_id`,
/// argument count, opaque handles and the ABI version before touching VM state.
pub type JitHelperDispatch = unsafe extern "C" fn(
    vm_context: u64,
    helper_id: u32,
    args: *const JitAbiSlot,
    arg_count: u32,
    out: *mut JitCallResult,
) -> i32;

/// Published runtime helper contract. `struct_size` permits append-only growth
/// without generated code reading beyond the table supplied by an older VM.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct JitRuntimeHelperTable {
    pub abi_version: u32,
    pub struct_size: u32,
    pub abi_hash: u64,
    pub dispatch: Option<JitHelperDispatch>,
}

impl JitRuntimeHelperTable {
    #[must_use]
    pub const fn new(dispatch: JitHelperDispatch) -> Self {
        Self {
            abi_version: JIT_RUNTIME_ABI_VERSION,
            struct_size: std::mem::size_of::<Self>() as u32,
            abi_hash: JIT_RUNTIME_ABI_HASH,
            dispatch: Some(dispatch),
        }
    }
}

/// Default typed-deopt dispatcher used until a VM publishes service-specific
/// opaque-slot implementations through the same table contract.
pub unsafe extern "C" fn jit_default_helper_dispatch(
    _vm_context: u64,
    helper_id: u32,
    args: *const JitAbiSlot,
    arg_count: u32,
    out: *mut JitCallResult,
) -> i32 {
    if out.is_null() || (arg_count != 0 && args.is_null()) {
        return crate::JIT_HELPER_STATUS_FALLBACK;
    }
    let known = matches!(
        helper_id,
        helper_id::ARRAY_LEN
            | helper_id::ARRAY_FETCH_INT
            | helper_id::STRLEN
            | helper_id::COUNT
            | helper_id::CONCAT
            | helper_id::RECORD_LOOKUP
            | helper_id::PROPERTY_LOAD
            | helper_id::BUILTIN_DISPATCH
    );
    let result = JitCallResult {
        status: if known {
            JitCallStatus::DEOPT
        } else {
            JitCallStatus::ABI_MISMATCH
        },
        detail: helper_id,
        value: JitAbiSlot::default(),
    };
    // SAFETY: `out` was checked non-null and the ABI requires one writable
    // result record for this synchronous call.
    unsafe { out.write(result) };
    if known {
        crate::JIT_HELPER_STATUS_FALLBACK
    } else {
        -1
    }
}

/// Opaque non-zero handle owned by the VM side of the ABI.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct JitOpaqueHandle(NonZeroU64);

impl JitOpaqueHandle {
    /// Creates an opaque handle. Zero is reserved for "no handle".
    #[must_use]
    pub fn new(raw: u64) -> Option<Self> {
        NonZeroU64::new(raw).map(Self)
    }

    /// Returns the stable raw value for logging and test snapshots.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0.get()
    }
}

/// Opaque VM request/context handle.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct JitVmContextHandle(JitOpaqueHandle);

impl JitVmContextHandle {
    /// Creates a VM context handle.
    #[must_use]
    pub fn new(raw: u64) -> Option<Self> {
        JitOpaqueHandle::new(raw).map(Self)
    }

    /// Returns the stable raw handle value.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0.raw()
    }
}

/// Opaque frame handle.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct JitFrameHandle(JitOpaqueHandle);

impl JitFrameHandle {
    /// Creates a frame handle.
    #[must_use]
    pub fn new(raw: u64) -> Option<Self> {
        JitOpaqueHandle::new(raw).map(Self)
    }

    /// Returns the stable raw handle value.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0.raw()
    }
}

/// Read-only frame/register metadata exported to future JIT code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitFrameView {
    /// VM context that owns the frame.
    pub context: JitVmContextHandle,
    /// Opaque active-frame handle.
    pub frame: JitFrameHandle,
    /// IR function represented by this frame.
    pub function: FunctionId,
    /// Number of VM registers available to this frame.
    pub register_count: u32,
    /// Number of local slots available to this frame.
    pub local_count: u32,
}

impl JitFrameView {
    /// Creates a frame view from opaque VM-owned handles and arena sizes.
    #[must_use]
    pub const fn new(
        context: JitVmContextHandle,
        frame: JitFrameHandle,
        function: FunctionId,
        register_count: u32,
        local_count: u32,
    ) -> Self {
        Self {
            context,
            frame,
            function,
            register_count,
            local_count,
        }
    }

    /// Returns true when a register can be addressed through this view.
    #[must_use]
    pub const fn contains_register(&self, register: RegId) -> bool {
        register.raw() < self.register_count
    }

    /// Returns true when a local can be addressed through this view.
    #[must_use]
    pub const fn contains_local(&self, local: LocalId) -> bool {
        local.raw() < self.local_count
    }
}

/// Heap-backed PHP value categories crossing the ABI as opaque handles only.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JitOpaqueValueKind {
    /// PHP string storage.
    String,
    /// PHP array storage.
    Array,
    /// PHP object storage.
    Object,
    /// PHP resource storage.
    Resource,
    /// PHP reference cell.
    Reference,
    /// PHP callable/closure value.
    Callable,
    /// PHP generator value.
    Generator,
    /// PHP fiber value.
    Fiber,
}

impl JitOpaqueValueKind {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Array => "array",
            Self::Object => "object",
            Self::Resource => "resource",
            Self::Reference => "reference",
            Self::Callable => "callable",
            Self::Generator => "generator",
            Self::Fiber => "fiber",
        }
    }
}

/// ABI-safe value representation.
#[derive(Clone, Debug, PartialEq)]
pub enum JitAbiValue {
    /// PHP null.
    Null,
    /// PHP bool.
    Bool(bool),
    /// PHP int.
    Int(i64),
    /// PHP float as raw IEEE-754 bits.
    FloatBits(u64),
    /// Uninitialized register/local marker.
    Uninitialized,
    /// VM-owned heap value represented by an opaque handle.
    Opaque {
        /// Heap value family.
        kind: JitOpaqueValueKind,
        /// VM-owned handle.
        handle: JitOpaqueHandle,
    },
}

impl JitAbiValue {
    /// Creates a float value while preserving exact bits.
    #[must_use]
    pub const fn float(value: f64) -> Self {
        Self::FloatBits(value.to_bits())
    }

    /// Returns true for heap-backed values that require VM side handling.
    #[must_use]
    pub const fn is_opaque(&self) -> bool {
        matches!(self, Self::Opaque { .. })
    }
}

/// Why future native code left the compiled region.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JitBailoutKind {
    /// Type/value guard failed.
    GuardFailed,
    /// Encountered a value outside the primitive subset.
    UnsupportedValue,
    /// Runtime callout requested interpreter fallback.
    RuntimeCallout,
    /// Deoptimization requested by invalidation or missing metadata.
    Deopt,
}

impl JitBailoutKind {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GuardFailed => "guard_failed",
            Self::UnsupportedValue => "unsupported_value",
            Self::RuntimeCallout => "runtime_callout",
            Self::Deopt => "deopt",
        }
    }
}

/// Stable reason codes for JIT side exits back to the interpreter.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SideExitReason {
    /// Runtime value type did not match the compiled specialization.
    TypeMismatch = 1,
    /// Checked arithmetic or conversion overflowed.
    Overflow = 2,
    /// Runtime value shape is outside the compiled subset.
    UnsupportedValue = 3,
    /// A generated guard failed.
    GuardFailed = 4,
    /// Runtime helper returned a non-OK status.
    HelperStatus = 5,
    /// PHP exception/error state is pending.
    ExceptionPending = 6,
    /// VM/JIT ABI hash or call boundary did not match.
    AbiMismatch = 7,
}

impl SideExitReason {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TypeMismatch => "type_mismatch",
            Self::Overflow => "overflow",
            Self::UnsupportedValue => "unsupported_value",
            Self::GuardFailed => "guard_failed",
            Self::HelperStatus => "helper_status",
            Self::ExceptionPending => "exception_pending",
            Self::AbiMismatch => "abi_mismatch",
        }
    }

    /// Stable numeric ABI code.
    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }
}

/// Structured side-exit metadata observed before interpreter fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitSideExit {
    /// Stable reason.
    pub reason: SideExitReason,
    /// Optional block to resume in interpreter mode.
    pub resume_block: Option<BlockId>,
    /// Optional instruction to resume in interpreter mode.
    pub resume_instruction: Option<InstrId>,
    /// Optional helper status or guard code.
    pub status_code: Option<i32>,
}

impl JitSideExit {
    /// Creates side-exit metadata without a resume point.
    #[must_use]
    pub const fn new(reason: SideExitReason) -> Self {
        Self {
            reason,
            resume_block: None,
            resume_instruction: None,
            status_code: None,
        }
    }

    /// Adds an interpreter resume point.
    #[must_use]
    pub const fn with_resume(mut self, block: BlockId, instruction: InstrId) -> Self {
        self.resume_block = Some(block);
        self.resume_instruction = Some(instruction);
        self
    }

    /// Adds the raw helper/guard status that caused the exit.
    #[must_use]
    pub const fn with_status(mut self, status_code: i32) -> Self {
        self.status_code = Some(status_code);
        self
    }
}

/// Bailout/deoptimization metadata returned to the VM.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitBailout {
    /// Bailout family.
    pub kind: JitBailoutKind,
    /// Optional block to resume in interpreter mode.
    pub resume_block: Option<BlockId>,
    /// Optional instruction to resume in interpreter mode.
    pub resume_instruction: Option<InstrId>,
    /// Stable debug reason.
    pub reason: String,
}

impl JitBailout {
    /// Creates a bailout result.
    #[must_use]
    pub fn new(kind: JitBailoutKind, reason: impl Into<String>) -> Self {
        Self {
            kind,
            resume_block: None,
            resume_instruction: None,
            reason: reason.into(),
        }
    }

    /// Adds an interpreter resume point.
    #[must_use]
    pub const fn with_resume(mut self, block: BlockId, instruction: InstrId) -> Self {
        self.resume_block = Some(block);
        self.resume_instruction = Some(instruction);
        self
    }
}

/// Exception marker crossing the ABI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitExceptionMarker {
    /// Stable PHP exception/error class name when known.
    pub class_name: Option<String>,
    /// Stable message snapshot when known.
    pub message: Option<String>,
    /// Opaque VM-owned exception object handle when already allocated.
    pub exception: Option<JitOpaqueHandle>,
}

impl JitExceptionMarker {
    /// Creates a marker from a class/message pair without exposing the object.
    #[must_use]
    pub fn named(class_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            class_name: Some(class_name.into()),
            message: Some(message.into()),
            exception: None,
        }
    }

    /// Creates a marker for an existing VM-owned exception object.
    #[must_use]
    pub fn opaque(exception: JitOpaqueHandle) -> Self {
        Self {
            class_name: None,
            message: None,
            exception: Some(exception),
        }
    }
}

/// Runtime callout identity and arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct JitRuntimeCallout {
    /// Stable callout name.
    pub name: String,
    /// ABI values copied or represented by opaque handles.
    pub args: Vec<JitAbiValue>,
    /// True when the VM side may report an exception marker.
    pub can_throw: bool,
}

impl JitRuntimeCallout {
    /// Creates a runtime callout descriptor.
    #[must_use]
    pub fn new(name: impl Into<String>, args: Vec<JitAbiValue>, can_throw: bool) -> Self {
        Self {
            name: name.into(),
            args,
            can_throw,
        }
    }
}

/// Result returned from a VM runtime callout.
#[derive(Clone, Debug, PartialEq)]
pub enum JitRuntimeCalloutResult {
    /// Callout returned a normal ABI value.
    Returned(JitAbiValue),
    /// Callout requested interpreter fallback/deopt.
    Bailout(JitBailout),
    /// Callout propagated a PHP exception/error.
    Exception(JitExceptionMarker),
}

/// Result of a future compiled region.
#[derive(Clone, Debug, PartialEq)]
pub enum JitRegionResult {
    /// Region produced a normal PHP value.
    Returned(JitAbiValue),
    /// Region bailed out to interpreter execution.
    Bailout(JitBailout),
    /// Region propagated an exception marker to the VM.
    Exception(JitExceptionMarker),
}

/// C-compatible value tags used by native entry and helper call boundaries.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JitCValueTag {
    /// Uninitialized register/local marker.
    Uninitialized = 0,
    /// PHP null.
    Null = 1,
    /// PHP bool; payload is 0 or 1.
    Bool = 2,
    /// PHP int; payload is the two's-complement `i64` bit pattern.
    Int = 3,
    /// PHP float; payload is the raw IEEE-754 bits.
    FloatBits = 4,
    /// VM-owned string handle.
    OpaqueString = 16,
    /// VM-owned array handle.
    OpaqueArray = 17,
    /// VM-owned object handle.
    OpaqueObject = 18,
    /// VM-owned resource handle.
    OpaqueResource = 19,
    /// VM-owned reference handle.
    OpaqueReference = 20,
    /// VM-owned callable handle.
    OpaqueCallable = 21,
    /// VM-owned generator handle.
    OpaqueGenerator = 22,
    /// VM-owned fiber handle.
    OpaqueFiber = 23,
}

/// C-compatible ABI value.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JitCValue {
    /// Value tag.
    pub tag: JitCValueTag,
    /// Reserved for alignment and future ABI-compatible extensions.
    pub reserved: u32,
    /// Primary value payload or opaque handle.
    pub payload: u64,
    /// Auxiliary payload. Zero unless documented by a later ABI revision.
    pub aux: u64,
}

impl JitCValue {
    /// Creates an uninitialized marker.
    #[must_use]
    pub const fn uninitialized() -> Self {
        Self {
            tag: JitCValueTag::Uninitialized,
            reserved: 0,
            payload: 0,
            aux: 0,
        }
    }

    /// Creates a null value.
    #[must_use]
    pub const fn null() -> Self {
        Self {
            tag: JitCValueTag::Null,
            reserved: 0,
            payload: 0,
            aux: 0,
        }
    }

    /// Creates a bool value.
    #[must_use]
    pub const fn bool(value: bool) -> Self {
        Self {
            tag: JitCValueTag::Bool,
            reserved: 0,
            payload: value as u64,
            aux: 0,
        }
    }

    /// Creates an int value.
    #[must_use]
    pub const fn int(value: i64) -> Self {
        Self {
            tag: JitCValueTag::Int,
            reserved: 0,
            payload: value as u64,
            aux: 0,
        }
    }

    /// Creates a float value from exact bits.
    #[must_use]
    pub const fn float_bits(bits: u64) -> Self {
        Self {
            tag: JitCValueTag::FloatBits,
            reserved: 0,
            payload: bits,
            aux: 0,
        }
    }

    /// Creates a float value from an `f64` (stored as its IEEE-754 bits).
    #[must_use]
    pub fn float(value: f64) -> Self {
        Self::float_bits(value.to_bits())
    }

    /// Creates an opaque heap value handle.
    #[must_use]
    pub const fn opaque(kind: JitOpaqueValueKind, handle: JitOpaqueHandle) -> Self {
        let tag = match kind {
            JitOpaqueValueKind::String => JitCValueTag::OpaqueString,
            JitOpaqueValueKind::Array => JitCValueTag::OpaqueArray,
            JitOpaqueValueKind::Object => JitCValueTag::OpaqueObject,
            JitOpaqueValueKind::Resource => JitCValueTag::OpaqueResource,
            JitOpaqueValueKind::Reference => JitCValueTag::OpaqueReference,
            JitOpaqueValueKind::Callable => JitCValueTag::OpaqueCallable,
            JitOpaqueValueKind::Generator => JitCValueTag::OpaqueGenerator,
            JitOpaqueValueKind::Fiber => JitCValueTag::OpaqueFiber,
        };
        Self {
            tag,
            reserved: 0,
            payload: handle.raw(),
            aux: 0,
        }
    }
}

/// C-compatible frame metadata view.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JitCFrameView {
    /// VM context handle.
    pub context: u64,
    /// VM frame handle.
    pub frame: u64,
    /// IR function id.
    pub function: u32,
    /// Number of VM registers available to this frame.
    pub register_count: u32,
    /// Number of local slots available to this frame.
    pub local_count: u32,
    /// Reserved for ABI-compatible expansion.
    pub reserved: u32,
}

impl From<JitFrameView> for JitCFrameView {
    fn from(view: JitFrameView) -> Self {
        Self {
            context: view.context.raw(),
            frame: view.frame.raw(),
            function: view.function.raw(),
            register_count: view.register_count,
            local_count: view.local_count,
            reserved: 0,
        }
    }
}

/// C-compatible region exit tags.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JitCExitTag {
    /// Region returned normally.
    Returned = 0,
    /// Region bailed out before completing.
    Bailout = 1,
    /// Region propagated a PHP exception/error marker.
    Exception = 2,
    /// Region requested a runtime helper call.
    RuntimeCallout = 3,
}

/// C-compatible region exit record.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JitCExit {
    /// Exit tag.
    pub tag: JitCExitTag,
    /// Stable reason or helper-symbol id. Zero means no reason id.
    pub reason_code: u32,
    /// Value returned or associated with the exit.
    pub value: JitCValue,
    /// Interpreter resume block; `u32::MAX` means no resume point.
    pub resume_block: u32,
    /// Interpreter resume instruction; `u32::MAX` means no resume point.
    pub resume_instruction: u32,
    /// Reserved for ABI-compatible expansion.
    pub reserved: u32,
}

impl JitCExit {
    /// Creates a normal return exit.
    #[must_use]
    pub const fn returned(value: JitCValue) -> Self {
        Self {
            tag: JitCExitTag::Returned,
            reason_code: 0,
            value,
            resume_block: u32::MAX,
            resume_instruction: u32::MAX,
            reserved: 0,
        }
    }

    /// Creates a bailout exit with a stable reason id.
    #[must_use]
    pub const fn bailout(reason_code: u32, value: JitCValue) -> Self {
        Self {
            tag: JitCExitTag::Bailout,
            reason_code,
            value,
            resume_block: u32::MAX,
            resume_instruction: u32::MAX,
            reserved: 0,
        }
    }

    /// Creates a side-exit bailout record with a stable reason.
    #[must_use]
    pub const fn side_exit(reason: SideExitReason, value: JitCValue) -> Self {
        Self::bailout(reason.code(), value)
    }

    /// Adds an interpreter resume point.
    #[must_use]
    pub const fn with_resume(mut self, block: BlockId, instruction: InstrId) -> Self {
        self.resume_block = block.raw();
        self.resume_instruction = instruction.raw();
        self
    }
}

#[cfg(test)]
mod tests {
    use std::mem::{align_of, size_of};

    use php_ir::{BlockId, FunctionId, InstrId};

    use super::{
        JIT_RUNTIME_ABI_HASH, JIT_RUNTIME_ABI_VERSION, JitCExit, JitCExitTag, JitCFrameView,
        JitCValue, JitCValueTag, JitCallResult, JitCallStatus, JitFrameHandle, JitFrameView,
        JitNativeArgFlags, JitNativeCallArgument, JitNativeCallFrame, JitNativeCallKind,
        JitNativeControlRecord, JitNativeExceptionHandler, JitNativeFrameHeader,
        JitNativeIndirectionEntry, JitNativePcMetadata, JitNativeRootEntry, JitOpaqueHandle,
        JitOpaqueValueKind, JitSideExit, JitVmContextHandle, SideExitReason, helper_id,
        jit_default_helper_dispatch,
    };

    #[test]
    fn default_helper_dispatch_rejects_invalid_buffers_and_deopts_known_ids() {
        // SAFETY: null-buffer cases are the explicit negative ABI contract and
        // the positive case supplies one live caller-owned output record.
        unsafe {
            assert_eq!(
                jit_default_helper_dispatch(
                    0,
                    helper_id::STRLEN,
                    std::ptr::null(),
                    0,
                    std::ptr::null_mut()
                ),
                crate::JIT_HELPER_STATUS_FALLBACK
            );
            let mut out = JitCallResult::default();
            assert_eq!(
                jit_default_helper_dispatch(0, helper_id::STRLEN, std::ptr::null(), 0, &mut out,),
                crate::JIT_HELPER_STATUS_FALLBACK
            );
            assert_eq!(out.status, JitCallStatus::DEOPT);
            assert_eq!(out.detail, helper_id::STRLEN);
        }
    }

    #[test]
    fn c_abi_layout_is_stable() {
        assert_eq!(JIT_RUNTIME_ABI_VERSION, 6);
        assert_ne!(JIT_RUNTIME_ABI_HASH, 0);
        assert_eq!(size_of::<JitOpaqueHandle>(), 8);
        assert_eq!(size_of::<JitCValueTag>(), 4);
        assert_eq!(size_of::<JitCValue>(), 24);
        assert_eq!(align_of::<JitCValue>(), 8);
        assert_eq!(size_of::<JitCFrameView>(), 32);
        assert_eq!(align_of::<JitCFrameView>(), 8);
        assert_eq!(size_of::<JitCExitTag>(), 4);
        assert_eq!(size_of::<JitCExit>(), 48);
        assert_eq!(align_of::<JitCExit>(), 8);
        assert_eq!(align_of::<JitNativeCallArgument>(), 8);
        assert_eq!(align_of::<JitNativeCallFrame>(), 8);
        assert_eq!(align_of::<JitNativeControlRecord>(), 8);
        assert_eq!(align_of::<JitNativeExceptionHandler>(), 4);
        assert_eq!(align_of::<JitNativeFrameHeader>(), 8);
        assert_eq!(align_of::<JitNativePcMetadata>(), 4);
        assert_eq!(align_of::<JitNativeRootEntry>(), 4);
        assert_eq!(
            JitNativeCallFrame::default().struct_size as usize,
            size_of::<JitNativeCallFrame>()
        );
    }

    #[test]
    fn native_control_status_numbers_are_stable() {
        assert_eq!(JitCallStatus::CONTINUE.0, 0);
        assert_eq!(JitCallStatus::RETURN.0, 1);
        assert_eq!(JitCallStatus::RETURN_REFERENCE.0, 2);
        assert_eq!(JitCallStatus::THROW.0, 3);
        assert_eq!(JitCallStatus::EXIT.0, 4);
        assert_eq!(JitCallStatus::SUSPEND_GENERATOR.0, 5);
        assert_eq!(JitCallStatus::SUSPEND_FIBER.0, 6);
        assert_eq!(JitCallStatus::RUNTIME_ERROR.0, 7);
        assert_eq!(JitCallStatus::COMPILE_REQUIRED.0, 8);
        assert_eq!(JitCallStatus::RECOMPILE_REQUESTED.0, 9);
        assert!(JitCallStatus::RETURN.is_terminal_return());
        assert!(JitCallStatus::RETURN_REFERENCE.is_terminal_return());
        assert!(!JitCallStatus::THROW.is_terminal_return());
    }

    #[test]
    fn native_call_frame_and_generation_indirection_are_stable() {
        let mut frame = JitNativeCallFrame::default();
        frame.function_id = 7;
        frame.continuation_id = 11;
        frame.target.kind = JitNativeCallKind::METHOD;
        frame.argument_count = 2;
        assert_eq!(frame.abi_version, JIT_RUNTIME_ABI_VERSION);
        assert_eq!(frame.target.kind, JitNativeCallKind::METHOD);

        let argument = JitNativeCallArgument {
            flags: JitNativeArgFlags::NAMED.union(JitNativeArgFlags::BY_REFERENCE),
            source_slot: 3,
            ..JitNativeCallArgument::default()
        };
        assert_ne!(argument.flags.0 & JitNativeArgFlags::NAMED.0, 0);

        let entry = JitNativeIndirectionEntry::new(7);
        assert_eq!(entry.function_id(), 7);
        assert_eq!(entry.resolve(1), None);
        entry.publish(1, 0x1234);
        assert_eq!(entry.resolve(1), Some(0x1234));
        assert_eq!(entry.resolve(2), None);
        entry.publish(2, 0x5678);
        assert_eq!(entry.resolve(1), None);
        assert_eq!(entry.resolve(2), Some(0x5678));
    }

    #[test]
    fn c_abi_values_encode_scalars_and_opaque_handles() {
        assert_eq!(JitCValue::null().tag, JitCValueTag::Null);
        assert_eq!(JitCValue::bool(true).payload, 1);
        assert_eq!(JitCValue::int(-1).payload, u64::MAX);
        assert_eq!(
            JitCValue::float_bits(1.5f64.to_bits()).payload,
            1.5f64.to_bits()
        );

        let handle = JitOpaqueHandle::new(77).expect("non-zero handle");
        let opaque = JitCValue::opaque(JitOpaqueValueKind::Array, handle);
        assert_eq!(opaque.tag, JitCValueTag::OpaqueArray);
        assert_eq!(opaque.payload, 77);
    }

    #[test]
    fn c_frame_and_exit_records_do_not_expose_rust_references() {
        let context = JitVmContextHandle::new(1).expect("context");
        let frame = JitFrameHandle::new(2).expect("frame");
        let view = JitFrameView::new(context, frame, FunctionId::new(3), 4, 5);
        let c_view = JitCFrameView::from(view);

        assert_eq!(c_view.context, 1);
        assert_eq!(c_view.frame, 2);
        assert_eq!(c_view.function, 3);
        assert_eq!(c_view.register_count, 4);
        assert_eq!(c_view.local_count, 5);
        assert_eq!(c_view.reserved, 0);

        let exit =
            JitCExit::bailout(9, JitCValue::int(42)).with_resume(BlockId::new(7), InstrId::new(8));
        assert_eq!(exit.tag, JitCExitTag::Bailout);
        assert_eq!(exit.reason_code, 9);
        assert_eq!(exit.resume_block, 7);
        assert_eq!(exit.resume_instruction, 8);
    }

    #[test]
    fn side_exit_reasons_have_stable_report_codes_and_resume_metadata() {
        assert_eq!(SideExitReason::TypeMismatch.as_str(), "type_mismatch");
        assert_eq!(SideExitReason::Overflow.as_str(), "overflow");
        assert_eq!(
            SideExitReason::UnsupportedValue.as_str(),
            "unsupported_value"
        );
        assert_eq!(SideExitReason::GuardFailed.as_str(), "guard_failed");
        assert_eq!(SideExitReason::HelperStatus.as_str(), "helper_status");
        assert_eq!(
            SideExitReason::ExceptionPending.as_str(),
            "exception_pending"
        );
        assert_eq!(SideExitReason::AbiMismatch.as_str(), "abi_mismatch");
        assert_eq!(SideExitReason::HelperStatus.code(), 5);

        let metadata = JitSideExit::new(SideExitReason::HelperStatus)
            .with_status(1)
            .with_resume(BlockId::new(2), InstrId::new(3));
        assert_eq!(metadata.reason, SideExitReason::HelperStatus);
        assert_eq!(metadata.status_code, Some(1));
        assert_eq!(metadata.resume_block, Some(BlockId::new(2)));
        assert_eq!(metadata.resume_instruction, Some(InstrId::new(3)));

        let exit = JitCExit::side_exit(SideExitReason::HelperStatus, JitCValue::null());
        assert_eq!(exit.tag, JitCExitTag::Bailout);
        assert_eq!(exit.reason_code, SideExitReason::HelperStatus.code());
    }
}
