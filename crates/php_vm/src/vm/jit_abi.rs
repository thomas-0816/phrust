// Audited native ABI surface; see ADR 0017. The product compiler graph always
// includes this module.
use php_ir::module::{normalize_class_name, normalized_class_name};
use php_runtime::api::PhpString;
use php_runtime::api::Value;
use php_runtime::experimental::WeakObjectHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

mod call_dispatch;
mod call_support;
mod diagnostic_helpers;
mod diagnostics;
mod dynamic_code;
mod dynamic_units;
mod frame_arena;
mod internal_classes;
mod native_builtins;
mod object_support;
mod request_state;
mod root_index;
mod runtime_ops;
mod semantic_dispatch;
mod telemetry;

use dynamic_units::*;
pub(super) use dynamic_units::{jit_native_function_resolve_abi, native_entries_from_records};
use frame_arena::NativeFrameArena;
pub(super) use frame_arena::{jit_native_frame_alloc_abi, jit_native_frame_release_abi};

pub(super) use call_dispatch::{
    jit_native_builtin_dispatch_abi, jit_native_builtin_dispatch_diagnostic_abi,
    jit_native_call_dispatch_abi, jit_native_call_dispatch_diagnostic_abi,
};
use call_support::*;
pub(in crate::vm) use diagnostic_helpers::*;
use diagnostics::*;
pub(super) use dynamic_code::jit_native_dynamic_code_abi;
use internal_classes::*;
use native_builtins::{
    NativeDimensionOperation, emit_native_array_dimension_conversion_diagnostic,
    emit_native_deprecated_call, emit_native_dimension_conversion_diagnostic,
    emit_native_php_diagnostic, emit_native_php_warning, execute_native_builtin,
    execute_prepared_runtime_builtin, native_source_line, native_source_line_for_span,
    native_string,
};
use object_support::*;
use request_state::{
    NativeBacktraceFrame, NativeFunctionNameScope, NativeLastError,
    NativeRegisteredExtensionRequestState,
};
use root_index::{RequestRootIndex, RootMutationReason, values_contain_object};
pub(super) use runtime_ops::{
    jit_native_argument_check_abi, jit_native_array_fetch_abi, jit_native_array_insert_abi,
    jit_native_array_insert_local_abi, jit_native_array_new_abi, jit_native_array_spread_abi,
    jit_native_array_unset_abi, jit_native_binary_abi, jit_native_cast_abi, jit_native_compare_abi,
    jit_native_constant_fetch_abi, jit_native_echo_abi, jit_native_exception_new_abi,
    jit_native_execution_poll_abi, jit_native_foreach_cleanup_abi, jit_native_foreach_init_abi,
    jit_native_foreach_next_abi, jit_native_local_fetch_abi, jit_native_local_store_abi,
    jit_native_object_clone_abi, jit_native_object_clone_with_abi, jit_native_object_new_abi,
    jit_native_property_assign_abi, jit_native_property_fetch_abi, jit_native_reference_bind_abi,
    jit_native_return_check_abi, jit_native_runtime_fatal_abi, jit_native_stable_length_abi,
    jit_native_string_predicate_abi, jit_native_truthy_abi, jit_native_type_predicate_abi,
    jit_native_unary_abi, jit_native_value_release_abi,
};
use semantic_dispatch::*;
pub(super) use semantic_dispatch::{
    jit_native_semantic_dispatch_abi, jit_native_semantic_dispatch_diagnostic_abi,
};
use telemetry::NativeRuntimeTelemetry;

thread_local! {
    static NATIVE_INCLUDE_GLOBALS: RefCell<Option<std::collections::BTreeMap<String, Value>>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_CONSTANTS: RefCell<Option<std::collections::BTreeMap<String, Value>>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_INI: RefCell<Option<php_runtime::api::IniRegistry>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_DEFAULT_TIMEZONE: RefCell<Option<String>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_HTTP_RESPONSE: RefCell<Option<php_runtime::api::RuntimeHttpResponseState>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_FILES: RefCell<Option<std::collections::BTreeSet<std::path::PathBuf>>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_MYSQL: RefCell<Option<std::rc::Rc<RefCell<php_runtime::api::MysqlState>>>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_FILTER_INPUT_ARRAYS: RefCell<Option<Rc<std::collections::BTreeMap<i64, php_runtime::api::PhpArray>>>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_FUNCTION_NAMES: RefCell<Option<Rc<NativeFunctionNameScope>>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_SYMBOLS: RefCell<Option<NativeIncludeSymbols>> = const { RefCell::new(None) };
    static NATIVE_INCLUDE_EXPORTS: RefCell<Option<NativeIncludeExports>> =
        const { RefCell::new(None) };
}

static NATIVE_TEMPNAM_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
type NativeRequestFastState = NativeExecutionContext<'static>;
// Real applications routinely cross dozens of PHP frames (for example,
// WordPress metadata and hook dispatch). Keep a deterministic native-stack
// guard, but leave enough headroom for those non-recursive call chains.
const NATIVE_CALL_DEPTH_LIMIT: usize = 256;
const NATIVE_RUNTIME_ERROR_MARKER: &str = "E_PHP_NATIVE_RUNTIME_ERROR";

#[derive(Default)]
struct NativeIncludeExports {
    functions: Vec<(String, php_ir::FunctionId)>,
    native_entries:
        std::sync::Arc<std::collections::BTreeMap<php_ir::FunctionId, php_jit::JitFunctionHandle>>,
    native_entry_signature_hashes: std::collections::BTreeMap<php_ir::FunctionId, u64>,
    classes: Vec<String>,
    constants: std::collections::BTreeMap<String, Value>,
    autoload_callbacks: Vec<Value>,
    shutdown_callbacks: Vec<NativeShutdownCallback>,
}

#[derive(Clone, Default)]
struct NativeIncludeSymbols {
    deployment_functions:
        std::sync::Arc<std::collections::HashMap<std::sync::Arc<str>, php_ir::FunctionId>>,
    deployment_classes: std::sync::Arc<std::collections::HashSet<std::sync::Arc<str>>>,
    external_functions: std::collections::HashMap<String, NativeDynamicFunction>,
    external_class_units: std::collections::HashMap<String, usize>,
    external_signature_epoch: u64,
    dynamic_units: Vec<NativeDynamicUnit>,
    dynamic_classes: std::collections::BTreeSet<String>,
    class_aliases: std::collections::BTreeMap<String, String>,
    autoload_callbacks: Vec<Value>,
    shutdown_callbacks: Vec<NativeShutdownCallback>,
    static_properties: std::collections::BTreeMap<(String, String), Value>,
    static_locals: std::collections::BTreeMap<(u64, u32, u32), php_runtime::api::ReferenceCell>,
    enum_cases: std::collections::BTreeMap<(String, String), php_runtime::api::ObjectRef>,
    destroyed_objects: std::collections::BTreeMap<u64, WeakObjectHandle>,
    error_reporting: Option<i64>,
    display_errors: Option<bool>,
    error_handlers: Vec<NativeErrorHandler>,
    exception_handlers: Vec<Value>,
    last_error: Option<NativeLastError>,
}

#[derive(Clone)]
struct NativeShutdownCallback {
    callable: Value,
    arguments: Vec<Value>,
    source: php_ir::Instruction,
}

#[derive(Clone)]
struct NativeErrorHandler {
    callback: Value,
    levels: i64,
}

#[derive(Clone, Copy)]
struct NativeDynamicFunction {
    unit: usize,
    function: php_ir::FunctionId,
}

#[derive(Clone, Copy)]
enum NativeMethodPicTarget {
    CurrentUnit {
        function: php_ir::FunctionId,
        is_static: bool,
    },
    DynamicUnit {
        function: NativeDynamicFunction,
        is_static: bool,
    },
}

struct NativeMethodPicEntry {
    receiver_class: std::sync::Arc<str>,
    method: std::sync::Arc<str>,
    class_layout_epoch: u64,
    method_table_epoch: u64,
    target: NativeMethodPicTarget,
}

#[derive(Default)]
struct NativeMethodPic {
    entries: Vec<NativeMethodPicEntry>,
    megamorphic: bool,
}

const NATIVE_METHOD_PIC_LIMIT: usize = 4;
// WordPress routinely reaches several hundred native functions from distinct
const NATIVE_GLOBAL_REFERENCE_CACHE_SIZE: usize = php_jit::JIT_NATIVE_GLOBAL_REFERENCE_CACHE_SIZE;

struct NativeGlobalReferenceCache {
    entries:
        Box<[php_jit::JitNativeGlobalReferenceCacheRecord; NATIVE_GLOBAL_REFERENCE_CACHE_SIZE]>,
    /// Exact source name used only by rare runtime invalidation/reconciliation.
    /// Generated hits inspect the numeric ABI record and never touch this Rust
    /// metadata.
    names: Vec<Option<Box<str>>>,
}

impl Default for NativeGlobalReferenceCache {
    fn default() -> Self {
        Self {
            entries: Box::new(
                [php_jit::JitNativeGlobalReferenceCacheRecord::default();
                    NATIVE_GLOBAL_REFERENCE_CACHE_SIZE],
            ),
            names: (0..NATIVE_GLOBAL_REFERENCE_CACHE_SIZE)
                .map(|_| None)
                .collect(),
        }
    }
}

impl NativeGlobalReferenceCache {
    fn index(unit_identity: u64, function: u32, continuation: u32) -> usize {
        php_jit::jit_native_global_reference_cache_index(
            unit_identity,
            function,
            continuation,
            (NATIVE_GLOBAL_REFERENCE_CACHE_SIZE - 1) as u32,
        )
    }
}

#[derive(Clone)]
struct NativeDynamicUnit {
    compiled: crate::compiled_unit::CompiledUnit,
    native_entries:
        std::sync::Arc<std::collections::BTreeMap<php_ir::FunctionId, php_jit::JitFunctionHandle>>,
    native_entry_signature_hashes: std::collections::BTreeMap<php_ir::FunctionId, u64>,
    native_entry_signature_epochs: std::collections::BTreeMap<php_ir::FunctionId, u64>,
}

fn native_active_class_handle(
    context: &NativeExecutionContext<'_>,
    name: &str,
) -> Option<crate::compiled_unit::CompiledClass> {
    context.current_dynamic_unit.map_or_else(
        || context.compiled.lookup_unit_class_handle(name),
        |unit| {
            context
                .dynamic_units
                .get(unit)?
                .compiled
                .lookup_unit_class_handle(name)
        },
    )
}

#[derive(Clone, Copy)]
struct ActiveNativeUnit(*const php_ir::IrUnit);

impl ActiveNativeUnit {
    fn new(compiled: &crate::compiled_unit::CompiledUnit) -> Self {
        Self(compiled.unit() as *const php_ir::IrUnit)
    }
}

// SAFETY: The pointed-to IR is owned by `NativeExecutionContext::compiled` or
// by one of its `dynamic_units`. Scoped unit switches retain the prior and new
// `CompiledUnit` handles until after this pointer is restored.
#[allow(unsafe_code)]
impl std::ops::Deref for ActiveNativeUnit {
    type Target = php_ir::IrUnit;

    fn deref(&self) -> &Self::Target {
        // SAFETY: Established by `ActiveNativeUnit::new` and the context
        // ownership invariant documented on this implementation.
        unsafe { &*self.0 }
    }
}

#[derive(Clone, Copy)]
struct NativeInstructionPtr(*const php_ir::Instruction);

// SAFETY: Continuation instructions are owned by the active immutable
// CompiledUnit (or its immutable IR unit fallback). Both outlive every
// synchronous native helper invocation that receives this pointer.
#[allow(unsafe_code)]
impl std::ops::Deref for NativeInstructionPtr {
    type Target = php_ir::Instruction;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.0 }
    }
}

#[derive(Clone, Copy)]
pub(super) struct NativeFunctionMetadataPtr(
    *const crate::compiled_unit::PreparedNativeFunctionMetadata,
);

impl NativeFunctionMetadataPtr {
    fn from_compiled(
        compiled: &crate::compiled_unit::CompiledUnit,
        function: php_ir::FunctionId,
    ) -> Option<Self> {
        compiled
            .prepared_native_function_metadata_ptr(function)
            .map(Self)
    }
}

// SAFETY: Prepared function metadata is immutable and owned by the active
// CompiledUnit. NativeExecutionContext retains that unit (including dynamic
// units) for the lifetime of every synchronous native frame using this view.
#[allow(unsafe_code)]
impl std::ops::Deref for NativeFunctionMetadataPtr {
    type Target = crate::compiled_unit::PreparedNativeFunctionMetadata;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.0 }
    }
}

pub(super) struct NativeExecutionContext<'a> {
    compiled: crate::compiled_unit::CompiledUnit,
    unit: ActiveNativeUnit,
    unit_identity: u64,
    options: &'a super::VmOptions,
    worker_state: &'a super::VmWorkerState,
    native_entries:
        std::sync::Arc<std::collections::BTreeMap<php_ir::FunctionId, php_jit::JitFunctionHandle>>,
    native_call_encoded_scratch: Vec<i64>,
    native_frame_arena: NativeFrameArena,
    native_method_pics: std::collections::BTreeMap<u64, NativeMethodPic>,
    pub(super) output: php_runtime::api::OutputBuffer,
    values: Vec<Option<NativeStoredValue>>,
    value_slots: Vec<php_jit::JitNativeValueSlot>,
    direct_value_slots: Box<[php_jit::JitNativeValueSlot]>,
    direct_value_next: Box<u32>,
    direct_array_entries: Box<[php_jit::JitNativeDirectArrayEntry]>,
    direct_array_next: Box<u32>,
    direct_value_free_head: Box<u32>,
    direct_array_free_heads: Box<[u32; php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS]>,
    direct_string_bytes: Box<[u8]>,
    direct_string_next: Box<u32>,
    object_property_caches: Vec<Option<Box<[php_jit::JitNativePropertyCacheEntry]>>>,
    interned_value_handles: NativeValueIdentityMap,
    global_reference_cache: NativeGlobalReferenceCache,
    native_poll_counter: Box<u32>,
    free_value_slots: Vec<u32>,
    /// Successfully resolved IR constants are immutable for the lifetime of
    /// their owning unit. Keep one request-local value per unit/index so hot
    /// native operands do not repeatedly search runtime constant registries.
    decoded_constant_cache:
        RefCell<std::collections::HashMap<(Option<usize>, usize), php_runtime::api::Value>>,
    runtime_class_cache:
        RefCell<std::collections::HashMap<(Option<usize>, String), Rc<PreparedNativeRuntimeClass>>>,
    /// Long-lived request roots (globals, statics, callbacks, sessions, and
    /// suspended state). This index must not be invalidated by every call.
    root_index: RequestRootIndex,
    resources: php_runtime::api::ResourceTable,
    builtin_request_state: php_runtime::api::BuiltinRequestState,
    registered_extensions: NativeRegisteredExtensionRequestState,
    pub(super) http_response: php_runtime::api::RuntimeHttpResponseState,
    pub(super) upload_registry: php_runtime::api::UploadRegistry,
    pub(super) session: php_runtime::api::SessionState,
    session_global: php_runtime::api::ReferenceCell,
    filter_input_arrays: Rc<std::collections::BTreeMap<i64, php_runtime::api::PhpArray>>,
    ini_registry: php_runtime::api::IniRegistry,
    default_timezone: String,
    mysql_state: std::rc::Rc<RefCell<php_runtime::api::MysqlState>>,
    dynamic_constants: std::collections::BTreeMap<String, Value>,
    visible_function_names: Rc<NativeFunctionNameScope>,
    inherited_autoload_callback_count: usize,
    inherited_shutdown_callback_count: usize,
    dynamic_functions: std::collections::BTreeMap<String, php_ir::FunctionId>,
    deployment_functions:
        std::sync::Arc<std::collections::HashMap<std::sync::Arc<str>, php_ir::FunctionId>>,
    deployment_classes: std::sync::Arc<std::collections::HashSet<std::sync::Arc<str>>>,
    external_functions: std::collections::HashMap<String, NativeDynamicFunction>,
    external_class_units: std::collections::HashMap<String, usize>,
    /// Monotonic identity of the visible cross-unit by-reference signature
    /// set. By-value declarations cannot alter generated caller binding, so
    /// they must not invalidate every already-published native entry.
    external_signature_epoch: u64,
    dynamic_units: Vec<NativeDynamicUnit>,
    current_dynamic_unit: Option<usize>,
    static_properties: std::collections::BTreeMap<(String, String), Value>,
    static_locals: std::collections::BTreeMap<(u64, u32, u32), php_runtime::api::ReferenceCell>,
    enum_cases: std::collections::BTreeMap<(String, String), php_runtime::api::ObjectRef>,
    class_constant_cache: std::collections::HashMap<
        (Option<usize>, u32),
        std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    >,
    generator_iterators: std::collections::BTreeMap<u64, i64>,
    fiber_executions: std::collections::BTreeMap<u64, NativeFiberExecution>,
    active_fiber: Option<u64>,
    pending_fiber_suspension_value: Option<i64>,
    pending_nested_fiber_execution: Option<NativeFiberExecution>,
    completed_nested_fiber_call: Option<(u32, u32, i64)>,
    pending_throwable: Option<Value>,
    called_classes: Vec<Arc<str>>,
    lexical_scope_classes: Vec<String>,
    call_frames: Vec<NativeBacktraceFrame>,
    dynamic_classes: std::collections::BTreeSet<String>,
    class_aliases: std::collections::BTreeMap<String, String>,
    autoload_callbacks: Vec<Value>,
    shutdown_callbacks: Vec<NativeShutdownCallback>,
    destroyed_objects: std::collections::BTreeMap<u64, WeakObjectHandle>,
    autoload_in_progress: std::collections::BTreeSet<String>,
    error_reporting: i64,
    display_errors: bool,
    last_error: Option<NativeLastError>,
    error_handlers: Vec<NativeErrorHandler>,
    exception_handlers: Vec<Value>,
    explicit_reference_ids: std::collections::BTreeSet<u64>,
    environment: std::sync::Arc<Vec<(String, String)>>,
    included_files: std::collections::BTreeSet<std::path::PathBuf>,
    include_path: Arc<Vec<std::path::PathBuf>>,
    cwd: std::path::PathBuf,
    inherited_globals: std::collections::BTreeMap<String, Value>,
    continuation_instructions:
        std::sync::Arc<Vec<Vec<Option<std::sync::Arc<php_ir::Instruction>>>>>,
    native_callsites: std::sync::Arc<
        Vec<Vec<Option<std::sync::Arc<crate::compiled_unit::NativeCallSiteDescriptor>>>>,
    >,
    include_child: bool,
    execution_deadline_at: Option<std::time::Instant>,
    execution_deadline_mutable: bool,
    runtime_telemetry: Rc<RefCell<NativeRuntimeTelemetry>>,
    pub(super) diagnostic: Option<php_runtime::api::RuntimeDiagnostic>,
}

// Generated code holds raw pointers into these parallel vectors while a
// request is active, so their allocations must not move.
const NATIVE_VALUE_REFCOUNT_CAPACITY: usize = 1 << 20;
fn stored_value_slot(value: &NativeStoredValue) -> php_jit::JitNativeValueSlot {
    let mut slot = php_jit::JitNativeValueSlot {
        refcount: 1,
        ..php_jit::JitNativeValueSlot::default()
    };
    match value {
        NativeStoredValue::Php(Value::String(value)) => {
            slot.kind = php_jit::JIT_NATIVE_VALUE_VIEW_STRING;
            slot.flags = php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION;
            slot.reserved =
                u32::from(value.as_bytes() == b"0") * php_jit::JIT_NATIVE_STRING_VALUE_ZERO;
            slot.payload = u64::try_from(value.len()).unwrap_or(u64::MAX);
            slot.aux = value.as_bytes().as_ptr() as usize as u64;
        }
        NativeStoredValue::Php(Value::Reference(reference)) => {
            slot.kind = php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR;
            slot.flags = php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION;
            slot.payload = reference.native_scalar_view_address() as u64;
            slot.aux = reference.native_array_view_address() as u64;
        }
        NativeStoredValue::ArrayIterator(iterator) => {
            if let Some(direct) = iterator.direct.as_ref() {
                slot.kind = php_jit::JIT_NATIVE_VALUE_VIEW_FOREACH_DIRECT;
                slot.flags = php_jit::JIT_NATIVE_FOREACH_VIEW_ABI_VERSION;
                slot.payload = std::ptr::from_ref(direct.view.as_ref()) as usize as u64;
            }
        }
        _ => {}
    }
    slot
}

enum NativeStoredValue {
    Php(Value),
    /// Native closure record published once with request-owned encoded
    /// capture handles. Invocation borrows these handles and never rebuilds
    /// captured arrays through Rust `Value`.
    PreparedClosure(Box<NativePreparedClosure>),
    GlobalsProxy,
    ArrayIterator(Box<NativeArrayIteratorState>),
    Iterator(Box<NativeIteratorState>),
    GeneratorIterator(Box<NativeGeneratorIteratorState>),
}

struct NativePreparedClosure {
    callable: Box<php_runtime::api::CallableValue>,
    captures: Box<[i64]>,
}

struct NativeArrayIteratorState {
    source: php_runtime::api::PhpArray,
    index: usize,
    direct: Option<Box<NativeDirectForeachState>>,
}

struct NativeDirectForeachState {
    view: Box<php_jit::JitNativeForeachView>,
    entries: Box<[php_jit::JitNativeForeachEntry]>,
}

struct NativeIteratorState {
    entries: Vec<(Value, Value)>,
    index: usize,
    live_source: Option<i64>,
    live_global: Option<String>,
    live_object: Option<php_runtime::api::ObjectRef>,
    user_iterator: Option<php_runtime::api::ObjectRef>,
    user_iterator_started: bool,
}

struct NativeGeneratorIteratorState {
    generator: php_runtime::api::GeneratorRef,
    handle: Box<php_jit::JitFunctionHandle>,
    arguments: Vec<i64>,
    state: Box<Option<php_jit::JitDeoptState>>,
    delegation: Option<NativeGeneratorDelegation>,
    yields_seen: u64,
    finished: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum NativeValueIdentity {
    Object(u64),
    Reference(u64),
    String(php_runtime::api::PhpString),
    Closure(u64),
    GlobalsProxy,
}

#[derive(Default)]
struct NativeValueIdentityHasher(u64);

impl std::hash::Hasher for NativeValueIdentityHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(std::mem::size_of::<u64>()) {
            let mut word = [0_u8; std::mem::size_of::<u64>()];
            word[..chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_ne_bytes(word));
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = self.0.rotate_left(17) ^ value.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }
}

type NativeValueIdentityMap = std::collections::HashMap<
    NativeValueIdentity,
    u32,
    std::hash::BuildHasherDefault<NativeValueIdentityHasher>,
>;

pub(super) struct NativeValueArenaBuffers {
    values: Vec<Option<NativeStoredValue>>,
    value_slots: Vec<php_jit::JitNativeValueSlot>,
    direct_value_slots: Box<[php_jit::JitNativeValueSlot]>,
    direct_value_next: Box<u32>,
    direct_array_entries: Box<[php_jit::JitNativeDirectArrayEntry]>,
    direct_array_next: Box<u32>,
    direct_value_free_head: Box<u32>,
    direct_array_free_heads: Box<[u32; php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS]>,
    direct_string_bytes: Box<[u8]>,
    direct_string_next: Box<u32>,
    object_property_caches: Vec<Option<Box<[php_jit::JitNativePropertyCacheEntry]>>>,
    interned_value_handles: NativeValueIdentityMap,
    global_reference_cache: NativeGlobalReferenceCache,
    free_value_slots: Vec<u32>,
}

impl Default for NativeValueArenaBuffers {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            value_slots: Vec::with_capacity(NATIVE_VALUE_REFCOUNT_CAPACITY),
            direct_value_slots: vec![
                php_jit::JitNativeValueSlot::default();
                php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY
            ]
            .into_boxed_slice(),
            direct_value_next: Box::new(0),
            direct_array_entries: vec![
                php_jit::JitNativeDirectArrayEntry::default();
                php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
            ]
            .into_boxed_slice(),
            direct_array_next: Box::new(0),
            direct_value_free_head: Box::new(php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE),
            direct_array_free_heads: Box::new(
                [php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
                    php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS],
            ),
            direct_string_bytes: vec![0; php_jit::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY]
                .into_boxed_slice(),
            direct_string_next: Box::new(0),
            object_property_caches: Vec::with_capacity(NATIVE_VALUE_REFCOUNT_CAPACITY),
            interned_value_handles: NativeValueIdentityMap::default(),
            global_reference_cache: NativeGlobalReferenceCache::default(),
            free_value_slots: Vec::new(),
        }
    }
}

impl std::fmt::Debug for NativeValueArenaBuffers {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeValueArenaBuffers")
            .field("value_capacity", &self.values.capacity())
            .field("slot_capacity", &self.value_slots.capacity())
            .finish()
    }
}

thread_local! {
    static NATIVE_VALUE_ARENA_POOL: RefCell<Vec<NativeValueArenaBuffers>> = const {
        RefCell::new(Vec::new())
    };
}

fn take_native_value_arena() -> NativeValueArenaBuffers {
    NATIVE_VALUE_ARENA_POOL.with(|arenas| arenas.borrow_mut().pop().unwrap_or_default())
}

fn recycle_native_value_arena(arena: NativeValueArenaBuffers) {
    debug_assert!(arena.values.is_empty());
    debug_assert!(arena.value_slots.is_empty());
    debug_assert!(arena.interned_value_handles.is_empty());
    debug_assert!(
        arena
            .global_reference_cache
            .entries
            .iter()
            .all(|entry| entry.valid == 0)
    );
    debug_assert!(
        arena
            .global_reference_cache
            .names
            .iter()
            .all(Option::is_none)
    );
    debug_assert!(arena.free_value_slots.is_empty());
    const MAX_RETAINED_NATIVE_VALUE_ARENAS: usize = 1;
    NATIVE_VALUE_ARENA_POOL.with(|arenas| {
        let mut arenas = arenas.borrow_mut();
        if arenas.len() < MAX_RETAINED_NATIVE_VALUE_ARENAS {
            arenas.push(arena);
        }
    });
}

fn stored_value_identity(value: &NativeStoredValue) -> Option<NativeValueIdentity> {
    match value {
        NativeStoredValue::Php(Value::Object(object)) => {
            Some(NativeValueIdentity::Object(object.id()))
        }
        NativeStoredValue::Php(Value::Reference(reference)) => {
            Some(NativeValueIdentity::Reference(reference.gc_debug_id()))
        }
        NativeStoredValue::Php(Value::String(string)) => {
            Some(NativeValueIdentity::String(string.clone()))
        }
        NativeStoredValue::PreparedClosure(closure) => match closure.callable.as_ref() {
            php_runtime::api::CallableValue::Closure(closure) => {
                Some(NativeValueIdentity::Closure(closure.id))
            }
            _ => None,
        },
        NativeStoredValue::GlobalsProxy => Some(NativeValueIdentity::GlobalsProxy),
        _ => None,
    }
}

fn stored_value_tag(value: &NativeStoredValue) -> u64 {
    match value {
        NativeStoredValue::Php(Value::Reference(_)) => php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG,
        NativeStoredValue::Php(Value::Array(_)) => php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG,
        NativeStoredValue::Php(Value::Object(_)) => php_jit::JIT_VALUE_RUNTIME_OBJECT_TAG,
        NativeStoredValue::Php(Value::String(_)) => php_jit::JIT_VALUE_RUNTIME_STRING_TAG,
        NativeStoredValue::Php(Value::Float(_)) => php_jit::JIT_VALUE_RUNTIME_FLOAT_TAG,
        NativeStoredValue::Php(Value::Callable(_)) => php_jit::JIT_VALUE_RUNTIME_CALLABLE_TAG,
        NativeStoredValue::PreparedClosure(_) => php_jit::JIT_VALUE_RUNTIME_CALLABLE_TAG,
        NativeStoredValue::Php(Value::Resource(_)) => php_jit::JIT_VALUE_RUNTIME_RESOURCE_TAG,
        NativeStoredValue::Php(Value::Generator(_)) => php_jit::JIT_VALUE_RUNTIME_GENERATOR_TAG,
        NativeStoredValue::Php(Value::Fiber(_)) => php_jit::JIT_VALUE_RUNTIME_FIBER_TAG,
        NativeStoredValue::GlobalsProxy => php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG,
        NativeStoredValue::ArrayIterator(_)
        | NativeStoredValue::Iterator(_)
        | NativeStoredValue::GeneratorIterator(_) => php_jit::JIT_VALUE_RUNTIME_ITERATOR_TAG,
        NativeStoredValue::Php(
            Value::Null | Value::Bool(_) | Value::Int(_) | Value::Uninitialized,
        ) => php_jit::JIT_VALUE_RUNTIME_TAG,
    }
}

fn stored_value_kind(value: &NativeStoredValue) -> &'static str {
    match value {
        NativeStoredValue::Php(Value::Null) => "null",
        NativeStoredValue::Php(Value::Bool(_)) => "bool",
        NativeStoredValue::Php(Value::Int(_)) => "int",
        NativeStoredValue::Php(Value::Float(_)) => "float",
        NativeStoredValue::Php(Value::String(_)) => "string",
        NativeStoredValue::Php(Value::Array(_)) => "array",
        NativeStoredValue::Php(Value::Object(_)) => "object",
        NativeStoredValue::Php(Value::Resource(_)) => "resource",
        NativeStoredValue::Php(Value::Reference(_)) => "reference",
        NativeStoredValue::Php(Value::Callable(_)) => "callable",
        NativeStoredValue::PreparedClosure(_) => "prepared_closure",
        NativeStoredValue::Php(Value::Generator(_)) => "generator",
        NativeStoredValue::Php(Value::Fiber(_)) => "fiber",
        NativeStoredValue::Php(Value::Uninitialized) => "uninitialized",
        NativeStoredValue::GlobalsProxy => "globals_proxy",
        NativeStoredValue::ArrayIterator(_) => "array_iterator",
        NativeStoredValue::Iterator(_) => "iterator",
        NativeStoredValue::GeneratorIterator(_) => "generator_iterator",
    }
}

struct PreparedNativeRuntimeClass {
    entry: php_runtime::api::ClassEntry,
    default_declared_slots: Vec<Option<Value>>,
}

#[derive(Clone)]
enum NativeGeneratorDelegation {
    Array {
        entries: Vec<(Value, Value)>,
        index: usize,
    },
    Generator {
        generator: php_runtime::api::GeneratorRef,
        iterator: i64,
    },
}

struct NativeFiberExecution {
    handle: php_jit::JitFunctionHandle,
    arguments: Vec<i64>,
    state: php_jit::JitDeoptState,
    nested: Option<Box<NativeFiberExecution>>,
}

impl<'a> NativeExecutionContext<'a> {
    fn mark_roots_dirty(&mut self, reason: RootMutationReason) {
        self.root_index.mark_dirty(reason);
    }

    fn mark_rooted_container_dirty(&mut self, value: &Value) {
        self.root_index
            .mark_dirty(RootMutationReason::RootedContainer);
        self.root_index.refresh_container(value);
    }

    fn value_has_native_destructor(&self, value: &Value) -> bool {
        let mut value = value.clone();
        for _ in 0..16 {
            match value {
                Value::Reference(reference) => value = reference.get(),
                Value::Object(object) => {
                    return self.object_has_native_destructor(&object.class_name());
                }
                _ => return false,
            }
        }
        false
    }

    fn synchronize_destructor_root_change(&mut self, previous: &Value, replacement: &Value) {
        if self.value_has_native_destructor(previous)
            || self.value_has_native_destructor(replacement)
        {
            self.synchronize_request_roots();
        }
    }

    fn add_rooted_nested_container(&mut self, parent: &Value, child: &Value) {
        if self.root_index.is_dirty() || self.root_index.contains_container(parent) {
            self.root_index.add_nested_container(parent, child);
        }
    }

    fn request_root_values(&self) -> Vec<Value> {
        let mut roots = self
            .static_properties
            .values()
            .chain(self.dynamic_constants.values())
            .chain(self.inherited_globals.values())
            .chain(self.autoload_callbacks.iter())
            .chain(self.exception_handlers.iter())
            .cloned()
            .collect::<Vec<_>>();
        roots.extend(self.static_locals.values().cloned().map(Value::Reference));
        roots.push(Value::Reference(self.session_global.clone()));
        for callback in &self.shutdown_callbacks {
            roots.push(callback.callable.clone());
            roots.extend(callback.arguments.iter().cloned());
        }
        roots.extend(
            self.error_handlers
                .iter()
                .map(|handler| handler.callback.clone()),
        );
        roots.extend(self.pending_throwable.iter().cloned());
        roots.extend(self.enum_cases.values().cloned().map(Value::Object));
        roots
    }

