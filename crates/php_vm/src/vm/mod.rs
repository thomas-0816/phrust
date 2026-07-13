//! First minimal VM dispatch loop.

mod arguments;
mod builtin_adapter;
mod builtin_array_callbacks;
mod builtin_array_sort;
mod builtin_callback_validation;
mod builtin_callbacks;
mod builtin_classes;
mod builtin_environment;
mod builtin_error_output;
mod builtin_fileinfo;
mod builtin_filter_callbacks;
mod builtin_intrinsics;
mod builtin_pcre_callbacks;
mod builtin_settype;
mod builtins;
mod callable_builtins;
mod calls;
mod class_context;
mod class_operations;
mod class_relations;
mod class_validation;
mod closure_operations;
mod dense_activation;
mod dense_dispatch;
mod dense_method_dispatch;
mod dense_pcre_support;
mod dense_runtime_adapters;
mod diagnostics;
mod dim_execution_support;
mod direct_call_binding;
mod dispatch_contract;
mod exception_dispatch;
mod execution_control;
mod execution_cursor;
mod execution_optimization;
mod execution_state;
mod execution_tiering;
mod ext_redis;
mod generator_fiber;
mod include_execution;
mod inline_cache_access;
mod instrumentation;
mod iteration;
mod jit_abi;
mod jit_state;
mod layout_source;
mod method_cache_metadata;
mod method_dispatch;
mod object_lifecycle;
mod operand_read;
mod options;
mod prelude;
mod property_cache_metadata;
mod property_execution;
mod property_resolution;
mod property_state;
mod reflection;
mod request_lifecycle;
mod request_profile;
mod result;
mod rich_array_dispatch;
mod rich_call_dispatch;
mod rich_dispatch;
mod rich_exception_dispatch;
mod rich_foreach_dispatch;
mod rich_object_dispatch;
mod rich_property_dispatch;
mod runtime_class_metadata;
mod runtime_class_support;
mod runtime_environment_support;
mod runtime_operations;
mod scalar_handlers;
mod serialization;
mod shutdown_execution;
mod spl;
mod spl_heap_dispatch;
mod spl_iterator_dispatch;
mod spl_recursive_iterator_dispatch;
mod static_property_predicates;
mod stream_wrappers;
mod symbol_resolution;
mod value_support;

use arguments::{
    ParamTypecheckRequest, TypecheckFastPathContext, call_args_from_owned_php_array,
    call_args_from_php_array, call_args_to_positional, call_argument_reference_cell,
    check_property_type, coerce_or_check_param_type, coerce_return_value, ir_runtime_type,
    param_is_sensitive, sensitive_parameter_value, trace_value_for_param, type_error_value_name,
};
#[cfg(test)]
use builtin_adapter::internal_builtin_by_ref_param_name;
use builtin_adapter::{
    BuiltinAdapterState, InternalFunctionDispatchCache, InternalFunctionDispatchCacheOutcome,
    call_builtin_args_to_positional, internal_builtin_by_ref_temporary_fatal_result,
    request_filter_input_arrays, sorted_request_env,
};
use builtin_array_callbacks::is_array_callback_builtin_name;
use builtin_array_sort::is_array_sort_builtin_name;
use builtin_classes::*;
use builtin_fileinfo::FileinfoMethodCall;
use builtin_filter_callbacks::is_filter_callback_builtin_name;
use builtin_pcre_callbacks::is_pcre_callback_builtin_name;
use calls::*;
use class_context::*;
use class_operations::*;
use class_relations::*;
use class_validation::*;
use closure_operations::*;
use dense_pcre_support::*;
use diagnostics::*;
use dim_execution_support::*;
use dispatch_contract::{
    DenseExecutionRequest, RichBinaryError, RichBinaryRequest, RichCompareRequest,
    RichDispatchOutcome, RichUnaryRequest, dense_bytecode_unsupported_reason, dense_opcode_family,
    next_dense_block_index,
};
use exception_dispatch::*;
use execution_control::{
    ExceptionHandler, ExecutionLimitExceeded, PendingControl, RaiseOutcome,
    execution_limit_exceeded, next_block_id,
};
use execution_cursor::{ExecutionCursor, ExecutionView};
use execution_state::{
    DeclarationKind, DeclarationLoadKind, DeclarationOrigin, DestructorEntry, DestructorSweep,
    DestructorVisibility, DynamicClassEntry, DynamicConstantEntry, DynamicFunctionEntry,
    ErrorHandlerEntry, ExecutionState, GcObjectIdSet, LastErrorEntry, MagicMethodCall,
    MagicPropertyCall, PropertyHookCall, ShutdownFunctionEntry, destructor_candidates_for_value,
    gc_root_count_from_vm_roots, gc_snapshot_from_vm_roots,
    php_visible_non_register_root_object_ids, php_visible_root_object_ids,
    preserved_destructor_object_ids, release_unrooted_direct_object_handle,
    release_unrooted_object_handles,
};
use execution_tiering::{JitArgumentSlots, JitLeafRequest};
use ext_redis::*;
use generator_fiber::{
    FiberContinuation, FiberContinuationState, FiberResumeInput, FiberSuspension,
    GeneratorContinuation, GeneratorResumeInput, GeneratorYield, YieldFromDelegation, YieldFromKey,
    YieldFromStep, new_fiber_object,
};
use include_execution::{
    IncludeExecutionRequest, include_failure_allows_continuation, include_vm_error,
};
use inline_cache_access::{DenseInlineCacheSite, IrInlineCacheSite, UnitInlineCacheSite};
use instrumentation::*;
use iteration::{
    ForeachInvalidSourceBehavior, ForeachIterator, foreach_array_keys_from_local_at_frame,
    foreach_iterator_candidate_value, format_foreach_iterator_kind,
    object_property_iteration_entries,
};
use jit_state::*;
use method_cache_metadata::*;
use method_dispatch::MagicStaticCallRequest;
pub use options::{
    BytecodeLayoutMode, DenseIncludeMode, DenseJumpThreadingMode, ExecutionFormat,
    JitBlacklistMode, NativeOptimizationPolicy, SuperinstructionMode, VmOptions,
};
use property_cache_metadata::*;
use property_execution::PropertyDimProbe;
use property_resolution::*;
use property_state::*;
use reflection::*;
use request_lifecycle::RequestLifecycleState;
pub use result::VmResult;
use rich_exception_dispatch::*;
use rich_foreach_dispatch::*;
pub(crate) use runtime_class_metadata::dense_new_object_lowering_supported;
use runtime_class_metadata::*;
pub(crate) use runtime_class_support::normalize_function_name;
use runtime_class_support::*;
use runtime_environment_support::*;
use runtime_operations::{object_has_public_to_string_in_state, packed_array_get};
use scalar_handlers::{
    checked_int_binary, execute_arithmetic, execute_bitwise, execute_power, execute_rich_binary_op,
    execute_rich_compare_op, execute_rich_unary_op, implicit_int_deprecation_message,
    int_int_specialization_for_binary_op,
};
use spl::*;
use static_property_predicates::*;
use symbol_resolution::*;
use value_support::*;

