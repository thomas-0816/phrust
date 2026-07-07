//! Safe VM/JIT ABI boundary types.
//!
//! These types are intentionally handle-based. They do not expose raw pointers,
//! Rust references, frame internals, GC cells, refcount state, or COW storage to
//! future native code.

use std::num::NonZeroU64;

use php_ir::{BlockId, FunctionId, InstrId, LocalId, RegId};

/// Version for the C-compatible runtime ABI records.
pub const JIT_RUNTIME_ABI_VERSION: u32 = 1;

/// Stable ABI fingerprint for Cranelift ABI.
///
/// This is updated only when a `repr(C)` boundary type changes layout or tag
/// meaning. It is intentionally independent from Rust type names.
pub const JIT_RUNTIME_ABI_HASH: u64 = 0x07c1_a817_0000_0001;

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
        JitCValue, JitCValueTag, JitFrameHandle, JitFrameView, JitOpaqueHandle, JitOpaqueValueKind,
        JitSideExit, JitVmContextHandle, SideExitReason,
    };

    #[test]
    fn c_abi_layout_is_stable() {
        assert_eq!(JIT_RUNTIME_ABI_VERSION, 1);
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