    fn synchronize_request_roots(&mut self) {
        if self.root_index.is_dirty() {
            let roots = self.request_root_values();
            self.root_index.synchronize(&roots);
        }
    }

    fn finalize_replaced_value(&mut self, previous: Value) -> Result<(), String> {
        if let Value::Object(object) = previous {
            let uniquely_owned = object.gc_refcount_estimate() == 1;
            if uniquely_owned {
                self.run_object_destructor(object)?;
            }
        }
        Ok(())
    }

    pub(super) const fn process_exit_terminates_process(&self) -> bool {
        self.registered_extensions.is_fork_child()
    }

    pub(super) fn new(
        compiled: &'a crate::compiled_unit::CompiledUnit,
        unit_identity: u64,
        options: &'a super::VmOptions,
        worker_state: &'a super::VmWorkerState,
        output: php_runtime::api::OutputBuffer,
        native_entries: std::sync::Arc<
            std::collections::BTreeMap<php_ir::FunctionId, php_jit::JitFunctionHandle>,
        >,
    ) -> Self {
        let unit = compiled.unit();
        let inherited_globals = NATIVE_INCLUDE_GLOBALS.with(|globals| globals.borrow_mut().take());
        let inherited_constants =
            NATIVE_INCLUDE_CONSTANTS.with(|constants| constants.borrow_mut().take());
        let inherited_ini = NATIVE_INCLUDE_INI.with(|ini| ini.borrow_mut().take());
        let inherited_default_timezone =
            NATIVE_INCLUDE_DEFAULT_TIMEZONE.with(|timezone| timezone.borrow_mut().take());
        let inherited_http_response =
            NATIVE_INCLUDE_HTTP_RESPONSE.with(|response| response.borrow_mut().take());
        let inherited_files = NATIVE_INCLUDE_FILES.with(|files| files.borrow_mut().take());
        let inherited_mysql = NATIVE_INCLUDE_MYSQL.with(|mysql| mysql.borrow_mut().take());
        let inherited_filter_input_arrays =
            NATIVE_INCLUDE_FILTER_INPUT_ARRAYS.with(|arrays| arrays.borrow_mut().take());
        let inherited_function_names = NATIVE_INCLUDE_FUNCTION_NAMES.with(|names| {
            names
                .borrow_mut()
                .take()
                .unwrap_or_else(|| Rc::new(NativeFunctionNameScope::default()))
        });
        let visible_function_names = NativeFunctionNameScope::child(
            inherited_function_names,
            unit.function_table
                .iter()
                .map(|entry| entry.name.to_ascii_lowercase()),
        );
        let inherited_symbols =
            NATIVE_INCLUDE_SYMBOLS.with(|symbols| symbols.borrow_mut().take().unwrap_or_default());
        let inherited_error_reporting = inherited_symbols.error_reporting;
        let inherited_display_errors = inherited_symbols.display_errors;
        let inherited_autoload_callback_count = inherited_symbols.autoload_callbacks.len();
        let inherited_shutdown_callback_count = inherited_symbols.shutdown_callbacks.len();
        let include_child = inherited_globals.is_some();
        let mut inherited_globals = inherited_globals.unwrap_or_default();
        let session = options.runtime_context.session.clone();
        let session_global = inherited_globals
            .get("_SESSION")
            .and_then(|value| match value {
                Value::Reference(reference) => Some(reference.clone()),
                _ => None,
            })
            .unwrap_or_else(|| {
                php_runtime::api::ReferenceCell::new(
                    if session.status() == php_runtime::api::PHP_SESSION_ACTIVE || session.started()
                    {
                        session.data_value()
                    } else {
                        Value::Uninitialized
                    },
                )
            });
        inherited_globals.insert(
            "_SESSION".to_owned(),
            Value::Reference(session_global.clone()),
        );
        let filter_input_arrays = inherited_filter_input_arrays.unwrap_or_else(|| {
            Rc::new(
                [0_i64, 1, 2, 4, 5]
                    .into_iter()
                    .filter_map(|source| {
                        options
                            .runtime_context
                            .filter_input_array(source)
                            .map(|array| (source, array))
                    })
                    .collect(),
            )
        });
        let mut resources = php_runtime::api::ResourceTable::new();
        let stdin = resources.register_stdin(options.runtime_context.stdin.to_vec());
        let stdout = resources.register_stdout();
        let stderr = resources.register_stderr();
        let mut dynamic_constants = inherited_constants.unwrap_or_default();
        dynamic_constants
            .entry("STDIN".to_owned())
            .or_insert(Value::Resource(stdin));
        dynamic_constants
            .entry("STDOUT".to_owned())
            .or_insert(Value::Resource(stdout));
        dynamic_constants
            .entry("STDERR".to_owned())
            .or_insert(Value::Resource(stderr));
        let continuation_instructions = compiled.prepared_continuation_instructions();
        let native_callsites = compiled.prepared_native_callsites();
        let native_call_argument_capacity = compiled
            .prepared_deployment_image()
            .native_call_argument_capacity;
        let mut environment = std::sync::Arc::clone(&options.runtime_context.env);
        if !environment.windows(2).all(|pair| {
            pair[0].0 <= pair[1].0 && !(pair[0].0 == pair[1].0 && pair[0].1 > pair[1].1)
        }) {
            let mut sorted = environment.as_ref().clone();
            sorted.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
            environment = std::sync::Arc::new(sorted);
        }
        let value_arena = take_native_value_arena();
        Self {
            compiled: compiled.clone(),
            unit: ActiveNativeUnit::new(compiled),
            unit_identity,
            options,
            worker_state,
            native_entries,
            native_call_encoded_scratch: Vec::with_capacity(native_call_argument_capacity),
            native_frame_arena: NativeFrameArena::default(),
            native_method_pics: std::collections::BTreeMap::new(),
            output,
            values: value_arena.values,
            value_slots: value_arena.value_slots,
            direct_value_slots: value_arena.direct_value_slots,
            direct_value_next: value_arena.direct_value_next,
            direct_array_entries: value_arena.direct_array_entries,
            direct_array_next: value_arena.direct_array_next,
            direct_value_free_head: value_arena.direct_value_free_head,
            direct_array_free_heads: value_arena.direct_array_free_heads,
            direct_string_bytes: value_arena.direct_string_bytes,
            direct_string_next: value_arena.direct_string_next,
            object_property_caches: value_arena.object_property_caches,
            interned_value_handles: value_arena.interned_value_handles,
            global_reference_cache: value_arena.global_reference_cache,
            // Wrapping 4095 + 1 makes the first loop-header visit poll. Native
            // code then checks the deadline once per 4096 loop-header visits.
            native_poll_counter: Box::new(4095),
            free_value_slots: value_arena.free_value_slots,
            decoded_constant_cache: RefCell::new(std::collections::HashMap::new()),
            runtime_class_cache: RefCell::new(std::collections::HashMap::new()),
            root_index: RequestRootIndex::new_dirty(),
            resources,
            builtin_request_state: php_runtime::api::BuiltinRequestState::new(),
            registered_extensions: NativeRegisteredExtensionRequestState::default(),
            http_response: inherited_http_response.unwrap_or_default(),
            upload_registry: options.runtime_context.upload_registry(),
            session,
            session_global,
            filter_input_arrays,
            ini_registry: inherited_ini.unwrap_or_else(|| options.runtime_context.ini_registry()),
            default_timezone: inherited_default_timezone
                .unwrap_or_else(|| php_runtime::api::datetime::DEFAULT_TIMEZONE.to_owned()),
            mysql_state: inherited_mysql
                .unwrap_or_else(|| std::rc::Rc::new(RefCell::new(Default::default()))),
            dynamic_constants,
            visible_function_names,
            inherited_autoload_callback_count,
            inherited_shutdown_callback_count,
            dynamic_functions: std::collections::BTreeMap::new(),
            deployment_functions: inherited_symbols.deployment_functions,
            deployment_classes: inherited_symbols.deployment_classes,
            external_functions: inherited_symbols.external_functions,
            external_class_units: inherited_symbols.external_class_units,
            external_signature_epoch: inherited_symbols.external_signature_epoch,
            dynamic_units: inherited_symbols.dynamic_units,
            current_dynamic_unit: None,
            static_properties: inherited_symbols.static_properties,
            static_locals: inherited_symbols.static_locals,
            enum_cases: inherited_symbols.enum_cases,
            class_constant_cache: std::collections::HashMap::new(),
            generator_iterators: std::collections::BTreeMap::new(),
            fiber_executions: std::collections::BTreeMap::new(),
            active_fiber: None,
            pending_fiber_suspension_value: None,
            pending_nested_fiber_execution: None,
            completed_nested_fiber_call: None,
            pending_throwable: None,
            called_classes: Vec::new(),
            lexical_scope_classes: Vec::new(),
            call_frames: Vec::new(),
            dynamic_classes: inherited_symbols.dynamic_classes,
            class_aliases: inherited_symbols.class_aliases,
            autoload_callbacks: inherited_symbols.autoload_callbacks,
            shutdown_callbacks: inherited_symbols.shutdown_callbacks,
            destroyed_objects: inherited_symbols.destroyed_objects,
            autoload_in_progress: std::collections::BTreeSet::new(),
            error_reporting: inherited_error_reporting
                .unwrap_or(options.runtime_context.ini.error_reporting.mask),
            display_errors: inherited_display_errors
                .unwrap_or(options.runtime_context.ini.display_errors),
            last_error: inherited_symbols.last_error,
            error_handlers: inherited_symbols.error_handlers,
            exception_handlers: inherited_symbols.exception_handlers,
            explicit_reference_ids: std::collections::BTreeSet::new(),
            environment,
            included_files: inherited_files.unwrap_or_default(),
            include_path: Arc::new(options.runtime_context.include_path.clone()),
            cwd: options.runtime_context.cwd.clone(),
            inherited_globals,
            continuation_instructions,
            native_callsites,
            include_child,
            execution_deadline_at: options
                .runtime_context
                .execution_time_limit
                .and_then(|limit| std::time::Instant::now().checked_add(limit)),
            execution_deadline_mutable: options.runtime_context.execution_time_limit.is_some(),
            runtime_telemetry: Rc::new(RefCell::new(NativeRuntimeTelemetry::default())),
            diagnostic: None,
        }
    }

    pub(super) fn recycle_native_value_arena(&mut self) {
        let direct_value_used = usize::try_from(*self.direct_value_next).unwrap_or(0);
        let direct_array_used = usize::try_from(*self.direct_array_next).unwrap_or(0);
        let direct_string_used = usize::try_from(*self.direct_string_next).unwrap_or(0);
        for index in (0..direct_value_used).rev() {
            while self.direct_value_slots[index].refcount != 0 {
                if self.release_direct_value_index(index).is_err() {
                    break;
                }
            }
        }
        for entry in &mut *self.global_reference_cache.entries {
            *entry = php_jit::JitNativeGlobalReferenceCacheRecord::default();
        }
        self.global_reference_cache.names.fill_with(|| None);
        self.object_property_caches.clear();
        self.values.clear();
        self.value_slots.clear();
        self.direct_value_slots[..direct_value_used].fill(php_jit::JitNativeValueSlot::default());
        self.direct_array_entries[..direct_array_used]
            .fill(php_jit::JitNativeDirectArrayEntry::default());
        *self.direct_value_next = 0;
        *self.direct_array_next = 0;
        *self.direct_value_free_head = php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
        self.direct_array_free_heads
            .fill(php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE);
        self.direct_string_bytes[..direct_string_used].fill(0);
        *self.direct_string_next = 0;
        self.interned_value_handles.clear();
        self.free_value_slots.clear();
        recycle_native_value_arena(NativeValueArenaBuffers {
            values: std::mem::take(&mut self.values),
            value_slots: std::mem::take(&mut self.value_slots),
            direct_value_slots: std::mem::take(&mut self.direct_value_slots),
            direct_value_next: std::mem::take(&mut self.direct_value_next),
            direct_array_entries: std::mem::take(&mut self.direct_array_entries),
            direct_array_next: std::mem::take(&mut self.direct_array_next),
            direct_value_free_head: std::mem::take(&mut self.direct_value_free_head),
            direct_array_free_heads: std::mem::take(&mut self.direct_array_free_heads),
            direct_string_bytes: std::mem::take(&mut self.direct_string_bytes),
            direct_string_next: std::mem::take(&mut self.direct_string_next),
            object_property_caches: std::mem::take(&mut self.object_property_caches),
            interned_value_handles: std::mem::take(&mut self.interned_value_handles),
            global_reference_cache: std::mem::take(&mut self.global_reference_cache),
            free_value_slots: std::mem::take(&mut self.free_value_slots),
        });
    }

    fn reset_execution_deadline_seconds(&mut self, seconds: u64) {
        if !self.execution_deadline_mutable {
            return;
        }
        self.execution_deadline_at = if seconds == 0 {
            None
        } else {
            std::time::Instant::now().checked_add(std::time::Duration::from_secs(seconds))
        };
    }

    fn publish_native_entry_address(&self, function: php_ir::FunctionId, address: usize) {
        if let Some(cell) = self
            .compiled
            .prepared_deployment_image()
            .native_function_entries
            .get(function.index())
        {
            cell.store(address, std::sync::atomic::Ordering::Release);
        }
    }

    pub(super) fn attach_root_deployment_image(
        &mut self,
        compiled: crate::compiled_unit::CompiledUnit,
    ) {
        if self.include_child || self.current_dynamic_unit.is_some() {
            return;
        }
        let unit = self.dynamic_units.len();
        let deployment = compiled.prepared_deployment_image();
        for (function, handle) in self.native_entries.iter() {
            if !handle.region_state_metadata().is_some_and(|metadata| {
                metadata.compiler_tier == php_jit::region_ir::NativeCompilerTier::Baseline
            }) {
                continue;
            }
            if let (Some(cell), Some(address)) = (
                deployment.native_function_entries.get(function.index()),
                handle.native_entry_address(),
            ) {
                cell.store(address, std::sync::atomic::Ordering::Release);
            }
        }
        // Before the root image is attached there are no runtime declaration
        // overlays. Its compiled entries therefore all share the empty
        // external-signature set; do not rediscover call targets per request.
        let empty_signature_hash = super::external_function_signatures_hash(&[]);
        let native_entry_signature_hashes = self
            .native_entries
            .keys()
            .copied()
            .map(|function| (function, empty_signature_hash))
            .collect();
        if !deployment.function_exports.is_empty() {
            self.external_signature_epoch = self.external_signature_epoch.saturating_add(1);
        }
        let native_entry_signature_epochs = self
            .native_entries
            .keys()
            .copied()
            .map(|function| (function, self.external_signature_epoch))
            .collect();
        self.dynamic_units.push(NativeDynamicUnit {
            compiled: compiled.clone(),
            native_entries: self.native_entries.clone(),
            native_entry_signature_hashes,
            native_entry_signature_epochs,
        });
        debug_assert_eq!(unit, 0, "immutable deployment must be the root native unit");
        self.deployment_functions = std::sync::Arc::clone(&deployment.function_exports);
        self.deployment_classes = std::sync::Arc::clone(&deployment.exported_classes);
        self.current_dynamic_unit = Some(unit);
    }

    fn class_is_visible(&self, normalized: &str) -> bool {
        self.deployment_classes.contains(normalized) || self.dynamic_classes.contains(normalized)
    }

    fn ensure_native_global_references(&mut self) {
        const RUNTIME_GLOBALS: &[&str] = &[
            "argc", "argv", "_SERVER", "_ENV", "_GET", "_POST", "_COOKIE", "_REQUEST", "_FILES",
            "_SESSION",
        ];
        for name in RUNTIME_GLOBALS {
            if self.inherited_globals.contains_key(*name) {
                continue;
            }
            let Some(value) = self.options.runtime_context.global_value(name) else {
                continue;
            };
            let reference = match value {
                Value::Reference(reference) => reference,
                value => php_runtime::api::ReferenceCell::new(value),
            };
            self.inherited_globals
                .insert((*name).to_owned(), Value::Reference(reference));
        }
        for value in self.inherited_globals.values_mut() {
            if matches!(value, Value::Reference(_) | Value::Uninitialized) {
                continue;
            }
            let reference = php_runtime::api::ReferenceCell::new(value.clone());
            *value = Value::Reference(reference);
        }
    }

    fn invalidate_native_global_reference(
        &mut self,
        reference_identity: u64,
    ) -> Result<(), String> {
        let retained = self
            .global_reference_cache
            .entries
            .iter_mut()
            .zip(&mut self.global_reference_cache.names)
            .filter_map(|(entry, name)| {
                if entry.valid == 0 || entry.reference_identity != reference_identity {
                    return None;
                }
                let encoded = entry.encoded;
                *entry = php_jit::JitNativeGlobalReferenceCacheRecord::default();
                *name = None;
                Some(encoded)
            })
            .collect::<Vec<_>>();
        for encoded in retained {
            self.release(encoded)?;
        }
        Ok(())
    }

    fn reconcile_native_global_reference_cache(&mut self) -> Result<(), String> {
        let stale = self
            .global_reference_cache
            .entries
            .iter()
            .zip(&self.global_reference_cache.names)
            .enumerate()
            .filter_map(|(index, (entry, name))| {
                if entry.valid == 0 {
                    return None;
                }
                let still_bound = name.as_deref().is_some_and(|name| {
                    matches!(
                        self.inherited_globals.get(name),
                        Some(Value::Reference(reference))
                            if reference.gc_debug_id() == entry.reference_identity
                    )
                });
                (!still_bound).then_some(index)
            })
            .collect::<Vec<_>>();
        let retained = stale
            .into_iter()
            .map(|index| {
                let encoded = self.global_reference_cache.entries[index].encoded;
                self.global_reference_cache.entries[index] =
                    php_jit::JitNativeGlobalReferenceCacheRecord::default();
                self.global_reference_cache.names[index] = None;
                encoded
            })
            .collect::<Vec<_>>();
        for encoded in retained {
            self.release(encoded)?;
        }
        Ok(())
    }

    fn publish_native_global_reference(
        &mut self,
        unit_identity: u64,
        function: u32,
        continuation: u32,
        name: &str,
        encoded: i64,
    ) -> Result<(), String> {
        let Some(value_index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Err("native global binding did not produce a runtime reference".to_owned());
        };
        if encoded as u64 & php_jit::JIT_VALUE_RUNTIME_KIND_MASK
            != php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG
        {
            return Err("native global binding did not produce a reference handle".to_owned());
        }
        let reference_identity = match self
            .values
            .get(value_index as usize)
            .and_then(Option::as_ref)
        {
            Some(NativeStoredValue::Php(Value::Reference(reference))) => reference.gc_debug_id(),
            _ => {
                return Err(
                    "native global binding reference handle has no reference cell".to_owned(),
                );
            }
        };
        let index = NativeGlobalReferenceCache::index(unit_identity, function, continuation);
        let previous = self.global_reference_cache.entries[index];
        if previous.valid != 0
            && previous.unit_identity == unit_identity
            && previous.function_id == function
            && previous.continuation_id == continuation
            && previous.encoded == encoded
            && previous.reference_identity == reference_identity
            && self.global_reference_cache.names[index].as_deref() == Some(name)
        {
            return Ok(());
        }

        // The call result already owns one handle for the destination local.
        // The cache owns another until replacement or request reset.
        self.retain_runtime_value_index(value_index as usize)?;
        self.global_reference_cache.entries[index] = php_jit::JitNativeGlobalReferenceCacheRecord {
            unit_identity,
            encoded,
            reference_identity,
            function_id: function,
            continuation_id: continuation,
            valid: 1,
            reserved: 0,
        };
        self.global_reference_cache.names[index] = Some(name.into());
        if previous.valid != 0 {
            self.release(previous.encoded)?;
        }
        Ok(())
    }

    fn materialize_native_globals_array(&self) -> Value {
        let mut globals = php_runtime::api::PhpArray::with_capacity(self.inherited_globals.len());
        for (name, value) in &self.inherited_globals {
            if name == "GLOBALS" || matches!(value, Value::Uninitialized) {
                continue;
            }
            globals.insert(
                php_runtime::api::ArrayKey::String(PhpString::from_bytes(name.as_bytes().to_vec())),
                value.clone(),
            );
        }
        Value::Array(globals)
    }

    fn encode_globals_proxy(&mut self) -> Result<i64, String> {
        self.ensure_native_global_references();
        self.encode_stored_value(NativeStoredValue::GlobalsProxy)
    }

    fn is_globals_proxy(&self, encoded: i64) -> bool {
        php_jit::jit_decode_runtime_value(encoded).is_some_and(|index| {
            matches!(
                self.values.get(index as usize).and_then(Option::as_ref),
                Some(NativeStoredValue::GlobalsProxy)
            )
        })
    }