use self::execution_optimization::{
    ClassStaticCacheLookup, ObjectClassResolution, ResolvedConstantTables,
};
use crate::aliasing::{AliasState, slot_alias_state};
use crate::bytecode::{
    DenseBytecodeUnit, DenseCallArg, DenseCallShapeMeta, DenseCallableKind, DenseClosureCapture,
    DenseExecutionPlan, DenseFunction, DenseFunctionPlan, DenseInstruction, DenseOpcode,
    DenseOperand, DenseOperandKind, DenseOperands, SuperinstructionSelectionReport,
};
use crate::compiled_unit::{
    CompiledClass, CompiledUnit, DenseExecutionArtifactKey, DenseExecutionArtifactMode,
    PreparedClassValidationError, PreparedFunctionFacts,
};
use crate::counters::JitCompileDescriptor;
use crate::counters::{MethodCallProfileObservation, PropertyFetchProfileObservation, VmCounters};
use crate::error::VmError;
use crate::frame::{CallStack, Frame, FrameActivationContext, FrameTraceArgument, TraceArguments};
use crate::include::{IncludeCacheStats, LoadedInclude};
use crate::inline_cache::{
    AutoloadClassLookupCacheKey, AutoloadClassLookupCacheTarget, AutoloadClassLookupEpochs,
    AutoloadClassLookupKind, CallReferenceMask, ClassConstantStaticPropertyCacheKind,
    ClassConstantStaticPropertyCacheTarget, ClassRelationCache, ClassRelationCacheKey,
    ClassRelationCacheLookup, ClassRelationCacheTarget, ClassRelationEpochs, ClassRelationKind,
    FunctionCallBuiltinKind, FunctionCallBuiltinMetadata, FunctionCallCacheTarget,
    FunctionCallShape, IncludePathCacheKey, IncludePathCacheTarget, InlineCacheId, InlineCacheKind,
    InlineCacheObservation, InlineCacheTable, InvalidationEpoch, MethodCallCacheTarget,
    MethodCallDispatchRoute, MethodCallGuardMetadata, MethodCallResolvedTarget,
    MethodCallRouteIdentity, MethodCallShape, PropertyAssignCacheTarget,
    PropertyAssignLayoutMetadata, PropertyAssignResolvedTarget, PropertyFetchCacheTarget,
    PropertyFetchLayoutMetadata, PropertyFetchResolvedTarget,
};
use crate::literal_pool::LiteralPool;
use crate::quickening::{QuickeningObservation, QuickeningSpecialization, QuickeningTable};
use crate::tiering::{ExecutionTier, TieringState};
use jit_abi::{
    JIT_PROPERTY_LOAD_STATUS_CLASS_EXIT, JIT_PROPERTY_LOAD_STATUS_LAYOUT_EXIT,
    JIT_PROPERTY_LOAD_STATUS_STORAGE_EXIT, JIT_PROPERTY_LOAD_STATUS_UNINITIALIZED_EXIT,
    jit_array_fetch_int_slow_abi, jit_array_len_abi, jit_concat_string_string_fast,
    jit_count_known_abi, jit_guard_kind_for_side_exit, jit_property_load_monomorphic_fast,
    jit_record_array_lookup_abi, jit_runtime_helper_table, jit_strlen_known_abi,
};
use operand_read::{
    DenseOperandRead, operand_truthy_at_frame, read_operand, read_operand_at_frame,
    read_operand_ref_at_frame, take_discard_operand_at_frame,
    unset_consumed_assignment_value_operand_at_frame, unset_dense_register_operand,
    unset_register_operand_at_frame,
};
use php_extensions::BuiltinRegistry;
use php_ir::constants::IrConstant;
use php_ir::function::{IrFunction, IrParam, IrReturnType};
use php_ir::ids::{BlockId, ClassId, ConstId, FunctionId, InstrId, LocalId, RegId, UnitId};
use php_ir::instruction::{
    BinaryOp, CallableKind, CastKind, ClosureCaptureArg, CompareOp, IncludeKind, Instruction,
    InstructionKind, IrCallArg, IrCallArgValueKind, IrDiagnosticSeverity, TerminatorKind, UnaryOp,
};
use php_ir::module::{
    ClassConstantReference, ClassEntry, ClassPropertyEntry, DeferredConstExpr, IrUnit,
    NamedConstantReference, display_class_name, normalize_class_name,
};
use php_ir::operand::Operand;
use php_ir::source_map::IrSpan;
use php_runtime::api::IniRegistry;
use php_runtime::api::ResourceTable;
use php_runtime::api::{
    ArrayKey, AttributeEntry as RuntimeAttributeEntry, AutoloadRegistry, BuiltinContext,
    BuiltinEntry, BuiltinHandlerKind, BuiltinOutcome, CallableMethodTarget, CallableValue,
    ClassConstantEntry as RuntimeClassConstantEntry,
    ClassConstantFlags as RuntimeClassConstantFlags, ClassEntry as RuntimeClassEntry,
    ClassEnumBackingType as RuntimeClassEnumBackingType,
    ClassEnumCaseEntry as RuntimeClassEnumCaseEntry, ClassFlags as RuntimeClassFlags,
    ClassMethodEntry as RuntimeClassMethodEntry, ClassMethodFlags as RuntimeClassMethodFlags,
    ClassPropertyEntry as RuntimeClassPropertyEntry,
    ClassPropertyFlags as RuntimeClassPropertyFlags,
    ClassPropertyHooks as RuntimeClassPropertyHooks, ClosureCaptureValue, ClosureContext,
    ClosureDebugInfo, ClosureDebugParameter, ClosurePayload, ExecutionStatus, FiberRef, FiberState,
    GeneratorCallContext, GeneratorRef, GeneratorState, GlobalSymbolTable, JsonDiagnosticContext,
    Lvalue, LvalueKind, NumericValue, ObjectRef, OutputBuffer, PhpArray, PhpArrayKind,
    PhpArrayShapeKind, PhpArrayShapeLookup, PhpArrayShapeLookupFallback, PhpString,
    ProcessCapability, ReferenceCell, RuntimeBringupDiagnosticContext, RuntimeContext,
    RuntimeDiagnostic, RuntimeDiagnosticPayload, RuntimeHttpResponseState, RuntimeSeverity,
    RuntimeSourceSpan, RuntimeStackFrame, RuntimeType, Slot, UnserializeOptions, UploadRegistry,
    Value, VmCompileDiagnostic, array_to_string_warning, compare, division_by_zero_mvp,
    emit_php_diagnostic, equal, error_reporting_allows_level, identical,
    reset_float_string_precision, runtime_type_name, serialize as serialize_value,
    set_float_string_precision, to_arithmetic_number, to_bool, to_float, to_int, to_number,
    to_string, to_string_php, undefined_function, undefined_global_variable_warning,
    undefined_variable_warning, unserialize as unserialize_value, unsupported_feature,
    value_matches_runtime_type, value_type_name,
};
use php_runtime::debug::{GcEntityId, GcEntityKind, GcRoot, GcRootKind, GcSnapshot, scan_roots};
use php_runtime::experimental::numeric_string::{
    NumericStringKind, NumericStringValue, classify_php_string,
};
use request_profile::{RequestProfileFrame, RequestProfileOperationCategory};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_EVAL_DEPTH: usize = 16;
const DENSE_EXECUTION_PLAN_THREAD_CACHE_MAX: usize = 4096;
const SORT_REGULAR: i64 = 0;
const SORT_NUMERIC: i64 = 1;
const SORT_STRING: i64 = 2;
const SORT_DESC: i64 = 3;
const SORT_ASC: i64 = 4;
const SORT_LOCALE_STRING: i64 = 5;
const SORT_NATURAL: i64 = 6;
const SPL_RUNTIME_CLASS_PROPERTY: &str = "__spl_runtime_class";
const HASH_CONTEXT_ALGORITHM_PROPERTY: &str = "__phrust_hash_algorithm";
const HASH_CONTEXT_FLAGS_PROPERTY: &str = "__phrust_hash_flags";
const HASH_CONTEXT_DATA_PROPERTY: &str = "__phrust_hash_data";
const HASH_CONTEXT_FINALIZED_PROPERTY: &str = "__phrust_hash_finalized";
const HASH_HMAC_FLAG: i64 = 1;
const SPL_PRIORITY_QUEUE_EXTR_DATA: i64 = 1;
const SPL_PRIORITY_QUEUE_EXTR_PRIORITY: i64 = 2;
const SPL_PRIORITY_QUEUE_EXTR_BOTH: i64 = 3;
const SPL_DLLIST_IT_MODE_FIFO: i64 = 0;
const SPL_DLLIST_IT_MODE_LIFO: i64 = 2;
const SPL_DLLIST_IT_MODE_KEEP: i64 = 0;
const SPL_ARRAY_OBJECT_STD_PROP_LIST: i64 = 1;
const SPL_ARRAY_OBJECT_ARRAY_AS_PROPS: i64 = 2;
const SPL_FILESYSTEM_CURRENT_AS_PATHNAME: i64 = 32;
const SPL_FILESYSTEM_CURRENT_AS_FILEINFO: i64 = 0;
const SPL_FILESYSTEM_CURRENT_AS_SELF: i64 = 16;
const SPL_FILESYSTEM_CURRENT_MODE_MASK: i64 = 240;
const SPL_FILESYSTEM_KEY_AS_PATHNAME: i64 = 0;
const SPL_FILESYSTEM_KEY_AS_FILENAME: i64 = 256;
const SPL_FILESYSTEM_KEY_MODE_MASK: i64 = 3840;
const SPL_FILESYSTEM_FOLLOW_SYMLINKS: i64 = 16384;
const SPL_FILESYSTEM_OTHER_MODE_MASK: i64 = 28672;
const SPL_FILESYSTEM_SKIP_DOTS: i64 = 4096;
const SPL_FILESYSTEM_UNIX_PATHS: i64 = 8192;
const ZIP_CREATE: i64 = 1;
const ZIP_EXCL: i64 = 2;
const ZIP_CHECKCONS: i64 = 4;
const ZIP_OVERWRITE: i64 = 8;
const ZIP_RDONLY: i64 = 16;
const ZIP_FL_NOCASE: i64 = 1;
const ZIP_FL_NODIR: i64 = 2;
const ZIP_FL_UNCHANGED: i64 = 8;
const ZIP_FL_OVERWRITE: i64 = 8192;
const ZIP_FL_OPEN_FILE_NOW: i64 = 1 << 30;
const ZIP_LENGTH_TO_END: i64 = 0;
const ZIP_CM_DEFAULT: i64 = -1;
const ZIP_CM_STORE: i64 = 0;
const ZIP_CM_DEFLATE: i64 = 8;
const ZIP_CM_BZIP2: i64 = 12;
const ZIP_CM_XZ: i64 = 95;
const ZIP_EM_NONE: i64 = 0;
const ZIP_EM_TRAD_PKWARE: i64 = 1;
const ZIP_EM_AES_128: i64 = 257;
const ZIP_EM_AES_192: i64 = 258;
const ZIP_EM_AES_256: i64 = 259;
const ZIP_ER_EXISTS: i64 = 10;
const ZIP_ER_COMPNOTSUPP: i64 = 16;
const ZIP_ER_RDONLY: i64 = 25;
const ZIP_AFL_RDONLY: i64 = 2;
const ZIP_AFL_CREATE_OR_KEEP_FILE_FOR_EMPTY_ARCHIVE: i64 = 16;
const SPL_REGEX_MATCH: i64 = 0;
const SPL_REGEX_GET_MATCH: i64 = 1;
const SPL_REGEX_ALL_MATCHES: i64 = 2;
const SPL_REGEX_SPLIT: i64 = 3;
const SPL_REGEX_REPLACE: i64 = 4;
const SPL_REGEX_USE_KEY: i64 = 1;
const SPL_REGEX_INVERT_MATCH: i64 = 2;
const SPL_RII_LEAVES_ONLY: i64 = 0;
const SPL_RII_SELF_FIRST: i64 = 1;
const SPL_RII_CHILD_FIRST: i64 = 2;
const SPL_RII_CATCH_GET_CHILD: i64 = 16;
const SPL_RTI_BYPASS_CURRENT: i64 = 4;
const SPL_RTI_BYPASS_KEY: i64 = 8;
const SORT_FLAG_CASE: i64 = 8;
const NORMALIZER_FORM_C: i64 = php_runtime::api::NORMALIZER_FORM_C;
const JIT_TIERING_MIN_EXECUTIONS: u64 = 32;
const JIT_TIERING_MIN_SIDE_EXITS: u64 = 8;
const JIT_TIERING_MAX_EXIT_RATE_PERCENT: u64 = 50;
const JIT_TIERING_COOLDOWN_CALLS: u64 = 128;
const JIT_BLACKLIST_COMPILE_ERROR_THRESHOLD: u64 = 1;
const JIT_BLACKLIST_ABI_MISMATCH_THRESHOLD: u64 = 1;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AutoloadTraceOrigin {
    function_name: &'static str,
    span: php_ir::IrSpan,
}

enum PropertyFetchCacheRead {
    Value(Value),
    Fallback,
}

fn value_as_jit_int(value: &Value) -> Result<i64, ()> {
    match value {
        Value::Int(value) => Ok(*value),
        _ => Err(()),
    }
}

enum PropertyAssignCacheWrite {
    Written(Value),
    Fallback,
}

enum SemanticHelperResult {
    FastHit,
    Fallback(&'static str),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct DenseExecutionPlanThreadCacheKey {
    compiled_identity: u64,
    artifact: DenseExecutionArtifactKey,
}

/// Tracks declarations observed by this worker and its lookup invalidation epoch.
/// Stable include replay preserves slot-indexed inline caches across requests.
struct WorkerSymbolLedger {
    epoch: Cell<u64>,
    seen_units: RefCell<HashSet<u64>>,
}

thread_local! {
    static WORKER_SYMBOL_LEDGER: WorkerSymbolLedger = WorkerSymbolLedger {
        epoch: Cell::new(0),
        seen_units: RefCell::new(HashSet::new()),
    };
}

thread_local! {
    static DENSE_EXECUTION_PLAN_THREAD_CACHE:
        RefCell<HashMap<DenseExecutionPlanThreadCacheKey, Arc<DenseExecutionPlan>>> =
            RefCell::new(HashMap::new());
}

enum BytecodeEntryAttempt {
    Executed(Box<VmResult>),
    Unsupported(String),
}

#[allow(clippy::large_enum_variant)] // Continue variants carry the live FunctionCall.
enum BytecodeFunctionAttempt<'a> {
    Executed(Box<VmResult>, BytecodeFunctionTier),
    Unsupported(String, FunctionCall<'a>),
}

enum BytecodeFunctionTier {
    Dense,
    RichFallback(String),
}

#[allow(clippy::large_enum_variant)] // Continue variants carry the live FunctionCall.
enum CachedDenseFunctionDispatch<'a> {
    Executed(Box<VmResult>),
    Continue(FunctionCall<'a>),
}

enum ClassStaticCacheRead {
    Value(Value),
    Fallback,
}

/// Outcome of [`Vm::fetch_class_constant_value`] when it cannot produce a value.
/// The shared helper never routes control flow itself; each executor's arm
/// translates the fault into its own routing (the rich interpreter routes
/// through in-frame handlers, the dense executor propagates to outer frames).
enum ClassConstantFetch {
    /// Autoload raised a throwable; carries the pre-routing result.
    Throwable(Box<VmResult>),
    /// A catchable runtime `\Error` must be raised at this span.
    Raise(IrSpan, String),
    /// A non-catchable internal error with this message.
    Fatal(String),
}

/// Outcome of [`Vm::assign_property_dim_value`] when it cannot produce a value.
/// Like [`ClassConstantFetch`], the shared helper never routes control flow; the
/// caller translates the fault (rich routes/returns in-frame, dense propagates).
/// Outcome of a shared static-property assignment attempt.
enum StaticPropertyAssignError {
    /// Autoload or nested user code produced a final result.
    Vm(Box<VmResult>),
    /// A catchable runtime `\Error` must be raised at this span.
    Raise(IrSpan, String),
    /// A non-catchable runtime error with this message.
    Fatal(String),
}

enum PropertyDimAssign {
    /// A catchable runtime `\Error` must be raised at this span.
    Raise(IrSpan, String),
    /// A non-catchable internal error with this message.
    Fatal(String),
    /// Return this result directly (a userland ArrayAccess::offsetSet threw or
    /// otherwise produced a final result); it is not routed through handlers.
    Return(Box<VmResult>),
}

enum ClassDependencyValidationFailure {
    Throwable(Value),
    Result(Box<VmResult>),
}

enum InternalBuiltinArgError {
    Message(String),
    Fatal(Box<VmResult>),
}

struct MultisortArraySpec {
    cell: ReferenceCell,
    entries: Vec<(ArrayKey, Value)>,
    numeric_values: Option<Vec<f64>>,
    descending: bool,
    flags: i64,
}

struct TokenizerStaticCallTraceContext {
    call: String,
    values: Vec<Value>,
    call_span: php_ir::IrSpan,
}

enum ArrayCallbackError {
    Runtime(Box<VmResult>),
    BuiltinType {
        function: &'static str,
        actual: String,
    },
    BuiltinTypeMessage(String),
    Message(String),
}

type UnitFunctionKey = (u64, u32);
type TrivialMethodPlanCache = Rc<RefCell<HashMap<UnitFunctionKey, Option<TrivialMethodPlan>>>>;
type LastUseMovePlanCache =
    Rc<RefCell<HashMap<UnitFunctionKey, Rc<crate::last_use::LastUseMovePlan>>>>;
type WorkerQuickeningTables = Rc<RefCell<HashMap<u64, QuickeningTable>>>;

struct WorkerQuickeningLease<'a> {
    request_table: &'a RefCell<QuickeningTable>,
    worker_tables: WorkerQuickeningTables,
    unit_key: u64,
    enabled: bool,
}

impl<'a> WorkerQuickeningLease<'a> {
    fn begin(
        request_table: &'a RefCell<QuickeningTable>,
        worker_tables: WorkerQuickeningTables,
        unit_key: u64,
        enabled: bool,
    ) -> Self {
        let table = if enabled {
            worker_tables
                .borrow_mut()
                .remove(&unit_key)
                .unwrap_or_default()
        } else {
            QuickeningTable::default()
        };
        *request_table.borrow_mut() = table;
        Self {
            request_table,
            worker_tables,
            unit_key,
            enabled,
        }
    }
}

impl Drop for WorkerQuickeningLease<'_> {
    fn drop(&mut self) {
        let table = std::mem::take(&mut *self.request_table.borrow_mut());
        if self.enabled {
            self.worker_tables.borrow_mut().insert(self.unit_key, table);
        }
    }
}