    fn native_global_name<'b>(
        key: &'b php_runtime::api::ArrayKey,
    ) -> Option<std::borrow::Cow<'b, str>> {
        let php_runtime::api::ArrayKey::String(name) = key else {
            return None;
        };
        let name = String::from_utf8_lossy(name.as_bytes());
        (name.as_ref() != "GLOBALS").then_some(name)
    }

    fn fetch_native_global_dimension(&mut self, key: &php_runtime::api::ArrayKey) -> Option<Value> {
        self.ensure_native_global_references();
        let name = Self::native_global_name(key)?;
        self.inherited_globals
            .get(name.as_ref())
            .filter(|value| !matches!(value, Value::Uninitialized))
            .cloned()
    }

    fn store_native_global_dimension(
        &mut self,
        key: &php_runtime::api::ArrayKey,
        mut replacement: Value,
    ) -> Result<bool, String> {
        self.ensure_native_global_references();
        let Some(name) = Self::native_global_name(key) else {
            return Ok(false);
        };
        if let Value::Reference(reference) = replacement {
            replacement = reference.get();
        }
        if let Some(Value::Reference(reference)) =
            self.inherited_globals.get(name.as_ref()).cloned()
        {
            let previous = reference.get();
            reference.set(replacement.clone());
            self.mark_rooted_container_dirty(&Value::Reference(reference));
            self.finalize_replaced_value(previous)?;
        } else {
            self.inherited_globals.insert(
                name.into_owned(),
                Value::Reference(php_runtime::api::ReferenceCell::new(replacement)),
            );
            self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
        }
        Ok(true)
    }

    fn unset_native_global_dimension(
        &mut self,
        key: &php_runtime::api::ArrayKey,
    ) -> Result<bool, String> {
        self.ensure_native_global_references();
        let Some(name) = Self::native_global_name(key) else {
            return Ok(false);
        };
        if let Some(Value::Reference(reference)) = self.inherited_globals.get(name.as_ref()) {
            self.invalidate_native_global_reference(reference.gc_debug_id())?;
        }
        let previous = self
            .inherited_globals
            .insert(name.into_owned(), Value::Uninitialized);
        if let Some(Value::Reference(reference)) = previous {
            self.finalize_replaced_value(reference.get())?;
        }
        self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
        Ok(true)
    }

    fn reference_native_global_dimension(
        &mut self,
        key: &php_runtime::api::ArrayKey,
    ) -> Result<Option<php_runtime::api::ReferenceCell>, String> {
        self.ensure_native_global_references();
        let Some(name) = Self::native_global_name(key) else {
            return Ok(None);
        };
        if let Some(Value::Reference(reference)) = self.inherited_globals.get(name.as_ref()) {
            return Ok(Some(reference.clone()));
        }
        let reference = php_runtime::api::ReferenceCell::new(Value::Null);
        self.inherited_globals
            .insert(name.into_owned(), Value::Reference(reference.clone()));
        self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
        Ok(Some(reference))
    }

    fn direct_value_index(encoded: i64) -> Option<usize> {
        let index = php_jit::jit_decode_runtime_value(encoded)?;
        let index = index.checked_sub(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)? as usize;
        (index < php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY).then_some(index)
    }

    /// Moves one newly constructed PHP array into the canonical native array
    /// plane at a call-frame boundary. This is an ownership transfer, not a
    /// shadow view of a retained `PhpArray`: the direct slot and its entries
    /// become the sole representation consumed by optimizing code.
    #[track_caller]
    fn encode_direct_array_value(
        &mut self,
        array: php_runtime::api::PhpArray,
    ) -> Result<i64, String> {
        let mut entries: Vec<php_jit::JitNativeDirectArrayEntry> = Vec::with_capacity(array.len());
        for (key, value) in array.iter() {
            let key = match key {
                php_runtime::api::ArrayKey::Int(key) => self.encode(Value::Int(key)),
                php_runtime::api::ArrayKey::String(key) => self.encode(Value::String(key.clone())),
            };
            let key = match key {
                Ok(key) => key,
                Err(error) => {
                    for entry in entries {
                        let _ = self.release(entry.key);
                        let _ = self.release(entry.value);
                    }
                    return Err(error);
                }
            };
            let value = match self.encode(value.clone()) {
                Ok(value) => value,
                Err(error) => {
                    let _ = self.release(key);
                    for entry in entries {
                        let _ = self.release(entry.key);
                        let _ = self.release(entry.value);
                    }
                    return Err(error);
                }
            };
            entries.push(php_jit::JitNativeDirectArrayEntry { key, value });
        }

        let (start, capacity) = match self.reserve_direct_array_entries(entries.len()) {
            Ok(reserved) => reserved,
            Err(error) => {
                for entry in entries {
                    let _ = self.release(entry.key);
                    let _ = self.release(entry.value);
                }
                return Err(error);
            }
        };
        self.direct_array_entries[start..start + entries.len()].copy_from_slice(&entries);

        let index = if *self.direct_value_free_head != php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE {
            let index = *self.direct_value_free_head as usize;
            let slot = self
                .direct_value_slots
                .get(index)
                .ok_or_else(|| "direct native value free-list entry is missing".to_owned())?;
            *self.direct_value_free_head = slot.reserved;
            index
        } else {
            let index = usize::try_from(*self.direct_value_next)
                .map_err(|_| "direct native value index overflow".to_owned())?;
            if index >= self.direct_value_slots.len() {
                self.free_direct_array_entries(start, capacity);
                for entry in entries {
                    let _ = self.release(entry.key);
                    let _ = self.release(entry.value);
                }
                return Err(format!(
                    "direct native value arena exhausted at {} slots",
                    index.saturating_add(1)
                ));
            }
            *self.direct_value_next = u32::try_from(index + 1)
                .map_err(|_| "direct native value index overflow".to_owned())?;
            index
        };
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
            flags: php_jit::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
            reserved: u32::try_from(capacity).unwrap_or(u32::MAX),
            payload: entries.len() as u64,
            aux: self.direct_array_entries[start..].as_ptr() as usize as u64,
        };
        self.record_direct_array_materialization(entries.len(), std::panic::Location::caller());
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .ok_or_else(|| "direct native value handle overflow".to_owned())?;
        Ok((php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG | u64::from(runtime_index)) as i64)
    }

    fn reserve_direct_array_entries(&mut self, length: usize) -> Result<(usize, usize), String> {
        let capacity = length
            .max(php_jit::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize)
            .next_power_of_two();
        let bucket = capacity.trailing_zeros() as usize;
        let head = self.direct_array_free_heads[bucket];
        if head != php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE {
            let start = head as usize;
            let next = self
                .direct_array_entries
                .get(start)
                .map(|entry| entry.key as u32)
                .ok_or_else(|| "direct native array free-list entry is missing".to_owned())?;
            self.direct_array_free_heads[bucket] = next;
            return Ok((start, capacity));
        }
        let start = usize::try_from(*self.direct_array_next)
            .map_err(|_| "direct native array entry index overflow".to_owned())?;
        let end = start
            .checked_add(capacity)
            .ok_or_else(|| "direct native array entry range overflow".to_owned())?;
        if end > self.direct_array_entries.len() {
            let reusable = self
                .direct_array_free_heads
                .iter()
                .filter(|head| **head != php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE)
                .count();
            return Err(format!(
                "direct native array arena exhausted at {end} entries (next={start}, requested={capacity}, reusable_buckets={reusable})"
            ));
        }
        *self.direct_array_next = u32::try_from(end)
            .map_err(|_| "direct native array entry index overflow".to_owned())?;
        Ok((start, capacity))
    }

    fn free_direct_array_entries(&mut self, start: usize, capacity: usize) {
        if capacity == 0 {
            return;
        }
        if !capacity.is_power_of_two() {
            return;
        }
        let Ok(start_u32) = u32::try_from(start) else {
            return;
        };
        let bucket = capacity.trailing_zeros() as usize;
        if bucket >= self.direct_array_free_heads.len() || start >= self.direct_array_entries.len()
        {
            return;
        }
        let previous = self.direct_array_free_heads[bucket];
        self.direct_array_entries[start].key = i64::from(previous);
        self.direct_array_entries[start].value = 0;
        self.direct_array_free_heads[bucket] = start_u32;
    }

    fn replace_direct_array(
        &mut self,
        index: usize,
        array: php_runtime::api::PhpArray,
    ) -> Result<(), String> {
        let old = *self
            .direct_value_slots
            .get(index)
            .filter(|slot| {
                slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
            })
            .ok_or_else(|| format!("direct native array {index} is missing"))?;
        let source = array
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>();
        let mut encoded_entries: Vec<php_jit::JitNativeDirectArrayEntry> =
            Vec::with_capacity(source.len());
        for (key, value) in source {
            let key = match key {
                php_runtime::api::ArrayKey::Int(key) => self.encode(Value::Int(key)),
                php_runtime::api::ArrayKey::String(key) => self.encode(Value::String(key)),
            }?;
            let value = match self.encode(value) {
                Ok(value) => value,
                Err(error) => {
                    let _ = self.release(key);
                    for entry in encoded_entries.drain(..) {
                        let _ = self.release(entry.key);
                        let _ = self.release(entry.value);
                    }
                    return Err(error);
                }
            };
            encoded_entries.push(php_jit::JitNativeDirectArrayEntry { key, value });
        }

        let base = self.direct_array_entries.as_ptr() as usize;
        let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
        let old_start = usize::try_from(old.aux)
            .unwrap_or(base)
            .saturating_sub(base)
            / entry_size;
        let old_length = usize::try_from(old.payload).unwrap_or(0);
        let old_children = self
            .direct_array_entries
            .get(old_start..old_start.saturating_add(old_length))
            .unwrap_or_default()
            .iter()
            .flat_map(|entry| [entry.key, entry.value])
            .collect::<Vec<_>>();
        let moved = encoded_entries.len() > old.reserved as usize;
        let (start, capacity) = if !moved {
            (old_start, old.reserved as usize)
        } else {
            self.reserve_direct_array_entries(encoded_entries.len())?
        };
        self.direct_array_entries[start..start + encoded_entries.len()]
            .copy_from_slice(&encoded_entries);
        let slot = &mut self.direct_value_slots[index];
        slot.flags = php_jit::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION;
        slot.reserved = u32::try_from(capacity).unwrap_or(u32::MAX);
        slot.payload = encoded_entries.len() as u64;
        slot.aux = self.direct_array_entries[start..].as_ptr() as usize as u64;
        if moved {
            self.free_direct_array_entries(old_start, old.reserved as usize);
        }
        for child in old_children {
            self.release(child)?;
        }
        Ok(())
    }

    fn decode_direct_array(&self, index: usize) -> Result<Value, String> {
        let slot = self
            .direct_value_slots
            .get(index)
            .filter(|slot| slot.refcount != 0)
            .ok_or_else(|| format!("direct native value {index} is missing"))?;
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY {
            return Err(format!("direct native value {index} is not an array"));
        }
        let length = usize::try_from(slot.payload)
            .map_err(|_| format!("direct native array {index} length overflow"))?;
        let base = self.direct_array_entries.as_ptr() as usize;
        let address = usize::try_from(slot.aux)
            .map_err(|_| format!("direct native array {index} address overflow"))?;
        let byte_offset = address
            .checked_sub(base)
            .ok_or_else(|| format!("direct native array {index} address is outside its arena"))?;
        let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
        if byte_offset % entry_size != 0 {
            return Err(format!("direct native array {index} address is unaligned"));
        }
        let start = byte_offset / entry_size;
        let entries = self
            .direct_array_entries
            .get(start..start.saturating_add(length))
            .ok_or_else(|| format!("direct native array {index} entries are outside its arena"))?;
        let mut array = php_runtime::api::PhpArray::with_capacity(length);
        for entry in entries {
            let key = self.decode(entry.key)?;
            let key = php_runtime::api::ArrayKey::from_value(&key)
                .ok_or_else(|| format!("direct native array {index} has an invalid key"))?;
            array.insert(key, self.decode(entry.value)?);
        }
        Ok(Value::Array(array))
    }

    fn decode_direct_value(&self, index: usize) -> Result<Value, String> {
        let slot = self
            .direct_value_slots
            .get(index)
            .filter(|slot| slot.refcount != 0)
            .ok_or_else(|| format!("direct native value {index} is missing"))?;
        if matches!(
            slot.kind,
            php_jit::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY
                | php_jit::JIT_NATIVE_VALUE_VIEW_BORROWED_REFERENCE_ARRAY
        ) {
            let array = php_runtime::api::PhpArray::clone_from_native_storage_refcount(
                slot.payload as usize,
            )
            .ok_or_else(|| format!("shared native array {index} storage is unavailable"))?;
            return Ok(Value::Array(array));
        }
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_STRING {
            return self.decode_direct_array(index);
        }
        let length = usize::try_from(slot.payload)
            .map_err(|_| format!("direct native string {index} length overflow"))?;
        let base = self.direct_string_bytes.as_ptr() as usize;
        let address = usize::try_from(slot.aux)
            .map_err(|_| format!("direct native string {index} address overflow"))?;
        let start = address
            .checked_sub(base)
            .ok_or_else(|| format!("direct native string {index} is outside its arena"))?;
        let bytes = self
            .direct_string_bytes
            .get(start..start.saturating_add(length))
            .ok_or_else(|| format!("direct native string {index} bytes are outside its arena"))?;
        Ok(Value::String(PhpString::from_bytes(bytes.to_vec())))
    }

    fn decode(&self, encoded: i64) -> Result<Value, String> {
        if let Some(constant) = php_jit::jit_decode_constant(encoded) {
            if constant == u32::MAX {
                return Ok(Value::Null);
            }
            if constant == php_jit::JIT_VALUE_UNINITIALIZED {
                return Ok(Value::Uninitialized);
            }
            if constant == php_jit::JIT_VALUE_FALSE {
                return Ok(Value::Bool(false));
            }
            if constant == php_jit::JIT_VALUE_TRUE {
                return Ok(Value::Bool(true));
            }
            let constant_index = constant as usize;
            let cache_key = (self.current_dynamic_unit, constant_index);
            if let Some(value) = self.decoded_constant_cache.borrow().get(&cache_key) {
                return Ok(value.clone());
            }
            let constant = self
                .unit
                .constants
                .get(constant_index)
                .ok_or_else(|| {
                    format!(
                        "native constant {constant} is missing from active unit {} (dynamic={:?}, constants={}, source={})",
                        self.unit.id.raw(),
                        self.current_dynamic_unit,
                        self.unit.constants.len(),
                        self.unit
                            .files
                            .first()
                            .map_or("<unknown>", |file| file.path.as_str()),
                    )
                })?;
            // Constants embedded in native operands can still require the
            // active request context (for example a runtime-defined constant
            // used as a default argument in a bounded large-unit call graph).
            let value = native_runtime_constant_value(self, constant)?;
            self.decoded_constant_cache
                .borrow_mut()
                .insert(cache_key, value.clone());
            return Ok(value);
        }
        if let Some(index) = php_jit::jit_decode_runtime_value(encoded) {
            if let Some(direct) = Self::direct_value_index(encoded) {
                return self.decode_direct_value(direct);
            }
            return match self.values.get(index as usize).and_then(Option::as_ref) {
                Some(NativeStoredValue::Php(value)) => Ok(value.clone()),
                Some(NativeStoredValue::PreparedClosure(closure)) => {
                    Ok(Value::Callable(closure.callable.clone()))
                }
                Some(NativeStoredValue::GlobalsProxy) => {
                    Ok(self.materialize_native_globals_array())
                }
                Some(
                    NativeStoredValue::ArrayIterator(_)
                    | NativeStoredValue::Iterator(_)
                    | NativeStoredValue::GeneratorIterator(_),
                ) => Err(format!(
                    "native runtime value {index} is a foreach iterator"
                )),
                None => Err(format!("native runtime value {index} is missing")),
            };
        }
        Ok(Value::Int(encoded))
    }

    fn encode_prepared_closure(
        &mut self,
        callable: Box<php_runtime::api::CallableValue>,
    ) -> Result<i64, String> {
        let php_runtime::api::CallableValue::Closure(closure) = callable.as_ref() else {
            return self.encode_stored_value(NativeStoredValue::Php(Value::Callable(callable)));
        };
        let identity = NativeValueIdentity::Closure(closure.id);
        if let Some(index) = self.interned_value_handles.get(&identity).copied() {
            self.retain_runtime_value_index(index as usize)?;
            return Ok(php_jit::jit_encode_typed_runtime_value(
                index,
                php_jit::JIT_VALUE_RUNTIME_CALLABLE_TAG,
            ));
        }
        let capture_values = closure
            .captures
            .iter()
            .map(|capture| {
                if capture.name.eq_ignore_ascii_case("this")
                    && let Some(object) = &closure.bound_this
                {
                    self.encode(Value::Object(object.clone()))
                } else if let Some(reference) = capture.reference() {
                    self.encode(Value::Reference(reference))
                } else {
                    self.encode(capture.value().cloned().unwrap_or(Value::Null))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        match self.encode_stored_value(NativeStoredValue::PreparedClosure(Box::new(
            NativePreparedClosure {
                callable,
                captures: capture_values.clone().into_boxed_slice(),
            },
        ))) {
            Ok(encoded) => Ok(encoded),
            Err(error) => {
                for capture in capture_values {
                    let _ = self.release(capture);
                }
                Err(error)
            }
        }
    }

    #[track_caller]
    fn encode(&mut self, value: Value) -> Result<i64, String> {
        let value = match value {
            Value::Array(array) => return self.encode_direct_array_value(array),
            Value::Callable(callable)
                if matches!(
                    callable.as_ref(),
                    php_runtime::api::CallableValue::Closure(_)
                ) =>
            {
                return self.encode_prepared_closure(callable);
            }
            value => value,
        };
        match &value {
            Value::Null => return Ok(php_jit::jit_encode_constant(u32::MAX)),
            Value::Bool(false) => {
                return Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE));
            }
            Value::Bool(true) => {
                return Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE));
            }
            Value::Int(value)
                if php_jit::jit_decode_constant(*value).is_none()
                    && php_jit::jit_decode_runtime_value(*value).is_none() =>
            {
                return Ok(*value);
            }
            _ => {}
        }
        self.encode_stored_value(NativeStoredValue::Php(value))
    }

    /// Encodes one argument owned by the baseline-native compatibility
    /// binder. Arrays intentionally remain `PhpArray` handles here: the
    /// baseline callee performs PHP COW semantics, while an optimizing entry
    /// must admit or convert the value once at its version boundary. This
    /// avoids recursively rebuilding an entire array tree inside a generic
    /// callable helper.
    fn encode_baseline_call_value(&mut self, value: Value) -> Result<i64, String> {
        match value {
            Value::Array(array) => {
                self.encode_stored_value(NativeStoredValue::Php(Value::Array(array)))
            }
            value => self.encode(value),
        }
    }

    fn encode_stored_value(&mut self, value: NativeStoredValue) -> Result<i64, String> {
        let tag = stored_value_tag(&value);
        let kind = stored_value_kind(&value);
        let native_slot = stored_value_slot(&value);
        let identity = stored_value_identity(&value);
        if let Some(index) = identity
            .as_ref()
            .and_then(|identity| self.interned_value_handles.get(identity).copied())
        {
            let index = index as usize;
            self.retain_runtime_value_index(index)?;
            return Ok(php_jit::jit_encode_typed_runtime_value(index as u32, tag));
        }
        if let Some(index) = self.free_value_slots.pop() {
            let slot = self
                .values
                .get_mut(index as usize)
                .ok_or_else(|| format!("native free value slot {index} is missing"))?;
            if slot.is_some()
                || self
                    .value_slots
                    .get(index as usize)
                    .is_none_or(|slot| slot.refcount != 0)
            {
                return Err(format!("native free value slot {index} is still live"));
            }
            *slot = Some(value);
            self.value_slots[index as usize] = native_slot;
            self.object_property_caches[index as usize] = None;
            if let Some(identity) = identity {
                self.interned_value_handles.insert(identity, index);
            }
            self.record_value_table_reuse(kind);
            return Ok(php_jit::jit_encode_typed_runtime_value(index, tag));
        }
        let index = u32::try_from(self.values.len())
            .map_err(|_| "native runtime value table exhausted".to_owned())?;
        if self.value_slots.len() == self.value_slots.capacity() {
            return Err(format!(
                "native runtime value plane exhausted at {} entries",
                self.value_slots.len()
            ));
        }
        self.values.push(Some(value));
        self.value_slots.push(native_slot);
        self.object_property_caches.push(None);
        if let Some(identity) = identity {
            self.interned_value_handles.insert(identity, index);
        }
        self.record_value_table_allocation(self.values.len(), kind);
        Ok(php_jit::jit_encode_typed_runtime_value(index, tag))
    }

    /// Give a native callee an independently owned argument without routing
    /// every value through `Value::clone` and a second arena lookup.
    ///
    /// Runtime handles are request-wide, not unit-local. Objects, references,
    /// strings, callables, resources, generators, fibers, and stored scalars
    /// can therefore share the existing slot by incrementing its arena
    /// refcount. Arrays need a distinct `PhpArray` facade so a write in the
    /// callee triggers the runtime's copy-on-write separation instead of
    /// mutating the caller's facade in place. Unit-local constant operands are
    /// materialized before an external-unit switch because their indexes are
    /// interpreted against the active unit.
    fn duplicate_native_call_argument(&mut self, encoded: i64) -> Result<i64, String> {
        if let Some(index) = Self::direct_value_index(encoded) {
            let refcount = &mut self
                .direct_value_slots
                .get_mut(index)
                .ok_or_else(|| format!("direct native value {index} is missing"))?
                .refcount;
            *refcount = refcount
                .checked_add(1)
                .ok_or_else(|| format!("direct native value {index} refcount overflow"))?;
            return Ok(encoded);
        }
        if let Some(index) = php_jit::jit_decode_runtime_value(encoded) {
            let index = index as usize;
            if matches!(
                self.values.get(index).and_then(Option::as_ref),
                Some(NativeStoredValue::GlobalsProxy)
            ) {
                return self.encode(self.materialize_native_globals_array());
            }
            match self.values.get(index).and_then(Option::as_ref) {
                Some(NativeStoredValue::Php(Value::Array(array))) => {
                    // This is a baseline/dynamic call-frame duplication, not
                    // an optimizing-version entry.  Preserve the canonical
                    // PhpArray COW handle here.  Re-encoding through `encode`
                    // recursively rebuilt the complete array in the direct
                    // arena on every call and made WordPress hook dispatch
                    // consume hundreds of thousands of duplicate entries.
                    return self.encode_baseline_call_value(Value::Array(array.clone()));
                }
                Some(NativeStoredValue::Php(_) | NativeStoredValue::PreparedClosure(_)) => {}
                Some(NativeStoredValue::GlobalsProxy) => unreachable!(),
                Some(
                    NativeStoredValue::ArrayIterator(_)
                    | NativeStoredValue::Iterator(_)
                    | NativeStoredValue::GeneratorIterator(_),
                ) => {
                    return Err(format!(
                        "native runtime value {index} is a foreach iterator"
                    ));
                }
                None => return Err(format!("native runtime value {index} is missing")),
            }
            self.retain_runtime_value_index(index)?;
            return Ok(encoded);
        }
        if let Some(constant) = php_jit::jit_decode_constant(encoded)
            && !matches!(
                constant,
                u32::MAX
                    | php_jit::JIT_VALUE_UNINITIALIZED
                    | php_jit::JIT_VALUE_FALSE
                    | php_jit::JIT_VALUE_TRUE
            )
        {
            return self.decode(encoded).and_then(|value| self.encode(value));
        }
        Ok(encoded)
    }

    fn prepared_closure_captures(&self, encoded: i64) -> Option<&[i64]> {
        let index = php_jit::jit_decode_runtime_value(encoded)? as usize;
        match self.values.get(index)?.as_ref()? {
            NativeStoredValue::PreparedClosure(closure) => Some(&closure.captures),
            _ => None,
        }
    }

    /// Move an owned result from the active external unit back to its caller.
    /// Runtime handles already belong to the request-wide arena and need no
    /// clone or replacement slot. Only unit-indexed constants and an unowned
    /// closure require translation.
    fn transfer_external_return(&mut self, encoded: i64, owner_unit: usize) -> Result<i64, String> {
        if Self::direct_value_index(encoded).is_some() {
            // Direct arrays may still contain constants indexed by the
            // callee's IrUnit. Rewrite only those embedded constants while
            // the callee unit is active; otherwise the caller can interpret
            // the same numeric index as an unrelated value. The native
            // array slots remain authoritative and no Rust `PhpArray` is
            // reconstructed at this boundary.
            self.stabilize_direct_array_for_cross_unit(encoded)?;
            return Ok(encoded);
        }
        if let Some(index) = php_jit::jit_decode_runtime_value(encoded) {
            let needs_closure_owner = matches!(
                self.values.get(index as usize).and_then(Option::as_ref),
                Some(NativeStoredValue::Php(Value::Callable(callable)))
                    if matches!(
                        callable.as_ref(),
                        php_runtime::api::CallableValue::Closure(closure)
                            if closure.context.owner_unit.is_none()
                    )
            ) || matches!(
                self.values.get(index as usize).and_then(Option::as_ref),
                Some(NativeStoredValue::PreparedClosure(closure))
                    if matches!(
                        closure.callable.as_ref(),
                        php_runtime::api::CallableValue::Closure(closure)
                            if closure.context.owner_unit.is_none()
                    )
            );
            if !needs_closure_owner {
                return Ok(encoded);
            }
            let value = self.decode(encoded)?;
            let transferred = self.encode(native_external_return_value(value, owner_unit))?;
            self.release(encoded)?;
            return Ok(transferred);
        }
        if let Some(constant) = php_jit::jit_decode_constant(encoded)
            && !matches!(
                constant,
                u32::MAX
                    | php_jit::JIT_VALUE_UNINITIALIZED
                    | php_jit::JIT_VALUE_FALSE
                    | php_jit::JIT_VALUE_TRUE
            )
        {
            let value = self.decode(encoded)?;
            return self.encode(native_external_return_value(value, owner_unit));
        }
        Ok(encoded)
    }

    fn retain(&mut self, encoded: i64) -> Result<(), String> {
        if let Some(index) = Self::direct_value_index(encoded) {
            let refcount = &mut self
                .direct_value_slots
                .get_mut(index)
                .ok_or_else(|| format!("direct native value {index} is missing"))?
                .refcount;
            *refcount = refcount
                .checked_add(1)
                .ok_or_else(|| format!("direct native value {index} refcount overflow"))?;
            return Ok(());
        }
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Ok(());
        };
        let index = index as usize;
        if self.values.get(index).and_then(Option::as_ref).is_none() {
            return Err(format!("native runtime value {index} is missing"));
        }
        let refcount = &mut self
            .value_slots
            .get_mut(index)
            .ok_or_else(|| format!("native runtime value {index} has no slot"))?
            .refcount;
        *refcount = refcount
            .checked_add(1)
            .ok_or_else(|| format!("native runtime value {index} refcount overflow"))?;
        Ok(())
    }

    fn native_scalar_encoding(&mut self, value: &Value) -> Option<i64> {
        matches!(
            value,
            Value::Null | Value::Bool(_) | Value::Int(_) | Value::Uninitialized
        )
        .then(|| self.encode(value.clone()).ok())
        .flatten()
    }

    /// Classify an encoded PHP value without cloning it out of the request
    /// arena. Immediates are always plain; runtime iterator/control records
    /// are deliberately excluded because they are not PHP local values.
    fn php_handle_is_reference(&self, encoded: i64) -> Option<bool> {
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Some(false);
        };
        match self.values.get(index as usize).and_then(Option::as_ref) {
            Some(NativeStoredValue::Php(Value::Reference(_))) => Some(true),
            Some(
                NativeStoredValue::Php(_)
                | NativeStoredValue::PreparedClosure(_)
                | NativeStoredValue::GlobalsProxy,
            ) => Some(false),
            Some(
                NativeStoredValue::ArrayIterator(_)
                | NativeStoredValue::Iterator(_)
                | NativeStoredValue::GeneratorIterator(_),
            )
            | None => None,
        }
    }

    /// Borrow a plain PHP local through its existing opaque handle. A local
    /// read owns one reference to its result, so the arena refcount is bumped
    /// instead of decoding, cloning, and allocating an equivalent handle.
    fn retain_plain_php_handle(&mut self, encoded: i64) -> Result<Option<i64>, String> {
        let Some(index) = self.plain_php_storage_index(encoded).flatten() else {
            return Ok(None);
        };
        self.retain_runtime_value_index(index)?;
        Ok(Some(encoded))
    }

    /// Classifies a plain PHP value without repeatedly decoding its arena ID.
    /// `Some(None)` denotes an immediate or immutable constant handle;
    /// `Some(Some(index))` denotes a non-reference PHP arena value.
    fn plain_php_storage_index(&self, encoded: i64) -> Option<Option<usize>> {
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Some(None);
        };
        let index = index as usize;
        match self.values.get(index).and_then(Option::as_ref) {
            Some(NativeStoredValue::Php(Value::Reference(_)))
            | Some(NativeStoredValue::GlobalsProxy)
            | Some(
                NativeStoredValue::ArrayIterator(_)
                | NativeStoredValue::Iterator(_)
                | NativeStoredValue::GeneratorIterator(_),
            )
            | None => None,
            Some(NativeStoredValue::Php(_) | NativeStoredValue::PreparedClosure(_)) => {
                Some(Some(index))
            }
        }
    }

    fn borrowed_php_value(&self, encoded: i64) -> Option<&Value> {
        let index = php_jit::jit_decode_runtime_value(encoded)? as usize;
        match self.values.get(index).and_then(Option::as_ref) {
            Some(NativeStoredValue::Php(value)) => Some(value),
            Some(NativeStoredValue::PreparedClosure(_) | NativeStoredValue::GlobalsProxy) => None,
            Some(
                NativeStoredValue::ArrayIterator(_)
                | NativeStoredValue::Iterator(_)
                | NativeStoredValue::GeneratorIterator(_),
            )
            | None => None,
        }
    }

    fn retain_runtime_value_index(&mut self, index: usize) -> Result<(), String> {
        let refcount = &mut self
            .value_slots
            .get_mut(index)
            .ok_or_else(|| format!("native runtime value {index} has no slot"))?
            .refcount;
        *refcount = refcount
            .checked_add(1)
            .ok_or_else(|| format!("native runtime value {index} refcount overflow"))?;
        Ok(())
    }

    fn replace_plain_php_handle(&mut self, current: i64, value: i64) -> Result<Option<()>, String> {
        let Some(current_index) = self.plain_php_storage_index(current) else {
            return Ok(None);
        };
        let Some(value_index) = self.plain_php_storage_index(value) else {
            return Ok(None);
        };
        if let Some(index) = value_index {
            self.retain_runtime_value_index(index)?;
        }
        if let Some(index) = current_index {
            self.release_runtime_value_index(index)?;
        }
        Ok(Some(()))
    }

    fn release(&mut self, encoded: i64) -> Result<(), String> {
        if let Some(index) = Self::direct_value_index(encoded) {
            return self.release_direct_value_index(index);
        }
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Ok(());
        };
        self.release_runtime_value_index(index as usize)
    }

    fn release_direct_value_index(&mut self, index: usize) -> Result<(), String> {
        let reached_zero = {
            let slot = self
                .direct_value_slots
                .get_mut(index)
                .ok_or_else(|| format!("direct native value {index} is missing"))?;
            if slot.refcount == 0 {
                return Err(format!("direct native value {index} was already released"));
            }
            slot.refcount -= 1;
            slot.refcount == 0
        };
        if !reached_zero {
            return Ok(());
        }
        let slot = self.direct_value_slots[index];
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY
            && !php_runtime::api::PhpArray::release_native_storage_refcount(slot.payload as usize)
        {
            return Err(format!(
                "shared native array {index} storage was already released"
            ));
        }
        let (children, freed_array_range) =
            if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FOREACH {
                (vec![slot.payload as i64], None)
            } else if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY {
                let length = usize::try_from(slot.payload).unwrap_or(0);
                let base = self.direct_array_entries.as_ptr() as usize;
                let address = usize::try_from(slot.aux).unwrap_or(base);
                let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
                let start = address.saturating_sub(base) / entry_size;
                (
                    self.direct_array_entries
                        .get(start..start.saturating_add(length))
                        .unwrap_or_default()
                        .iter()
                        .flat_map(|entry| [entry.key, entry.value])
                        .collect::<Vec<_>>(),
                    Some((start, slot.reserved as usize)),
                )
            } else {
                (Vec::new(), None)
            };
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            reserved: *self.direct_value_free_head,
            ..php_jit::JitNativeValueSlot::default()
        };
        *self.direct_value_free_head = index as u32;
        if let Some((start, capacity)) = freed_array_range {
            self.free_direct_array_entries(start, capacity);
        }
        for child in children {
            self.release(child)?;
        }
        Ok(())
    }

    fn release_runtime_value_index(&mut self, index: usize) -> Result<(), String> {
        let reached_zero = {
            let refcount = &mut self
                .value_slots
                .get_mut(index)
                .ok_or_else(|| format!("native runtime value {index} has no slot"))?
                .refcount;
            if *refcount == 0 {
                return Err(format!("native runtime value {index} was already released"));
            }
            *refcount -= 1;
            *refcount == 0
        };
        if reached_zero {
            self.record_release_to_zero();
            self.invalidate_object_property_cache(index)?;
            let value = self
                .values
                .get_mut(index)
                .ok_or_else(|| format!("native runtime value {index} is missing"))?
                .take();
            if let Some(identity) = value.as_ref().and_then(stored_value_identity)
                && self.interned_value_handles.get(&identity) == Some(&(index as u32))
            {
                self.interned_value_handles.remove(&identity);
            }
            self.value_slots[index] = php_jit::JitNativeValueSlot::default();
            match value {
                Some(NativeStoredValue::Php(Value::Object(object))) => {
                    let class_name = object.class_name();
                    // Root membership is observable only for objects whose class
                    // can run user code during destruction. Scanning the complete
                    // request graph for ordinary objects cannot change PHP output
                    // and made every short-lived WordPress value pay for the
                    // largest live global array.
                    if self.object_has_native_destructor(&class_name) {
                        let uniquely_owned = object.gc_refcount_estimate() == 1;
                        if uniquely_owned {
                            self.record_object_release_root_check(true);
                        }
                        if uniquely_owned || !self.object_is_request_rooted(object.id()) {
                            self.run_object_destructor(object)?;
                        }
                    }
                }
                Some(NativeStoredValue::PreparedClosure(closure)) => {
                    for capture in closure.captures {
                        self.release(capture)?;
                    }
                }
                _ => {}
            }
            self.free_value_slots.push(index as u32);
        }
        Ok(())
    }

    fn release_if_live(&mut self, encoded: i64) -> Result<(), String> {
        if let Some(index) = Self::direct_value_index(encoded) {
            if self.direct_value_slots[index].refcount == 0 {
                return Ok(());
            }
            return self.release_direct_value_index(index);
        }
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Ok(());
        };
        if self
            .value_slots
            .get(index as usize)
            .is_some_and(|slot| slot.refcount == 0)
        {
            return Ok(());
        }
        self.release(encoded)
    }

    fn object_is_request_rooted(&mut self, object_id: u64) -> bool {
        if self.root_index.is_dirty() {
            let reason = self.root_index.last_reason().as_str();
            let roots = self.request_root_values();
            self.root_index.synchronize(&roots);
            self.record_object_release_root_check(false);
            self.record_root_rebuild_reason(reason);
        } else {
            self.record_object_release_root_check(true);
        }
        if self.root_index.contains(object_id) {
            return true;
        }
        self.live_native_values_contain_object(object_id)
    }

    fn live_native_values_contain_object(&self, object_id: u64) -> bool {
        self.values.iter().flatten().any(|stored| match stored {
            NativeStoredValue::Php(value) => values_contain_object([value], object_id),
            NativeStoredValue::PreparedClosure(prepared) => {
                let php_runtime::api::CallableValue::Closure(closure) = prepared.callable.as_ref()
                else {
                    return false;
                };
                closure
                    .bound_this
                    .as_ref()
                    .is_some_and(|object| object.id() == object_id)
                    || closure.captures.iter().any(|capture| {
                        capture
                            .value()
                            .is_some_and(|value| values_contain_object([value], object_id))
                            || capture.reference().is_some_and(|reference| {
                                let value = reference.get();
                                values_contain_object([&value], object_id)
                            })
                    })
            }
            NativeStoredValue::GlobalsProxy => false,
            NativeStoredValue::ArrayIterator(iterator) => {
                values_contain_object(iterator.source.iter().map(|(_, value)| value), object_id)
            }
            NativeStoredValue::Iterator(iterator) => {
                values_contain_object(
                    iterator
                        .entries
                        .iter()
                        .flat_map(|(key, value)| [key, value]),
                    object_id,
                ) || iterator
                    .live_object
                    .as_ref()
                    .is_some_and(|object| object.id() == object_id)
                    || iterator
                        .user_iterator
                        .as_ref()
                        .is_some_and(|object| object.id() == object_id)
            }
            NativeStoredValue::GeneratorIterator(iterator) => iterator
                .delegation
                .as_ref()
                .is_some_and(|delegation| match delegation {
                    NativeGeneratorDelegation::Array { entries, .. } => values_contain_object(
                        entries.iter().flat_map(|(key, value)| [key, value]),
                        object_id,
                    ),
                    NativeGeneratorDelegation::Generator { .. } => false,
                }),
        })
    }

    fn run_object_destructor(&mut self, object: php_runtime::api::ObjectRef) -> Result<(), String> {
        if self
            .destroyed_objects
            .get(&object.id())
            .is_some_and(WeakObjectHandle::is_alive)
        {
            return Ok(());
        }
        self.destroyed_objects
            .insert(object.id(), object.weak_handle());
        let class_name = object.class_name();
        let receiver = self.encode(Value::Object(object))?;
        if let Some(function) = self
            .unit
            .classes
            .iter()
            .find(|class| class.name == normalize_class_name(&class_name))
            .and_then(|class| {
                class
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("__destruct"))
            })
            .map(|method| method.function)
        {
            let _ = invoke_native_method(self, function, &[receiver])?;
        } else if let Some((function, _)) = native_external_method(self, &class_name, "__destruct")
        {
            let _ = invoke_native_external_function(
                self,
                function,
                &[receiver],
                Some(class_name),
                self.unit.strict_types,
            )?;
        }
        self.release(receiver)
    }

    fn object_has_native_destructor(&self, class_name: &str) -> bool {
        self.unit
            .classes
            .iter()
            .find(|class| class.name == normalize_class_name(class_name))
            .is_some_and(|class| {
                class
                    .methods
                    .iter()
                    .any(|method| method.name.eq_ignore_ascii_case("__destruct"))
            })
            || native_external_method(self, class_name, "__destruct").is_some()
    }

    fn function_id(&self, name: &str) -> Option<php_ir::FunctionId> {
        self.unit
            .function_table
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
            .map(|entry| entry.function)
            .or_else(|| {
                self.dynamic_functions.get(name).copied().or_else(|| {
                    name.bytes()
                        .any(|byte| byte.is_ascii_uppercase())
                        .then(|| name.to_ascii_lowercase())
                        .and_then(|normalized| self.dynamic_functions.get(&normalized).copied())
                })
            })
    }

    fn visible_include_function_names(&self) -> Rc<NativeFunctionNameScope> {
        self.visible_function_names.clone()
    }

    fn publish_function_names(&mut self, names: impl IntoIterator<Item = String>) {
        self.visible_function_names =
            NativeFunctionNameScope::child(self.visible_function_names.clone(), names);
    }

    fn take_include_symbols(&mut self) -> NativeIncludeSymbols {
        self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
        NativeIncludeSymbols {
            deployment_functions: std::sync::Arc::clone(&self.deployment_functions),
            deployment_classes: std::sync::Arc::clone(&self.deployment_classes),
            external_functions: std::mem::take(&mut self.external_functions),
            external_class_units: std::mem::take(&mut self.external_class_units),
            external_signature_epoch: self.external_signature_epoch,
            dynamic_units: std::mem::take(&mut self.dynamic_units),
            dynamic_classes: std::mem::take(&mut self.dynamic_classes),
            class_aliases: std::mem::take(&mut self.class_aliases),
            autoload_callbacks: std::mem::take(&mut self.autoload_callbacks),
            shutdown_callbacks: std::mem::take(&mut self.shutdown_callbacks),
            static_properties: std::mem::take(&mut self.static_properties),
            static_locals: std::mem::take(&mut self.static_locals),
            enum_cases: std::mem::take(&mut self.enum_cases),
            destroyed_objects: std::mem::take(&mut self.destroyed_objects),
            error_reporting: Some(self.error_reporting),
            display_errors: Some(self.display_errors),
            error_handlers: std::mem::take(&mut self.error_handlers),
            exception_handlers: std::mem::take(&mut self.exception_handlers),
            last_error: self.last_error.take(),
        }
    }

    fn restore_include_symbols(&mut self, symbols: NativeIncludeSymbols) {
        self.deployment_functions = symbols.deployment_functions;
        self.deployment_classes = symbols.deployment_classes;
        self.external_functions = symbols.external_functions;
        self.external_class_units = symbols.external_class_units;
        self.external_signature_epoch = symbols.external_signature_epoch;
        self.dynamic_units = symbols.dynamic_units;
        self.dynamic_classes = symbols.dynamic_classes;
        self.class_aliases = symbols.class_aliases;
        self.autoload_callbacks = symbols.autoload_callbacks;
        self.shutdown_callbacks = symbols.shutdown_callbacks;
        self.static_properties = symbols.static_properties;
        self.static_locals = symbols.static_locals;
        self.enum_cases = symbols.enum_cases;
        self.destroyed_objects = symbols.destroyed_objects;
        if let Some(error_reporting) = symbols.error_reporting {
            self.error_reporting = error_reporting;
        }
        if let Some(display_errors) = symbols.display_errors {
            self.display_errors = display_errors;
        }
        self.error_handlers = symbols.error_handlers;
        self.exception_handlers = symbols.exception_handlers;
        self.last_error = symbols.last_error;
        self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
    }

    fn external_function(&self, name: &str) -> Option<NativeDynamicFunction> {
        self.external_functions.get(name).copied().or_else(|| {
            let normalized = name
                .bytes()
                .any(|byte| byte.is_ascii_uppercase())
                .then(|| name.to_ascii_lowercase());
            normalized
                .as_deref()
                .and_then(|normalized| self.external_functions.get(normalized).copied())
                .or_else(|| {
                    let normalized = normalized.as_deref().unwrap_or(name);
                    self.deployment_functions
                        .get(normalized)
                        .copied()
                        .map(|function| NativeDynamicFunction { unit: 0, function })
                })
        })
    }

    fn can_invoke_external_in_place(&self, target: NativeDynamicFunction) -> bool {
        self.dynamic_units.get(target.unit).is_some_and(|package| {
            package
                .compiled
                .unit()
                .functions
                .get(target.function.index())
                .is_some()
        })
    }

    fn with_active_dynamic_unit<R>(
        &mut self,
        unit: usize,
        operation: impl FnOnce(&mut Self) -> R,
    ) -> Result<R, String> {
        let compiled = self
            .dynamic_units
            .get(unit)
            .map(|package| package.compiled.clone())
            .ok_or_else(|| "dynamic native unit is missing".to_owned())?;
        let active_entries = std::mem::take(
            &mut self
                .dynamic_units
                .get_mut(unit)
                .expect("dynamic native unit was already validated")
                .native_entries,
        );
        let previous_compiled = std::mem::replace(&mut self.compiled, compiled.clone());
        let previous_unit = std::mem::replace(&mut self.unit, ActiveNativeUnit::new(&compiled));
        let previous_identity =
            std::mem::replace(&mut self.unit_identity, compiled.artifact_identity());
        let previous_entries = std::mem::replace(&mut self.native_entries, active_entries);
        let previous_continuations = std::mem::replace(
            &mut self.continuation_instructions,
            compiled.prepared_continuation_instructions(),
        );
        let previous_callsites = std::mem::replace(
            &mut self.native_callsites,
            compiled.prepared_native_callsites(),
        );
        let previous_dynamic_unit = self.current_dynamic_unit.replace(unit);

        // Native code in an included/eval unit uses that unit's dense trusted
        // function-cell table. The outer request activation describes the
        // root deployment; refresh the by-value runtime view for the scoped
        // unit before constructing any nested JitDeoptState. Without this,
        // FunctionId N from an include indexed root FunctionId N and could
        // indirect-call arbitrary data as an address.
        let _runtime_view = activate_native_context(self);
        let result = operation(self);

        let active_entries = std::mem::replace(&mut self.native_entries, previous_entries);
        self.dynamic_units
            .get_mut(unit)
            .expect("active dynamic native unit disappeared")
            .native_entries = active_entries;
        self.current_dynamic_unit = previous_dynamic_unit;
        self.native_callsites = previous_callsites;
        self.continuation_instructions = previous_continuations;
        self.unit_identity = previous_identity;
        self.unit = previous_unit;
        self.compiled = previous_compiled;
        Ok(result)
    }

    fn direct_array_slot(&self, encoded: i64) -> Option<(usize, php_jit::JitNativeValueSlot)> {
        let index = Self::direct_value_index(encoded)?;
        let slot = *self.direct_value_slots.get(index)?;
        (slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY)
            .then_some((index, slot))
    }

    fn direct_array_entries_for(
        &self,
        encoded: i64,
    ) -> Option<&[php_jit::JitNativeDirectArrayEntry]> {
        let (_, slot) = self.direct_array_slot(encoded)?;
        let length = usize::try_from(slot.payload).ok()?;
        let base = self.direct_array_entries.as_ptr() as usize;
        let address = usize::try_from(slot.aux).ok()?;
        let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
        let offset = address.checked_sub(base)?;
        (offset % entry_size == 0).then_some(())?;
        let start = offset / entry_size;
        self.direct_array_entries
            .get(start..start.checked_add(length)?)
    }

    /// Rewrites unit-indexed constants embedded in a native array tree to
    /// request-owned native values exactly once before the tree crosses an
    /// IR-unit boundary.  The array slots remain the authoritative storage;
    /// this deliberately does not decode the tree to `PhpArray` or allocate
    /// a second direct-array facade.
    fn stabilize_direct_array_for_cross_unit(&mut self, encoded: i64) -> Result<(), String> {
        let mut pending = vec![encoded];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(array) = pending.pop() {
            let Some((index, slot)) = self.direct_array_slot(array) else {
                continue;
            };
            if !visited.insert(index) {
                continue;
            }
            let length = usize::try_from(slot.payload)
                .map_err(|_| format!("direct native array {index} length overflow"))?;
            let base = self.direct_array_entries.as_ptr() as usize;
            let address = usize::try_from(slot.aux)
                .map_err(|_| format!("direct native array {index} address overflow"))?;
            let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
            let offset = address
                .checked_sub(base)
                .ok_or_else(|| format!("direct native array {index} is outside its arena"))?;
            if offset % entry_size != 0 {
                return Err(format!("direct native array {index} address is unaligned"));
            }
            let start = offset / entry_size;
            let end = start
                .checked_add(length)
                .ok_or_else(|| format!("direct native array {index} range overflow"))?;
            if end > self.direct_array_entries.len() {
                return Err(format!(
                    "direct native array {index} entries are outside its arena"
                ));
            }
            for entry_index in start..end {
                let entry = self.direct_array_entries[entry_index];
                let key = self.stabilize_cross_unit_value(entry.key)?;
                let value = self.stabilize_cross_unit_value(entry.value)?;
                self.direct_array_entries[entry_index] =
                    php_jit::JitNativeDirectArrayEntry { key, value };
                if Self::direct_value_index(key).is_some() {
                    pending.push(key);
                }
                if Self::direct_value_index(value).is_some() {
                    pending.push(value);
                }
            }
        }
        Ok(())
    }

    fn stabilize_cross_unit_value(&mut self, encoded: i64) -> Result<i64, String> {
        let Some(constant) = php_jit::jit_decode_constant(encoded) else {
            return Ok(encoded);
        };
        if matches!(
            constant,
            u32::MAX
                | php_jit::JIT_VALUE_UNINITIALIZED
                | php_jit::JIT_VALUE_FALSE
                | php_jit::JIT_VALUE_TRUE
        ) {
            return Ok(encoded);
        }
        let value = self.decode(encoded)?;
        self.encode(value)
    }

    fn direct_array_length(&self, encoded: i64) -> Option<usize> {
        self.direct_array_entries_for(encoded).map(<[_]>::len)
    }

    fn direct_array_is_unique(&self, encoded: i64) -> Option<bool> {
        self.direct_array_slot(encoded)
            .map(|(_, slot)| slot.refcount == 1)
    }

    fn direct_array_can_append(&self, encoded: i64) -> Option<bool> {
        let Value::Array(array) = Self::direct_value_index(encoded)
            .and_then(|index| self.decode_direct_array(index).ok())?
        else {
            return None;
        };
        Some(array.can_append())
    }

    fn direct_array_find_encoded(
        &self,
        encoded: i64,
        key: &php_runtime::api::ArrayKey,
    ) -> Result<Option<i64>, String> {
        let Some(entries) = self.direct_array_entries_for(encoded) else {
            return Err("native value is not a direct array".to_owned());
        };
        for entry in entries {
            let entry_key = self.decode(entry.key)?;
            if php_runtime::api::ArrayKey::from_value(&entry_key).as_ref() == Some(key) {
                return Ok(Some(entry.value));
            }
        }
        Ok(None)
    }

    #[track_caller]
    fn clone_direct_array_handle(&mut self, encoded: i64) -> Result<i64, String> {
        let (_, source_slot) = self
            .direct_array_slot(encoded)
            .ok_or_else(|| "native value is not a direct array".to_owned())?;
        let entries = self
            .direct_array_entries_for(encoded)
            .ok_or_else(|| "direct native array entries are unavailable".to_owned())?
            .to_vec();
        let (start, capacity) = self.reserve_direct_array_entries(entries.len())?;
        let mut retained = Vec::with_capacity(entries.len() * 2);
        for entry in &entries {
            for child in [entry.key, entry.value] {
                if let Err(error) = self.retain(child) {
                    for child in retained {
                        let _ = self.release(child);
                    }
                    self.free_direct_array_entries(start, capacity);
                    return Err(error);
                }
                retained.push(child);
            }
        }
        self.direct_array_entries[start..start + entries.len()].copy_from_slice(&entries);
        let index = if *self.direct_value_free_head != php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE {
            let index = *self.direct_value_free_head as usize;
            let slot = self
                .direct_value_slots
                .get(index)
                .ok_or_else(|| "direct native value free-list entry is missing".to_owned())?;
            *self.direct_value_free_head = slot.reserved;
            index
        } else {
            let index = usize::try_from(*self.direct_value_next)
                .map_err(|_| "direct native value index overflow".to_owned())?;
            if index >= self.direct_value_slots.len() {
                for child in retained {
                    let _ = self.release(child);
                }
                self.free_direct_array_entries(start, capacity);
                return Err(format!(
                    "direct native value arena exhausted at {} slots",
                    index.saturating_add(1)
                ));
            }
            *self.direct_value_next = u32::try_from(index + 1)
                .map_err(|_| "direct native value index overflow".to_owned())?;
            index
        };
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
            flags: source_slot.flags,
            reserved: u32::try_from(capacity).unwrap_or(u32::MAX),
            payload: entries.len() as u64,
            aux: self.direct_array_entries[start..].as_ptr() as usize as u64,
        };
        self.record_direct_array_materialization(entries.len(), std::panic::Location::caller());
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .ok_or_else(|| "direct native value handle overflow".to_owned())?;
        Ok((php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG | u64::from(runtime_index)) as i64)
    }

    fn direct_array_insert_encoded(
        &mut self,
        encoded: i64,
        key: Option<&php_runtime::api::ArrayKey>,
        value: i64,
    ) -> Result<(), String> {
        let (array_index, mut slot) = self
            .direct_array_slot(encoded)
            .ok_or_else(|| "native value is not a direct array".to_owned())?;
        if slot.refcount != 1 {
            return Err("direct native array write requires unique ownership".to_owned());
        }
        let length = usize::try_from(slot.payload)
            .map_err(|_| "direct native array length overflow".to_owned())?;
        let base = self.direct_array_entries.as_ptr() as usize;
        let address = usize::try_from(slot.aux)
            .map_err(|_| "direct native array address overflow".to_owned())?;
        let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
        let offset = address
            .checked_sub(base)
            .ok_or_else(|| "direct native array address is outside its arena".to_owned())?;
        if offset % entry_size != 0 {
            return Err("direct native array address is unaligned".to_owned());
        }
        let mut start = offset / entry_size;
        let normalized_key = match key {
            Some(key) => key.clone(),
            None => {
                let mut maximum = None::<i64>;
                for entry in self
                    .direct_array_entries
                    .get(start..start.saturating_add(length))
                    .ok_or_else(|| "direct native array entries are outside its arena".to_owned())?
                {
                    if let Value::Int(key) = self.decode(entry.key)? {
                        maximum = Some(maximum.map_or(key, |current| current.max(key)));
                    }
                }
                php_runtime::api::ArrayKey::Int(match maximum {
                    Some(i64::MAX) => {
                        return Err(php_runtime::api::PHP_ARRAY_APPEND_OVERFLOW_MESSAGE.to_owned());
                    }
                    Some(maximum) => maximum + 1,
                    None => 0,
                })
            }
        };
        let entries = self
            .direct_array_entries
            .get(start..start.saturating_add(length))
            .ok_or_else(|| "direct native array entries are outside its arena".to_owned())?;
        let mut existing = None;
        for (position, entry) in entries.iter().enumerate() {
            let entry_key = self.decode(entry.key)?;
            if php_runtime::api::ArrayKey::from_value(&entry_key).as_ref() == Some(&normalized_key)
            {
                existing = Some(position);
                break;
            }
        }
        if let Some(position) = existing {
            let entry_index = start + position;
            let previous = self.direct_array_entries[entry_index].value;
            if self.php_handle_is_reference(previous) == Some(true)
                && self.php_handle_is_reference(value) == Some(false)
                && let Value::Reference(reference) = self.decode(previous)?
            {
                reference.set(self.decode(value)?);
                return Ok(());
            }
            self.retain(value)?;
            self.direct_array_entries[entry_index].value = value;
            self.release(previous)?;
            return Ok(());
        }

        let encoded_key = match normalized_key {
            php_runtime::api::ArrayKey::Int(key) => self.encode(Value::Int(key))?,
            php_runtime::api::ArrayKey::String(key) => self.encode(Value::String(key))?,
        };
        if let Err(error) = self.retain(value) {
            let _ = self.release(encoded_key);
            return Err(error);
        }
        let capacity = slot.reserved as usize;
        if length == capacity {
            let (new_start, new_capacity) = match self.reserve_direct_array_entries(length + 1) {
                Ok(range) => range,
                Err(error) => {
                    let _ = self.release(encoded_key);
                    let _ = self.release(value);
                    return Err(error);
                }
            };
            self.direct_array_entries
                .copy_within(start..start + length, new_start);
            self.free_direct_array_entries(start, capacity);
            start = new_start;
            slot.reserved = u32::try_from(new_capacity).unwrap_or(u32::MAX);
            slot.aux = self.direct_array_entries[start..].as_ptr() as usize as u64;
        }
        self.direct_array_entries[start + length] = php_jit::JitNativeDirectArrayEntry {
            key: encoded_key,
            value,
        };
        slot.payload = (length + 1) as u64;
        self.direct_value_slots[array_index] = slot;
        Ok(())
    }

    fn invalidate_object_property_cache(&mut self, index: usize) -> Result<(), String> {
        let Some(view) = self
            .object_property_caches
            .get_mut(index)
            .and_then(Option::take)
        else {
            return Ok(());
        };
        if let Some(slot) = self.value_slots.get_mut(index) {
            slot.kind = php_jit::JIT_NATIVE_VALUE_VIEW_NONE;
            slot.flags = 0;
            slot.reserved = 0;
            slot.payload = 0;
            slot.aux = 0;
        }
        for entry in view.iter().copied() {
            if entry.name_hash == php_jit::JIT_NATIVE_OBJECT_PROPERTY_CACHE_MISS {
                continue;
            }
            if let Some(value_index) = php_jit::jit_decode_runtime_value(entry.value) {
                self.release_runtime_value_index(value_index as usize)?;
            }
        }
        Ok(())
    }

    fn invalidate_object_property_cache_for_encoded(&mut self, object: i64) -> Result<(), String> {
        let Some(index) = php_jit::jit_decode_runtime_value(object) else {
            return Ok(());
        };
        self.invalidate_object_property_cache(index as usize)
    }

    fn publish_object_property_cache(
        &mut self,
        object: i64,
        property: &str,
        value: i64,
    ) -> Result<(), String> {
        let object_index = php_jit::jit_decode_runtime_value(object)
            .and_then(|index| usize::try_from(index).ok())
            .ok_or_else(|| "native property receiver is not a runtime object".to_owned())?;
        let object_ref = match self.values.get(object_index).and_then(Option::as_ref) {
            Some(NativeStoredValue::Php(Value::Object(object))) => object.clone(),
            _ => return Ok(()),
        };
        // Cache only immediates and immutable strings. Object/reference/array
        // handles can form observable cycles and remain on the baseline path.
        if let Some(value_index) = php_jit::jit_decode_runtime_value(value) {
            if !matches!(
                self.values
                    .get(value_index as usize)
                    .and_then(Option::as_ref),
                Some(NativeStoredValue::Php(Value::String(_)))
            ) {
                return Ok(());
            }
        }
        if self.object_property_caches[object_index].is_none() {
            self.object_property_caches[object_index] = Some(
                vec![
                    php_jit::JitNativePropertyCacheEntry::default();
                    php_jit::JIT_NATIVE_OBJECT_PROPERTY_CACHE_SIZE
                ]
                .into_boxed_slice(),
            );
        }
        let name_hash = php_jit::jit_native_property_name_hash(property);
        let mask = php_jit::JIT_NATIVE_OBJECT_PROPERTY_CACHE_SIZE - 1;
        let cache_index = name_hash as usize & mask;
        let old = self.object_property_caches[object_index]
            .as_ref()
            .expect("property cache was initialized")[cache_index];
        if old.name_hash == name_hash && old.value == value {
            return Ok(());
        }
        if old.name_hash != php_jit::JIT_NATIVE_OBJECT_PROPERTY_CACHE_MISS
            && let Some(value_index) = php_jit::jit_decode_runtime_value(old.value)
        {
            self.release_runtime_value_index(value_index as usize)?;
        }
        if let Some(value_index) = php_jit::jit_decode_runtime_value(value) {
            self.retain_runtime_value_index(value_index as usize)?;
        }
        let cache = self.object_property_caches[object_index]
            .as_mut()
            .expect("property cache remains initialized");
        cache[cache_index] = php_jit::JitNativePropertyCacheEntry {
            name_hash,
            property_epoch: object_ref.property_epoch(),
            value,
        };
        let slot = self
            .value_slots
            .get_mut(object_index)
            .ok_or_else(|| format!("native object value {object_index} has no native slot"))?;
        slot.kind = php_jit::JIT_NATIVE_VALUE_VIEW_OBJECT_PROPERTIES;
        slot.flags = php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION;
        slot.reserved = mask as u32;
        slot.payload = object_ref.property_epoch_address() as usize as u64;
        slot.aux = cache.as_ptr() as usize as u64;
        Ok(())
    }

    fn mutate_array(
        &mut self,
        encoded: i64,
        mutate: impl FnOnce(&mut php_runtime::api::PhpArray),
    ) -> Result<(), String> {
        self.mutate_array_with(encoded, mutate)
    }

    fn mutate_array_with<T>(
        &mut self,
        encoded: i64,
        mutate: impl FnOnce(&mut php_runtime::api::PhpArray) -> T,
    ) -> Result<T, String> {
        if let Some(index) = Self::direct_value_index(encoded) {
            let Value::Array(mut array) = self.decode_direct_array(index)? else {
                return Err("direct native value is not an array".to_owned());
            };
            let result = mutate(&mut array);
            self.replace_direct_array(index, array)?;
            return Ok(result);
        }
        if let Value::Reference(reference) = self.decode(encoded)? {
            let mut value = reference.get();
            let Value::Array(array) = &mut value else {
                return Err("native reference does not contain an array".to_owned());
            };
            let result = mutate(array);
            reference.set(value);
            return Ok(result);
        }
        let index = php_jit::jit_decode_runtime_value(encoded)
            .and_then(|index| usize::try_from(index).ok())
            .ok_or_else(|| "native value is not an array handle".to_owned())?;
        let array = match self.values.get_mut(index).and_then(Option::as_mut) {
            Some(NativeStoredValue::Php(Value::Array(array))) => array,
            _ => return Err("native value is not an array or array reference".to_owned()),
        };
        Ok(mutate(array))
    }

    fn encode_iterator(
        &mut self,
        entries: Vec<(Value, Value)>,
        live_source: Option<i64>,
        live_global: Option<String>,
        live_object: Option<php_runtime::api::ObjectRef>,
        user_iterator: Option<php_runtime::api::ObjectRef>,
    ) -> Result<i64, String> {
        self.encode_stored_value(NativeStoredValue::Iterator(Box::new(NativeIteratorState {
            entries,
            index: 0,
            live_source,
            live_global,
            live_object,
            user_iterator,
            user_iterator_started: false,
        })))
    }

    fn encode_array_iterator(&mut self, source: php_runtime::api::PhpArray) -> Result<i64, String> {
        // A by-value foreach over an immutable COW snapshot can publish all
        // non-reference entries once. Ordinary loop iterations then advance a
        // request-owned ABI cursor without crossing back into Rust. Reference
        // elements remain on the semantic helper path because their value is
        // intentionally observed at each iteration.
        let snapshot = source
            .iter()
            .map(|(key, value)| {
                let key = match key {
                    php_runtime::api::ArrayKey::Int(key) => Value::Int(key),
                    php_runtime::api::ArrayKey::String(key) => Value::String(key.clone()),
                };
                (key, value.clone())
            })
            .collect::<Vec<_>>();
        let direct = if snapshot
            .iter()
            .any(|(_, value)| matches!(value, Value::Reference(_)))
        {
            None
        } else {
            let mut entries = Vec::with_capacity(snapshot.len());
            for (key, value) in snapshot {
                let key = match self.encode(key) {
                    Ok(key) => key,
                    Err(error) => {
                        self.release_direct_foreach_entries(&entries);
                        return Err(error);
                    }
                };
                let value = match self.encode(value) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = self.release(key);
                        self.release_direct_foreach_entries(&entries);
                        return Err(error);
                    }
                };
                entries.push(php_jit::JitNativeForeachEntry { key, value });
            }
            let entries = entries.into_boxed_slice();
            let view = Box::new(php_jit::JitNativeForeachView {
                cursor: 0,
                length: entries.len() as u64,
                entries: entries.as_ptr() as usize as u64,
            });
            Some(Box::new(NativeDirectForeachState { view, entries }))
        };
        self.encode_stored_value(NativeStoredValue::ArrayIterator(Box::new(
            NativeArrayIteratorState {
                source,
                index: 0,
                direct,
            },
        )))
    }

    fn release_direct_foreach_entries(&mut self, entries: &[php_jit::JitNativeForeachEntry]) {
        for entry in entries {
            for encoded in [entry.key, entry.value] {
                let _ = self.release(encoded);
            }
        }
    }

    fn encode_generator_iterator(
        &mut self,
        generator: php_runtime::api::GeneratorRef,
    ) -> Result<i64, String> {
        let function = php_ir::FunctionId::new(generator.function());
        let handle = ensure_native_entry(self, function)?;
        let arguments = generator
            .args()
            .into_iter()
            .map(|value| self.encode(value))
            .collect::<Result<Vec<_>, _>>()?;
        self.encode_stored_value(NativeStoredValue::GeneratorIterator(Box::new(
            NativeGeneratorIteratorState {
                generator,
                handle: Box::new(handle),
                arguments,
                state: Box::new(None),
                delegation: None,
                yields_seen: 0,
                finished: false,
            },
        )))
    }

    fn generator_iterator(
        &mut self,
        generator: php_runtime::api::GeneratorRef,
    ) -> Result<i64, String> {
        if let Some(encoded) = self.generator_iterators.get(&generator.id()).copied() {
            return Ok(encoded);
        }
        let id = generator.id();
        let encoded = self.encode_generator_iterator(generator)?;
        self.generator_iterators.insert(id, encoded);
        Ok(encoded)
    }

    fn generator_resume(
        &mut self,
        encoded: i64,
        resume_kind: php_jit::JitNativeResumeInputKind,
        resume_value: i64,
    ) -> Result<Option<(Value, Value)>, String> {
        let index = php_jit::jit_decode_runtime_value(encoded)
            .ok_or_else(|| "native value is not a foreach iterator handle".to_owned())?;
        let user_iterator = match self.values.get(index as usize).and_then(Option::as_ref) {
            Some(NativeStoredValue::Iterator(iterator)) => iterator
                .user_iterator
                .as_ref()
                .map(|object| (object.clone(), iterator.user_iterator_started)),
            _ => None,
        };
        if let Some((object, started)) = user_iterator {
            let class_name = object.class_name();
            let receiver = self.encode(Value::Object(object))?;
            if started {
                let next = native_method_in_hierarchy(self, &class_name, "next")
                    .ok_or_else(|| "Iterator::next() is missing".to_owned())?;
                invoke_native_method(self, next, &[receiver])?;
            }
            let valid = native_method_in_hierarchy(self, &class_name, "valid")
                .ok_or_else(|| "Iterator::valid() is missing".to_owned())?;
            let valid = invoke_native_method(self, valid, &[receiver])?;
            if !native_property_truthy(&self.decode(valid)?) {
                return Ok(None);
            }
            let current = native_method_in_hierarchy(self, &class_name, "current")
                .ok_or_else(|| "Iterator::current() is missing".to_owned())?;
            let key = native_method_in_hierarchy(self, &class_name, "key")
                .ok_or_else(|| "Iterator::key() is missing".to_owned())?;
            let current = invoke_native_method(self, current, &[receiver])?;
            let key = invoke_native_method(self, key, &[receiver])?;
            if let Some(NativeStoredValue::Iterator(iterator)) =
                self.values.get_mut(index as usize).and_then(Option::as_mut)
            {
                iterator.user_iterator_started = true;
            }
            return Ok(Some((self.decode(key)?, self.decode(current)?)));
        }
        let object_entry = match self.values.get(index as usize).and_then(Option::as_ref) {
            Some(NativeStoredValue::Iterator(iterator)) => {
                iterator.live_object.as_ref().and_then(|object| {
                    iterator
                        .entries
                        .get(iterator.index)
                        .map(|(key, _)| (object.clone(), key.clone(), iterator.index))
                })
            }
            _ => None,
        };
        if let Some((object, key, cursor)) = object_entry {
            let name = match &key {
                Value::Int(key) => key.to_string(),
                Value::String(key) => key.to_string_lossy(),
                _ => return Err("native object iterator key is invalid".to_owned()),
            };
            let value = object.get_property(&name).unwrap_or(Value::Null);
            let value = match value {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            if let Some(NativeStoredValue::Iterator(iterator)) =
                self.values.get_mut(index as usize).and_then(Option::as_mut)
            {
                iterator.index = cursor.saturating_add(1);
            }
            return Ok(Some((key, value)));
        }
        let live = match self.values.get(index as usize).and_then(Option::as_ref) {
            Some(NativeStoredValue::Iterator(iterator)) => iterator
                .live_source
                .map(|source| (source, iterator.index, iterator.live_global.clone())),
            _ => None,
        };
        if let Some((source, cursor, live_global)) = live {
            let reference_entry = |array: &mut php_runtime::api::PhpArray| {
                let (key, value) = array
                    .iter()
                    .nth(cursor)
                    .map(|(key, value)| (key.clone(), value.clone()))?;
                let reference = match value {
                    Value::Reference(reference) => reference,
                    value => {
                        let reference = php_runtime::api::ReferenceCell::new(value);
                        array.insert(key.clone(), Value::Reference(reference.clone()));
                        reference
                    }
                };
                let key = match key {
                    php_runtime::api::ArrayKey::Int(key) => Value::Int(key),
                    php_runtime::api::ArrayKey::String(key) => Value::String(key),
                };
                Some((key, Value::Reference(reference)))
            };
            let entry = if let Some(global) = live_global {
                let Some(root) = self.inherited_globals.get(&global).cloned() else {
                    return Ok(None);
                };
                match root {
                    Value::Reference(reference) => {
                        let Value::Array(mut array) = reference.get() else {
                            return Ok(None);
                        };
                        let entry = reference_entry(&mut array);
                        reference.set(Value::Array(array));
                        entry
                    }
                    Value::Array(mut array) => {
                        let entry = reference_entry(&mut array);
                        self.inherited_globals.insert(global, Value::Array(array));
                        entry
                    }
                    _ => None,
                }
            } else {
                self.mutate_array_with(source, reference_entry)?
            };
            let Some(entry) = entry else {
                return Ok(None);
            };
            if let Some(NativeStoredValue::Iterator(iterator)) =
                self.values.get_mut(index as usize).and_then(Option::as_mut)
            {
                iterator.index = iterator.index.saturating_add(1);
            }
            return Ok(Some(entry));
        }
        if let Some(NativeStoredValue::Iterator(iterator)) =
            self.values.get_mut(index as usize).and_then(Option::as_mut)
        {
            let entry = iterator
                .entries
                .get(iterator.index)
                .cloned()
                .map(|(key, value)| {
                    let value = match value {
                        Value::Reference(reference) => reference.get(),
                        value => value,
                    };
                    (key, value)
                });
            iterator.index = iterator.index.saturating_add(usize::from(entry.is_some()));
            return Ok(entry);
        }
        let (generator, handle, arguments, state, delegation, finished) =
            match self.values.get(index as usize).and_then(Option::as_ref) {
                Some(NativeStoredValue::GeneratorIterator(iterator)) => (
                    iterator.generator.clone(),
                    iterator.handle.clone(),
                    iterator.arguments.clone(),
                    iterator.state.clone(),
                    iterator.delegation.clone(),
                    iterator.finished,
                ),
                _ => return Err(format!("native foreach iterator {index} is missing")),
            };
        if finished {
            return Ok(None);
        }
        let mut effective_resume_kind = resume_kind;
        let mut effective_resume_value = resume_value;
        if let Some(delegation) = delegation {
            match delegation {
                NativeGeneratorDelegation::Array {
                    entries,
                    index: cursor,
                } => {
                    if let Some((key, value)) = entries.get(cursor).cloned() {
                        if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                            self.values.get_mut(index as usize).and_then(Option::as_mut)
                            && let Some(NativeGeneratorDelegation::Array {
                                index: saved_cursor,
                                ..
                            }) = iterator.delegation.as_mut()
                        {
                            *saved_cursor = saved_cursor.saturating_add(1);
                        }
                        generator.suspend_forwarded(Some(key.clone()), value.clone());
                        if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                            self.values.get_mut(index as usize).and_then(Option::as_mut)
                        {
                            iterator.yields_seen = iterator.yields_seen.saturating_add(1);
                        }
                        return Ok(Some((key, value)));
                    }
                    if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                        self.values.get_mut(index as usize).and_then(Option::as_mut)
                    {
                        iterator.delegation = None;
                    }
                    effective_resume_kind = php_jit::JitNativeResumeInputKind::VALUE;
                    effective_resume_value = php_jit::jit_encode_constant(u32::MAX);
                }
                NativeGeneratorDelegation::Generator {
                    generator: delegated,
                    iterator,
                } => {
                    if let Some((key, value)) = self.iterator_next(iterator)? {
                        generator.suspend_forwarded(Some(key.clone()), value.clone());
                        if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                            self.values.get_mut(index as usize).and_then(Option::as_mut)
                        {
                            iterator.yields_seen = iterator.yields_seen.saturating_add(1);
                        }
                        return Ok(Some((key, value)));
                    }
                    effective_resume_kind = php_jit::JitNativeResumeInputKind::VALUE;
                    effective_resume_value =
                        self.encode(delegated.return_value().unwrap_or(Value::Null))?;
                    if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                        self.values.get_mut(index as usize).and_then(Option::as_mut)
                    {
                        iterator.delegation = None;
                    }
                }
            }
        }
        let outcome = if let Some(state) = state.as_ref() {
            let runtime = std::ptr::from_mut(&mut *self).cast::<std::ffi::c_void>();
            handle.invoke_i64_suspension_resume_with_native_unwind_runtime(
                &arguments,
                state,
                effective_resume_kind,
                effective_resume_value,
                php_jit::JIT_RUNTIME_ABI_HASH,
                runtime,
                |types, value| native_catch_matches(self, types, value),
            )
        } else {
            let runtime = std::ptr::from_mut(self).cast::<std::ffi::c_void>();
            handle.invoke_i64_with_deopt_runtime(&arguments, php_jit::JIT_RUNTIME_ABI_HASH, runtime)
        }
        .map_err(|error| format!("native generator invocation failed: {error:?}"))?;
        match outcome {
            php_jit::JitI64InvokeOutcome::SideExit {
                status,
                value,
                state,
            } if status == php_jit::JitCallStatus::SUSPEND_GENERATOR.0 as i32 => {
                if state.suspend_kind == php_jit::JitNativeSuspendKind::GENERATOR_DELEGATE.0 {
                    let delegated = self.decode(state.delegation_handle as i64)?;
                    let delegation = match delegated {
                        Value::Array(array) => NativeGeneratorDelegation::Array {
                            entries: array
                                .iter()
                                .map(|(key, value)| {
                                    let key = match key {
                                        php_runtime::api::ArrayKey::Int(value) => Value::Int(value),
                                        php_runtime::api::ArrayKey::String(value) => {
                                            Value::String(value.clone())
                                        }
                                    };
                                    (key, value.clone())
                                })
                                .collect(),
                            index: 0,
                        },
                        Value::Generator(delegated) => NativeGeneratorDelegation::Generator {
                            iterator: self.generator_iterator(delegated.clone())?,
                            generator: delegated,
                        },
                        other => {
                            return Err(format!(
                                "yield from expects an array or Traversable, got {}",
                                native_value_type_name(&other)
                            ));
                        }
                    };
                    if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                        self.values.get_mut(index as usize).and_then(Option::as_mut)
                    {
                        *iterator.state = Some(state);
                        iterator.delegation = Some(delegation);
                    }
                    return self.iterator_next(encoded);
                }
                let key = if state.suspend_flags & 1 != 0 {
                    Some(self.decode(state.yielded_key)?)
                } else {
                    None
                };
                let value = self.decode(value)?;
                generator.suspend(key, value.clone());
                if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                    self.values.get_mut(index as usize).and_then(Option::as_mut)
                {
                    *iterator.state = Some(state);
                }
                if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                    self.values.get_mut(index as usize).and_then(Option::as_mut)
                {
                    iterator.yields_seen = iterator.yields_seen.saturating_add(1);
                }
                let (key, value) = generator
                    .current()
                    .ok_or_else(|| "native generator suspension value is missing".to_owned())?;
                Ok(Some((key.unwrap_or(Value::Null), value)))
            }
            php_jit::JitI64InvokeOutcome::Returned(value)
            | php_jit::JitI64InvokeOutcome::SideExit {
                status: 1 | 2,
                value,
                ..
            } => {
                generator.close(Some(self.decode(value)?));
                if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                    self.values.get_mut(index as usize).and_then(Option::as_mut)
                {
                    iterator.finished = true;
                }
                Ok(None)
            }
            php_jit::JitI64InvokeOutcome::SideExit { status, .. } => {
                Err(format!("native generator returned status {status}"))
            }
        }
    }

    fn iterator_next(&mut self, encoded: i64) -> Result<Option<(Value, Value)>, String> {
        if let Some(entry) = self.array_iterator_next(encoded) {
            return Ok(entry);
        }
        self.generator_resume(
            encoded,
            php_jit::JitNativeResumeInputKind::VALUE,
            php_jit::jit_encode_constant(u32::MAX),
        )
    }

    fn array_iterator_next(&mut self, encoded: i64) -> Option<Option<(Value, Value)>> {
        if let Some(index) = Self::direct_value_index(encoded) {
            let iterator = *self.direct_value_slots.get(index)?;
            if iterator.refcount == 0
                || iterator.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FOREACH
            {
                return None;
            }
            let cursor = usize::try_from(iterator.aux).ok()?;
            let length = iterator.reserved as usize;
            if cursor >= length {
                return Some(None);
            }
            let source = Self::direct_value_index(iterator.payload as i64)?;
            let source = *self.direct_value_slots.get(source)?;
            let base = self.direct_array_entries.as_ptr() as usize;
            let address = usize::try_from(source.aux).ok()?;
            let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
            let start = address.checked_sub(base)? / entry_size;
            let entry = *self.direct_array_entries.get(start.checked_add(cursor)?)?;
            self.direct_value_slots[index].aux = iterator.aux.saturating_add(1);
            let key = self.decode(entry.key).ok()?;
            let value = self.decode(entry.value).ok()?;
            return Some(Some((key, value)));
        }
        let index = php_jit::jit_decode_runtime_value(encoded)? as usize;
        let NativeStoredValue::ArrayIterator(iterator) = self.values.get_mut(index)?.as_mut()?
        else {
            return None;
        };
        Some(
            iterator
                .source
                .next_pair_at_cursor(&mut iterator.index)
                .map(|(key, value)| {
                    let key = match key {
                        php_runtime::api::ArrayKey::Int(key) => Value::Int(key),
                        php_runtime::api::ArrayKey::String(key) => Value::String(key),
                    };
                    let value = match value {
                        Value::Reference(reference) => reference.get(),
                        value => value,
                    };
                    (key, value)
                }),
        )
    }

    fn generator_can_rewind(&self, encoded: i64) -> bool {
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return false;
        };
        self.values
            .get(index as usize)
            .and_then(Option::as_ref)
            .is_some_and(|value| match value {
                NativeStoredValue::GeneratorIterator(iterator) => {
                    matches!(iterator.yields_seen, 0 | 1) && !iterator.finished
                }
                _ => false,
            })
    }

    fn close_iterator(&mut self, encoded: i64) -> Result<(), String> {
        if let Some(index) = Self::direct_value_index(encoded) {
            return self.release_direct_value_index(index);
        }
        let index = php_jit::jit_decode_runtime_value(encoded)
            .ok_or_else(|| "native value is not a foreach iterator handle".to_owned())?;
        let value = self
            .values
            .get_mut(index as usize)
            .ok_or_else(|| format!("native foreach iterator {index} is missing"))?;
        match value.take() {
            Some(NativeStoredValue::ArrayIterator(iterator)) => {
                if let Some(slot) = self.value_slots.get_mut(index as usize) {
                    *slot = php_jit::JitNativeValueSlot::default();
                }
                if let Some(direct) = iterator.direct.as_ref() {
                    let entries = direct.entries.to_vec();
                    drop(iterator);
                    self.release_direct_foreach_entries(&entries);
                }
                Ok(())
            }
            Some(NativeStoredValue::Iterator(_) | NativeStoredValue::GeneratorIterator(_)) => {
                if let Some(slot) = self.value_slots.get_mut(index as usize) {
                    *slot = php_jit::JitNativeValueSlot::default();
                }
                Ok(())
            }
            other => {
                *value = other;
                Err(format!("native foreach iterator {index} is missing"))
            }
        }
    }

    fn instruction_for_continuation(
        &self,
        function: u32,
        continuation: u32,
    ) -> Option<NativeInstructionPtr> {
        self.continuation_instructions
            .get(function as usize)
            .and_then(|instructions| instructions.get(continuation as usize))
            .and_then(Option::as_ref)
            .map(|instruction| NativeInstructionPtr(std::sync::Arc::as_ptr(instruction)))
    }

    pub(super) fn instruction_kind_debug(&self, function: u32, continuation: u32) -> String {
        self.instruction_for_continuation(function, continuation)
            .map(|instruction| format!("{:?}", instruction.kind))
            .unwrap_or_else(|| "<missing continuation>".to_owned())
    }

    fn prepared_native_callsite(
        &self,
        function: u32,
        continuation: u32,
    ) -> Option<*const crate::compiled_unit::NativeCallSiteDescriptor> {
        self.native_callsites
            .get(function as usize)
            .and_then(|callsites| callsites.get(continuation as usize))
            .and_then(Option::as_ref)
            .map(std::sync::Arc::as_ptr)
    }

    fn deferred_function_argument_requires_reference(
        &self,
        function: u32,
        continuation: u32,
        argument: usize,
    ) -> Option<bool> {
        let descriptor = self
            .native_callsites
            .get(function as usize)
            .and_then(|callsites| callsites.get(continuation as usize))
            .and_then(Option::as_deref)?;
        if !matches!(
            descriptor.kind,
            crate::compiled_unit::NativeCallSiteKind::Function
        ) {
            return None;
        }
        let name = descriptor.target_symbol.as_deref()?;
        let parameters = if let Some(function) = self.function_id(name) {
            self.unit
                .functions
                .get(function.index())
                .map(|function| function.params.as_slice())
        } else if let Some(target) = self.external_function(name) {
            self.dynamic_units
                .get(target.unit)
                .and_then(|unit| unit.compiled.unit().functions.get(target.function.index()))
                .map(|function| function.params.as_slice())
        } else {
            None
        }?;
        call_dispatch::native_function_argument_requires_reference_at(
            descriptor.arguments.as_ref(),
            parameters,
            argument,
        )
    }

    fn native_method_epochs(&self) -> (u64, u64) {
        let dynamic_epoch = self.dynamic_units.len() as u64;
        (
            self.unit_identity ^ dynamic_epoch.rotate_left(17),
            self.unit_identity.rotate_left(29) ^ dynamic_epoch,
        )
    }

    fn lookup_native_method_pic(
        &self,
        descriptor: &crate::compiled_unit::NativeCallSiteDescriptor,
        receiver_class: &str,
        method: &str,
    ) -> Option<NativeMethodPicTarget> {
        let (class_layout_epoch, method_table_epoch) = self.native_method_epochs();
        if let Some((function, is_static)) = descriptor.lookup_method_pic(
            receiver_class,
            method,
            class_layout_epoch,
            method_table_epoch,
        ) {
            return Some(NativeMethodPicTarget::CurrentUnit {
                function,
                is_static,
            });
        }
        let pic = self.native_method_pics.get(&descriptor.pic_slot)?;
        if pic.megamorphic {
            return None;
        }
        pic.entries
            .iter()
            .find(|entry| {
                entry.receiver_class.eq_ignore_ascii_case(receiver_class)
                    && entry.method.eq_ignore_ascii_case(method)
                    && entry.class_layout_epoch == class_layout_epoch
                    && entry.method_table_epoch == method_table_epoch
            })
            .map(|entry| entry.target)
    }

    fn install_native_method_pic(
        &mut self,
        descriptor: &crate::compiled_unit::NativeCallSiteDescriptor,
        receiver_class: &str,
        method: &str,
        target: NativeMethodPicTarget,
    ) -> bool {
        let (class_layout_epoch, method_table_epoch) = self.native_method_epochs();
        if let NativeMethodPicTarget::CurrentUnit {
            function,
            is_static,
        } = target
        {
            return descriptor.install_method_pic(
                receiver_class,
                method,
                class_layout_epoch,
                method_table_epoch,
                function,
                is_static,
            );
        }
        let pic = self
            .native_method_pics
            .entry(descriptor.pic_slot)
            .or_default();
        if pic.megamorphic {
            return false;
        }
        if pic.entries.iter().any(|entry| {
            entry.receiver_class.eq_ignore_ascii_case(receiver_class)
                && entry.method.eq_ignore_ascii_case(method)
                && entry.class_layout_epoch == class_layout_epoch
                && entry.method_table_epoch == method_table_epoch
        }) {
            return true;
        }
        if pic.entries.len() >= NATIVE_METHOD_PIC_LIMIT {
            pic.entries.clear();
            pic.megamorphic = true;
            return false;
        }
        pic.entries.push(NativeMethodPicEntry {
            receiver_class: std::sync::Arc::from(receiver_class),
            method: std::sync::Arc::from(method),
            class_layout_epoch,
            method_table_epoch,
            target,
        });
        true
    }

    fn lookup_constant(&self, name: &str) -> Result<Value, String> {
        if let Some(value) = self.dynamic_constants.get(name) {
            return Ok(value.clone());
        }
        if let Some(constant) = self
            .unit
            .constant_table
            .iter()
            .find(|constant| constant.name == name)
            .and_then(|constant| self.unit.constants.get(constant.value.index()))
        {
            return ir_constant_value(constant);
        }
        php_std::ExtensionRegistry::standard_library()
            .enabled_constant(name)
            .and_then(php_std::ConstantDescriptor::value)
            .map(php_std::constants::constant_to_value)
            .ok_or_else(|| format!("Undefined constant \"{name}\""))
    }

    fn visible_include_constants(&self) -> std::collections::BTreeMap<String, Value> {
        let mut constants = self.dynamic_constants.clone();
        for entry in &self.unit.constant_table {
            if let Some(value) = self.unit.constants.get(entry.value.index())
                && let Ok(value) = ir_constant_value(value)
            {
                constants.entry(entry.name.clone()).or_insert(value);
            }
        }
        constants
    }

    pub(super) fn decode_result(&self, encoded: i64) -> Result<Value, String> {
        self.decode(encoded)
    }

    fn record_last_error(&mut self, error_type: i64, message: &str, file: &str, line: usize) {
        self.last_error = Some(NativeLastError {
            error_type,
            message: message.to_owned(),
            file: file.to_owned(),
            line,
        });
    }

    fn last_error_value(&self) -> Value {
        let Some(error) = &self.last_error else {
            return Value::Null;
        };
        let mut value = php_runtime::api::PhpArray::new();
        for (name, field) in [
            ("type", Value::Int(error.error_type)),
            (
                "message",
                Value::String(PhpString::from_bytes(error.message.as_bytes().to_vec())),
            ),
            (
                "file",
                Value::String(PhpString::from_bytes(error.file.as_bytes().to_vec())),
            ),
            (
                "line",
                Value::Int(i64::try_from(error.line).unwrap_or(i64::MAX)),
            ),
        ] {
            value.insert(
                php_runtime::api::ArrayKey::String(PhpString::from_bytes(name.as_bytes().to_vec())),
                field,
            );
        }
        Value::Array(value)
    }

    pub(super) fn take_pending_throwable(&mut self) -> Option<Value> {
        let throwable = self.pending_throwable.take();
        if throwable.is_some() {
            self.mark_roots_dirty(RootMutationReason::PendingThrowable);
        }
        throwable
    }

    pub(super) fn run_shutdown_callbacks(&mut self) -> Result<(), String> {
        if self.include_child {
            return Ok(());
        }
        while !self.shutdown_callbacks.is_empty() {
            let NativeShutdownCallback {
                callable,
                arguments,
                source,
            } = self.shutdown_callbacks.remove(0);
            self.mark_roots_dirty(RootMutationReason::CallbackOrHandler);
            let result = invoke_native_callable_value(self, callable, &arguments, &source, None);
            if matches!(&result, Err(error) if error == "E_PHP_RETHROW")
                && let Some(throwable) = self.take_pending_throwable()
            {
                self.pending_throwable = Some(native_throwable_with_internal_frame(
                    self, throwable, &source,
                ));
                self.mark_roots_dirty(RootMutationReason::PendingThrowable);
            }
            result?;
        }
        loop {
            let mut objects = Vec::new();
            let mut seen = std::collections::BTreeSet::new();
            for stored in &self.values {
                let Some(NativeStoredValue::Php(Value::Object(object))) = stored else {
                    continue;
                };
                if !self
                    .destroyed_objects
                    .get(&object.id())
                    .is_some_and(WeakObjectHandle::is_alive)
                    && seen.insert(object.id())
                {
                    objects.push(object.clone());
                }
            }
            let Some(object) = objects.pop() else {
                break;
            };
            self.destroyed_objects
                .insert(object.id(), object.weak_handle());
            let class_name = object.class_name();
            let receiver = self.encode(Value::Object(object))?;
            if let Some(function) = self
                .unit
                .classes
                .iter()
                .find(|class| class.name == normalize_class_name(&class_name))
                .and_then(|class| {
                    class
                        .methods
                        .iter()
                        .find(|method| method.name.eq_ignore_ascii_case("__destruct"))
                })
                .map(|method| method.function)
            {
                let _ = invoke_native_method(self, function, &[receiver])?;
            } else if let Some((function, _)) =
                native_external_method(self, &class_name, "__destruct")
            {
                let _ = invoke_native_external_function(
                    self,
                    function,
                    &[receiver],
                    Some(class_name),
                    self.unit.strict_types,
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn handle_uncaught_throwable(&mut self, encoded: i64) -> Result<bool, String> {
        let Some(handler) = self.exception_handlers.last().cloned() else {
            return Ok(false);
        };
        let throwable = self.decode(encoded)?;
        let source = self
            .unit
            .functions
            .get(self.unit.entry.index())
            .and_then(|function| {
                function
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .next()
            })
            .cloned()
            .ok_or_else(|| "exception handler call source is missing".to_owned())?;
        let _ = invoke_native_callable_value(self, handler, &[throwable], &source, None)?;
        Ok(true)
    }

    pub(super) fn publish_include_globals(&mut self) {
        if self.include_child {
            let entry_file = self
                .unit
                .functions
                .get(self.unit.entry.index())
                .map(|function| function.span.file);
            NATIVE_INCLUDE_GLOBALS.with(|globals| {
                globals.replace(Some(std::mem::take(&mut self.inherited_globals)));
            });
            NATIVE_INCLUDE_INI.with(|ini| {
                ini.replace(Some(std::mem::take(&mut self.ini_registry)));
            });
            NATIVE_INCLUDE_DEFAULT_TIMEZONE.with(|timezone| {
                timezone.replace(Some(std::mem::take(&mut self.default_timezone)));
            });
            NATIVE_INCLUDE_HTTP_RESPONSE.with(|response| {
                response.replace(Some(std::mem::take(&mut self.http_response)));
            });
            NATIVE_INCLUDE_FILES.with(|files| {
                files.replace(Some(std::mem::take(&mut self.included_files)));
            });
            NATIVE_INCLUDE_MYSQL.with(|mysql| {
                mysql.replace(Some(self.mysql_state.clone()));
            });
            let mut functions = self
                .unit
                .function_table
                .iter()
                .map(|entry| (entry.name.clone(), entry.function))
                .collect::<Vec<_>>();
            functions.extend(
                self.dynamic_functions
                    .iter()
                    .map(|(name, function)| (name.clone(), *function)),
            );
            let classes = self
                .unit
                .classes
                .iter()
                .filter(|class| {
                    (!class.flags.is_conditional
                        || self.class_is_visible(&normalize_class_name(&class.name)))
                        && (class.span.start != 0 || class.span.end != 0)
                        && entry_file.is_none_or(|file| class.span.file == file)
                })
                .map(|class| class.name.clone())
                .collect::<Vec<_>>();
            let mut constants = std::collections::BTreeMap::new();
            for entry in &self.unit.constant_table {
                if entry_file.is_none_or(|file| entry.span.file == file)
                    && let Some(value) = self.unit.constants.get(entry.value.index())
                    && let Ok(value) = ir_constant_value(value)
                {
                    constants.insert(entry.name.clone(), value);
                }
            }
            NATIVE_INCLUDE_CONSTANTS.with(|constants| {
                constants.replace(Some(std::mem::take(&mut self.dynamic_constants)));
            });
            let autoload_callbacks = self
                .autoload_callbacks
                .split_off(self.inherited_autoload_callback_count);
            let shutdown_callbacks = self
                .shutdown_callbacks
                .split_off(self.inherited_shutdown_callback_count);
            let native_entry_signature_hashes = self
                .native_entries
                .keys()
                .copied()
                .map(|function| {
                    let signatures =
                        visible_external_function_signatures(self, &self.compiled, function);
                    (
                        function,
                        super::external_function_signatures_hash(&signatures),
                    )
                })
                .collect();
            let mut symbols = self.take_include_symbols();
            for class in &classes {
                let class = normalize_class_name(class);
                symbols.dynamic_classes.remove(&class);
                symbols.external_class_units.remove(&class);
            }
            NATIVE_INCLUDE_SYMBOLS.with(|slot| {
                slot.replace(Some(symbols));
            });
            NATIVE_INCLUDE_EXPORTS.with(|exports| {
                exports.replace(Some(NativeIncludeExports {
                    functions,
                    native_entries: std::mem::take(&mut self.native_entries),
                    native_entry_signature_hashes,
                    classes,
                    constants,
                    autoload_callbacks,
                    shutdown_callbacks,
                }));
            });
        }
    }
}

pub(super) struct NativeExecutionContextGuard {
    _runtime_view: php_jit::JitNativeRuntimeViewGuard,
}

fn rooted_membership_may_change(previous: &Value, replacement: &Value) -> bool {
    match (previous, replacement) {
        (Value::Object(lhs), Value::Object(rhs)) => lhs.id() != rhs.id(),
        (Value::Array(lhs), Value::Array(rhs)) => lhs.gc_debug_id() != rhs.gc_debug_id(),
        (Value::Reference(lhs), Value::Reference(rhs)) => !lhs.ptr_eq(rhs),
        (
            Value::Object(_) | Value::Array(_) | Value::Reference(_),
            Value::Object(_) | Value::Array(_) | Value::Reference(_),
        ) => true,
        (Value::Object(_) | Value::Array(_) | Value::Reference(_), _) => true,
        (_, Value::Object(_) | Value::Array(_) | Value::Reference(_)) => true,
        _ => false,
    }
}

pub(super) fn activate_native_context(
    context: &mut NativeExecutionContext<'_>,
) -> NativeExecutionContextGuard {
    let deployment = context.compiled.prepared_deployment_image();
    let runtime_view = php_jit::activate_native_runtime_view(php_jit::JitNativeRuntimeView {
        abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
        value_slot_capacity: u32::try_from(context.value_slots.capacity()).unwrap_or(u32::MAX),
        value_slots: context.value_slots.as_mut_ptr() as usize as u64,
        direct_value_slots: context.direct_value_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(context.direct_value_next.as_mut()) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(context.direct_value_free_head.as_mut()) as usize
            as u64,
        direct_array_entries: context.direct_array_entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(context.direct_array_next.as_mut()) as usize as u64,
        direct_array_free_heads: context.direct_array_free_heads.as_mut_ptr() as usize as u64,
        direct_string_bytes: context.direct_string_bytes.as_mut_ptr() as usize as u64,
        direct_string_next: std::ptr::from_mut(context.direct_string_next.as_mut()) as usize as u64,
        trusted_constant_views: deployment.constant_views.as_ptr() as usize as u64,
        trusted_constant_view_count: u32::try_from(deployment.constant_views.len())
            .unwrap_or(u32::MAX),
        trusted_constant_view_reserved: 0,
        trusted_function_entries: deployment.native_function_entries.as_ptr() as usize as u64,
        trusted_function_entry_count: u32::try_from(deployment.native_function_entries.len())
            .unwrap_or(u32::MAX),
        trusted_function_entry_reserved: 0,
        trusted_optimizing_function_entries: deployment.optimizing_function_entries.as_ptr()
            as usize as u64,
        trusted_optimizing_function_entry_count: u32::try_from(
            deployment.optimizing_function_entries.len(),
        )
        .unwrap_or(u32::MAX),
        trusted_optimizing_function_entry_reserved: 0,
        poll_counter: std::ptr::from_mut(context.native_poll_counter.as_mut()) as usize as u64,
        global_reference_cache: context.global_reference_cache.entries.as_mut_ptr() as usize as u64,
        global_reference_cache_mask: (NATIVE_GLOBAL_REFERENCE_CACHE_SIZE - 1) as u32,
        global_reference_cache_reserved: 0,
    });
    NativeExecutionContextGuard {
        _runtime_view: runtime_view,
    }
}

#[allow(unsafe_code)]
fn with_native_context_for<R>(
    runtime: *mut NativeRequestFastState,
    _helper_id: &'static str,
    operation: impl FnOnce(&mut NativeExecutionContext<'_>) -> R,
) -> Option<R> {
    // SAFETY: every published native entry receives the live request context
    // from its Rust caller and passes it unchanged to helpers and callees.
    // Helper execution is synchronous and cannot outlive that request.
    let context = unsafe { &mut *runtime };
    Some(operation(context))
}

fn ir_constant_value(constant: &php_ir::IrConstant) -> Result<Value, String> {
    match constant {
        php_ir::IrConstant::Null => Ok(Value::Null),
        php_ir::IrConstant::Bool(value) => Ok(Value::Bool(*value)),
        php_ir::IrConstant::Int(value) => Ok(Value::Int(*value)),
        php_ir::IrConstant::Float(value) => Ok(Value::float(*value)),
        php_ir::IrConstant::String(value) => Ok(Value::String(PhpString::from_bytes(
            value.as_bytes().to_vec(),
        ))),
        php_ir::IrConstant::StringBytes(value) => {
            Ok(Value::String(PhpString::from_bytes(value.clone())))
        }
        php_ir::IrConstant::Array(entries) => {
            let mut array = php_runtime::api::PhpArray::new();
            for entry in entries {
                let value = ir_constant_value(&entry.value)?;
                if let Some(key) = &entry.key {
                    let key = ir_constant_value(key)?;
                    let key = php_runtime::api::ArrayKey::from_value(&key)
                        .ok_or_else(|| "native constant array key is invalid".to_owned())?;
                    array.insert(key, value);
                } else {
                    array
                        .try_append(value)
                        .map_err(|error| format!("E_PHP_THROW:Error:{error}"))?;
                }
            }
            Ok(Value::Array(array))
        }
        other => Err(format!(
            "native constant {other:?} requires runtime resolution"
        )),
    }
}

fn native_runtime_constant_value(
    context: &NativeExecutionContext<'_>,
    constant: &php_ir::IrConstant,
) -> Result<Value, String> {
    fn resolve(
        context: &NativeExecutionContext<'_>,
        constant: &php_ir::IrConstant,
        depth: usize,
    ) -> Result<Value, String> {
        if depth > 32 {
            return Err("native constant resolution exceeded its recursion limit".to_owned());
        }
        match constant {
            php_ir::IrConstant::NamedConstant(name) => context.lookup_constant(name),
            php_ir::IrConstant::ClassConstant {
                class_name,
                display_class_name: _,
                constant_name,
            } => {
                let normalized = normalize_class_name(class_name);
                if let Some(entry) = context
                    .unit
                    .classes
                    .iter()
                    .find(|class| class.name == normalized)
                    .and_then(|class| {
                        class
                            .constants
                            .iter()
                            .find(|entry| entry.name.eq_ignore_ascii_case(constant_name))
                    })
                {
                    if let Some(value) = entry
                        .value
                        .and_then(|id| context.unit.constants.get(id.index()))
                    {
                        return resolve(context, value, depth + 1);
                    }
                    if let Some(reference) = &entry.value_named_constant {
                        for name in &reference.names {
                            if let Ok(value) = context.lookup_constant(name) {
                                return Ok(value);
                            }
                        }
                    }
                }
                if let Some((unit, class)) = native_external_class_handle(context, &normalized)
                    && let Some(entry) = class
                        .constants
                        .iter()
                        .find(|entry| entry.name.eq_ignore_ascii_case(constant_name))
                    && let Some(value) = entry.value.and_then(|id| {
                        context
                            .dynamic_units
                            .get(unit)
                            .and_then(|package| package.compiled.unit().constants.get(id.index()))
                    })
                {
                    return resolve(context, value, depth + 1);
                }
                Err(format!("Undefined constant {class_name}::{constant_name}"))
            }
            php_ir::IrConstant::Array(entries) => {
                let mut array = php_runtime::api::PhpArray::new();
                for entry in entries {
                    let value = resolve(context, &entry.value, depth + 1)?;
                    if let Some(key) = &entry.key {
                        let key = resolve(context, key, depth + 1)?;
                        let key = php_runtime::api::ArrayKey::from_value(&key)
                            .ok_or_else(|| "native constant array key is invalid".to_owned())?;
                        array.insert(key, value);
                    } else {
                        array
                            .try_append(value)
                            .map_err(|error| format!("E_PHP_THROW:Error:{error}"))?;
                    }
                }
                Ok(Value::Array(array))
            }
            value => ir_constant_value(value),
        }
    }
    resolve(context, constant, 0)
}

fn native_runtime_type(type_: &php_ir::IrReturnType) -> php_runtime::api::RuntimeType {
    use php_ir::IrReturnType as Ir;
    use php_runtime::api::RuntimeType as Runtime;
    match type_ {
        Ir::Int => Runtime::Int,
        Ir::Float => Runtime::Float,
        Ir::String => Runtime::String,
        Ir::Array => Runtime::Array,
        Ir::Callable => Runtime::Callable,
        Ir::Iterable => Runtime::Iterable,
        Ir::Object => Runtime::Object,
        Ir::Bool => Runtime::Bool,
        Ir::Null => Runtime::Null,
        Ir::Void => Runtime::Void,
        Ir::Mixed => Runtime::Mixed,
        Ir::Never => Runtime::Never,
        Ir::False => Runtime::False,
        Ir::True => Runtime::True,
        Ir::Class { name, display_name } => Runtime::Class {
            name: name.clone(),
            display_name: display_name.clone(),
        },
        Ir::Nullable { inner } => Runtime::Nullable {
            inner: Box::new(native_runtime_type(inner)),
        },
        Ir::Union { members } => Runtime::Union {
            members: members.iter().map(native_runtime_type).collect(),
        },
        Ir::Intersection { members } => Runtime::Intersection {
            members: members.iter().map(native_runtime_type).collect(),
        },
        Ir::Dnf { members } => Runtime::Dnf {
            clauses: members.iter().map(native_runtime_type).collect(),
        },
    }
}

fn native_value_matches_ir_type(value: &Value, type_: &php_ir::IrReturnType) -> bool {
    use php_ir::IrReturnType as Ir;
    let value = match value {
        Value::Reference(reference) => {
            return native_value_matches_ir_type(&reference.get(), type_);
        }
        value => value,
    };
    match type_ {
        Ir::Int => matches!(value, Value::Int(_)),
        Ir::Float => matches!(value, Value::Float(_) | Value::Int(_)),
        Ir::String => matches!(value, Value::String(_)),
        Ir::Array => matches!(value, Value::Array(_)),
        Ir::Callable => matches!(value, Value::Callable(_)),
        Ir::Iterable => matches!(value, Value::Array(_) | Value::Object(_)),
        Ir::Object | Ir::Class { .. } => matches!(value, Value::Object(_)),
        Ir::Bool => matches!(value, Value::Bool(_)),
        Ir::Null | Ir::Void => matches!(value, Value::Null),
        Ir::Mixed => true,
        Ir::Never => false,
        Ir::False => matches!(value, Value::Bool(false)),
        Ir::True => matches!(value, Value::Bool(true)),
        Ir::Nullable { inner } => {
            matches!(value, Value::Null) || native_value_matches_ir_type(value, inner)
        }
        Ir::Union { members } => members
            .iter()
            .any(|member| native_value_matches_ir_type(value, member)),
        Ir::Intersection { members } => members
            .iter()
            .all(|member| native_value_matches_ir_type(value, member)),
        Ir::Dnf { members } => members
            .iter()
            .any(|member| native_value_matches_ir_type(value, member)),
    }
}

fn native_value_matches_ir_type_in_context(
    context: &NativeExecutionContext<'_>,
    value: &Value,
    type_: &php_ir::IrReturnType,
) -> bool {
    use php_ir::IrReturnType as Ir;
    let value = match value {
        Value::Reference(reference) => {
            return native_value_matches_ir_type_in_context(context, &reference.get(), type_);
        }
        value => value,
    };
    match type_ {
        Ir::Class { name, .. } => match value {
            Value::Object(object) => native_class_is_a(context, &object.class_name(), name),
            _ => false,
        },
        Ir::Nullable { inner } => {
            matches!(value, Value::Null)
                || native_value_matches_ir_type_in_context(context, value, inner)
        }
        Ir::Union { members } | Ir::Dnf { members } => members
            .iter()
            .any(|member| native_value_matches_ir_type_in_context(context, value, member)),
        Ir::Intersection { members } => members
            .iter()
            .all(|member| native_value_matches_ir_type_in_context(context, value, member)),
        _ => native_value_matches_ir_type(value, type_),
    }
}

fn native_value_is_callable(context: &NativeExecutionContext<'_>, value: &Value) -> bool {
    match value {
        Value::Reference(reference) => native_value_is_callable(context, &reference.get()),
        Value::Callable(_) => true,
        Value::Object(object) => {
            native_method_in_hierarchy(context, &object.class_name(), "__invoke").is_some()
                || native_external_method(context, &object.class_name(), "__invoke").is_some()
        }
        Value::String(name) => {
            let name = name.to_string_lossy();
            if let Some((class, method)) = name.split_once("::") {
                native_method_in_hierarchy(context, class, method).is_some()
                    || native_external_method(context, class, method).is_some()
            } else {
                context.function_id(&name).is_some()
                    || context.external_function(&name).is_some()
                    || php_extensions::BuiltinRegistry::new().contains(&name.to_ascii_lowercase())
            }
        }
        Value::Array(array) if array.len() == 2 => {
            let target = array.get(&php_runtime::api::ArrayKey::Int(0));
            let method = array.get(&php_runtime::api::ArrayKey::Int(1));
            match (target, method) {
                (Some(Value::Object(object)), Some(Value::String(method))) => {
                    let class = object.class_name();
                    native_method_in_hierarchy(context, &class, &method.to_string_lossy()).is_some()
                        || native_external_method(context, &class, &method.to_string_lossy())
                            .is_some()
                }
                (Some(Value::String(class)), Some(Value::String(method))) => {
                    let class = class.to_string_lossy();
                    native_method_in_hierarchy(context, &class, &method.to_string_lossy()).is_some()
                        || native_external_method(context, &class, &method.to_string_lossy())
                            .is_some()
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn native_ir_type_name(type_: &php_ir::IrReturnType) -> String {
    use php_ir::IrReturnType as Ir;
    match type_ {
        Ir::Int => "int".to_owned(),
        Ir::Float => "float".to_owned(),
        Ir::String => "string".to_owned(),
        Ir::Array => "array".to_owned(),
        Ir::Callable => "callable".to_owned(),
        Ir::Iterable => "iterable".to_owned(),
        Ir::Object => "object".to_owned(),
        Ir::Bool => "bool".to_owned(),
        Ir::Null => "null".to_owned(),
        Ir::Void => "void".to_owned(),
        Ir::Mixed => "mixed".to_owned(),
        Ir::Never => "never".to_owned(),
        Ir::False => "false".to_owned(),
        Ir::True => "true".to_owned(),
        Ir::Class { display_name, name } => display_name.clone().unwrap_or_else(|| name.clone()),
        Ir::Nullable { inner } => format!("?{}", native_ir_type_name(inner)),
        Ir::Union { members } => {
            let mut names = members.iter().map(native_ir_type_name).collect::<Vec<_>>();
            if names.len() == 2
                && names.iter().any(|name| name == "int")
                && names.iter().any(|name| name == "string")
            {
                names = vec!["string".to_owned(), "int".to_owned()];
            }
            names.join("|")
        }
        Ir::Intersection { members } => members
            .iter()
            .map(native_ir_type_name)
            .collect::<Vec<_>>()
            .join("&"),
        Ir::Dnf { members } => members
            .iter()
            .map(native_ir_type_name)
            .collect::<Vec<_>>()
            .join("|"),
    }
}

fn native_runtime_class_with_owner(
    context: &NativeExecutionContext<'_>,
    owner_unit: Option<usize>,
    class: &php_ir::module::ClassEntry,
) -> Result<php_runtime::api::ClassEntry, String> {
    use php_runtime::api as runtime;

    let owner_ir_unit = |owner: Option<usize>| -> Option<&php_ir::IrUnit> {
        match owner {
            None => Some(&*context.unit),
            Some(unit) => context
                .dynamic_units
                .get(unit)
                .map(|package| package.compiled.unit()),
        }
    };
    let mut lineage = Vec::new();
    let mut current = Some((owner_unit, class));
    let mut visited = std::collections::BTreeSet::new();
    while let Some((owner, candidate)) = current {
        if !visited.insert(candidate.name.clone()) {
            return Err(format!(
                "native class hierarchy for {} contains a cycle",
                class.display_name
            ));
        }
        let parent = candidate.parent.clone();
        lineage.push((owner, candidate));
        current = parent.as_deref().and_then(|parent| {
            let parent = normalize_class_name(parent);
            owner_ir_unit(owner)
                .into_iter()
                .flat_map(|unit| &unit.classes)
                .find(|class| class.name == parent)
                .map(|class| (owner, class))
                .or_else(|| {
                    native_external_class_ref(context, &parent)
                        .map(|(unit, class)| (Some(unit), class))
                })
        });
    }
    lineage.reverse();
    let properties = lineage
        .iter()
        .flat_map(|(owner, class)| {
            class
                .properties
                .iter()
                .map(move |property| (*owner, property))
        })
        .map(|(owner, property)| {
            let default = property
                .default
                .and_then(|constant| owner_ir_unit(owner)?.constants.get(constant.index()))
                .map(|value| native_runtime_constant_value(context, value))
                .transpose()?
                .unwrap_or_else(|| {
                    if property.flags.is_typed {
                        Value::Uninitialized
                    } else {
                        Value::Null
                    }
                });
            Ok(runtime::ClassPropertyEntry {
                name: property.name.clone(),
                default,
                type_: property.type_.as_ref().map(native_runtime_type),
                flags: runtime::ClassPropertyFlags {
                    is_static: property.flags.is_static,
                    is_private: property.flags.is_private,
                    is_protected: property.flags.is_protected,
                    set_is_private: property.flags.set_is_private,
                    set_is_protected: property.flags.set_is_protected,
                    is_readonly: property.flags.is_readonly,
                    is_typed: property.flags.is_typed,
                },
                hooks: runtime::ClassPropertyHooks {
                    get_function_id: property.hooks.get.map(|function| function.raw()),
                    set_function_id: property.hooks.set.map(|function| function.raw()),
                    backed: property.hooks.backed,
                },
                attributes: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let runtime_class = runtime::ClassEntry {
        name: class.name.clone().into(),
        parent: class.parent.clone(),
        interfaces: class.interfaces.clone(),
        methods: lineage
            .iter()
            .flat_map(|(_, class)| &class.methods)
            .map(|method| runtime::ClassMethodEntry {
                name: method.name.clone(),
                origin_class: method.origin_class.clone(),
                function_id: method.function.raw(),
                flags: runtime::ClassMethodFlags {
                    is_static: method.flags.is_static,
                    is_private: method.flags.is_private,
                    is_protected: method.flags.is_protected,
                    is_abstract: method.flags.is_abstract,
                    is_final: method.flags.is_final,
                },
                attributes: Vec::new(),
            })
            .collect(),
        properties,
        constants: class
            .constants
            .iter()
            .filter_map(|constant| {
                let value = constant
                    .value
                    .and_then(|value| owner_ir_unit(owner_unit)?.constants.get(value.index()))
                    .and_then(|value| native_runtime_constant_value(context, value).ok())?;
                Some(runtime::ClassConstantEntry {
                    name: constant.name.clone(),
                    value,
                    flags: runtime::ClassConstantFlags {
                        is_private: constant.flags.is_private,
                        is_protected: constant.flags.is_protected,
                    },
                    attributes: Vec::new(),
                })
            })
            .collect(),
        enum_cases: class
            .enum_cases
            .iter()
            .map(|case| runtime::ClassEnumCaseEntry {
                name: case.name.clone(),
                value: case
                    .value
                    .and_then(|value| owner_ir_unit(owner_unit)?.constants.get(value.index()))
                    .and_then(|value| ir_constant_value(value).ok()),
                attributes: Vec::new(),
            })
            .collect(),
        attributes: Vec::new(),
        enum_backing_type: class.enum_backing_type.map(|backing| match backing {
            php_ir::module::ClassEnumBackingType::Int => runtime::ClassEnumBackingType::Int,
            php_ir::module::ClassEnumBackingType::String => runtime::ClassEnumBackingType::String,
        }),
        constructor_id: class.constructor.map(|function| function.raw()),
        flags: runtime::ClassFlags {
            is_abstract: class.flags.is_abstract || class.flags.is_trait,
            is_final: class.flags.is_final,
            is_readonly: class.flags.is_readonly,
            is_interface: class.flags.is_interface,
            is_enum: class.flags.is_enum,
        },
    };
    Ok(runtime_class)
}

fn new_native_object(
    context: &NativeExecutionContext<'_>,
    owner_unit: Option<usize>,
    class: &php_ir::module::ClassEntry,
) -> Result<php_runtime::api::ObjectRef, String> {
    let key = (owner_unit, class.name.clone());
    let prepared = if let Some(prepared) = context.runtime_class_cache.borrow().get(&key) {
        Rc::clone(prepared)
    } else {
        let entry = native_runtime_class_with_owner(context, owner_unit, class)?;
        let default_declared_slots =
            php_runtime::api::ObjectRef::default_declared_slots(&entry, &class.display_name);
        let prepared = Rc::new(PreparedNativeRuntimeClass {
            entry,
            default_declared_slots,
        });
        context
            .runtime_class_cache
            .borrow_mut()
            .insert(key, Rc::clone(&prepared));
        prepared
    };
    Ok(php_runtime::api::ObjectRef::from_layout_slots(
        &prepared.entry,
        class.display_name.clone(),
        prepared.default_declared_slots.clone(),
    ))
}

fn native_prepare_runtime_class_constants(
    context: &mut NativeExecutionContext<'_>,
    owner_unit: Option<usize>,
    class: &php_ir::module::ClassEntry,
    source: &php_ir::Instruction,
) -> Result<(), String> {
    fn prepare_constant(
        context: &mut NativeExecutionContext<'_>,
        constant: &php_ir::IrConstant,
        source: &php_ir::Instruction,
    ) -> Result<(), String> {
        match constant {
            php_ir::IrConstant::ClassConstant {
                class_name,
                display_class_name,
                ..
            } => {
                let autoload_name = if display_class_name.is_empty() {
                    class_name
                } else {
                    display_class_name
                };
                native_autoload_class(context, autoload_name, source)
            }
            php_ir::IrConstant::Array(entries) => {
                for entry in entries {
                    if let Some(key) = &entry.key {
                        prepare_constant(context, key, source)?;
                    }
                    prepare_constant(context, &entry.value, source)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    let constants = match owner_unit {
        None => &context.unit.constants,
        Some(unit) => {
            &context
                .dynamic_units
                .get(unit)
                .ok_or_else(|| format!("dynamic native unit {unit} is missing"))?
                .compiled
                .unit()
                .constants
        }
    };
    let defaults = class
        .properties
        .iter()
        .filter_map(|property| {
            property
                .default
                .and_then(|constant| constants.get(constant.index()))
                .cloned()
        })
        .collect::<Vec<_>>();
    for constant in &defaults {
        prepare_constant(context, constant, source)?;
    }
    Ok(())
}

fn encode_native_enum_case(
    context: &mut NativeExecutionContext<'_>,
    class: &php_ir::module::ClassEntry,
    case: &php_ir::module::ClassEnumCaseEntry,
) -> Result<i64, String> {
    let key = (class.name.clone(), case.name.clone());
    if let Some(object) = context.enum_cases.get(&key).cloned() {
        return context.encode(Value::Object(object));
    }
    let object = new_native_object(context, None, class)?;
    object.set_property(
        "name",
        Value::String(PhpString::from_bytes(case.name.as_bytes().to_vec())),
    );
    if let Some(value) = case
        .value
        .and_then(|value| context.unit.constants.get(value.index()))
        .and_then(|value| ir_constant_value(value).ok())
    {
        object.set_property("value", value);
    }
    context.enum_cases.insert(key, object.clone());
    context.mark_roots_dirty(RootMutationReason::EnumOrStaticObject);
    context.encode(Value::Object(object))
}

struct NativeStaticPropertyDeclaration {
    owner_unit: Option<usize>,
    owner_name: String,
    owner_display_name: String,
    caller_owns_scope: bool,
    flags: php_ir::module::ClassPropertyFlags,
    default: Option<php_ir::ConstId>,
    type_: Option<php_ir::IrReturnType>,
}

fn native_static_property_declaration(
    context: &NativeExecutionContext<'_>,
    class_name: &str,
    property: &str,
    caller_function: u32,
) -> Option<NativeStaticPropertyDeclaration> {
    let mut candidate = normalize_class_name(class_name);
    let mut visited = std::collections::BTreeSet::new();
    while visited.insert(candidate.clone()) {
        let (unit, class) = if let Some(class) = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == candidate)
        {
            (None, class)
        } else {
            let (unit, class) = native_external_class_ref(context, &candidate)?;
            (Some(unit), class)
        };
        if let Some(entry) = class
            .properties
            .iter()
            .find(|entry| entry.flags.is_static && entry.name == property)
        {
            return Some(NativeStaticPropertyDeclaration {
                owner_unit: unit,
                owner_name: class.name.clone(),
                owner_display_name: class.display_name.clone(),
                caller_owns_scope: class
                    .methods
                    .iter()
                    .any(|method| method.function.raw() == caller_function),
                flags: entry.flags,
                default: entry.default,
                type_: entry.type_.clone(),
            });
        }
        candidate = normalize_class_name(class.parent.as_ref()?);
    }
    None
}

fn native_nested_array_reference(
    value: &mut Value,
    keys: &[php_runtime::api::ArrayKey],
) -> Result<php_runtime::api::ReferenceCell, String> {
    if keys.is_empty() {
        return Ok(match value {
            Value::Reference(reference) => reference.clone(),
            value => {
                let reference = php_runtime::api::ReferenceCell::new(value.clone());
                *value = Value::Reference(reference.clone());
                reference
            }
        });
    }

    if let Value::Reference(reference) = value {
        let mut referenced = reference.get();
        let result = native_nested_array_reference(&mut referenced, keys)?;
        reference.set(referenced);
        return Ok(result);
    }

    if matches!(value, Value::Null | Value::Uninitialized) {
        *value = Value::Array(php_runtime::api::PhpArray::new());
    }
    let Value::Array(array) = value else {
        return Err(format!(
            "Cannot use a value of type {} as an array",
            native_value_type_name(value)
        ));
    };

    let key = keys[0].clone();
    let mut element = array.get(&key).cloned().unwrap_or(Value::Null);
    let reference = native_nested_array_reference(&mut element, &keys[1..])?;
    array.insert(key, element);
    Ok(reference)
}

fn dereference_native_assignment_value(mut value: Value) -> Value {
    for _ in 0..16 {
        let Value::Reference(reference) = value else {
            break;
        };
        value = reference.get();
    }
    value
}

fn execute_native_static_property(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
    caller_function: u32,
) -> Option<Result<i64, String>> {
    if let php_ir::InstructionKind::BindReferenceFromStaticPropertyDim {
        class_name,
        property,
        dims,
        ..
    } = &instruction.kind
    {
        let keys = match arguments
            .iter()
            .map(|argument| {
                context.decode(*argument).and_then(|value| {
                    php_runtime::api::ArrayKey::from_value(&value)
                        .ok_or_else(|| "Illegal offset type".to_owned())
                })
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(keys) if keys.len() == dims.len() => keys,
            Ok(_) => {
                return Some(Err(
                    "static property dimension operands are missing".to_owned()
                ));
            }
            Err(error) => return Some(Err(error)),
        };
        let calling_class = native_calling_class(context, caller_function);
        let resolved_class = match class_name.to_ascii_lowercase().as_str() {
            "self" => calling_class.map_or_else(|| class_name.clone(), |class| class.name.clone()),
            "parent" => calling_class
                .and_then(|class| class.parent.clone())
                .unwrap_or_else(|| class_name.clone()),
            "static" => context
                .called_classes
                .last()
                .map(|class| class.to_string())
                .or_else(|| calling_class.map(|class| class.name.clone()))
                .unwrap_or_else(|| class_name.clone()),
            _ => class_name.clone(),
        };
        let Some(declaration) =
            native_static_property_declaration(context, &resolved_class, property, caller_function)
        else {
            return Some(Err(format!(
                "E_PHP_THROW:Error:Access to undeclared static property {resolved_class}::${property}"
            )));
        };
        let key = (declaration.owner_name, property.clone());
        let current = context.static_properties.get(&key).cloned().or_else(|| {
            declaration
                .default
                .and_then(|constant| {
                    if declaration.owner_unit.is_none() {
                        context.unit.constants.get(constant.index())
                    } else {
                        declaration.owner_unit.and_then(|unit| {
                            context.dynamic_units.get(unit).and_then(|package| {
                                package.compiled.unit().constants.get(constant.index())
                            })
                        })
                    }
                })
                .and_then(|constant| ir_constant_value(constant).ok())
        });
        let reference = match current.unwrap_or(Value::Null) {
            Value::Reference(reference) => reference,
            value => php_runtime::api::ReferenceCell::new(value),
        };
        context
            .static_properties
            .insert(key, Value::Reference(reference.clone()));
        context.mark_roots_dirty(RootMutationReason::EnumOrStaticObject);
        if keys.is_empty() {
            return Some(context.encode(Value::Reference(reference)));
        }
        let mut root = Value::Reference(reference);
        let reference = match native_nested_array_reference(&mut root, &keys) {
            Ok(reference) => reference,
            Err(error) => return Some(Err(error)),
        };
        return Some(context.encode(Value::Reference(reference)));
    }
    let (class_name, property, assigned, bind_reference) = match &instruction.kind {
        php_ir::InstructionKind::FetchStaticProperty {
            class_name,
            property,
            ..
        } => (class_name.clone(), property.clone(), None, false),
        php_ir::InstructionKind::AssignStaticProperty {
            class_name,
            property,
            ..
        } => {
            let Some(value) = arguments.first() else {
                return Some(Err("static property assignment value is missing".to_owned()));
            };
            (class_name.clone(), property.clone(), Some(*value), false)
        }
        php_ir::InstructionKind::AssignDynamicStaticProperty { property, .. } => {
            let [class_name, value] = arguments else {
                return Some(Err(
                    "dynamic static property assignment operands are missing".to_owned(),
                ));
            };
            let class_name = match context.decode(*class_name) {
                Ok(Value::Reference(reference)) => reference.get(),
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let class_name = match class_name {
                Value::String(class_name) => class_name.to_string_lossy(),
                Value::Object(object) => object.class_name(),
                value => {
                    return Some(Err(format!(
                        "class name must be a valid object or a string, {} given",
                        native_value_type_name(&value)
                    )));
                }
            };
            (class_name, property.clone(), Some(*value), false)
        }
        php_ir::InstructionKind::FetchDynamicStaticProperty { property, .. } => {
            let Some(class_name) = arguments.first() else {
                return Some(Err(
                    "dynamic static property class operand is missing".to_owned()
                ));
            };
            let class_name = match context.decode(*class_name) {
                Ok(Value::Reference(reference)) => reference.get(),
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let class_name = match class_name {
                Value::String(class_name) => class_name.to_string_lossy(),
                Value::Object(object) => object.class_name(),
                value => {
                    return Some(Err(format!(
                        "class name must be a valid object or a string, {} given",
                        native_value_type_name(&value)
                    )));
                }
            };
            (class_name, property.clone(), None, false)
        }
        php_ir::InstructionKind::BindReferenceStaticProperty {
            class_name,
            property,
            ..
        } => {
            let Some(value) = arguments.first() else {
                return Some(Err("static property reference source is missing".to_owned()));
            };
            (class_name.clone(), property.clone(), Some(*value), true)
        }
        php_ir::InstructionKind::IssetStaticProperty {
            class_name,
            property,
            ..
        }
        | php_ir::InstructionKind::EmptyStaticProperty {
            class_name,
            property,
            ..
        }
        | php_ir::InstructionKind::IssetStaticPropertyDim {
            class_name,
            property,
            ..
        }
        | php_ir::InstructionKind::EmptyStaticPropertyDim {
            class_name,
            property,
            ..
        }
        | php_ir::InstructionKind::UnsetStaticPropertyDim {
            class_name,
            property,
            ..
        } => (class_name.clone(), property.clone(), None, false),
        _ => return None,
    };
    let calling_class = native_calling_class(context, caller_function);
    let resolved_class = match class_name.to_ascii_lowercase().as_str() {
        "self" => calling_class.map_or_else(|| class_name.clone(), |class| class.name.clone()),
        "parent" => calling_class
            .and_then(|class| class.parent.clone())
            .unwrap_or_else(|| class_name.clone()),
        "static" => context
            .called_classes
            .last()
            .map(|class| class.to_string())
            .or_else(|| calling_class.map(|class| class.name.clone()))
            .unwrap_or_else(|| class_name.clone()),
        _ => class_name.clone(),
    };
    let normalized = normalize_class_name(&resolved_class);
    let requested_local_display_name = context
        .unit
        .classes
        .iter()
        .find(|class| class.name == normalized)
        .map(|class| class.display_name.clone());
    if requested_local_display_name.is_none()
        && !native_external_class_exists(context, &resolved_class)
        && context.autoload_in_progress.insert(normalized.clone())
    {
        let callbacks = context.autoload_callbacks.clone();
        for callback in callbacks {
            if let Err(error) = invoke_native_callable_value(
                context,
                callback,
                &[Value::String(PhpString::from_bytes(
                    resolved_class.as_bytes().to_vec(),
                ))],
                instruction,
                None,
            ) {
                context.autoload_in_progress.remove(&normalized);
                return Some(Err(error));
            }
            if native_external_class_exists(context, &resolved_class) {
                break;
            }
        }
        context.autoload_in_progress.remove(&normalized);
    }
    let requested_display_name = requested_local_display_name
        .or_else(|| {
            native_external_class_ref(context, &resolved_class)
                .map(|(_, class)| class.display_name.clone())
        })
        .unwrap_or_else(|| resolved_class.clone());
    let Some(declaration) =
        native_static_property_declaration(context, &resolved_class, &property, caller_function)
    else {
        if matches!(
            instruction.kind,
            php_ir::InstructionKind::IssetStaticProperty { .. }
                | php_ir::InstructionKind::IssetStaticPropertyDim { .. }
        ) {
            return Some(context.encode(Value::Bool(false)));
        }
        if matches!(
            instruction.kind,
            php_ir::InstructionKind::EmptyStaticProperty { .. }
                | php_ir::InstructionKind::EmptyStaticPropertyDim { .. }
        ) {
            return Some(context.encode(Value::Bool(true)));
        }
        return Some(Err(format!(
            "E_PHP_THROW:Error:Access to undeclared static property {requested_display_name}::${property}"
        )));
    };
    let display_name = declaration.owner_display_name;
    if (declaration.flags.is_private || declaration.flags.is_protected)
        && !declaration.caller_owns_scope
    {
        return Some(Err(format!(
            "E_PHP_THROW:Error:Cannot access {} property {}::${property}",
            if declaration.flags.is_private {
                "private"
            } else {
                "protected"
            },
            display_name
        )));
    }
    let key = (declaration.owner_name, property.clone());
    let result = if bind_reference {
        let Some(source) = assigned else {
            return Some(Err("static property reference source is missing".to_owned()));
        };
        let value = match context.decode(source) {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        };
        let reference = match value {
            Value::Reference(reference) => reference,
            value => php_runtime::api::ReferenceCell::new(value),
        };
        let effective = reference.get();
        if let Some(type_) = &declaration.type_
            && !native_value_matches_ir_type_in_context(context, &effective, type_)
        {
            return Some(Err(format!(
                "E_PHP_THROW:TypeError:Cannot assign {} to property {}::${} of type {}",
                native_assignment_type_name(&effective),
                display_name,
                property,
                native_ir_type_name(type_)
            )));
        }
        let previous = context
            .static_properties
            .insert(key.clone(), Value::Reference(reference.clone()));
        context.mark_roots_dirty(RootMutationReason::EnumOrStaticObject);
        if let Some(previous) = previous.map(dereference_native_assignment_value)
            && let Value::Object(previous) = previous
            && let Err(error) = context.run_object_destructor(previous)
        {
            return Some(Err(error));
        }
        Value::Reference(reference)
    } else if let Some(assigned) = assigned {
        let mut value = match context.decode(assigned) {
            Ok(value) => dereference_native_assignment_value(value),
            Err(error) => return Some(Err(error)),
        };
        if declaration.owner_unit.is_some() {
            // Closure function ids are unit-local. Preserve the assigning
            // unit when a closure crosses into a class owned by another unit.
            value = native_value_with_owner_unit(value, context.current_dynamic_unit);
        }
        if let Some(type_) = &declaration.type_
            && !native_value_matches_ir_type_in_context(context, &value, type_)
        {
            return Some(Err(format!(
                "E_PHP_THROW:TypeError:Cannot assign {} to property {}::${} of type {}",
                native_assignment_type_name(&value),
                display_name,
                property,
                native_ir_type_name(type_)
            )));
        }
        let existing_reference = context.static_properties.get(&key).and_then(|current| {
            let Value::Reference(reference) = current else {
                return None;
            };
            Some(reference.clone())
        });
        let previous = if let Some(reference) = existing_reference {
            let previous = reference.get();
            reference.set(value.clone());
            Some(previous)
        } else {
            context.static_properties.insert(key.clone(), value.clone())
        };
        context.mark_roots_dirty(RootMutationReason::EnumOrStaticObject);
        if let Some(Value::Object(previous)) = previous
            && let Err(error) = context.run_object_destructor(previous)
        {
            return Some(Err(error));
        }
        value
    } else if let Some(value) = context.static_properties.get(&key).cloned() {
        value
    } else {
        let value = declaration.default.and_then(|constant| {
            if declaration.owner_unit.is_none() {
                context.unit.constants.get(constant.index())
            } else {
                declaration.owner_unit.and_then(|unit| {
                    context
                        .dynamic_units
                        .get(unit)
                        .and_then(|package| package.compiled.unit().constants.get(constant.index()))
                })
            }
        });
        let value = value.map_or(Ok(Value::Null), |value| {
            native_runtime_constant_value(context, value)
        });
        match value {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        }
    };
    let result = match &instruction.kind {
        php_ir::InstructionKind::IssetStaticProperty { .. } => {
            Value::Bool(!matches!(result, Value::Null | Value::Uninitialized))
        }
        php_ir::InstructionKind::EmptyStaticProperty { .. } => {
            Value::Bool(!native_property_truthy(&result))
        }
        php_ir::InstructionKind::IssetStaticPropertyDim { dims, .. } => {
            let value = match native_dimension_path_value(
                context,
                Some(result),
                arguments,
                dims.len(),
                instruction,
                NativeDimensionOperation::Fetch { quiet: true },
            ) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            Value::Bool(
                value.is_some_and(|value| !matches!(value, Value::Null | Value::Uninitialized)),
            )
        }
        php_ir::InstructionKind::EmptyStaticPropertyDim { dims, .. } => {
            let value = match native_dimension_path_value(
                context,
                Some(result),
                arguments,
                dims.len(),
                instruction,
                NativeDimensionOperation::Fetch { quiet: true },
            ) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            Value::Bool(value.is_none_or(|value| !native_property_truthy(&value)))
        }
        php_ir::InstructionKind::UnsetStaticPropertyDim { dims, .. } => {
            let keys = arguments
                .iter()
                .take(dims.len())
                .map(|encoded| {
                    context
                        .decode(*encoded)
                        .ok()
                        .and_then(|value| php_runtime::api::ArrayKey::from_value(&value))
                })
                .collect::<Option<Vec<_>>>();
            if let Some(keys) = keys {
                match result {
                    Value::Reference(reference) => {
                        let mut value = reference.get();
                        unset_native_array_dims(&mut value, &keys);
                        reference.set(value);
                        context.mark_roots_dirty(RootMutationReason::EnumOrStaticObject);
                    }
                    mut value => {
                        unset_native_array_dims(&mut value, &keys);
                        context.static_properties.insert(key.clone(), value);
                        context.mark_roots_dirty(RootMutationReason::EnumOrStaticObject);
                    }
                }
            }
            Value::Null
        }
        php_ir::InstructionKind::FetchStaticProperty { .. }
        | php_ir::InstructionKind::FetchDynamicStaticProperty { .. }
        | php_ir::InstructionKind::AssignStaticProperty { .. }
        | php_ir::InstructionKind::AssignDynamicStaticProperty { .. } => {
            dereference_native_assignment_value(result)
        }
        php_ir::InstructionKind::BindReferenceStaticProperty { .. } => result,
        _ => result,
    };
    Some(context.encode(result))
}

fn native_dimension_path_value(
    context: &mut NativeExecutionContext<'_>,
    mut value: Option<Value>,
    arguments: &[i64],
    dimension_count: usize,
    source: &php_ir::Instruction,
    operation: NativeDimensionOperation,
) -> Result<Option<Value>, String> {
    if arguments.len() != dimension_count {
        return Ok(None);
    }
    for encoded in arguments {
        let Some(mut target) = value else {
            return Ok(None);
        };
        while let Value::Reference(reference) = target {
            target = reference.get();
        }
        let mut key = context.decode(*encoded)?;
        while let Value::Reference(reference) = key {
            key = reference.get();
        }
        emit_native_dimension_conversion_diagnostic(
            context,
            &target,
            &key,
            Some(source),
            operation,
        )?;
        let Some(key) = php_runtime::api::ArrayKey::from_value(&key) else {
            return Ok(None);
        };
        value = match target {
            Value::Array(array) => array.get(&key).cloned(),
            Value::Object(object) => native_simple_xml_dimension(&object, &key),
            _ => None,
        };
    }
    if let Some(mut value) = value {
        while let Value::Reference(reference) = value {
            value = reference.get();
        }
        Ok(Some(value))
    } else {
        Ok(None)
    }
}

fn native_property_truthy(value: &Value) -> bool {
    match value {
        Value::Null | Value::Uninitialized | Value::Bool(false) => false,
        Value::Int(0) => false,
        Value::Float(value) if value.to_f64() == 0.0 => false,
        Value::String(value) if value.as_bytes().is_empty() || value.as_bytes() == b"0" => false,
        Value::Array(value) if value.is_empty() => false,
        Value::Reference(reference) => native_property_truthy(&reference.get()),
        Value::Object(object) if native_simple_xml_empty(object).is_some() => {
            !native_simple_xml_empty(object).unwrap_or(true)
        }
        _ => true,
    }
}

fn native_property_is_set(value: &Value) -> bool {
    match value {
        Value::Null | Value::Uninitialized => false,
        Value::Reference(reference) => native_property_is_set(&reference.get()),
        _ => true,
    }
}

fn unset_native_array_dims(value: &mut Value, keys: &[php_runtime::api::ArrayKey]) {
    if let Value::Reference(reference) = value {
        let mut target = reference.get();
        unset_native_array_dims(&mut target, keys);
        reference.set(target);
        return;
    }
    let Some((key, rest)) = keys.split_first() else {
        return;
    };
    let Value::Array(array) = value else {
        return;
    };
    if rest.is_empty() {
        array.remove(key);
    } else if let Some(mut nested) = array.get_mut(key) {
        unset_native_array_dims(&mut nested, rest);
    }
}

fn assign_native_array_dims(
    value: &mut Value,
    keys: &[php_runtime::api::ArrayKey],
    replacement: Value,
    append: bool,
) {
    if let Value::Reference(reference) = value {
        let mut target = reference.get();
        assign_native_array_dims(&mut target, keys, replacement, append);
        reference.set(target);
        return;
    }
    if !matches!(value, Value::Array(_)) {
        *value = Value::Array(php_runtime::api::PhpArray::new());
    }
    let Value::Array(array) = value else {
        unreachable!("array value was initialized above")
    };
    let Some((key, rest)) = keys.split_first() else {
        if append {
            array.append(replacement);
        }
        return;
    };
    if rest.is_empty() && !append {
        if let Some(Value::Reference(reference)) = array.get(key).cloned() {
            reference.set(replacement);
        } else {
            array.insert(key.clone(), replacement);
        }
    } else {
        let mut nested = array.get(key).cloned().unwrap_or(Value::Null);
        assign_native_array_dims(&mut nested, rest, replacement, append);
        array.insert(key.clone(), nested);
    }
}

fn native_external_method(
    context: &NativeExecutionContext<'_>,
    class_name: &str,
    method: &str,
) -> Option<(NativeDynamicFunction, php_ir::module::ClassMethodEntry)> {
    let (mut unit, mut class) =
        native_external_class_handle(context, class_name).or_else(|| {
            let local = context
                .unit
                .classes
                .iter()
                .find(|class| class.name == normalize_class_name(class_name))?;
            native_external_class_handle(context, local.parent.as_deref()?)
        })?;
    loop {
        if let Some(entry) = class
            .methods
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(method))
            .cloned()
        {
            return Some((
                NativeDynamicFunction {
                    unit,
                    function: entry.function,
                },
                entry,
            ));
        }
        let parent = class.parent.as_deref()?;
        let normalized_parent = normalize_class_name(parent);
        let (parent_unit, parent_class) = context
            .current_dynamic_unit
            .and_then(|unit| {
                context
                    .dynamic_units
                    .get(unit)?
                    .compiled
                    .lookup_unit_class_handle(&normalized_parent)
                    .map(|class| (unit, class))
            })
            .or_else(|| native_external_class_handle(context, parent))?;
        unit = parent_unit;
        class = parent_class;
    }
}

fn create_native_external_object(
    context: &mut NativeExecutionContext<'_>,
    class_name: &str,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    let (unit, class) = native_external_class_handle(context, class_name)
        .ok_or_else(|| format!("E_PHP_VM_UNKNOWN_CLASS: Class {class_name} not found"))?;
    if class.flags.is_abstract
        || class.flags.is_interface
        || class.flags.is_trait
        || class.flags.is_enum
    {
        return Err(format!(
            "Cannot instantiate {} {}",
            class_name, class.display_name
        ));
    }
    native_prepare_runtime_class_constants(context, Some(unit), &class, source)?;
    let object = new_native_object(context, Some(unit), &class)?;
    let receiver = context.encode(Value::Object(object))?;
    if let Some((constructor, _)) = native_external_method(context, class_name, "__construct") {
        let mut constructor_arguments = Vec::with_capacity(arguments.len() + 1);
        constructor_arguments.push(receiver);
        constructor_arguments.extend_from_slice(arguments);
        let _ = invoke_native_external_function(
            context,
            constructor,
            &constructor_arguments,
            Some(class.name.clone()),
            context.unit.strict_types,
        )?;
    }
    Ok(receiver)
}

fn native_coerce_call_argument(value: Value, type_: &php_ir::IrReturnType, strict: bool) -> Value {
    use php_ir::IrReturnType as Type;
    if let Value::Reference(reference) = &value {
        return Value::Reference(reference.clone());
    }
    if matches!(type_, Type::Float)
        && let Value::Int(value) = value
    {
        return Value::Float(php_runtime::api::FloatValue::from_f64(value as f64));
    }
    if strict || native_value_matches_ir_type(&value, type_) {
        return value;
    }
    match (type_, value) {
        (Type::Int, Value::String(value)) => value
            .to_string_lossy()
            .trim()
            .parse::<i64>()
            .map(Value::Int)
            .unwrap_or(Value::String(value)),
        (Type::Int, Value::Float(value)) => Value::Int(value.to_f64() as i64),
        (Type::Int, Value::Bool(value)) => Value::Int(i64::from(value)),
        (Type::Float, Value::String(value)) => value
            .to_string_lossy()
            .trim()
            .parse::<f64>()
            .map(|value| Value::Float(php_runtime::api::FloatValue::from_f64(value)))
            .unwrap_or(Value::String(value)),
        (Type::Float, Value::Bool(value)) => {
            Value::Float(php_runtime::api::FloatValue::from_f64(if value {
                1.0
            } else {
                0.0
            }))
        }
        (Type::String, Value::Int(value)) => {
            Value::String(PhpString::from_bytes(value.to_string().into_bytes()))
        }
        (Type::String, Value::Float(value)) => Value::String(PhpString::from_bytes(
            value.to_f64().to_string().into_bytes(),
        )),
        (Type::String, Value::Bool(value)) => Value::String(PhpString::from_bytes(if value {
            b"1".to_vec()
        } else {
            Vec::new()
        })),
        (Type::Bool, value @ (Value::Int(_) | Value::Float(_) | Value::String(_))) => {
            Value::Bool(native_property_truthy(&value))
        }
        (Type::Nullable { inner }, value) => native_coerce_call_argument(value, inner, strict),
        (Type::Union { members }, value) => members
            .iter()
            .map(|member| native_coerce_call_argument(value.clone(), member, strict))
            .find(|candidate| native_value_matches_ir_type(candidate, type_))
            .unwrap_or(value),
        (_, value) => value,
    }
}

fn native_function_has_implicit_closure_this(function: &php_ir::IrFunction) -> bool {
    function.flags.is_closure
        && !function.flags.is_static
        && function.locals.first().is_some_and(|name| name == "this")
        && !function
            .captures
            .iter()
            .any(|capture| capture.local == php_ir::LocalId::new(0))
}

#[cfg(test)]
fn native_backtrace_frame(
    compiled: &crate::compiled_unit::CompiledUnit,
    function: php_ir::FunctionId,
    called_class: Option<Arc<str>>,
    object: Option<php_runtime::api::ObjectRef>,
    arguments: request_state::NativeTraceArguments,
) -> NativeBacktraceFrame {
    let metadata = NativeFunctionMetadataPtr::from_compiled(compiled, function);
    native_backtrace_frame_from_metadata(metadata, called_class, object, arguments)
}

fn native_backtrace_frame_from_metadata(
    metadata: Option<NativeFunctionMetadataPtr>,
    called_class: Option<Arc<str>>,
    object: Option<php_runtime::api::ObjectRef>,
    arguments: request_state::NativeTraceArguments,
) -> NativeBacktraceFrame {
    let class = metadata.as_ref().and_then(|metadata| {
        metadata
            .trace_class
            .as_ref()
            .map(|class| called_class.unwrap_or_else(|| Arc::clone(class)))
    });
    NativeBacktraceFrame {
        metadata,
        class,
        object,
        arguments,
    }
}

fn invoke_native_external_function(
    context: &mut NativeExecutionContext<'_>,
    target: NativeDynamicFunction,
    arguments: &[i64],
    called_class: Option<String>,
    strict: bool,
) -> Result<i64, String> {
    invoke_native_external_function_with_metadata(
        context,
        target,
        arguments,
        None,
        called_class,
        strict,
    )
}

fn invoke_native_external_function_with_metadata(
    context: &mut NativeExecutionContext<'_>,
    target: NativeDynamicFunction,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    called_class: Option<String>,
    strict: bool,
) -> Result<i64, String> {
    prepare_dynamic_native_entry(context, target.unit, target.function)?;
    let transferred_arguments = arguments
        .iter()
        .map(|argument| {
            let encoded = *argument;
            let unit_local_constant = php_jit::jit_decode_constant(encoded).is_some_and(|index| {
                index != u32::MAX
                    && index != php_jit::JIT_VALUE_UNINITIALIZED
                    && index != php_jit::JIT_VALUE_FALSE
                    && index != php_jit::JIT_VALUE_TRUE
            });
            let direct_array = NativeExecutionContext::direct_value_index(encoded)
                .and_then(|index| context.direct_value_slots.get(index))
                .is_some_and(|slot| {
                    slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
                });
            if unit_local_constant {
                // Constant indexes are scoped to the caller's IrUnit. Convert
                // a scalar/string constant exactly once at the boundary.
                let value = context.decode(encoded)?;
                context.encode(value)
            } else if direct_array {
                // Do not rebuild the entire native array through Rust
                // `Value`.  Stabilize only embedded unit-local constants in
                // the authoritative slots, then share the same COW handle
                // with the external callee.
                context.stabilize_direct_array_for_cross_unit(encoded)?;
                context.duplicate_native_call_argument(encoded)
            } else {
                context.duplicate_native_call_argument(encoded)
            }
        })
        .collect::<Result<smallvec::SmallVec<[i64; 8]>, _>>()?;
    context.with_active_dynamic_unit(target.unit, |context| {
        let pushed_called_class = called_class.is_some();
        if let Some(called_class) = &called_class {
            context
                .called_classes
                .push(Arc::from(called_class.as_str()));
        }
        let result = invoke_native_function_with_metadata_strict(
            context,
            target.function,
            &transferred_arguments,
            metadata,
            strict,
        );
        if pushed_called_class {
            context.called_classes.pop();
        }
        match result {
            Ok(encoded) => context.transfer_external_return(encoded, target.unit),
            Err(error) if error.starts_with("E_PHP_EXIT:") => {
                let encoded = error
                    .trim_start_matches("E_PHP_EXIT:")
                    .parse::<i64>()
                    .map_err(|_| "external native exit value is invalid".to_owned())?;
                let encoded = context.transfer_external_return(encoded, target.unit)?;
                Err(format!("E_PHP_EXIT:{encoded}"))
            }
            Err(error) => Err(error),
        }
    })?
}

fn native_external_return_value(value: Value, owner_unit: usize) -> Value {
    match value {
        Value::Callable(callable) => match callable.as_ref() {
            php_runtime::api::CallableValue::Closure(closure)
                if closure.context.owner_unit.is_none() =>
            {
                Value::Callable(Box::new(php_runtime::api::CallableValue::Closure(
                    closure.clone().with_owner_unit(Some(owner_unit)),
                )))
            }
            _ => Value::Callable(callable),
        },
        value => value,
    }
}

fn native_value_with_owner_unit(value: Value, owner_unit: Option<usize>) -> Value {
    match value {
        Value::Callable(callable) => match callable.as_ref() {
            php_runtime::api::CallableValue::Closure(closure)
                if closure.context.owner_unit.is_none() && owner_unit.is_some() =>
            {
                Value::Callable(Box::new(php_runtime::api::CallableValue::Closure(
                    closure.clone().with_owner_unit(owner_unit),
                )))
            }
            _ => Value::Callable(callable),
        },
        value => value,
    }
}

fn invoke_native_method(
    context: &mut NativeExecutionContext<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
) -> Result<i64, String> {
    invoke_native_method_with_trace_arguments(context, function, arguments, None)
}

fn invoke_native_method_with_trace_arguments(
    context: &mut NativeExecutionContext<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
    trace_arguments: Option<request_state::NativeTraceArguments>,
) -> Result<i64, String> {
    let metadata = NativeFunctionMetadataPtr::from_compiled(&context.compiled, function);
    invoke_native_method_with_prepared_trace_arguments(
        context,
        function,
        arguments,
        trace_arguments,
        metadata,
    )
}

fn invoke_native_method_with_prepared_trace_arguments(
    context: &mut NativeExecutionContext<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
    trace_arguments: Option<request_state::NativeTraceArguments>,
    metadata: Option<NativeFunctionMetadataPtr>,
) -> Result<i64, String> {
    let function_name = metadata
        .as_ref()
        .map_or("<unknown>", |metadata| metadata.name.as_ref());
    if context.call_frames.len() >= NATIVE_CALL_DEPTH_LIMIT {
        return Err(format!(
            "E_PHP_NATIVE_CALL_DEPTH: maximum native call depth of {NATIVE_CALL_DEPTH_LIMIT} exceeded in {function_name}()"
        ));
    }
    let handle = ensure_native_entry(context, function)?;
    let instance_method = metadata
        .as_ref()
        .is_some_and(|metadata| metadata.instance_method);
    let object = instance_method
        .then(|| arguments.first())
        .flatten()
        .and_then(|receiver| context.decode(*receiver).ok())
        .and_then(|receiver| match receiver {
            Value::Object(object) => Some(object),
            _ => None,
        });
    let called_class = object
        .as_ref()
        .map(php_runtime::api::ObjectRef::class_name_handle)
        .or_else(|| context.called_classes.last().cloned());
    let pushed_called_class = called_class.is_some();
    if let Some(class) = called_class.as_ref() {
        context.called_classes.push(Arc::clone(class));
    }
    let leading = metadata.as_ref().map_or(0, |metadata| {
        metadata.capture_count
            + usize::from(instance_method)
            + usize::from(metadata.implicit_closure_this)
    });
    let frame_arguments = trace_arguments.map_or_else(
        || {
            arguments
                .iter()
                .skip(leading)
                .copied()
                .collect::<request_state::NativeTraceArguments>()
        },
        |arguments| arguments,
    );
    context
        .call_frames
        .push(native_backtrace_frame_from_metadata(
            metadata,
            called_class,
            object,
            frame_arguments,
        ));
    let transition_started_at = context.options.collect_counters.then(|| {
        (
            std::time::Instant::now(),
            context.active_helper_child_time_nanos(),
        )
    });
    context.record_native_direct_calls(&handle);
    let runtime = std::ptr::from_mut(&mut *context).cast::<std::ffi::c_void>();
    let outcome = handle.invoke_i64_with_native_unwind_runtime(
        arguments,
        php_jit::JIT_RUNTIME_ABI_HASH,
        runtime,
        |types, value| native_catch_matches(context, types, value),
    );
    let outcome = resume_native_optimizing_exit(context, outcome);
    if let Some((started_at, child_time_before)) = transition_started_at {
        let nested_helper_time = context
            .active_helper_child_time_nanos()
            .saturating_sub(child_time_before);
        context.record_native_transition("same_unit", started_at.elapsed(), nested_helper_time);
    }
    let completed_frame = context
        .call_frames
        .pop()
        .expect("native call frame stack underflow");
    if pushed_called_class {
        context.called_classes.pop();
    }
    match outcome {
        Ok(php_jit::JitI64InvokeOutcome::Returned(value)) => {
            let returns_by_ref = context
                .unit
                .functions
                .get(function.index())
                .is_some_and(|function| function.returns_by_ref);
            if returns_by_ref {
                let target = &context.unit.functions[function.index()];
                let span = target
                    .blocks
                    .iter()
                    .filter_map(|block| block.terminator.as_ref())
                    .find(|terminator| {
                        matches!(
                            terminator.kind,
                            php_ir::instruction::TerminatorKind::Return {
                                by_ref_local: None,
                                ..
                            }
                        )
                    })
                    .map_or(target.span, |terminator| terminator.span);
                let path = context
                    .unit
                    .files
                    .get(span.file.index())
                    .map_or("<unknown>", |file| file.path.as_str());
                let line = std::fs::read(path).ok().map_or(1, |bytes| {
                    bytes
                        .iter()
                        .take(span.start as usize)
                        .filter(|byte| **byte == b'\n')
                        .count()
                        + 1
                });
                context.output.write_bytes(format!(
                    "\nNotice: Only variable references should be returned by reference in {path} on line {line}\n"
                ));
                let value = context.decode(value)?;
                return context.encode(Value::Reference(php_runtime::api::ReferenceCell::new(
                    value,
                )));
            }
            Ok(value)
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
            if status == php_jit::JitCallStatus::RETURN_REFERENCE.0 as i32 =>
        {
            Ok(value)
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit {
            status,
            value,
            state,
        }) if status == php_jit::JitCallStatus::THROW.0 as i32 => {
            let throwable = context.decode(value).map_err(|error| {
                let continuation = context
                    .instruction_for_continuation(state.function_id, state.continuation_id)
                    .map(|instruction| format!(" at {:?}", instruction.kind))
                    .unwrap_or_else(|| {
                        format!(
                            " at native continuation {}:{}",
                            state.function_id, state.continuation_id
                        )
                    });
                format!(
                    "native method {function_name} returned an undecodable throwable {value}{continuation}: {error}"
                )
            })?;
            let arguments = completed_frame
                .arguments
                .iter()
                .map(|argument| context.decode(*argument))
                .collect::<Result<Vec<_>, _>>()?;
            context.pending_throwable = Some(native_throwable_with_frame(
                throwable,
                &function_name,
                arguments,
            ));
            context.mark_roots_dirty(RootMutationReason::PendingThrowable);
            Err("E_PHP_RETHROW".to_owned())
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
            if status == php_jit::JitCallStatus::EXIT.0 as i32 =>
        {
            Err(format!("E_PHP_EXIT:{value}"))
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit { status, state, .. })
            if status == php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32 =>
        {
            if context.diagnostic.is_some() {
                // The callee has already published the PHP diagnostic in the
                // shared execution context. Preserve that diagnostic and
                // carry the status through the call trampoline unchanged.
                Err(NATIVE_RUNTIME_ERROR_MARKER.to_owned())
            } else {
                let continuation = context
                    .instruction_for_continuation(state.function_id, state.continuation_id)
                    .map(|instruction| format!(" at {:?}", instruction.kind))
                    .unwrap_or_else(|| {
                        format!(
                            " at native continuation {}:{}",
                            state.function_id, state.continuation_id
                        )
                    });
                Err(format!(
                    "native method {function_name} returned a runtime error{continuation} (control_reserved={:#x}, control_value={}, native_version={}, direct_values={}/{}, direct_array_entries={}/{}, direct_string_bytes={}/{})",
                    state.control_reserved,
                    state.control_value,
                    state.native_version,
                    *context.direct_value_next,
                    context.direct_value_slots.len(),
                    *context.direct_array_next,
                    context.direct_array_entries.len(),
                    *context.direct_string_next,
                    context.direct_string_bytes.len(),
                ))
            }
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit {
            status,
            value,
            state,
        }) if status == php_jit::JitCallStatus::SUSPEND_FIBER.0 as i32
            && context.active_fiber.is_some() =>
        {
            context.pending_nested_fiber_execution = Some(NativeFiberExecution {
                handle,
                arguments: arguments.to_vec(),
                state,
                nested: None,
            });
            context.pending_fiber_suspension_value = Some(value);
            Err("E_PHP_SUSPEND_FIBER".to_owned())
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit { status, state, .. }) => {
            let continuation = context
                .instruction_for_continuation(state.function_id, state.continuation_id)
                .map(|instruction| format!(" at {:?}", instruction.kind))
                .unwrap_or_else(|| {
                    format!(
                        " at native continuation {}:{}",
                        state.function_id, state.continuation_id
                    )
                });
            let diagnostic = context
                .diagnostic
                .as_ref()
                .map(|diagnostic| format!(": {}", diagnostic.message()))
                .unwrap_or_default();
            Err(format!(
                "native method {function_name} returned status {status}{continuation}{diagnostic}"
            ))
        }
        Err(error) => Err(format!("native method invocation failed: {error:?}")),
    }
}

pub(super) fn resume_native_optimizing_exit(
    context: &mut NativeExecutionContext<'_>,
    mut outcome: Result<php_jit::JitI64InvokeOutcome, php_jit::JitInvokeError>,
) -> Result<php_jit::JitI64InvokeOutcome, php_jit::JitInvokeError> {
    loop {
        let Ok(php_jit::JitI64InvokeOutcome::SideExit { status, state, .. }) = &outcome else {
            return outcome;
        };
        if *status != php_jit::JitCallStatus::RECOMPILE_REQUESTED.0 as i32 {
            return outcome;
        }
        let transition_instruction =
            context.instruction_for_continuation(state.function_id, state.continuation_id);
        let mut transition_reason = transition_instruction
            .as_ref()
            .map(|instruction| native_optimizing_transition_reason(&instruction.kind))
            .unwrap_or_else(|| std::borrow::Cow::Borrowed("optimizer_unknown"));
        if transition_reason.as_ref() == "optimizer_array:IssetDim" {
            let mut detail = match state.control_reserved {
                php_jit::JIT_OPTIMIZING_EXIT_ARRAY_NOT_TAGGED => "not_tagged",
                php_jit::JIT_OPTIMIZING_EXIT_ARRAY_VIEW_MISSING => "view_missing",
                php_jit::JIT_OPTIMIZING_EXIT_ARRAY_KEY_UNSUPPORTED => "key_unsupported",
                _ => "unknown",
            }
            .to_owned();
            if state.control_reserved == php_jit::JIT_OPTIMIZING_EXIT_ARRAY_NOT_TAGGED
                && let Some(instruction) = transition_instruction.as_ref()
                && let php_ir::InstructionKind::IssetDim { local, .. } = &instruction.kind
                && state.local_initialized(*local)
            {
                detail.push(':');
                detail.push_str(native_transition_value_kind(state.slots[local.index()]));
            }
            transition_reason =
                std::borrow::Cow::Owned(format!("{}:{detail}", transition_reason.as_ref()));
        } else if transition_reason.as_ref() == "optimizer_local:LoadLocal"
            && let Some(instruction) = transition_instruction.as_ref()
            && let php_ir::InstructionKind::LoadLocal { local, .. } = &instruction.kind
            && state.local_initialized(*local)
        {
            let stored = native_transition_stored_value_kind(context, state.slots[local.index()]);
            let next = context
                .instruction_for_continuation(
                    state.function_id,
                    state.continuation_id.saturating_add(1),
                )
                .map(|instruction| {
                    let rendered = format!("{:?}", instruction.kind);
                    rendered
                        .split_once([' ', '{', '('])
                        .map_or(rendered.as_str(), |(name, _)| name)
                        .to_owned()
                })
                .unwrap_or_else(|| "terminal".to_owned());
            transition_reason = std::borrow::Cow::Owned(format!(
                "{}:{stored}:next_{next}",
                transition_reason.as_ref()
            ));
        } else if transition_reason.as_ref() == "optimizer_array:AssignDim"
            && let Some(instruction) = transition_instruction.as_ref()
            && let php_ir::InstructionKind::AssignDim { local, .. } = &instruction.kind
            && state.local_initialized(*local)
        {
            let encoded = state.slots[local.index()];
            let raw = native_transition_value_kind(encoded);
            let stored = native_transition_stored_value_kind(context, encoded);
            let descriptor = php_jit::jit_decode_runtime_value(encoded).map_or_else(
                || "immediate".to_owned(),
                |index| {
                    if index >= php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE {
                        return context
                            .direct_value_slots
                            .get((index - php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE) as usize)
                            .map_or_else(
                                || "direct_missing".to_owned(),
                                |slot| format!("direct_kind_{}_refs_{}", slot.kind, slot.refcount),
                            );
                    }
                    match context.values.get(index as usize).and_then(Option::as_ref) {
                        Some(NativeStoredValue::Php(Value::Reference(reference))) => reference
                            .try_with_value(|value| match value {
                                Value::Array(array) => format!(
                                    "reference_storage_refs_{}",
                                    array.gc_refcount_estimate()
                                ),
                                _ => "reference_non_array".to_owned(),
                            })
                            .unwrap_or_else(|_| "reference_borrowed".to_owned()),
                        _ => "table".to_owned(),
                    }
                },
            );
            transition_reason = std::borrow::Cow::Owned(format!(
                "{}:{raw}:{stored}:{descriptor}",
                transition_reason.as_ref()
            ));
        }
        let transition_started = context
            .options
            .collect_counters
            .then(std::time::Instant::now);
        let function = php_ir::FunctionId::new(state.function_id);
        let baseline = ensure_native_baseline_entry(context, function).map_err(|_| {
            php_jit::JitInvokeError::MissingNativeTransition {
                function: state.function_id,
                continuation: state.continuation_id,
            }
        })?;
        let runtime = std::ptr::from_mut(&mut *context).cast::<std::ffi::c_void>();
        outcome = baseline.invoke_i64_native_transition_with_unwind_runtime(
            state,
            php_jit::JIT_RUNTIME_ABI_HASH,
            runtime,
            |types, value| native_catch_matches(context, types, value),
        );
        if let Some(started) = transition_started {
            context.record_native_transition(transition_reason.as_ref(), started.elapsed(), 0);
        }
    }
}

fn native_transition_value_kind(encoded: i64) -> &'static str {
    let encoded = encoded as u64;
    match encoded & php_jit::JIT_VALUE_RUNTIME_KIND_MASK {
        php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG => "reference",
        php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG => "array",
        php_jit::JIT_VALUE_RUNTIME_OBJECT_TAG => "object",
        php_jit::JIT_VALUE_RUNTIME_STRING_TAG => "string",
        php_jit::JIT_VALUE_RUNTIME_FLOAT_TAG => "float",
        php_jit::JIT_VALUE_RUNTIME_RESOURCE_TAG => "resource",
        php_jit::JIT_VALUE_RUNTIME_CALLABLE_TAG => "callable",
        php_jit::JIT_VALUE_RUNTIME_GENERATOR_TAG => "generator",
        php_jit::JIT_VALUE_RUNTIME_FIBER_TAG => "fiber",
        php_jit::JIT_VALUE_RUNTIME_ITERATOR_TAG => "iterator",
        _ if encoded == php_jit::jit_encode_constant(u32::MAX) as u64 => "null",
        _ if encoded & php_jit::JIT_VALUE_TAG_MASK == php_jit::JIT_VALUE_CONSTANT_TAG => "constant",
        _ => "immediate",
    }
}

fn native_transition_stored_value_kind(
    context: &NativeExecutionContext<'_>,
    encoded: i64,
) -> &'static str {
    let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
        return native_transition_value_kind(encoded);
    };
    match context.values.get(index as usize).and_then(Option::as_ref) {
        Some(NativeStoredValue::Php(Value::Reference(reference))) => reference
            .try_with_value(native_value_type_name)
            .unwrap_or("borrowed_reference"),
        Some(NativeStoredValue::Php(value)) => native_value_type_name(value),
        Some(NativeStoredValue::GlobalsProxy) => "globals_proxy",
        Some(NativeStoredValue::ArrayIterator(_)) => "array_iterator",
        Some(NativeStoredValue::Iterator(_)) => "iterator",
        Some(NativeStoredValue::GeneratorIterator(_)) => "generator_iterator",
        Some(NativeStoredValue::PreparedClosure(_)) => "prepared_closure",
        None => "missing",
    }
}

fn native_optimizing_transition_reason(
    kind: &php_ir::InstructionKind,
) -> std::borrow::Cow<'static, str> {
    use php_ir::InstructionKind;

    let family = match kind {
        InstructionKind::LoadLocal { .. }
        | InstructionKind::StoreLocal { .. }
        | InstructionKind::Discard { .. }
        | InstructionKind::IssetLocal { .. }
        | InstructionKind::EmptyLocal { .. }
        | InstructionKind::UnsetLocal { .. } => "optimizer_local",
        InstructionKind::Unary { .. }
        | InstructionKind::Binary { .. }
        | InstructionKind::Compare { .. }
        | InstructionKind::Cast { .. } => "optimizer_scalar",
        InstructionKind::NewArray { .. }
        | InstructionKind::ArrayInsert { .. }
        | InstructionKind::ArraySpread { .. }
        | InstructionKind::FetchDim { .. }
        | InstructionKind::AssignDim { .. }
        | InstructionKind::AppendDim { .. }
        | InstructionKind::UnsetDim { .. }
        | InstructionKind::IssetDim { .. }
        | InstructionKind::EmptyDim { .. } => "optimizer_array",
        InstructionKind::ForeachInit { .. }
        | InstructionKind::ForeachInitRef { .. }
        | InstructionKind::ForeachNext { .. }
        | InstructionKind::ForeachNextRef { .. }
        | InstructionKind::ForeachCleanup { .. } => "optimizer_foreach",
        InstructionKind::FetchProperty { .. }
        | InstructionKind::AssignProperty { .. }
        | InstructionKind::FetchDynamicStaticProperty { .. }
        | InstructionKind::AssignDynamicStaticProperty { .. }
        | InstructionKind::FetchObjectClassName { .. } => "optimizer_property",
        InstructionKind::BindReference { .. }
        | InstructionKind::BindReferenceDim { .. }
        | InstructionKind::BindReferenceProperty { .. }
        | InstructionKind::BindReferenceFromProperty { .. }
        | InstructionKind::BindReferenceFromPropertyDim { .. }
        | InstructionKind::BindReferencePropertyDim { .. }
        | InstructionKind::BindReferenceDimFromProperty { .. }
        | InstructionKind::BindReferenceFromDim { .. }
        | InstructionKind::BindReferenceFromStaticPropertyDim { .. }
        | InstructionKind::BindReferenceStaticProperty { .. }
        | InstructionKind::BindReferenceFromCall { .. }
        | InstructionKind::BindReferenceFromMethodCall { .. } => "optimizer_reference",
        InstructionKind::CallFunction { .. }
        | InstructionKind::CallMethod { .. }
        | InstructionKind::CallStaticMethod { .. }
        | InstructionKind::CallClosure { .. }
        | InstructionKind::CallCallable { .. }
        | InstructionKind::Pipe { .. }
        | InstructionKind::NewObject { .. }
        | InstructionKind::DynamicNewObject { .. } => "optimizer_call",
        InstructionKind::Include { .. }
        | InstructionKind::Eval { .. }
        | InstructionKind::DeclareFunction { .. }
        | InstructionKind::DeclareClass { .. } => "optimizer_dynamic_code",
        _ => "optimizer_other",
    };
    // This runs only while diagnostic counters are enabled. Preserve the
    // exact IR opcode, but not its operands, so an aggregate family cannot
    // hide the next dominant warm transition after an earlier exit is
    // removed.
    if let InstructionKind::Binary { op, .. } = kind {
        return format!("{family}:Binary:{op:?}").into();
    }
    if let InstructionKind::CallFunction { name, args, .. } = kind {
        let named = args
            .iter()
            .filter(|argument| argument.name.is_some())
            .count();
        let unpacked = args.iter().filter(|argument| argument.unpack).count();
        return format!(
            "{family}:CallFunction:{}:argc{}:named{}:unpack{}",
            name.trim_start_matches('\\').to_ascii_lowercase(),
            args.len(),
            named,
            unpacked,
        )
        .into();
    }
    let debug = format!("{kind:?}");
    let end = debug
        .find(|character: char| matches!(character, ' ' | '{' | '('))
        .unwrap_or(debug.len());
    format!("{family}:{}", &debug[..end]).into()
}

fn invoke_native_property_magic(
    context: &mut NativeExecutionContext<'_>,
    class: &php_ir::module::ClassEntry,
    receiver: i64,
    property: &str,
    magic: &str,
    caller_function: u32,
) -> Result<Option<Value>, String> {
    let Some(method) = class
        .methods
        .iter()
        .find(|method| method.name.eq_ignore_ascii_case(magic))
    else {
        return Ok(None);
    };
    if method.function.raw() == caller_function {
        return Ok(None);
    }
    let name = context.encode(Value::String(PhpString::from_bytes(
        property.as_bytes().to_vec(),
    )))?;
    let value = invoke_native_method(context, method.function, &[receiver, name])?;
    context.decode(value).map(Some)
}

fn execute_native_property_instruction(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
    caller_function: u32,
) -> Option<Result<i64, String>> {
    use php_ir::InstructionKind;
    let (object, property, dynamic_property) = match &instruction.kind {
        InstructionKind::FetchDynamicProperty { .. }
        | InstructionKind::IssetDynamicProperty { .. }
        | InstructionKind::EmptyDynamicProperty { .. }
        | InstructionKind::IssetDynamicPropertyDim { .. }
        | InstructionKind::EmptyDynamicPropertyDim { .. }
        | InstructionKind::AssignDynamicProperty { .. }
        | InstructionKind::UnsetDynamicProperty { .. } => {
            let [object, property, ..] = arguments else {
                return Some(Err("dynamic property operands are missing".to_owned()));
            };
            (*object, String::new(), Some(*property))
        }
        InstructionKind::IssetProperty {
            object: _,
            property,
            ..
        }
        | InstructionKind::EmptyProperty {
            object: _,
            property,
            ..
        }
        | InstructionKind::UnsetProperty {
            object: _,
            property,
            ..
        }
        | InstructionKind::UnsetPropertyDim {
            object: _,
            property,
            ..
        }
        | InstructionKind::AssignPropertyDim {
            object: _,
            property,
            ..
        }
        | InstructionKind::IssetPropertyDim {
            object: _,
            property,
            ..
        }
        | InstructionKind::EmptyPropertyDim {
            object: _,
            property,
            ..
        } => {
            let [object, ..] = arguments else {
                return Some(Err("property object operand is missing".to_owned()));
            };
            (*object, property.clone(), None)
        }
        _ => return None,
    };
    let property = if let Some(property) = dynamic_property {
        match context.decode(property).and_then(native_string) {
            Ok(property) => String::from_utf8_lossy(&property).into_owned(),
            Err(error) => return Some(Err(error)),
        }
    } else {
        property
    };
    let object_encoded = object;
    let closure_operand = context
        .unit
        .functions
        .get(caller_function as usize)
        .and_then(|function| {
            let object_register = match &instruction.kind {
                InstructionKind::AssignDynamicProperty {
                    object: php_ir::Operand::Register(register),
                    ..
                } => Some(*register),
                _ => None,
            }?;
            let local = function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .find_map(|candidate| match candidate.kind {
                    InstructionKind::LoadLocal { dst, local }
                    | InstructionKind::LoadLocalQuiet { dst, local }
                        if dst == object_register =>
                    {
                        Some(local)
                    }
                    _ => None,
                })?;
            function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(|candidate| match candidate.kind {
                    InstructionKind::StoreLocal {
                        local: target,
                        src: php_ir::Operand::Register(source),
                    } if target == local => function
                        .blocks
                        .iter()
                        .flat_map(|block| &block.instructions)
                        .any(|origin| {
                            matches!(origin.kind, InstructionKind::MakeClosure { dst, .. } if dst == source)
                        }),
                    _ => false,
                })
                .then_some(())
        })
        .is_some();
    let mut decoded_object = match context.decode(object) {
        Ok(value) => value,
        Err(error) => return Some(Err(error)),
    };
    for _ in 0..16 {
        let Value::Reference(reference) = decoded_object else {
            break;
        };
        decoded_object = reference.get();
    }
    if !matches!(decoded_object, Value::Object(_)) {
        let quiet_result = match instruction.kind {
            InstructionKind::IssetProperty { .. }
            | InstructionKind::IssetDynamicProperty { .. }
            | InstructionKind::IssetDynamicPropertyDim { .. }
            | InstructionKind::IssetPropertyDim { .. } => Some(false),
            InstructionKind::EmptyProperty { .. }
            | InstructionKind::EmptyDynamicProperty { .. }
            | InstructionKind::EmptyDynamicPropertyDim { .. }
            | InstructionKind::EmptyPropertyDim { .. } => Some(true),
            _ => None,
        };
        if let Some(value) = quiet_result {
            return Some(context.encode(Value::Bool(value)));
        }
    }
    let object = match decoded_object {
        Value::Object(object) => object,
        Value::Callable(_) => {
            return Some(Err(format!(
                "E_PHP_THROW:Error:Cannot create dynamic property Closure::${property}"
            )));
        }
        _ if closure_operand => {
            return Some(Err(format!(
                "E_PHP_THROW:Error:Cannot create dynamic property Closure::${property}"
            )));
        }
        value => {
            return Some(Err(format!(
                "Attempt to access property {property} on {}",
                native_value_type_name(&value)
            )));
        }
    };
    let normalized_class = normalize_class_name(&object.class_name());
    let class = native_active_class_handle(context, &normalized_class);
    let caller_owns_class_scope = class.as_ref().is_some_and(|class| {
        class
            .methods
            .iter()
            .any(|method| method.function.raw() == caller_function)
    });
    let result = match &instruction.kind {
        InstructionKind::FetchDynamicProperty { .. } => {
            if object.get_property(&property).is_none()
                && native_calling_class(context, caller_function).is_some_and(|class| {
                    class.methods.iter().any(|method| {
                        method.function.raw() == caller_function
                            && method.name.eq_ignore_ascii_case("__get")
                    })
                })
            {
                return Some(Err(format!(
                    "Undefined property: {}::${property}",
                    object.display_name()
                )));
            }
            object.get_property(&property).unwrap_or(Value::Null)
        }
        InstructionKind::IssetProperty { .. } | InstructionKind::IssetDynamicProperty { .. } => {
            if object.get_property(&property).is_none()
                && let Some(class) = &class
                && let Some(value) = match invoke_native_property_magic(
                    context,
                    class,
                    object_encoded,
                    &property,
                    "__isset",
                    caller_function,
                ) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                }
            {
                Value::Bool(native_property_truthy(&value))
            } else {
                Value::Bool(
                    object
                        .get_property(&property)
                        .is_some_and(|value| native_property_is_set(&value)),
                )
            }
        }
        InstructionKind::EmptyProperty { .. } | InstructionKind::EmptyDynamicProperty { .. } => {
            if object.get_property(&property).is_none()
                && let Some(class) = &class
                && let Some(isset) = match invoke_native_property_magic(
                    context,
                    class,
                    object_encoded,
                    &property,
                    "__isset",
                    caller_function,
                ) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                }
            {
                if native_property_truthy(&isset) {
                    let value = match invoke_native_property_magic(
                        context,
                        class,
                        object_encoded,
                        &property,
                        "__get",
                        caller_function,
                    ) {
                        Ok(value) => value.unwrap_or(Value::Null),
                        Err(error) => return Some(Err(error)),
                    };
                    Value::Bool(!native_property_truthy(&value))
                } else {
                    Value::Bool(true)
                }
            } else {
                Value::Bool(
                    object
                        .get_property(&property)
                        .is_none_or(|value| !native_property_truthy(&value)),
                )
            }
        }
        InstructionKind::IssetPropertyDim { dims, .. }
        | InstructionKind::EmptyPropertyDim { dims, .. }
        | InstructionKind::IssetDynamicPropertyDim { dims, .. }
        | InstructionKind::EmptyDynamicPropertyDim { dims, .. } => {
            let key_offset = match instruction.kind {
                InstructionKind::IssetDynamicPropertyDim { .. }
                | InstructionKind::EmptyDynamicPropertyDim { .. } => 2,
                _ => 1,
            };
            let value = match native_dimension_path_value(
                context,
                object.get_property(&property),
                &arguments[key_offset..],
                dims.len(),
                instruction,
                NativeDimensionOperation::Fetch { quiet: true },
            ) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            if matches!(
                instruction.kind,
                InstructionKind::IssetPropertyDim { .. }
                    | InstructionKind::IssetDynamicPropertyDim { .. }
            ) {
                Value::Bool(value.is_some_and(|value| native_property_is_set(&value)))
            } else {
                Value::Bool(value.is_none_or(|value| !native_property_truthy(&value)))
            }
        }
        InstructionKind::AssignDynamicProperty { .. } => {
            let Some(value) = arguments.get(2).copied() else {
                return Some(Err(
                    "dynamic property assignment value is missing".to_owned()
                ));
            };
            let value = match context.decode(value) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            if let Some(class) = &class {
                if let Some(entry) = class.properties.iter().find(|entry| entry.name == property) {
                    if let Some(type_) = &entry.type_
                        && !native_value_matches_ir_type_in_context(context, &value, type_)
                    {
                        return Some(Err(format!(
                            "E_PHP_THROW:TypeError:Cannot assign {} to property {}::${} of type {}",
                            native_assignment_type_name(&value),
                            class.display_name,
                            property,
                            native_ir_type_name(type_)
                        )));
                    }
                    if entry.flags.is_private && !caller_owns_class_scope {
                        return Some(Err(format!(
                            "E_PHP_THROW:Error:Cannot access private property {}::${property}",
                            class.display_name
                        )));
                    }
                } else if let Some(method) = class
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("__set"))
                    .filter(|method| method.function.raw() != caller_function)
                {
                    let name = match context.encode(Value::String(PhpString::from_bytes(
                        property.as_bytes().to_vec(),
                    ))) {
                        Ok(name) => name,
                        Err(error) => return Some(Err(error)),
                    };
                    if let Err(error) = invoke_native_method(
                        context,
                        method.function,
                        &[object_encoded, name, arguments[2]],
                    ) {
                        return Some(Err(error));
                    }
                    return Some(context.encode(value));
                }
            }
            object.set_property(property, value.clone());
            value
        }
        InstructionKind::UnsetProperty { .. } | InstructionKind::UnsetDynamicProperty { .. } => {
            if let Some(class) = &class {
                if let Some(entry) = class.properties.iter().find(|entry| entry.name == property) {
                    if entry.flags.is_private && !caller_owns_class_scope {
                        return Some(Err(format!(
                            "E_PHP_THROW:Error:Cannot access private property {}::${property}",
                            class.display_name
                        )));
                    }
                } else if let Some(method) = class
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("__unset"))
                    .filter(|method| method.function.raw() != caller_function)
                {
                    let name = match context.encode(Value::String(PhpString::from_bytes(
                        property.as_bytes().to_vec(),
                    ))) {
                        Ok(name) => name,
                        Err(error) => return Some(Err(error)),
                    };
                    if let Err(error) =
                        invoke_native_method(context, method.function, &[object_encoded, name])
                    {
                        return Some(Err(error));
                    }
                    return Some(context.encode(Value::Null));
                }
            }
            object.unset_property(&property);
            Value::Null
        }
        InstructionKind::UnsetPropertyDim { dims, .. } => {
            let keys = arguments
                .iter()
                .skip(1)
                .take(dims.len())
                .map(|key| {
                    context
                        .decode(*key)
                        .ok()
                        .and_then(|key| php_runtime::api::ArrayKey::from_value(&key))
                })
                .collect::<Option<Vec<_>>>();
            let Some(keys) = keys else {
                let block = context
                    .unit
                    .functions
                    .get(caller_function as usize)
                    .and_then(|function| {
                        function.blocks.iter().find(|block| {
                            block
                                .instructions
                                .iter()
                                .any(|candidate| candidate == instruction)
                        })
                    });
                return Some(Err(format!(
                    "property dimension key is invalid: instruction={:?} arguments={arguments:?} decoded={:?} block={:?}",
                    instruction.kind,
                    arguments
                        .iter()
                        .map(|value| context.decode(*value))
                        .collect::<Vec<_>>(),
                    block.map(|block| block
                        .instructions
                        .iter()
                        .map(|candidate| &candidate.kind)
                        .collect::<Vec<_>>())
                )));
            };
            let _ = object.try_modify_property_value(&property, |value| {
                unset_native_array_dims(value, &keys);
            });
            Value::Null
        }
        InstructionKind::AssignPropertyDim { dims, append, .. } => {
            let value_index = 1 + dims.len();
            let Some(replacement) = arguments.get(value_index).copied() else {
                return Some(Err("property dimension value is missing".to_owned()));
            };
            let replacement = match context.decode(replacement) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let keys = arguments
                .iter()
                .skip(1)
                .take(dims.len())
                .map(|key| {
                    context
                        .decode(*key)
                        .ok()
                        .and_then(|key| php_runtime::api::ArrayKey::from_value(&key))
                })
                .collect::<Option<Vec<_>>>();
            let Some(keys) = keys else {
                let block = context
                    .unit
                    .functions
                    .get(caller_function as usize)
                    .and_then(|function| {
                        function.blocks.iter().find(|block| {
                            block
                                .instructions
                                .iter()
                                .any(|candidate| candidate == instruction)
                        })
                    });
                return Some(Err(format!(
                    "property dimension key is invalid: instruction={:?} arguments={arguments:?} decoded={:?} block={:?}",
                    instruction.kind,
                    arguments
                        .iter()
                        .map(|value| context.decode(*value))
                        .collect::<Vec<_>>(),
                    block.map(|block| block
                        .instructions
                        .iter()
                        .map(|candidate| &candidate.kind)
                        .collect::<Vec<_>>())
                )));
            };
            if let Some(class) = &class
                && let Some(entry) = class.properties.iter().find(|entry| entry.name == property)
                && entry.flags.is_readonly
            {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot indirectly modify readonly property {}::${property}",
                    class.display_name
                )));
            }
            if let Some(Value::Object(target)) = object.get_property(&property)
                && let Some(target_class) = context.unit.classes.iter().find(|class| {
                    class.name == normalize_class_name(&target.class_name())
                        && class
                            .interfaces
                            .iter()
                            .any(|interface| interface.eq_ignore_ascii_case("ArrayAccess"))
                })
                && let Some(offset_set) = target_class
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("offsetSet"))
                    .map(|method| method.function)
            {
                let key = keys.first().cloned().map_or(Value::Null, |key| match key {
                    php_runtime::api::ArrayKey::Int(value) => Value::Int(value),
                    php_runtime::api::ArrayKey::String(value) => Value::String(value),
                });
                let receiver = match context.encode(Value::Object(target)) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                let key = match context.encode(key) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                let replacement_encoded = match context.encode(replacement.clone()) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                if let Err(error) =
                    invoke_native_method(context, offset_set, &[receiver, key, replacement_encoded])
                {
                    return Some(Err(error));
                }
                return Some(context.encode(replacement));
            }
            let result = replacement.clone();
            let modified = object.try_modify_property_value(&property, |value| {
                assign_native_array_dims(value, &keys, replacement, *append);
            });
            if !matches!(modified, Ok(Some(()))) {
                let mut value = object.get_property(&property).unwrap_or(Value::Null);
                assign_native_array_dims(&mut value, &keys, result.clone(), *append);
                object.set_property(property, value);
            }
            result
        }
        _ => return None,
    };
    Some(context.encode(result))
}

fn cached_native_class_constant(
    context: &NativeExecutionContext<'_>,
    caller_function: u32,
    class: &str,
    constant: &str,
) -> Option<Value> {
    context
        .class_constant_cache
        .get(&(context.current_dynamic_unit, caller_function))
        .and_then(|classes| classes.get(class))
        .and_then(|constants| constants.get(constant))
        .cloned()
}

fn encode_and_cache_native_class_constant(
    context: &mut NativeExecutionContext<'_>,
    caller_function: u32,
    class: &str,
    constant: &str,
    value: Value,
) -> Result<i64, String> {
    context
        .class_constant_cache
        .entry((context.current_dynamic_unit, caller_function))
        .or_default()
        .entry(class.to_owned())
        .or_default()
        .insert(constant.to_owned(), value.clone());
    context.encode(value)
}

fn execute_native_class_constant(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    caller_function: u32,
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::FetchClassConstant {
        class_name,
        constant,
        ..
    } = &instruction.kind
    else {
        return None;
    };
    let resolved_class = match class_name.to_ascii_lowercase().as_str() {
        "self" => {
            native_effective_calling_class(context, caller_function).map(|class| class.name.clone())
        }
        "static" => context
            .called_classes
            .last()
            .map(|class| class.to_string())
            .or_else(|| {
                native_effective_calling_class(context, caller_function)
                    .map(|class| class.name.clone())
            }),
        "parent" => native_effective_calling_class(context, caller_function)
            .and_then(|class| class.parent.clone()),
        _ => Some(normalize_class_name(class_name)),
    };
    let Some(mut resolved_class) = resolved_class else {
        let message = if class_name.eq_ignore_ascii_case("self") {
            "Cannot use \"self\" in the global scope".to_owned()
        } else if class_name.eq_ignore_ascii_case("parent") {
            "Cannot use \"parent\" when no class scope is active".to_owned()
        } else {
            format!("Cannot resolve class {class_name}")
        };
        return Some(Err(format!("E_PHP_THROW:Error:{message}")));
    };
    if let Some(original) = context
        .class_aliases
        .get(&normalize_class_name(&resolved_class))
    {
        resolved_class = original.clone();
    }
    if constant.eq_ignore_ascii_case("class") {
        let display = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == normalize_class_name(&resolved_class))
            .map_or(resolved_class.as_str(), |class| class.display_name.as_str());
        return Some(context.encode(Value::String(PhpString::from_bytes(
            display.as_bytes().to_vec(),
        ))));
    }
    resolved_class = normalize_class_name(&resolved_class);
    if class_name.eq_ignore_ascii_case("ArrayObject")
        && constant.eq_ignore_ascii_case("ARRAY_AS_PROPS")
    {
        return Some(Ok(2));
    }
    if let Some((legacy, modern)) = pdo_mysql_deprecated_constant(&resolved_class, constant)
        && let Err(error) = emit_native_php_diagnostic(
            context,
            php_runtime::api::PHP_E_DEPRECATED,
            &format!(
                "Constant PDO::{legacy} is deprecated since 8.5, use Pdo\\Mysql::{modern} instead"
            ),
            instruction,
            true,
        )
    {
        return Some(Err(error));
    }
    if let Some(value) =
        cached_native_class_constant(context, caller_function, &resolved_class, constant)
    {
        return Some(context.encode(value));
    }
    if let Some(value) = native_internal_class_constant(&resolved_class, constant) {
        return Some(encode_and_cache_native_class_constant(
            context,
            caller_function,
            &resolved_class,
            constant,
            value,
        ));
    }
    let mut candidate = resolved_class.clone();
    while let Some(class) = native_active_class_handle(context, &candidate) {
        if let Some(entry) = class
            .constants
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(constant))
        {
            let caller = native_calling_class(context, caller_function);
            if entry.flags.is_private && caller.is_none_or(|caller| caller.name != class.name) {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot access private constant {}::{}",
                    class.display_name, entry.name
                )));
            }
            if entry.flags.is_protected
                && caller
                    .is_none_or(|caller| !native_class_is_a(context, &caller.name, &class.name))
            {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot access protected constant {}::{}",
                    class.display_name, entry.name
                )));
            }
            if let Some(value) = entry
                .value
                .and_then(|value| context.unit.constants.get(value.index()))
            {
                return Some(
                    native_runtime_constant_value(context, value).and_then(|value| {
                        encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        )
                    }),
                );
            }
            if let Some(reference) = &entry.value_named_constant {
                for name in &reference.names {
                    if let Ok(value) = context.lookup_constant(name) {
                        return Some(encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        ));
                    }
                }
            }
            if let Some(reference) = &entry.value_class_constant {
                let value = php_ir::IrConstant::ClassConstant {
                    class_name: reference.class_name.clone(),
                    display_class_name: reference.display_class_name.clone(),
                    constant_name: reference.constant_name.clone(),
                };
                return Some(
                    native_runtime_constant_value(context, &value).and_then(|value| {
                        encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        )
                    }),
                );
            }
        }
        if let Some(case) = class
            .enum_cases
            .iter()
            .find(|case| case.name.eq_ignore_ascii_case(constant))
            .cloned()
        {
            return Some(encode_native_enum_case(context, &class, &case));
        }
        let Some(parent) = class.parent.clone() else {
            break;
        };
        candidate = normalize_class_name(&parent);
    }
    if context
        .unit
        .classes
        .iter()
        .all(|class| class.name != resolved_class)
        && !native_external_class_exists(context, &resolved_class)
    {
        let normalized = resolved_class.clone();
        let autoload_name = if matches!(
            class_name.to_ascii_lowercase().as_str(),
            "self" | "static" | "parent"
        ) {
            resolved_class.as_str()
        } else {
            class_name.as_str()
        };
        if context.autoload_in_progress.insert(normalized.clone()) {
            let callbacks = context.autoload_callbacks.clone();
            for callback in callbacks {
                if let Err(error) = invoke_native_callable_value(
                    context,
                    callback,
                    &[Value::String(PhpString::from_bytes(
                        autoload_name.as_bytes().to_vec(),
                    ))],
                    instruction,
                    None,
                ) {
                    context.autoload_in_progress.remove(&normalized);
                    return Some(Err(error));
                }
                if native_external_class_exists(context, &resolved_class) {
                    break;
                }
            }
            context.autoload_in_progress.remove(&normalized);
        }
    }
    // The late-static class may live in another unit while the requested
    // constant is declared by a parent in the current unit (or vice versa).
    // Walk the combined hierarchy instead of checking only the first external
    // class.
    let mut candidate = resolved_class.clone();
    loop {
        let (owner_unit, class) =
            if let Some(class) = native_active_class_handle(context, &candidate) {
                (None, class)
            } else if let Some((unit, class)) = native_external_class_handle(context, &candidate) {
                (Some(unit), class)
            } else {
                break;
            };
        if let Some(entry) = class
            .constants
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(constant))
        {
            let caller = native_calling_class(context, caller_function);
            if entry.flags.is_private && caller.is_none_or(|caller| caller.name != class.name) {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot access private constant {}::{}",
                    class.display_name, entry.name
                )));
            }
            if entry.flags.is_protected
                && caller
                    .is_none_or(|caller| !native_class_is_a(context, &caller.name, &class.name))
            {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot access protected constant {}::{}",
                    class.display_name, entry.name
                )));
            }
            if let Some(value) = entry.value.and_then(|value| {
                owner_unit.map_or_else(
                    || context.unit.constants.get(value.index()),
                    |unit| {
                        context.dynamic_units.get(unit).and_then(|package| {
                            package.compiled.unit().constants.get(value.index())
                        })
                    },
                )
            }) {
                return Some(
                    native_runtime_constant_value(context, value).and_then(|value| {
                        encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        )
                    }),
                );
            }
            if let Some(reference) = &entry.value_named_constant {
                for name in &reference.names {
                    if let Ok(value) = context.lookup_constant(name) {
                        return Some(encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        ));
                    }
                }
            }
            if let Some(reference) = &entry.value_class_constant {
                let value = php_ir::IrConstant::ClassConstant {
                    class_name: reference.class_name.clone(),
                    display_class_name: reference.display_class_name.clone(),
                    constant_name: reference.constant_name.clone(),
                };
                return Some(
                    native_runtime_constant_value(context, &value).and_then(|value| {
                        encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        )
                    }),
                );
            }
        }
        let Some(parent) = class.parent.clone() else {
            break;
        };
        candidate = normalize_class_name(&parent);
    }
    Some(Err(format!(
        "Undefined constant {resolved_class}::{constant}"
    )))
}