/// Minimal interpreter VM.
#[derive(Clone, Debug)]
pub struct Vm {
    options: VmOptions,
    trace: RefCell<Vec<String>>,
    counters: RefCell<Option<VmCounters>>,
    literal_pool: RefCell<LiteralPool>,
    quickening: RefCell<QuickeningTable>,
    worker_quickening_tables: WorkerQuickeningTables,
    /// Replay-stable snapshot captured before the request quickening lease
    /// returns its table to worker-owned adaptive state.
    persistent_quickening_snapshot: RefCell<Vec<crate::quickening::QuickeningSiteSnapshot>>,
    /// Final invalidation epochs of the last `execute` call, stashed before
    /// request state drops so the persistent-feedback writer can stamp entries
    /// with the true observation state instead of cold-start zeros.
    persistent_feedback_epochs: Cell<Option<crate::persistent_feedback::PersistentFeedbackEpochs>>,
    /// IC-table unit key (`compiled_unit_cache_key`) of the last executed
    /// entry unit, for scoping persistent callsite exports to replay-stable
    /// (entry-unit) IC sites.
    persistent_feedback_entry_unit_key: Cell<Option<u64>>,
    inline_caches: Rc<RefCell<InlineCacheTable>>,
    #[allow(dead_code)]
    jit: Rc<RefCell<JitRuntimeState>>,
    tiering: Rc<RefCell<TieringState>>,
    internal_function_dispatch_cache: Rc<RefCell<InternalFunctionDispatchCache>>,
    /// Memoized per-(unit, function) trivial-method inline plans.
    trivial_method_plans: TrivialMethodPlanCache,
    /// Memoized activation-context class-name handles keyed by the exact name
    /// spelling dispatch sees. The normalized/display forms of a spelling never
    /// change, so hot method-call sites attach shared handles with refcount
    /// bumps instead of re-normalizing three fresh `String`s per call.
    class_name_handles: Rc<RefCell<HashMap<String, ClassNameHandles>>>,
    /// Memoized resolved runtime class entries so repeated instantiations of a
    /// class do not rebuild the whole entry (lineage walk, property/constant
    /// evaluation, method mapping) on every `new`. Invalidated whenever the
    /// class table changes (tracked by `ExecutionState::class_table_epoch`).
    runtime_class_entry_cache: Rc<RefCell<RuntimeClassEntryCache>>,
    /// Memoized raw IR class entries (shared via `Rc`) so repeated `new` of a
    /// class does not deep-clone the whole class definition per instantiation.
    /// Invalidated by `ExecutionState::class_table_epoch`.
    ir_class_entry_cache: Rc<RefCell<IrClassEntryCache>>,
    /// Memoized default declared-slot templates so the hot `new C(...)` path
    /// clones a prebuilt slot vector instead of re-running the per-property
    /// default-materialization loop on every instantiation. Invalidated by
    /// `ExecutionState::class_table_epoch`, so a redefinition rebuilds it from
    /// the current class entry.
    default_slot_template_cache: Rc<RefCell<DefaultSlotTemplateCache>>,
    /// Memoized `__construct` resolution outcomes so the hot `new C(...)` path
    /// does not re-run the inheritance + visibility method-resolution walk on
    /// every instantiation. Keyed by (normalized class name, normalized caller
    /// scope) and guarded by `ExecutionState::class_table_epoch`, so a
    /// redeclaration or autoload (both bump the epoch) drops stale outcomes.
    constructor_resolution_cache: Rc<RefCell<ConstructorResolutionCache>>,
    adaptive_tiny_unit_setup_skipped: Cell<bool>,
    include_execution_depth: Cell<u32>,
    request_profile_stack: RefCell<Vec<RequestProfileFrame>>,
    /// Memoized per-(unit, function) last-use move plans (Runtime lever R3).
    /// Built only when `options.last_use_moves` is on; empty and never consulted
    /// otherwise, keeping the default dense read path byte-identical.
    last_use_move_plans: LastUseMovePlanCache,
    /// Per-request receiver-class lookup by shared class-name identity.
    object_class_resolution: Rc<RefCell<ObjectClassResolution>>,
    class_relation_cache: Rc<RefCell<ClassRelationCache>>,
    /// Per-unit resolved constant tables (zend literal-table parity): each
    /// materializable `IrConstant` resolves once into an interned value and
    /// every later operand read is an indexed refcount bump instead of a
    /// fresh allocation (strings) or a full rebuild (constant arrays).
    /// Keyed by the compiled unit's cache identity; unit constant tables
    /// are immutable per identity, so entries never invalidate.
    resolved_constants: Rc<RefCell<ResolvedConstantTables>>,
}

/// Engine-owned caches retained by one worker across isolated requests.
/// PHP-visible request state, frames, globals, resources and live IC values
/// are intentionally absent.
#[derive(Clone, Debug)]
pub struct VmWorkerState {
    trivial_method_plans: TrivialMethodPlanCache,
    class_name_handles: Rc<RefCell<HashMap<String, ClassNameHandles>>>,
    last_use_move_plans: LastUseMovePlanCache,
    resolved_constants: Rc<RefCell<ResolvedConstantTables>>,
    internal_function_dispatch_cache: Rc<RefCell<InternalFunctionDispatchCache>>,
    jit: Rc<RefCell<JitRuntimeState>>,
    tiering: Rc<RefCell<TieringState>>,
    inline_caches: Rc<RefCell<InlineCacheTable>>,
    quickening_tables: WorkerQuickeningTables,
    runtime_class_entry_cache: Rc<RefCell<RuntimeClassEntryCache>>,
    ir_class_entry_cache: Rc<RefCell<IrClassEntryCache>>,
    default_slot_template_cache: Rc<RefCell<DefaultSlotTemplateCache>>,
    constructor_resolution_cache: Rc<RefCell<ConstructorResolutionCache>>,
    object_class_resolution: Rc<RefCell<ObjectClassResolution>>,
    class_relation_cache: Rc<RefCell<ClassRelationCache>>,
}

impl VmWorkerState {
    #[must_use]
    pub fn new(tiering: crate::tiering::TieringOptions) -> Self {
        Self {
            trivial_method_plans: Rc::new(RefCell::new(HashMap::new())),
            class_name_handles: Rc::new(RefCell::new(HashMap::new())),
            last_use_move_plans: Rc::new(RefCell::new(HashMap::new())),
            resolved_constants: Rc::new(RefCell::new(ResolvedConstantTables::default())),
            internal_function_dispatch_cache: Rc::new(RefCell::new(
                InternalFunctionDispatchCache::default(),
            )),
            jit: Rc::new(RefCell::new(JitRuntimeState::default())),
            tiering: Rc::new(RefCell::new(TieringState::new(tiering))),
            inline_caches: Rc::new(RefCell::new(InlineCacheTable::default())),
            quickening_tables: Rc::new(RefCell::new(HashMap::new())),
            runtime_class_entry_cache: Rc::new(RefCell::new(RuntimeClassEntryCache::default())),
            ir_class_entry_cache: Rc::new(RefCell::new(IrClassEntryCache::default())),
            default_slot_template_cache: Rc::new(RefCell::new(DefaultSlotTemplateCache::default())),
            constructor_resolution_cache: Rc::new(RefCell::new(
                ConstructorResolutionCache::default(),
            )),
            object_class_resolution: Rc::new(RefCell::new(ObjectClassResolution::default())),
            class_relation_cache: Rc::new(RefCell::new(ClassRelationCache::default())),
        }
    }
}

impl Default for VmWorkerState {
    fn default() -> Self {
        Self::new(crate::tiering::TieringOptions::default())
    }
}
impl Vm {
    /// Creates a VM with default options.
    #[must_use]
    pub fn new() -> Self {
        Self::with_options(VmOptions::default())
    }

    /// Creates a VM with explicit options.
    #[must_use]
    pub fn with_options(options: VmOptions) -> Self {
        let worker_state = VmWorkerState::new(options.tiering.clone());
        Self::with_options_and_worker_state(options, worker_state)
    }

    /// Creates an isolated request VM backed by engine-only worker caches.
    #[must_use]
    pub fn with_options_and_worker_state(options: VmOptions, worker_state: VmWorkerState) -> Self {
        Self {
            options,
            trace: RefCell::new(Vec::new()),
            counters: RefCell::new(None),
            literal_pool: RefCell::new(LiteralPool::default()),
            trivial_method_plans: worker_state.trivial_method_plans,
            class_name_handles: worker_state.class_name_handles,
            runtime_class_entry_cache: worker_state.runtime_class_entry_cache,
            ir_class_entry_cache: worker_state.ir_class_entry_cache,
            default_slot_template_cache: worker_state.default_slot_template_cache,
            constructor_resolution_cache: worker_state.constructor_resolution_cache,
            quickening: RefCell::new(QuickeningTable::default()),
            worker_quickening_tables: worker_state.quickening_tables,
            persistent_quickening_snapshot: RefCell::new(Vec::new()),
            persistent_feedback_epochs: Cell::new(None),
            persistent_feedback_entry_unit_key: Cell::new(None),
            inline_caches: worker_state.inline_caches,
            jit: worker_state.jit,
            tiering: worker_state.tiering,
            internal_function_dispatch_cache: worker_state.internal_function_dispatch_cache,
            adaptive_tiny_unit_setup_skipped: Cell::new(false),
            include_execution_depth: Cell::new(0),
            request_profile_stack: RefCell::new(Vec::new()),
            last_use_move_plans: worker_state.last_use_move_plans,
            resolved_constants: worker_state.resolved_constants,
            object_class_resolution: worker_state.object_class_resolution,
            class_relation_cache: worker_state.class_relation_cache,
        }
    }