fn execute_native_enum_static_method(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::CallStaticMethod {
        class_name, method, ..
    } = &instruction.kind
    else {
        return None;
    };
    let class =
        native_active_class_handle(context, class_name).filter(|class| class.flags.is_enum)?;
    if method.eq_ignore_ascii_case("cases") {
        let mut result = php_runtime::api::PhpArray::new();
        for case in &class.enum_cases {
            let encoded = match encode_native_enum_case(context, &class, case) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let value = match context.decode(encoded) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            result.append(value);
        }
        return Some(context.encode(Value::Array(result)));
    }
    if method.eq_ignore_ascii_case("from") || method.eq_ignore_ascii_case("tryFrom") {
        let Some(argument) = arguments.first() else {
            return Some(Err(format!(
                "{class_name}::{method}() expects exactly 1 argument"
            )));
        };
        let argument = match context.decode(*argument) {
            Ok(Value::Reference(reference)) => reference.get(),
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        };
        let matching = class.enum_cases.iter().find(|case| {
            case.value
                .and_then(|value| context.unit.constants.get(value.index()))
                .and_then(|value| ir_constant_value(value).ok())
                .is_some_and(|value| value == argument)
        });
        if let Some(case) = matching {
            return Some(encode_native_enum_case(context, &class, case));
        }
        if method.eq_ignore_ascii_case("tryFrom") {
            return Some(context.encode(Value::Null));
        }
        return Some(Err(format!(
            "E_PHP_THROW:ValueError:{} is not a valid backing value for enum {}",
            native_value_type_name(&argument),
            class.display_name
        )));
    }
    None
}

fn native_class_is_a(context: &NativeExecutionContext<'_>, class_name: &str, target: &str) -> bool {
    let target = normalize_class_name(target);
    let class_name = normalize_class_name(class_name);
    if class_name == "arrayiterator" && matches!(target.as_str(), "iterator" | "traversable") {
        return true;
    }
    let mut pending = vec![class_name];
    let mut visited = std::collections::BTreeSet::new();
    while let Some(candidate) = pending.pop() {
        if candidate == target {
            return true;
        }
        if !visited.insert(candidate.clone()) {
            continue;
        }
        if let Some(class) = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == candidate)
        {
            if let Some(parent) = &class.parent {
                pending.push(normalize_class_name(parent));
            }
            pending.extend(
                class
                    .interfaces
                    .iter()
                    .map(|interface| normalize_class_name(interface)),
            );
        } else if let Some((_, class)) = native_external_class_ref(context, &candidate) {
            if let Some(parent) = &class.parent {
                pending.push(normalize_class_name(parent));
            }
            pending.extend(
                class
                    .interfaces
                    .iter()
                    .map(|interface| normalize_class_name(interface)),
            );
        }
    }
    false
}

fn native_method_in_hierarchy(
    context: &NativeExecutionContext<'_>,
    class_name: &str,
    method: &str,
) -> Option<php_ir::FunctionId> {
    let mut candidate = normalize_class_name(class_name);
    loop {
        let class = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == candidate)?;
        if let Some(entry) = class
            .methods
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(method))
        {
            return Some(entry.function);
        }
        candidate = normalize_class_name(class.parent.as_ref()?);
    }
}