    /// Compiles the entry function with mandatory Cranelift and invokes its
    /// native entry point. Unsupported lowering is a compile error; product
    /// execution never resumes through an interpreter.
    #[cfg(not(test))]
    #[must_use]
    pub fn execute(&self, unit: impl Into<CompiledUnit>) -> VmResult {
        self.execute_native_only(unit.into())
    }

    #[must_use]
    fn execute_native_only(&self, unit: CompiledUnit) -> VmResult {
        let output = OutputBuffer::default();
        let entry = unit.unit().entry;
        let Some(function) = unit.unit().functions.get(entry.index()) else {
            return VmResult::compile_error(output, "entry function is missing");
        };
        if self.options.verify_ir && unit.prepared_ir_verification_errors() > 0 {
            return VmResult::compile_error(
                output,
                format!(
                    "IR verifier failed with {} error(s)",
                    unit.prepared_ir_verification_errors()
                ),
            );
        }

        let eligibility = php_jit::analyze_jit_eligibility(unit.unit(), entry);
        if !matches!(eligibility.eligibility, php_jit::JitEligibility::Eligible) {
            let reason = eligibility.reasons.first();
            let location = reason
                .and_then(|reason| reason.block.zip(reason.instruction))
                .and_then(|(block, instruction)| {
                    function
                        .blocks
                        .get(block as usize)
                        .and_then(|block| block.instructions.get(instruction as usize))
                });
            let instruction = location
                .map(|instruction| format!("{:?}", instruction.kind))
                .unwrap_or_else(|| "function-signature".to_owned());
            let span = location.map_or(function.span, |instruction| instruction.span);
            let detail = reason.map_or("unsupported function shape", |reason| {
                reason.detail.as_str()
            });
            return VmResult::compile_error(
                output,
                format!(
                    "E_NATIVE_UNSUPPORTED_LOWERING: function={} instruction_kind={} span={}:{}-{}: {}",
                    function.name,
                    instruction,
                    span.file.raw(),
                    span.start,
                    span.end,
                    detail
                ),
            );
        }

        let mut compiler = php_jit::JitEngine::new();
        let compiled = match compiler.compile_function(
            unit.unit(),
            entry,
            php_jit::JitCompileRequest::new(format!("entry.{}", function.name))
                .with_function_name(function.name.clone())
                .with_opt_level(if self.options.native_optimization.is_optimizing() {
                    2
                } else {
                    0
                }),
        ) {
            Ok(compiled) => compiled,
            Err(error) => {
                return VmResult::compile_error(output, format!("E_NATIVE_COMPILE_SETUP: {error}"));
            }
        };
        let Some(handle) = compiled.handle else {
            let reason = match compiled.status {
                php_jit::JitCompileStatus::Rejected { reason } => reason,
                php_jit::JitCompileStatus::Compiled => {
                    "compiler reported success without a native entry".to_owned()
                }
            };
            return VmResult::compile_error(output, format!("E_NATIVE_COMPILE: {reason}"));
        };
        match handle.invoke_i64(&[], php_jit::JIT_RUNTIME_ABI_HASH) {
            Ok(value) => VmResult::success(output, Some(Value::Int(value))),
            Err(error) => VmResult::compile_error(
                output,
                format!("E_NATIVE_ENTRY: native entry invocation failed: {error:?}"),
            ),
        }
    }