fn native_function_is_generator(
    context: &NativeExecutionContext<'_>,
    function: php_ir::FunctionId,
) -> bool {
    context
        .unit
        .functions
        .get(function.index())
        .is_some_and(|function| {
            function.flags.is_generator
                || function
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .any(|instruction| {
                        matches!(
                            instruction.kind,
                            php_ir::InstructionKind::Yield { .. }
                                | php_ir::InstructionKind::YieldFrom { .. }
                        )
                    })
        })
}

fn native_calling_class<'a>(
    context: &'a NativeExecutionContext<'_>,
    function: u32,
) -> Option<&'a php_ir::ClassEntry> {
    context.unit.classes.iter().find(|class| {
        class
            .methods
            .iter()
            .any(|method| method.function.raw() == function)
    })
}

fn native_effective_calling_class<'a>(
    context: &'a NativeExecutionContext<'_>,
    function: u32,
) -> Option<&'a php_ir::ClassEntry> {
    native_calling_class(context, function).or_else(|| {
        let scope = context.lexical_scope_classes.last()?;
        let normalized = normalize_class_name(scope);
        context
            .unit
            .classes
            .iter()
            .find(|class| class.name == normalized)
    })
}

fn native_resolve_scoped_class_name(
    context: &NativeExecutionContext<'_>,
    class_name: &str,
    caller_function: u32,
) -> Result<String, String> {
    match class_name.to_ascii_lowercase().as_str() {
        "self" => native_effective_calling_class(context, caller_function)
            .map(|class| class.display_name.clone())
            .ok_or_else(|| "Cannot use \"self\" in the global scope".to_owned()),
        "static" => context
            .called_classes
            .last()
            .map(|class| class.to_string())
            .or_else(|| {
                native_effective_calling_class(context, caller_function)
                    .map(|class| class.display_name.clone())
            })
            .ok_or_else(|| "Cannot use \"static\" in the global scope".to_owned()),
        "parent" => native_effective_calling_class(context, caller_function)
            .and_then(|class| {
                class
                    .parent_display_name
                    .clone()
                    .or_else(|| class.parent.clone())
            })
            .ok_or_else(|| "Cannot use \"parent\" when no parent scope is active".to_owned()),
        _ => Ok(class_name.to_owned()),
    }
}