    /// Executes through the frozen interpreter only in php_vm's internal test
    /// build. Product dependencies compile the native-only method above.
    #[cfg(test)]
    #[must_use]
    pub fn execute(&self, unit: impl Into<CompiledUnit>) -> VmResult {
        let unit = unit.into();
        let entry_unit_key = compiled_unit_cache_key(&unit);
        let _quickening_lease = WorkerQuickeningLease::begin(
            &self.quickening,
            Rc::clone(&self.worker_quickening_tables),
            entry_unit_key,
            self.options.persistent_adaptive_state && self.options.quickening.enabled(),
        );
        let persistent_quickening_reused_sites = self.quickening.borrow().touched_site_count();
        self.tiering
            .borrow_mut()
            .begin_request(self.options.tiering.clone());
        let skip_adaptive_tiny_unit_setup = self.should_skip_adaptive_tiny_unit_setup(unit.unit());
        self.adaptive_tiny_unit_setup_skipped
            .set(skip_adaptive_tiny_unit_setup);
        let mut output = OutputBuffer::with_capacity(output_preallocation_hint(unit.unit()));
        self.trace.borrow_mut().clear();
        *self.literal_pool.borrow_mut() = LiteralPool::default();
        self.persistent_feedback_epochs.set(None);
        self.persistent_quickening_snapshot.borrow_mut().clear();
        // IC slots and the entry-unit scope filter share the compiled unit's
        // stable cache identity.
        self.persistent_feedback_entry_unit_key
            .set(Some(entry_unit_key));
        let mut persistent_feedback_seeded_sites = 0usize;
        if self.options.quickening.enabled() && !self.options.quickening_seed.is_empty() {
            persistent_feedback_seeded_sites = self
                .quickening
                .borrow_mut()
                .seed_persistent_sites(&self.options.quickening_seed);
        }
        let dynamic_ic_invalidations = self
            .inline_caches
            .borrow_mut()
            .begin_request(self.options.persistent_adaptive_state);
        let mut persistent_feedback_seeded_callsites = 0usize;
        if self.options.inline_caches.enabled() && !self.options.callsite_seed.is_empty() {
            // Only seed a callsite whose recorded target function still exists
            // in this unit and whose normalized name equals the recorded call
            // name. The lookup guard matches name/arity/epoch but never
            // re-resolves name→target, so this is the one place a seed with a
            // stale or tampered (name, target) pair — including a
            // namespace-fallback call whose namespaced definition now exists —
            // is rejected before it can dispatch the wrong function.
            let entry_functions = &unit.unit().functions;
            persistent_feedback_seeded_callsites = self
                .inline_caches
                .borrow_mut()
                .seed_persistent_function_callsites(
                    compiled_unit_cache_key(&unit),
                    &self.options.callsite_seed,
                    |site| {
                        entry_functions
                            .get(site.target_function as usize)
                            .is_some_and(|function| {
                                normalize_function_name(&function.name) == site.lowered_name
                            })
                    },
                );
        }
        self.include_execution_depth.set(0);
        *self.counters.borrow_mut() = self.options.collect_counters.then(|| {
            let mut counters = VmCounters::default();
            counters.set_jit_config(
                self.options.native_optimization.as_str(),
                self.options.jit_threshold,
            );
            if skip_adaptive_tiny_unit_setup {
                counters.record_adaptive_tiny_unit_setup_skip();
            }
            if persistent_feedback_seeded_sites > 0 {
                counters.record_persistent_feedback_seeded_sites(
                    persistent_feedback_seeded_sites as u64,
                );
            }
            if persistent_feedback_seeded_callsites > 0 {
                counters.record_persistent_feedback_seeded_callsites(
                    persistent_feedback_seeded_callsites as u64,
                );
            }
            if persistent_quickening_reused_sites > 0 {
                counters.record_persistent_worker_quickening_reuse(
                    persistent_quickening_reused_sites as u64,
                );
            }
            counters
        });
        for _ in 0..dynamic_ic_invalidations {
            self.record_counter_persistent_worker_invalidation("dynamic_unit_target");
        }
        if self.options.collect_counters {
            php_runtime::experimental::numeric_string::reset_cache_and_stats();
            php_runtime::experimental::layout_stats::reset_layout_stats();
            if self.options.collect_layout_source_attribution {
                php_runtime::experimental::layout_stats::enable_layout_source_attribution();
            } else {
                php_runtime::experimental::layout_stats::disable_layout_source_attribution();
            }
        } else {
            php_runtime::experimental::layout_stats::disable_layout_source_attribution();
        }
        reset_float_string_precision();

        if self.options.verify_ir {
            let prepared_ir_errors = unit.prepared_ir_verification_errors();
            if self.options.revalidate_prepared_unit {
                let recomputed_ir_errors = php_ir::verify::verify_unit(unit.unit())
                    .map_or_else(|errors| errors.len(), |()| 0);
                if recomputed_ir_errors != prepared_ir_errors {
                    return VmResult::compile_error(
                        output,
                        format!(
                            "E_PHP_VM_PREPARED_VALIDATION_MISMATCH: cached IR errors={prepared_ir_errors}, recomputed={recomputed_ir_errors}"
                        ),
                    );
                }
            }
            if prepared_ir_errors > 0 {
                return VmResult::compile_error(
                    output,
                    format!("IR verifier failed with {prepared_ir_errors} error(s)"),
                );
            }
        }

        let entry = unit.unit().entry;
        if unit.unit().functions.get(entry.index()).is_none() {
            return VmResult::compile_error(output, "entry function is missing");
        }
        let prepared_class_validation = unit.prepared_class_validation(|| {
            validate_class_table(&unit).map_err(|error| {
                let (message, diagnostic) = error.into_parts();
                Box::new(PreparedClassValidationError {
                    message,
                    diagnostic,
                })
            })
        });
        if self.options.revalidate_prepared_unit {
            let recomputed = validate_class_table(&unit).err().map(|error| error.message);
            let prepared = prepared_class_validation
                .as_ref()
                .err()
                .map(|error| error.message.as_str());
            if recomputed.as_deref() != prepared {
                return VmResult::compile_error(
                    output,
                    "E_PHP_VM_PREPARED_VALIDATION_MISMATCH: class validation changed",
                );
            }
        }
        if let Err(error) = prepared_class_validation {
            let message = error.message;
            let diagnostic = error.diagnostic;
            return match diagnostic {
                Some(diagnostic) => VmResult {
                    status: ExecutionStatus::compile_error(message),
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
                },
                None => VmResult::compile_error(output, message),
            };
        }
        self.warm_literal_pool(unit.unit());

        let mut stack = CallStack::new();
        let ini = self.options.runtime_context.ini_registry();
        let parsed_include_path = parse_ini_include_path(&ini);
        let env = sorted_request_env(&self.options.runtime_context.env);
        let filter_input_arrays = request_filter_input_arrays(&self.options.runtime_context);
        let network_requests_enabled = env
            .iter()
            .any(|(name, value)| name == "PHRUST_NET_TESTS" && value == "1");
        let mut state = ExecutionState {
            worker_symbol_epoch: self.options.worker_symbol_epoch,
            function_table_epoch: if self.options.worker_symbol_epoch {
                WORKER_SYMBOL_LEDGER.with(|ledger| ledger.epoch.get())
            } else {
                0
            },
            cwd: self.options.runtime_context.cwd.clone(),
            ini,
            parsed_include_path,
            default_timezone: php_runtime::api::datetime::DEFAULT_TIMEZONE.to_owned(),
            env,
            filter_input_arrays,
            network_requests_enabled,
            spl_autoload_extensions: ".inc,.php".to_owned(),
            class_relation_cache: Rc::clone(&self.class_relation_cache),
            request: RequestLifecycleState::from_runtime_context(&self.options.runtime_context),
            execution_deadline_at: self
                .options
                .runtime_context
                .execution_time_limit
                .and_then(|limit| Instant::now().checked_add(limit)),
            execution_deadline_mutable: self.options.runtime_context.execution_time_limit.is_some(),
            ..ExecutionState::default()
        };
        state.stdin = Some(
            state
                .resources
                .register_stdin(self.options.runtime_context.stdin.to_vec()),
        );
        register_dynamic_unit(&mut state, &unit, unit.clone(), DeclarationLoadKind::Main);
        apply_float_string_precision(&state.ini);
        let auto_start_span = unit
            .unit()
            .functions
            .get(entry.index())
            .map_or_else(RuntimeSourceSpan::default, |function| {
                runtime_source_span(&unit, function.span)
            });
        auto_start_session_if_configured(&mut state, auto_start_span);
        seed_runtime_globals(&mut state.globals, &self.options.runtime_context);
        emit_private_final_method_warnings(&unit, &mut output, &mut state);
        emit_serializable_interface_deprecations(&unit, &mut output, &mut state);
        let mut result = if self.options.execution_format.attempts_bytecode() {
            match self.try_execute_bytecode_entry(&unit, &mut output, &mut stack, &mut state) {
                BytecodeEntryAttempt::Executed(result) => *result,
                BytecodeEntryAttempt::Unsupported(message) => {
                    let reason = dense_bytecode_unsupported_reason(&message);
                    self.record_counter_bytecode_unsupported_reason(reason);
                    if self.options.execution_format.is_strict_bytecode() {
                        VmResult {
                            status: ExecutionStatus::unsupported(message),
                            output: output.clone(),
                            diagnostics: Vec::new(),
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
                            http_response: None,
                            upload_registry: None,
                            session: None,
                        }
                    } else {
                        self.record_counter_bytecode_unsupported_fallback();
                        self.record_counter_bytecode_auto_fallback_reason(reason);
                        self.execute_function(
                            &unit,
                            entry,
                            FunctionCall::new(Vec::new(), Vec::new()),
                            &mut output,
                            &mut stack,
                            &mut state,
                        )
                    }
                }
            }
        } else {
            self.execute_function(
                &unit,
                entry,
                FunctionCall::new(Vec::new(), Vec::new()),
                &mut output,
                &mut stack,
                &mut state,
            )
        };
        // A throwable that unwound past `main` without a handler is uncaught:
        // render it as PHP's fatal error here, at the top of the call stack.
        if let Some(throwable) = state.pending_throw.take()
            && !vm_result_has_php_fatal_output(&result)
        {
            result = self.handle_uncaught_exception(
                &unit,
                &mut output,
                &mut stack,
                &mut state,
                throwable,
            );
        }
        if result.status.is_success()
            && let Some(error) = self.validate_runtime_class_dependencies(
                &unit,
                &unit,
                &mut output,
                &mut stack,
                &mut state,
            )
        {
            result = error;
        }
        if result.status.is_success() {
            match self.run_shutdown_functions(&unit, &mut output, &mut state) {
                Ok(diagnostics) => {
                    result.diagnostics.extend(diagnostics);
                }
                Err(error) => {
                    result = *error;
                }
            }
        }
        if result.status.is_success() {
            match self.run_shutdown_user_stream_wrappers(&unit, &mut output, &mut state) {
                Ok(diagnostics) => {
                    result.diagnostics.extend(diagnostics);
                }
                Err(error) => {
                    result = *error;
                }
            }
        }
        if result.status.is_success() {
            match self.run_shutdown_destructors(&unit, &mut output, &mut state) {
                Ok(diagnostics) => {
                    result.diagnostics.extend(diagnostics);
                }
                Err(error) => {
                    result = *error;
                }
            }
        }
        if self.options.trace_runtime {
            self.record_gc_root_trace_event(&stack, &state);
        }
        output.flush_all_buffers();
        let output_len = output.len();
        let output_stats = output.stats();
        sync_session_state_from_globals(&mut state);
        self.persistent_feedback_epochs.set(Some(
            crate::persistent_feedback::PersistentFeedbackEpochs {
                class_table: state.class_table_epoch,
                function_table: state.function_table_epoch,
                autoload: state.autoload_stack_epoch,
                include_path: state.include_config_epoch,
            },
        ));
        result.diagnostics.extend(state.diagnostics);
        result.http_response = Some(Box::new(state.request.http_response));
        result.upload_registry = Some(Box::new(state.request.upload_registry));
        result.session = Some(Box::new(state.request.session));
        if self.options.trace || self.options.trace_runtime || self.options.trace_includes {
            result.trace = self.trace.borrow().clone();
        }
        if self.options.collect_counters {
            let stats = php_runtime::experimental::numeric_string::take_cache_stats();
            let layout_stats = php_runtime::experimental::layout_stats::take_layout_stats();
            let layout_source_stats =
                php_runtime::experimental::layout_stats::take_layout_source_stats();
            let (interned_names, interned_name_bytes) =
                php_runtime::experimental::string::symbol_interner_footprint();
            if let Some(counters) = self.counters.borrow_mut().as_mut() {
                counters.record_numeric_string_cache_stats(stats);
                counters.record_runtime_layout_stats(layout_stats);
                counters.record_runtime_layout_source_stats(layout_source_stats);
                counters.record_output_stats(output_len, output_stats);
                counters.record_persistent_engine_footprint(interned_names, interned_name_bytes);
                counters.fold_scratch_counters();
            }
            result.counters = self.counters.borrow().clone().map(Box::new);
        }
        if self.options.tiering.collect_stats {
            result.tiering_stats = Some(Box::new(self.tiering.borrow().stats()));
        }
        *self.persistent_quickening_snapshot.borrow_mut() =
            self.quickening.borrow().export_persistent_sites();
        result.output = output;
        result
    }

    /// Exports adaptive quickening sites observed by the last `execute` call
    /// for persistent feedback. Empty when quickening was disabled.
    #[must_use]
    pub fn export_persistent_quickening(&self) -> Vec<crate::quickening::QuickeningSiteSnapshot> {
        self.persistent_quickening_snapshot.borrow().clone()
    }

    /// Final invalidation epochs of the last `execute` call, for stamping
    /// persistent-feedback entries with their true observation state. `None`
    /// when the last execution ended before request teardown (compile
    /// errors), which callers must treat as cold-start zeros.
    #[must_use]
    pub fn export_persistent_feedback_epochs(
        &self,
    ) -> Option<crate::persistent_feedback::PersistentFeedbackEpochs> {
        self.persistent_feedback_epochs.get()
    }

    /// Exports the last `execute` call's replay-stable monomorphic
    /// function-call IC sites (entry unit only) for persistent feedback.
    #[must_use]
    pub fn export_persistent_function_callsites(
        &self,
    ) -> Vec<crate::inline_cache::FunctionCallSiteSnapshot> {
        let Some(entry_unit_key) = self.persistent_feedback_entry_unit_key.get() else {
            return Vec::new();
        };
        self.inline_caches
            .borrow()
            .export_persistent_function_callsites(entry_unit_key)
    }