fn native_method_access_error(
    context: &NativeExecutionContext<'_>,
    function: php_ir::FunctionId,
    caller_function: u32,
    _late_static_call: bool,
) -> Option<String> {
    let (declaring_class, method) = context.unit.classes.iter().find_map(|class| {
        class
            .methods
            .iter()
            .find(|method| method.function == function)
            .map(|method| (class, method))
    })?;
    if !method.flags.is_private && !method.flags.is_protected {
        return None;
    }
    let caller = native_effective_calling_class(context, caller_function);
    if method.flags.is_private && caller.is_none_or(|caller| caller.name != declaring_class.name) {
        if caller.is_none() {
            return Some(format!(
                "Call to private method {}::{}() from global scope",
                declaring_class.display_name, method.name
            ));
        }
        return Some(format!(
            "Cannot access private method {}::{}()",
            declaring_class.display_name, method.name
        ));
    }
    if method.flags.is_protected
        && caller
            .is_none_or(|caller| !native_class_is_a(context, &caller.name, &declaring_class.name))
    {
        return Some(format!(
            "Cannot access protected method {}::{}()",
            declaring_class.display_name, method.name
        ));
    }
    None
}

fn native_external_method_access_error(
    context: &NativeExecutionContext<'_>,
    target: NativeDynamicFunction,
    caller_function: u32,
    _late_static_call: bool,
) -> Option<String> {
    let unit = context.dynamic_units.get(target.unit)?.compiled.unit();
    let (declaring_class, method) = unit.classes.iter().find_map(|class| {
        class
            .methods
            .iter()
            .find(|method| method.function == target.function)
            .map(|method| (class, method))
    })?;
    if !method.flags.is_private && !method.flags.is_protected {
        return None;
    }
    let caller = native_effective_calling_class(context, caller_function);
    if method.flags.is_private && caller.is_none_or(|caller| caller.name != declaring_class.name) {
        if caller.is_none() {
            return Some(format!(
                "Call to private method {}::{}() from global scope",
                declaring_class.display_name, method.name
            ));
        }
        return Some(format!(
            "Cannot access private method {}::{}()",
            declaring_class.display_name, method.name
        ));
    }
    if method.flags.is_protected
        && caller
            .is_none_or(|caller| !native_class_is_a(context, &caller.name, &declaring_class.name))
    {
        return Some(format!(
            "Cannot access protected method {}::{}()",
            declaring_class.display_name, method.name
        ));
    }
    None
}

fn encode_native_call_arguments_array(
    context: &mut NativeExecutionContext<'_>,
    arguments: &[i64],
) -> Result<i64, String> {
    let mut array = php_runtime::api::PhpArray::new();
    for argument in arguments {
        array.append(context.decode(*argument)?);
    }
    context.encode(Value::Array(array))
}

fn execute_native_instanceof(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    let (object, static_target) = match &instruction.kind {
        php_ir::InstructionKind::InstanceOf { class_name, .. } => {
            (arguments.first().copied(), Some(class_name.as_str()))
        }
        php_ir::InstructionKind::DynamicInstanceOf { .. } => (arguments.first().copied(), None),
        _ => return None,
    };
    let Some(object) = object else {
        return Some(Err("instanceof receiver is missing".to_owned()));
    };
    let target = if let Some(target) = static_target {
        target.to_owned()
    } else {
        let Some(target) = arguments.get(1) else {
            return Some(Err("instanceof target is missing".to_owned()));
        };
        match context.decode(*target) {
            Ok(Value::String(value)) => value.to_string_lossy(),
            Ok(Value::Object(object)) => object.class_name(),
            Ok(value) => {
                return Some(Err(format!(
                    "instanceof target must be a class name, {} given",
                    native_value_type_name(&value)
                )));
            }
            Err(error) => return Some(Err(error)),
        }
    };
    let result = match context.decode(object) {
        Ok(Value::Object(object)) => native_internal_instanceof(&object.class_name(), &target)
            .unwrap_or_else(|| native_class_is_a(context, &object.class_name(), &target)),
        Ok(Value::Callable(_)) => target.eq_ignore_ascii_case("Closure"),
        Ok(Value::Fiber(_)) => target.eq_ignore_ascii_case("Fiber"),
        Ok(Value::Generator(_)) => target.eq_ignore_ascii_case("Generator"),
        Ok(Value::Array(array)) => {
            super::native_exception_fields(Value::Array(array)).is_some_and(|(class, _, _)| {
                let normalized = class.to_ascii_lowercase();
                target.eq_ignore_ascii_case(&class)
                    || target.eq_ignore_ascii_case("Throwable")
                    || (target.eq_ignore_ascii_case("Exception")
                        && normalized.ends_with("exception"))
                    || (target.eq_ignore_ascii_case("Error") && normalized.ends_with("error"))
            })
        }
        Ok(Value::Reference(reference)) => match reference.get() {
            Value::Object(object) => native_internal_instanceof(&object.class_name(), &target)
                .unwrap_or_else(|| native_class_is_a(context, &object.class_name(), &target)),
            Value::Callable(_) => target.eq_ignore_ascii_case("Closure"),
            Value::Fiber(_) => target.eq_ignore_ascii_case("Fiber"),
            Value::Generator(_) => target.eq_ignore_ascii_case("Generator"),
            _ => false,
        },
        Ok(_) => false,
        Err(error) => return Some(Err(error)),
    };
    Some(context.encode(Value::Bool(result)))
}

fn execute_native_acquire_callable(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    if !matches!(
        instruction.kind,
        php_ir::InstructionKind::AcquireCallable { .. }
    ) {
        return None;
    }
    let Some(value) = arguments.first() else {
        return Some(Err("callable value is missing".to_owned()));
    };
    let value = match context.decode(*value) {
        Ok(value) => dereference_native_callable_value(value),
        Err(error) => return Some(Err(error)),
    };
    let callable = match value {
        Value::Callable(callable) => return Some(context.encode(Value::Callable(callable))),
        Value::String(name) => php_runtime::api::CallableValue::UserFunction {
            name: name.to_string_lossy(),
        },
        Value::Object(object) => php_runtime::api::CallableValue::BoundMethod {
            target: php_runtime::api::CallableMethodTarget::Object(object),
            method: "__invoke".to_owned(),
            scope: None,
        },
        Value::Array(array) => {
            let target = array
                .get(&php_runtime::api::ArrayKey::Int(0))
                .cloned()
                .map(dereference_native_callable_value)
                .ok_or_else(|| "callable array target is missing".to_owned());
            let method = array
                .get(&php_runtime::api::ArrayKey::Int(1))
                .cloned()
                .map(dereference_native_callable_value)
                .ok_or_else(|| "callable array method is missing".to_owned());
            let (target, method) = match (target, method) {
                (Ok(target), Ok(Value::String(method))) => (target, method.to_string_lossy()),
                (Err(error), _) | (_, Err(error)) => return Some(Err(error)),
                _ => return Some(Err("callable array method must be a string".to_owned())),
            };
            let target = match target {
                Value::Object(object) => php_runtime::api::CallableMethodTarget::Object(object),
                Value::String(class) => {
                    php_runtime::api::CallableMethodTarget::Class(class.to_string_lossy())
                }
                value => {
                    return Some(Err(format!(
                        "callable array target must be object or class-string, {} given",
                        native_value_type_name(&value)
                    )));
                }
            };
            php_runtime::api::CallableValue::BoundMethod {
                target,
                method,
                scope: None,
            }
        }
        other => {
            return Some(Err(format!(
                "{} is not callable",
                native_value_type_name(&other)
            )));
        }
    };
    Some(context.encode(Value::Callable(Box::new(callable))))
}

fn execute_native_resolve_callable(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::ResolveCallable { callable, .. } = &instruction.kind else {
        return None;
    };
    let name = match callable {
        php_ir::instruction::CallableKind::FunctionName { name } => name,
        php_ir::instruction::CallableKind::MethodPlaceholder { target }
        | php_ir::instruction::CallableKind::UnresolvedDynamic { target } => {
            return Some(Err(format!("E_PHP_THROW:Error:{target}")));
        }
    };
    let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
    let fallback = normalized
        .rsplit_once('\\')
        .map(|(_, basename)| basename.to_owned());
    let exists = context.function_id(&normalized).is_some()
        || context.external_function(&normalized).is_some()
        || context.visible_function_names.contains(&normalized)
        || php_extensions::BuiltinRegistry::new().contains(&normalized)
        || fallback.as_ref().is_some_and(|fallback| {
            context.function_id(fallback).is_some()
                || context.external_function(fallback).is_some()
                || context.visible_function_names.contains(fallback)
                || php_extensions::BuiltinRegistry::new().contains(fallback)
        });
    if !exists {
        return Some(Err(format!(
            "E_PHP_THROW:Error:Call to undefined function {name}()"
        )));
    }
    Some(context.encode(Value::Callable(Box::new(
        php_runtime::api::CallableValue::UserFunction { name: name.clone() },
    ))))
}

fn native_rebind_closure(
    closure: &php_runtime::api::ClosurePayload,
    new_this: Option<Value>,
    new_scope: Option<Value>,
) -> Result<Value, String> {
    let bound_this = match new_this {
        Some(Value::Object(object)) => Some(object),
        Some(Value::Null) | None => None,
        Some(value) => {
            return Err(format!(
                "Closure::bind(): Argument #2 ($newThis) must be of type ?object, {} given",
                native_value_type_name(&value)
            ));
        }
    };
    let scope: Option<std::sync::Arc<str>> = match new_scope {
        Some(Value::Object(object)) => Some(object.display_name().into()),
        Some(Value::String(class)) => {
            let class = class.to_string_lossy();
            (class != "static").then(|| class.into())
        }
        Some(Value::Null) => None,
        Some(value) => {
            return Err(format!(
                "Closure::bind(): Argument #3 ($newScope) must be of type object|string|null, {} given",
                native_value_type_name(&value)
            ));
        }
        None => bound_this
            .as_ref()
            .map(|object| object.display_name().into()),
    };
    let mut context = closure.context.clone();
    if let Some(scope) = scope {
        context.scope_class = Some(scope.clone());
        context.called_class = Some(scope.clone());
        context.declaring_class = Some(scope);
    }
    Ok(Value::Callable(Box::new(
        php_runtime::api::CallableValue::Closure(
            closure
                .clone()
                .with_bound_this(bound_this)
                .with_context(context),
        ),
    )))
}

fn execute_native_bind_global(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::BindGlobal { name, .. } = &instruction.kind else {
        return None;
    };
    let current = context
        .inherited_globals
        .get(name)
        .filter(|value| !matches!(value, Value::Uninitialized))
        .cloned()
        .or_else(|| context.options.runtime_context.global_value(name))
        .unwrap_or(Value::Null);
    let reference = match current {
        Value::Reference(reference) => reference,
        value => php_runtime::api::ReferenceCell::new(value),
    };
    context
        .inherited_globals
        .insert(name.clone(), Value::Reference(reference.clone()));
    context.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
    Some(context.encode(Value::Reference(reference)))
}

#[cfg(test)]
mod tests;