    fn should_skip_adaptive_tiny_unit_setup(&self, unit: &IrUnit) -> bool {
        let Some(threshold) = self.options.adaptive_tiny_unit_setup_threshold else {
            return false;
        };
        if !self.options.tiering.enabled || !self.options.quickening.enabled() {
            return false;
        }
        ir_unit_instruction_count(unit) <= threshold
    }

    fn record_include_cache_stats_delta(
        &self,
        before: IncludeCacheStats,
        after: IncludeCacheStats,
    ) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            for _ in 0..after.resolution_hits.saturating_sub(before.resolution_hits) {
                counters.record_include_resolution_hit();
            }
            for _ in 0..after
                .resolution_misses
                .saturating_sub(before.resolution_misses)
            {
                counters.record_include_resolution_miss();
            }
            for _ in 0..after.compile_hits.saturating_sub(before.compile_hits) {
                counters.record_include_compile_hit();
            }
            for _ in 0..after.compile_misses.saturating_sub(before.compile_misses) {
                counters.record_include_compile_miss();
            }
            for _ in 0..after
                .stale_invalidations
                .saturating_sub(before.stale_invalidations)
            {
                counters.record_include_stale_invalidation_by_reason("file_fingerprint_changed");
            }
            for _ in 0..after
                .directory_version_hits
                .saturating_sub(before.directory_version_hits)
            {
                counters.record_directory_version_hit();
            }
            for _ in 0..after
                .directory_version_misses
                .saturating_sub(before.directory_version_misses)
            {
                counters.record_directory_version_miss();
            }
            for _ in 0..after
                .negative_cache_hits
                .saturating_sub(before.negative_cache_hits)
            {
                counters.record_negative_include_cache_hit();
            }
            for _ in 0..after
                .negative_cache_installs
                .saturating_sub(before.negative_cache_installs)
            {
                counters.record_negative_include_cache_install();
            }
            for _ in 0..after
                .negative_cache_invalidations
                .saturating_sub(before.negative_cache_invalidations)
            {
                counters.record_negative_include_cache_invalidation();
            }
            for _ in 0..after
                .negative_cache_blocked_unversioned
                .saturating_sub(before.negative_cache_blocked_unversioned)
            {
                counters.record_negative_include_cache_blocked("candidate_directory_unversioned");
            }
            for _ in 0..after
                .negative_cache_blocked_capacity
                .saturating_sub(before.negative_cache_blocked_capacity)
            {
                counters.record_negative_include_cache_blocked("capacity");
            }
        }
    }

    fn record_include_graph_resolution_fallback(&self, path: &str, message: &str) {
        if php_runtime::api::phar::is_phar_uri(path) {
            self.record_counter_fallback_by_path_semantics("phar_stream");
        } else if path.contains("://") {
            self.record_counter_fallback_by_path_semantics("stream_wrapper");
        } else if message.contains("OUTSIDE_ROOT") {
            self.record_counter_fallback_by_path_semantics("outside_allowed_root");
        } else if message.contains("MISSING") {
            self.record_counter_fallback_by_path_semantics("missing_path");
            // The shared include cache installs directory-version-guarded
            // negative entries for missing paths (its install/blocked
            // accounting arrives via the cache stats delta). The request-local
            // IC path performs no such validation, so its misses stay
            // uncached and record why.
            if self.options.include_cache.is_none() {
                self.record_counter_negative_include_cache_blocked(
                    "directory_versions_unvalidated",
                );
            }
        } else {
            self.record_counter_fallback_by_path_semantics("loader_error");
        }
    }

    /// Returns the request's Composer autoload-map fingerprint, computing it
    /// once on first use. The value is stable for the whole request, so wiring
    /// it into autoload cache keys never changes hit/miss behavior within a
    /// request; it only keys the (request-local) entries on the deployment's
    /// autoload maps. `None` means no map was detected — unknown, which any
    /// future persistent reuse must treat as blocking.
    fn composer_map_fingerprint(&self, state: &mut ExecutionState) -> Option<Arc<str>> {
        // The fingerprint keys only the (request-local) autoload inline cache;
        // when inline caches are disabled (baseline/oracle mode) the key is
        // never stored or compared, so skip the ~10-stat vendor/composer probe
        // entirely rather than paying it per lookup for a discarded key.
        if !self.options.inline_caches.enabled() {
            return None;
        }
        if state.composer_map_fingerprint.is_none() {
            let fingerprint = self
                .composer_probe_anchor(state)
                .and_then(|anchor| crate::include::composer_autoload_map_fingerprint(&anchor))
                .map(Arc::<str>::from);
            if self.options.collect_counters
                && let Some(counters) = self.counters.borrow_mut().as_mut()
            {
                counters.record_composer_fingerprint(fingerprint.is_some());
            }
            if let Some(cache) = &self.options.include_cache
                && matches!(
                    cache.note_composer_fingerprint(fingerprint.as_deref()),
                    crate::include::ComposerFingerprintTransition::Changed
                )
                && self.options.collect_counters
                && let Some(counters) = self.counters.borrow_mut().as_mut()
            {
                counters.record_composer_fingerprint_stale();
            }
            state.composer_map_fingerprint = Some(fingerprint);
        }
        state.composer_map_fingerprint.clone().unwrap_or(None)
    }

    /// Anchor directory for the Composer map probe: the entry script's
    /// directory (HTTP script filename or CLI argv[0]), falling back to the
    /// request CWD.
    fn composer_probe_anchor(&self, state: &ExecutionState) -> Option<PathBuf> {
        let script = match &self.options.runtime_context.request_mode {
            php_runtime::api::RuntimeRequestMode::Http(request) => {
                Some(PathBuf::from(&request.script_filename))
            }
            _ => self.options.runtime_context.argv.first().map(PathBuf::from),
        };
        let script = script.filter(|path| !path.as_os_str().is_empty())?;
        let script = if script.is_absolute() {
            script
        } else {
            state.cwd.join(script)
        };
        script
            .parent()
            .map(Path::to_path_buf)
            .or_else(|| Some(state.cwd.clone()))
    }

    fn record_tiering_backedge(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        current: BlockId,
        target: BlockId,
    ) {
        self.tiering.borrow_mut().record_loop_backedge(
            compiled_unit_cache_key(compiled),
            function_id,
            current,
            target,
        );
    }

    fn object_instanceof_cached(
        &self,
        compiled: &CompiledUnit,
        state: &mut ExecutionState,
        value: &Value,
        class_name: &str,
    ) -> Result<bool, String> {
        if !self.options.inline_caches.enabled() {
            return object_instanceof_in_state(compiled, state, value, class_name);
        }
        let Some(subject) = class_relation_subject_name(value) else {
            return object_instanceof_in_state(compiled, state, value, class_name);
        };
        let key = ClassRelationCacheKey {
            kind: ClassRelationKind::InstanceOf,
            subject,
            target: normalize_class_name(class_name),
            member: None,
            visibility_context: None,
            config_fingerprint: class_relation_config_fingerprint(compiled),
        };
        let epochs = state.class_relation_epochs();
        let lookup = state.class_relation_cache.borrow_mut().lookup(&key, epochs);
        match lookup {
            ClassRelationCacheLookup::Hit(target) => {
                self.record_counter_persistent_worker_ic("class_relation", true);
                self.record_counter_class_relation_cache_hit();
                self.record_counter_instanceof_cache_hit();
                return Ok(target.matches);
            }
            ClassRelationCacheLookup::Invalidated => {
                self.record_counter_persistent_worker_ic("class_relation", false);
                self.record_counter_class_relation_cache_invalidation();
                self.record_counter_instanceof_cache_miss();
            }
            ClassRelationCacheLookup::Miss => {
                self.record_counter_persistent_worker_ic("class_relation", false);
                self.record_counter_class_relation_cache_miss();
                self.record_counter_instanceof_cache_miss();
            }
        }
        let matches = object_instanceof_in_state(compiled, state, value, class_name)?;
        state.class_relation_cache.borrow_mut().install(
            key,
            epochs,
            ClassRelationCacheTarget {
                matches,
                method_slot: None,
                declaring_class: None,
            },
        );
        Ok(matches)
    }

    fn php_token_static_method_error_result(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        error: PhpTokenStaticMethodError,
    ) -> VmResult {
        self.runtime_error(output, compiled, stack, error.into_message())
    }

    fn route_tokenizer_static_method_diagnostics(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: Vec<RuntimeDiagnostic>,
        trace_context: Option<&TokenizerStaticCallTraceContext>,
    ) -> Result<(), Box<VmResult>> {
        for diagnostic in diagnostics {
            let (level, channel) = match diagnostic.severity() {
                RuntimeSeverity::Warning => (
                    php_runtime::api::PHP_E_WARNING,
                    php_runtime::api::PhpDiagnosticChannel::Warning,
                ),
                RuntimeSeverity::Deprecation => (
                    php_runtime::api::PHP_E_DEPRECATED,
                    php_runtime::api::PhpDiagnosticChannel::Deprecated,
                ),
                _ => {
                    state.diagnostics.push(diagnostic);
                    continue;
                }
            };
            let handled = match self.dispatch_error_handler(
                compiled,
                output,
                stack,
                state,
                level,
                &diagnostic,
            ) {
                Ok(handled) => handled,
                Err(result) => {
                    if let Some(trace_context) = trace_context {
                        attach_tokenizer_static_error_handler_throw_trace(
                            compiled,
                            stack,
                            state,
                            &result,
                            trace_context,
                            level,
                            &diagnostic,
                        );
                    }
                    return Err(result);
                }
            };
            if handled {
                continue;
            }
            if error_reporting_allows(state, level) {
                Self::record_last_error(state, level, &diagnostic);
                emit_vm_diagnostic(output, state, &diagnostic, channel, level);
                state.diagnostics.push(diagnostic);
            }
        }
        Ok(())
    }

    fn php_token_static_method_value_for_class(
        &self,
        compiled: &CompiledUnit,
        state: &ExecutionState,
        class: &php_ir::module::ClassEntry,
        called_class_name: &str,
        args: Vec<CallArgument>,
    ) -> Result<PhpTokenStaticMethodValue, PhpTokenStaticMethodError> {
        let runtime_class = runtime_class_entry(
            compiled,
            state,
            class,
            &|value| self.constant_value(compiled.unit(), value),
            &|reference| class_constant_reference_value(compiled, state, reference),
            &|reference| named_constant_reference_value(compiled, state, reference),
        )
        .map_err(PhpTokenStaticMethodError::RuntimeClass)?;
        validate_object_mvp_with_display_name(&runtime_class, &class.display_name)
            .map_err(PhpTokenStaticMethodError::Runtime)?;
        php_token_static_method_value_for_class_with_diagnostics(
            called_class_name,
            "tokenize",
            args,
            &runtime_class,
            class.display_name.clone(),
        )
        .map_err(PhpTokenStaticMethodError::Runtime)
    }

    fn call_static_method_callable(
        &self,
        cursor: ExecutionCursor<'_>,
        request: StaticMethodCallableRequest<'_>,
    ) -> VmResult {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let StaticMethodCallableRequest {
            class_name,
            method,
            args,
            call_span,
            allow_by_ref_value_warnings,
            by_ref_warning_callable_name,
        } = request;
        if is_closure_runtime_class(class_name) {
            let value = match closure_static_method_value(
                compiled,
                state,
                stack,
                method,
                args,
                output,
                RuntimeSourceSpan::default(),
            ) {
                Ok(value) => value,
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            };
            return VmResult::success_no_output(Some(value));
        }
        if is_php_token_runtime_class(class_name) {
            let trace_values = args.iter().map(|arg| arg.value.clone()).collect::<Vec<_>>();
            let result =
                match php_token_static_method_value_with_diagnostics(class_name, method, args) {
                    Ok(result) => result,
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
            let trace_context = call_span.map(|call_span| TokenizerStaticCallTraceContext {
                call: format!("{class_name}::{method}"),
                values: trace_values,
                call_span,
            });
            if let Err(result) = self.route_tokenizer_static_method_diagnostics(
                compiled,
                output,
                stack,
                state,
                result.diagnostics,
                trace_context.as_ref(),
            ) {
                return *result;
            }
            return VmResult::success_no_output(Some(result.value));
        }
        if internal_extension_static_class(class_name) {
            let values = args.into_iter().map(|arg| arg.value).collect();
            let value = match call_internal_extension_static_method(class_name, method, values) {
                Ok(value) => value,
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            };
            return VmResult::success_no_output(Some(value));
        }
        if let Err(result) = self.autoload_static_class_if_missing(
            ExecutionCursor::new(compiled, output, stack, state),
            class_name,
            call_span.unwrap_or_default(),
            None,
        ) {
            return *result;
        }
        let class = match resolve_static_class_name(compiled, state, stack, class_name) {
            Ok(class) => class,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        if let Err(result) =
            self.autoload_class_parents_if_missing(compiled, &class, output, stack, state)
        {
            return *result;
        }
        let normalized_method = normalize_method_name(method);
        if class.flags.is_enum && matches!(normalized_method.as_str(), "cases" | "from" | "tryfrom")
        {
            let value = match enum_static_method(compiled, state, &class, method, args, &|value| {
                self.constant_value(compiled.unit(), value)
            }) {
                Ok(value) => value,
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            };
            return VmResult::success_no_output(Some(value));
        }
        if class_extends_php_token(compiled, state, &class) && normalized_method == "tokenize" {
            let trace_values = args.iter().map(|arg| arg.value.clone()).collect::<Vec<_>>();
            let result = match self
                .php_token_static_method_value_for_class(compiled, state, &class, class_name, args)
            {
                Ok(result) => result,
                Err(error) => {
                    return self.runtime_error(output, compiled, stack, error.into_message());
                }
            };
            let trace_context = call_span.map(|call_span| TokenizerStaticCallTraceContext {
                call: format!("{class_name}::{method}"),
                values: trace_values,
                call_span,
            });
            if let Err(result) = self.route_tokenizer_static_method_diagnostics(
                compiled,
                output,
                stack,
                state,
                result.diagnostics,
                trace_context.as_ref(),
            ) {
                return *result;
            }
            return VmResult::success_no_output(Some(result.value));
        }
        let scope = method_lookup_scope_for_static_call(compiled, stack, class_name);
        let resolved = match lookup_resolved_method_in_state(
            compiled,
            state,
            &class.name,
            method,
            scope.as_deref(),
        ) {
            Ok(Some(method)) => method,
            Ok(None) => {
                let called_class =
                    called_class_for_static_call(compiled, stack, class_name, &class);
                return match self.call_magic_static_method(
                    ExecutionCursor::new(compiled, output, stack, state),
                    MagicStaticCallRequest {
                        class: &class,
                        magic_method: "__callStatic",
                        called_method: method,
                        args,
                        called_class,
                        call_span,
                    },
                ) {
                    Ok(Some(result)) => result,
                    Ok(None) => self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
                            class.name, method
                        ),
                    ),
                    Err(result) => *result,
                };
            }
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        let method_entry = &resolved.method;
        let declaring_class = &resolved.class;
        let is_constructor_call = normalize_method_name(method) == "__construct";
        let bound_this_for_scoped_call =
            scoped_static_call_this_object(compiled, state, stack, declaring_class, method_entry);
        if !method_entry.flags.is_static && bound_this_for_scoped_call.is_none() {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_NON_STATIC_METHOD_CALL: method {}::{} is not static",
                    declaring_class.name, method_entry.name
                ),
            );
        }
        if !is_constructor_call
            && (method_entry.flags.is_private || method_entry.flags.is_protected)
            && let Err(inaccessible) = validate_method_callable_in_state_scope(
                compiled,
                state,
                current_scope_class(compiled, stack).as_deref(),
                declaring_class,
                method_entry,
            )
        {
            let called_class = called_class_for_static_call(compiled, stack, class_name, &class);
            return match self.call_magic_static_method(
                ExecutionCursor::new(compiled, output, stack, state),
                MagicStaticCallRequest {
                    class: &class,
                    magic_method: "__callStatic",
                    called_method: method,
                    args,
                    called_class,
                    call_span,
                },
            ) {
                Ok(Some(result)) => result,
                Ok(None) => self.runtime_error(output, compiled, stack, inaccessible),
                Err(result) => *result,
            };
        }
        let visibility = if is_constructor_call {
            validate_scoped_constructor_callable_in_state_scope(
                compiled,
                state,
                scope.as_deref(),
                declaring_class,
                method_entry,
            )
        } else {
            validate_method_callable_in_state_scope(
                compiled,
                state,
                current_scope_class(compiled, stack).as_deref(),
                declaring_class,
                method_entry,
            )
        };
        if let Err(message) = visibility {
            return self.runtime_error(output, compiled, stack, message);
        }
        let class_owner = class_owner_in_state(compiled, state, &declaring_class.name);
        let called_class = called_class_for_static_call(compiled, stack, class_name, &class);
        let mut call = FunctionCall::new(args, Vec::new())
            .with_call_site_strict_types(call_site_strictness(compiled, call_span))
            .with_class_context_handles(
                self.class_name_handles(&declaring_class.name).normalized,
                self.class_name_handles(&called_class).display,
                self.class_name_handles(&declaring_class.name).normalized,
            )
            .with_optional_call_span(call_span);
        if let Some(bound_this) = bound_this_for_scoped_call {
            call = call.with_this(bound_this);
        }
        let call = if allow_by_ref_value_warnings {
            call.with_by_ref_value_warnings()
        } else {
            call
        }
        .with_optional_by_ref_warning_callable_name(by_ref_warning_callable_name);
        self.execute_function(
            &class_owner,
            method_entry.function,
            call,
            output,
            stack,
            state,
        )
    }
}
impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
