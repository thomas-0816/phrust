//! First minimal VM dispatch loop.

#![allow(clippy::result_large_err)]
#![allow(clippy::too_many_arguments)]

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
mod builtins;
mod callable_builtins;
mod calls;
mod class_context;
mod class_operations;
mod class_relations;
mod class_validation;
mod closure_operations;
mod dense_dispatch;
mod dense_method_dispatch;
mod diagnostics;
mod dispatch_contract;
mod exception_dispatch;
mod execution_control;
mod execution_state;
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
mod rich_dispatch;
mod runtime_class_metadata;
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

use arguments::{
    TypecheckFastPathContext, call_args_from_owned_php_array, call_args_from_php_array,
    call_args_to_positional, call_argument_reference_cell, check_property_type,
    coerce_or_check_param_type, coerce_return_value, ir_runtime_type, param_is_sensitive,
    sensitive_parameter_value, trace_value_for_param, type_error_value_name,
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
use diagnostics::*;
use dispatch_contract::{
    DenseExecutionRequest, RichBinaryError, RichBinaryRequest, RichCompareRequest,
    RichUnaryRequest, dense_bytecode_unsupported_reason, dense_opcode_family,
    next_dense_block_index,
};
use exception_dispatch::*;
use execution_control::{
    ExceptionHandler, ExecutionLimitExceeded, PendingControl, RaiseOutcome,
    execution_limit_exceeded, next_block_id,
};
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
use ext_redis::*;
use generator_fiber::{
    FiberContinuation, FiberResumeInput, FiberSuspension, GeneratorContinuation,
    GeneratorResumeInput, GeneratorYield, YieldFromDelegation, YieldFromKey, YieldFromStep,
    new_fiber_object,
};
use include_execution::{include_failure_allows_continuation, include_vm_error};
use instrumentation::*;
use iteration::{
    ForeachInvalidSourceBehavior, ForeachIterator, foreach_array_keys_from_local_at_frame,
    foreach_iterator_candidate_value, format_foreach_iterator_kind,
    object_property_iteration_entries,
};
use jit_state::*;
use method_cache_metadata::*;
pub use options::{
    BytecodeLayoutMode, DenseIncludeMode, DenseJumpThreadingMode, ExecutionFormat,
    JitBlacklistMode, JitMode, SuperinstructionMode, VmOptions,
};
use property_cache_metadata::*;
use property_resolution::*;
use property_state::*;
use reflection::*;
use request_lifecycle::RequestLifecycleState;
pub use result::VmStepLimitDiagnostic;
pub use result::{VmControlFlow, VmResult};
pub(crate) use runtime_class_metadata::dense_new_object_lowering_supported;
use runtime_class_metadata::*;
use runtime_operations::{object_has_public_to_string_in_state, packed_array_get};
use scalar_handlers::{
    checked_int_binary, execute_arithmetic, execute_bitwise, execute_power, execute_rich_binary_op,
    execute_rich_compare_op, execute_rich_unary_op, int_int_specialization_for_binary_op,
};
use spl::*;
use static_property_predicates::*;
use symbol_resolution::*;

use crate::aliasing::{AliasState, slot_alias_state};
use crate::bytecode::{
    DenseBytecodeUnit, DenseCallArg, DenseCallShapeMeta, DenseCallableKind, DenseClosureCapture,
    DenseExecutionPlan, DenseFunction, DenseFunctionPlan, DenseInstruction, DenseOpcode,
    DenseOperand, DenseOperandKind, DenseOperands, SuperinstructionSelectionReport,
};
use crate::compiled_unit::{
    CompiledUnit, DenseExecutionArtifactKey, DenseExecutionArtifactMode,
    PreparedClassValidationError, PreparedFunctionFacts,
};
#[cfg(feature = "jit-cranelift")]
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
#[cfg(all(feature = "jit-copy-patch", unix, target_arch = "aarch64"))]
pub(crate) use jit_abi::jit_property_load_fetch;
#[cfg(all(feature = "jit-copy-patch", unix, target_arch = "aarch64"))]
pub(crate) use jit_abi::jit_property_store_commit;
#[cfg(feature = "jit-cranelift")]
use jit_abi::{
    JIT_PROPERTY_LOAD_STATUS_CLASS_EXIT, JIT_PROPERTY_LOAD_STATUS_LAYOUT_EXIT,
    JIT_PROPERTY_LOAD_STATUS_STORAGE_EXIT, JIT_PROPERTY_LOAD_STATUS_UNINITIALIZED_EXIT,
    jit_array_fetch_int_slow_abi, jit_array_len_abi, jit_concat_string_string_fast,
    jit_count_known_abi, jit_guard_kind_for_side_exit, jit_property_load_monomorphic_fast,
    jit_record_array_lookup_abi, jit_strlen_known_abi,
};
use operand_read::operand_truthy_at_frame;
use operand_read::take_discard_operand_at_frame;
use operand_read::{
    DenseOperandRead, read_operand, read_operand_at_frame, read_operand_ref_at_frame,
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
use php_runtime::IniRegistry;
use php_runtime::ResourceTable;
use php_runtime::debug::{GcEntityId, GcEntityKind, GcRoot, GcRootKind, GcSnapshot, scan_roots};
use php_runtime::numeric_string::{NumericStringKind, NumericStringValue, classify_php_string};
use php_runtime::{
    ArrayKey, AttributeEntry as RuntimeAttributeEntry, AutoloadRegistry, BuiltinContext,
    BuiltinEntry, CallableMethodTarget, CallableValue,
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
const NORMALIZER_FORM_C: i64 = php_runtime::builtins::NORMALIZER_FORM_C;
#[cfg(feature = "jit-cranelift")]
const JIT_BLACKLIST_SIDE_EXIT_THRESHOLD: u64 = 2;
#[cfg(feature = "jit-cranelift")]
const JIT_BLACKLIST_GUARD_FAILURE_THRESHOLD: u64 = 2;
#[cfg(feature = "jit-cranelift")]
const JIT_BLACKLIST_COMPILE_ERROR_THRESHOLD: u64 = 1;
#[cfg(feature = "jit-cranelift")]
const JIT_BLACKLIST_ABI_MISMATCH_THRESHOLD: u64 = 1;
fn output_preallocation_hint(unit: &IrUnit) -> usize {
    unit.functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction.kind {
            InstructionKind::Echo {
                src: Operand::Constant(id),
            } => unit.constants.get(id.index()),
            _ => None,
        })
        .filter_map(|constant| match constant {
            IrConstant::String(value) => Some(value.len()),
            IrConstant::StringBytes(value) => Some(value.len()),
            _ => None,
        })
        .sum()
}

fn ir_unit_instruction_count(unit: &IrUnit) -> u32 {
    unit.functions
        .iter()
        .flat_map(|function| function.blocks.iter())
        .map(|block| block.instructions.len() as u32 + u32::from(block.terminator.is_some()))
        .sum()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AutoloadTraceOrigin {
    function_name: &'static str,
    span: php_ir::IrSpan,
}

enum PropertyFetchCacheRead {
    Value(Value),
    Fallback,
}

#[cfg(feature = "jit-cranelift")]
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

/// Per-worker symbol-replay ledger: units whose declarations this thread
/// has already observed, plus the worker-monotonic lookup epoch. Identical
/// include replay keeps the epoch constant so slot-indexed inline caches
/// with request-stable targets survive the request boundary.
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

/// Minimal interpreter VM.
#[derive(Clone, Debug)]
pub struct Vm {
    options: VmOptions,
    trace: RefCell<Vec<String>>,
    counters: RefCell<Option<VmCounters>>,
    literal_pool: RefCell<LiteralPool>,
    quickening: RefCell<QuickeningTable>,
    /// Final invalidation epochs of the last `execute` call, stashed before
    /// request state drops so the persistent-feedback writer can stamp entries
    /// with the true observation state instead of cold-start zeros.
    persistent_feedback_epochs: Cell<Option<crate::persistent_feedback::PersistentFeedbackEpochs>>,
    /// IC-table unit key (`compiled_unit_cache_key`) of the last executed
    /// entry unit, for scoping persistent callsite exports to replay-stable
    /// (entry-unit) IC sites.
    persistent_feedback_entry_unit_key: Cell<Option<u64>>,
    inline_caches: RefCell<InlineCacheTable>,
    jit: RefCell<JitRuntimeState>,
    tiering: RefCell<TieringState>,
    internal_function_dispatch_cache: RefCell<InternalFunctionDispatchCache>,
    /// Memoized per-(unit, function) trivial-method inline plans.
    trivial_method_plans: RefCell<HashMap<(u64, u32), Option<TrivialMethodPlan>>>,
    /// Memoized activation-context class-name handles keyed by the exact name
    /// spelling dispatch sees. The normalized/display forms of a spelling never
    /// change, so hot method-call sites attach shared handles with refcount
    /// bumps instead of re-normalizing three fresh `String`s per call.
    class_name_handles: RefCell<HashMap<String, ClassNameHandles>>,
    /// Memoized resolved runtime class entries so repeated instantiations of a
    /// class do not rebuild the whole entry (lineage walk, property/constant
    /// evaluation, method mapping) on every `new`. Invalidated whenever the
    /// class table changes (tracked by `ExecutionState::class_table_epoch`).
    runtime_class_entry_cache: RefCell<RuntimeClassEntryCache>,
    /// Memoized raw IR class entries (shared via `Rc`) so repeated `new` of a
    /// class does not deep-clone the whole class definition per instantiation.
    /// Invalidated by `ExecutionState::class_table_epoch`.
    ir_class_entry_cache: RefCell<IrClassEntryCache>,
    /// Memoized default declared-slot templates so the hot `new C(...)` path
    /// clones a prebuilt slot vector instead of re-running the per-property
    /// default-materialization loop on every instantiation. Invalidated by
    /// `ExecutionState::class_table_epoch`, so a redefinition rebuilds it from
    /// the current class entry.
    default_slot_template_cache: RefCell<DefaultSlotTemplateCache>,
    /// Memoized `__construct` resolution outcomes so the hot `new C(...)` path
    /// does not re-run the inheritance + visibility method-resolution walk on
    /// every instantiation. Keyed by (normalized class name, normalized caller
    /// scope) and guarded by `ExecutionState::class_table_epoch`, so a
    /// redeclaration or autoload (both bump the epoch) drops stale outcomes.
    constructor_resolution_cache: RefCell<ConstructorResolutionCache>,
    adaptive_tiny_unit_setup_skipped: Cell<bool>,
    include_execution_depth: Cell<u32>,
    request_profile_stack: RefCell<Vec<RequestProfileFrame>>,
    /// Memoized per-(unit, function) last-use move plans (Runtime lever R3).
    /// Built only when `options.last_use_moves` is on; empty and never consulted
    /// otherwise, keeping the default dense read path byte-identical.
    last_use_move_plans: RefCell<HashMap<(u64, u32), Rc<crate::last_use::LastUseMovePlan>>>,
    /// Per-unit resolved constant tables (zend literal-table parity): each
    /// materializable `IrConstant` resolves once into an interned value and
    /// every later operand read is an indexed refcount bump instead of a
    /// fresh allocation (strings) or a full rebuild (constant arrays).
    /// Keyed by the compiled unit's cache identity; unit constant tables
    /// are immutable per identity, so entries never invalidate.
    resolved_constants: RefCell<ResolvedConstantTables>,
}

/// Per-unit lazily-resolved constant values, with a one-entry hot-unit
/// cache in front of the map because consecutive reads overwhelmingly
/// come from the same unit.
#[derive(Clone, Debug, Default)]
struct ResolvedConstantTables {
    last: Option<(u64, Rc<[std::cell::OnceCell<Value>]>)>,
    tables: HashMap<u64, Rc<[std::cell::OnceCell<Value>]>>,
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
        let tiering = TieringState::new(options.tiering.clone());
        Self {
            options,
            trace: RefCell::new(Vec::new()),
            counters: RefCell::new(None),
            literal_pool: RefCell::new(LiteralPool::default()),
            trivial_method_plans: RefCell::new(HashMap::new()),
            class_name_handles: RefCell::new(HashMap::new()),
            runtime_class_entry_cache: RefCell::new(RuntimeClassEntryCache::default()),
            ir_class_entry_cache: RefCell::new(IrClassEntryCache::default()),
            default_slot_template_cache: RefCell::new(DefaultSlotTemplateCache::default()),
            constructor_resolution_cache: RefCell::new(ConstructorResolutionCache::default()),
            quickening: RefCell::new(QuickeningTable::default()),
            persistent_feedback_epochs: Cell::new(None),
            persistent_feedback_entry_unit_key: Cell::new(None),
            inline_caches: RefCell::new(InlineCacheTable::default()),
            jit: RefCell::new(JitRuntimeState::default()),
            tiering: RefCell::new(tiering),
            internal_function_dispatch_cache: RefCell::new(InternalFunctionDispatchCache::default()),
            adaptive_tiny_unit_setup_skipped: Cell::new(false),
            include_execution_depth: Cell::new(0),
            request_profile_stack: RefCell::new(Vec::new()),
            last_use_move_plans: RefCell::new(HashMap::new()),
            resolved_constants: RefCell::new(ResolvedConstantTables::default()),
        }
    }

    /// Executes a compiled unit from its entry function.
    #[must_use]
    pub fn execute(&self, unit: impl Into<CompiledUnit>) -> VmResult {
        let unit = unit.into();
        let skip_adaptive_tiny_unit_setup = self.should_skip_adaptive_tiny_unit_setup(unit.unit());
        self.adaptive_tiny_unit_setup_skipped
            .set(skip_adaptive_tiny_unit_setup);
        let mut output = OutputBuffer::with_capacity(output_preallocation_hint(unit.unit()));
        self.trace.borrow_mut().clear();
        *self.literal_pool.borrow_mut() = LiteralPool::default();
        *self.resolved_constants.borrow_mut() = ResolvedConstantTables::default();
        self.trivial_method_plans.borrow_mut().clear();
        self.last_use_move_plans.borrow_mut().clear();
        *self.runtime_class_entry_cache.borrow_mut() = RuntimeClassEntryCache::default();
        *self.ir_class_entry_cache.borrow_mut() = IrClassEntryCache::default();
        *self.default_slot_template_cache.borrow_mut() = DefaultSlotTemplateCache::default();
        *self.constructor_resolution_cache.borrow_mut() = ConstructorResolutionCache::default();
        *self.quickening.borrow_mut() = QuickeningTable::default();
        self.persistent_feedback_epochs.set(None);
        // IC slots and the entry-unit scope filter share the compiled unit's
        // stable cache identity.
        self.persistent_feedback_entry_unit_key
            .set(Some(compiled_unit_cache_key(&unit)));
        let mut persistent_feedback_seeded_sites = 0usize;
        if self.options.quickening.enabled() && !self.options.quickening_seed.is_empty() {
            persistent_feedback_seeded_sites = self
                .quickening
                .borrow_mut()
                .seed_persistent_sites(&self.options.quickening_seed);
        }
        *self.inline_caches.borrow_mut() = InlineCacheTable::default();
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
        *self.jit.borrow_mut() = JitRuntimeState::default();
        *self.tiering.borrow_mut() = TieringState::new(self.options.tiering.clone());
        self.internal_function_dispatch_cache.borrow_mut().clear();
        self.include_execution_depth.set(0);
        *self.counters.borrow_mut() = self.options.collect_counters.then(|| {
            let mut counters = VmCounters::default();
            counters.set_jit_config(self.options.jit.as_str(), self.options.jit_threshold);
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
            counters
        });
        if self.options.collect_counters {
            php_runtime::numeric_string::reset_cache_and_stats();
            php_runtime::layout_stats::reset_layout_stats();
            if self.options.collect_layout_source_attribution {
                php_runtime::layout_stats::enable_layout_source_attribution();
            } else {
                php_runtime::layout_stats::disable_layout_source_attribution();
            }
        } else {
            php_runtime::layout_stats::disable_layout_source_attribution();
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
            default_timezone: php_runtime::datetime::DEFAULT_TIMEZONE.to_owned(),
            env,
            filter_input_arrays,
            network_requests_enabled,
            spl_autoload_extensions: ".inc,.php".to_owned(),
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
                    result = error;
                }
            }
        }
        if result.status.is_success() {
            match self.run_shutdown_user_stream_wrappers(&unit, &mut output, &mut state) {
                Ok(diagnostics) => {
                    result.diagnostics.extend(diagnostics);
                }
                Err(error) => {
                    result = error;
                }
            }
        }
        if result.status.is_success() {
            match self.run_shutdown_destructors(&unit, &mut output, &mut state) {
                Ok(diagnostics) => {
                    result.diagnostics.extend(diagnostics);
                }
                Err(error) => {
                    result = error;
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
            let stats = php_runtime::numeric_string::take_cache_stats();
            let layout_stats = php_runtime::layout_stats::take_layout_stats();
            let layout_source_stats = php_runtime::layout_stats::take_layout_source_stats();
            let (interned_names, interned_name_bytes) =
                php_runtime::string::symbol_interner_footprint();
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
        result.output = output;
        result
    }

    /// Exports adaptive quickening sites observed by the last `execute` call
    /// for persistent feedback. Empty when quickening was disabled.
    #[must_use]
    pub fn export_persistent_quickening(&self) -> Vec<crate::quickening::QuickeningSiteSnapshot> {
        self.quickening.borrow().export_persistent_sites()
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
        if php_runtime::phar::is_phar_uri(path) {
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
            php_runtime::RuntimeRequestMode::Http(request) => {
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

    #[cfg(feature = "jit-cranelift")]
    fn maybe_write_cranelift_clif_dump(&self, compiled: &CompiledUnit, function_id: FunctionId) {
        let Some(path) = self.options.jit_dump_clif.as_ref() else {
            return;
        };
        let Ok(result) = php_jit::lower_function_to_cranelift(compiled.unit(), function_id) else {
            return;
        };
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && fs::create_dir_all(parent).is_err()
        {
            return;
        }
        let _ = fs::write(path, result.clif);
    }

    #[cfg(feature = "jit-cranelift")]
    fn record_jit_compile_budget_spent(&self, compile_time_nanos: u64) {
        self.tiering
            .borrow_mut()
            .record_jit_compiled_function(compile_time_nanos);
    }

    #[cfg(feature = "jit-cranelift")]
    fn record_jit_compile_failure_for_key(&self, key: JitFunctionKey) {
        if !self.options.jit_blacklist.enabled() {
            return;
        }
        let (blacklist_reason, invalidations) = {
            let mut jit = self.jit.borrow_mut();
            let entry = jit.functions.entry(key).or_default();
            let reason = entry.record_compile_error();
            let invalidations = if reason.is_some() {
                jit.invalidate_compile_cache_for_function(key.function)
            } else {
                0
            };
            (reason, invalidations)
        };
        self.record_counter_jit_compile_cache_invalidations(invalidations);
        if let Some(reason) = blacklist_reason {
            self.record_counter_jit_blacklisted_region(reason);
        }
    }

    #[cfg(feature = "jit-cranelift")]
    fn record_jit_side_exit_for_key(&self, key: JitFunctionKey, side_exit: php_jit::JitSideExit) {
        self.record_counter_jit_side_exit(side_exit.reason);
        let region_id = side_exit
            .resume_instruction
            .map(|instruction| format!("bytecode-{}", instruction.raw()))
            .unwrap_or_else(|| format!("function-{}", key.function.raw()));
        self.tiering.borrow_mut().record_jit_side_exit(
            key.function,
            region_id,
            side_exit.reason.as_str(),
            jit_guard_kind_for_side_exit(side_exit.reason),
        );
        if side_exit.reason == php_jit::SideExitReason::GuardFailed {
            self.record_counter_jit_guard_failure();
        }
        if !self.options.jit_blacklist.enabled() {
            return;
        }
        let (blacklist_reason, invalidations) = {
            let mut jit = self.jit.borrow_mut();
            let entry = jit.functions.entry(key).or_default();
            let reason = entry.record_side_exit(side_exit.reason);
            let invalidations = if reason.is_some() {
                jit.invalidate_compile_cache_for_function(key.function)
            } else {
                0
            };
            (reason, invalidations)
        };
        self.record_counter_jit_compile_cache_invalidations(invalidations);
        if let Some(reason) = blacklist_reason {
            self.record_counter_jit_blacklisted_region(reason);
        }
    }

    fn record_inline_cache_site_event(
        &self,
        function_id: FunctionId,
        instruction_id: php_ir::ids::InstrId,
        observation: InlineCacheObservation,
    ) {
        self.record_counter_inline_cache_site(function_id, instruction_id.raw(), observation);
    }

    fn observe_inline_cache(
        &self,
        unit_key: u64,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        kind: &InstructionKind,
    ) {
        if !self.options.tiering.enabled || !self.options.inline_caches.enabled() {
            return;
        }
        let Some(cache_kind) = crate::inline_cache::inline_cache_kind_for_instruction(kind) else {
            return;
        };
        let observation = self.inline_caches.borrow_mut().observe_slot(
            unit_key,
            function_id,
            block_id,
            instruction_id,
            cache_kind,
        );
        self.record_counter_inline_cache_site(function_id, instruction_id.raw(), observation);
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "property IC installation needs the resolved property metadata and callsite guard context"
    )]
    fn maybe_install_property_fetch_inline_cache_target(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        property: &str,
        receiver_class: &str,
        receiver_entry: &ClassEntry,
        declaring_class: &ClassEntry,
        declaring_property: &ClassPropertyEntry,
        storage_name: &str,
        normalized_scope: Option<&str>,
        lookup_epoch: InvalidationEpoch,
        receiver_has_magic_get: bool,
        state: &ExecutionState,
        object: &ObjectRef,
        cache_id: Option<InlineCacheId>,
    ) {
        if !self.options.inline_caches.enabled()
            || declaring_property.flags.is_static
            || declaring_property.flags.is_protected
            || declaring_property.hooks.get.is_some()
            || declaring_property.hooks.set.is_some()
            || property_hook_is_active(state, object, declaring_class, declaring_property)
        {
            return;
        }
        let cache_scope = declaring_property
            .flags
            .is_private
            .then(|| normalize_class_name(&declaring_class.name));
        if declaring_property.flags.is_private && cache_scope.as_deref() != normalized_scope {
            return;
        }
        let layout = property_fetch_layout_metadata(
            receiver_entry,
            declaring_class,
            declaring_property,
            cache_scope.as_deref(),
            lookup_epoch,
            receiver_has_magic_get,
            false,
            false,
            true,
        );
        let target_payload = Arc::new(PropertyFetchResolvedTarget {
            receiver_class: receiver_class.to_owned(),
            declaring_class: declaring_class.name.clone(),
            property: declaring_property.name.clone(),
            storage_name: storage_name.to_owned(),
            layout,
            object_layout_epoch: object.class_layout_epoch(),
            declared_slot: object.declared_slot_index(storage_name),
        });
        let target = match dynamic_class_owner_index_in_state(state, &declaring_class.name) {
            Some(unit_index) => PropertyFetchCacheTarget::DynamicUnit {
                unit_index,
                target: target_payload,
            },
            None => PropertyFetchCacheTarget::CurrentUnit {
                target: target_payload,
            },
        };
        if let Some(id) = cache_id {
            self.inline_caches
                .borrow_mut()
                .install_property_fetch_by_id(
                    id,
                    property,
                    receiver_class,
                    cache_scope.as_deref(),
                    lookup_epoch,
                    target,
                );
        } else {
            self.install_property_fetch_inline_cache(
                compiled,
                function_id,
                block_id,
                instruction_id,
                property,
                receiver_class,
                cache_scope.as_deref(),
                lookup_epoch,
                target,
            );
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "property assignment IC installation needs resolved metadata and write guard context"
    )]
    fn maybe_install_property_assign_inline_cache_target(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        property: &str,
        receiver_class: &str,
        receiver_entry: &ClassEntry,
        declaring_class: &ClassEntry,
        declaring_property: &ClassPropertyEntry,
        storage_name: &str,
        normalized_scope: Option<&str>,
        lookup_epoch: InvalidationEpoch,
        receiver_has_magic_set: bool,
        state: &ExecutionState,
        object: &ObjectRef,
        cache_id: Option<InlineCacheId>,
    ) {
        if !self.options.inline_caches.enabled() {
            return;
        }
        if declaring_property.flags.is_static || declaring_property.flags.is_protected {
            return;
        }
        if receiver_has_magic_set {
            return;
        }
        if declaring_property.flags.is_readonly || declaring_class.flags.is_readonly {
            return;
        }
        if declaring_property.hooks.get.is_some()
            || declaring_property.hooks.set.is_some()
            || property_hook_is_active(state, object, declaring_class, declaring_property)
        {
            return;
        }
        let cache_scope = declaring_property
            .flags
            .is_private
            .then(|| normalize_class_name(&declaring_class.name));
        if declaring_property.flags.is_private && cache_scope.as_deref() != normalized_scope {
            return;
        }
        if declaring_property.flags.set_is_private || declaring_property.flags.set_is_protected {
            return;
        }
        if matches!(object.get_property(storage_name), Some(Value::Reference(_))) {
            return;
        }
        let layout = property_assign_layout_metadata(
            receiver_entry,
            declaring_class,
            declaring_property,
            cache_scope.as_deref(),
            lookup_epoch,
            receiver_has_magic_set,
            false,
            false,
            false,
        );
        let target_payload = Arc::new(PropertyAssignResolvedTarget {
            receiver_class: receiver_class.to_owned(),
            declaring_class: declaring_class.name.clone(),
            property: declaring_property.name.clone(),
            storage_name: storage_name.to_owned(),
            layout,
            object_layout_epoch: object.class_layout_epoch(),
            declared_slot: object.declared_slot_index(storage_name),
            // Typed properties still need the per-write type check, so they
            // stay on the generic re-validation path. Readonly, hooks,
            // asymmetric set visibility, and references were rejected above.
            slot_write_eligible: declaring_property.type_.is_none(),
        });
        let target = match dynamic_class_owner_index_in_state(state, &declaring_class.name) {
            Some(unit_index) => PropertyAssignCacheTarget::DynamicUnit {
                unit_index,
                target: target_payload,
            },
            None => PropertyAssignCacheTarget::CurrentUnit {
                target: target_payload,
            },
        };
        if let Some(id) = cache_id {
            self.inline_caches
                .borrow_mut()
                .install_property_assign_by_id(
                    id,
                    property,
                    receiver_class,
                    cache_scope.as_deref(),
                    lookup_epoch,
                    target,
                );
        } else {
            self.install_property_assign_inline_cache(
                compiled,
                function_id,
                block_id,
                instruction_id,
                property,
                receiver_class,
                cache_scope.as_deref(),
                lookup_epoch,
                target,
            );
        }
    }

    fn lookup_class_constant_static_property_inline_cache(
        &self,
        compiled: &CompiledUnit,
        cache_id: Option<InlineCacheId>,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        kind: ClassConstantStaticPropertyCacheKind,
        resolved_class: &str,
        member: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
    ) -> Option<ClassConstantStaticPropertyCacheTarget> {
        if !self.options.inline_caches.enabled() {
            return None;
        }
        let (target, observation) = if let Some(id) = cache_id {
            self.inline_caches
                .borrow_mut()
                .lookup_class_constant_static_property_by_id(
                    id,
                    kind,
                    resolved_class,
                    member,
                    scope,
                    epoch,
                )
        } else {
            self.inline_caches
                .borrow_mut()
                .lookup_class_constant_static_property(
                    compiled_unit_cache_key(compiled),
                    function_id,
                    block_id,
                    instruction_id,
                    kind,
                    resolved_class,
                    member,
                    scope,
                    epoch,
                )
        };
        self.record_inline_cache_site_event(function_id, instruction_id, observation);
        target
    }

    fn install_class_constant_static_property_inline_cache(
        &self,
        compiled: &CompiledUnit,
        cache_id: Option<InlineCacheId>,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        kind: ClassConstantStaticPropertyCacheKind,
        resolved_class: &str,
        member: &str,
        scope: Option<&str>,
        epoch: InvalidationEpoch,
        target: ClassConstantStaticPropertyCacheTarget,
    ) {
        if !self.options.inline_caches.enabled() {
            return;
        }
        if let Some(id) = cache_id {
            self.inline_caches
                .borrow_mut()
                .install_class_constant_static_property_by_id(
                    id,
                    kind,
                    resolved_class,
                    member,
                    scope,
                    epoch,
                    target,
                );
        } else {
            self.inline_caches
                .borrow_mut()
                .install_class_constant_static_property(
                    compiled_unit_cache_key(compiled),
                    function_id,
                    block_id,
                    instruction_id,
                    kind,
                    resolved_class,
                    member,
                    scope,
                    epoch,
                    target,
                );
        }
    }

    fn observe_quickening(
        &self,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        kind: &InstructionKind,
    ) {
        if !self.options.tiering.enabled
            || !self.options.quickening.enabled()
            || self.adaptive_tiny_unit_setup_skipped.get()
            || !rich_quickening_candidate_kind(kind)
        {
            return;
        }
        let observation =
            self.quickening
                .borrow_mut()
                .observe(function_id, block_id, instruction_id);
        self.record_counter_quickening_site(function_id, instruction_id.raw(), observation);
    }

    fn record_quickening_guard(
        &self,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        hit: bool,
    ) {
        if !self.options.tiering.enabled
            || !self.options.quickening.enabled()
            || self.adaptive_tiny_unit_setup_skipped.get()
        {
            return;
        }
        let observation = self.quickening.borrow_mut().record_specialized_guard(
            function_id,
            block_id,
            instruction_id,
            hit,
        );
        self.record_counter_quickening_site(function_id, instruction_id.raw(), observation);
    }

    fn observe_dense_quickening(
        &self,
        unit_id: UnitId,
        function_id: FunctionId,
        instruction_index: u32,
        opcode: DenseOpcode,
    ) {
        if !self.options.tiering.enabled
            || !self.options.quickening.enabled()
            || self.adaptive_tiny_unit_setup_skipped.get()
            || !dense_quickening_candidate_opcode(opcode)
        {
            return;
        }
        let observation =
            self.quickening
                .borrow_mut()
                .observe_dense(unit_id, function_id, instruction_index);
        self.record_counter_quickening_site(function_id, instruction_index, observation);
    }

    fn record_dense_quickening_guard(
        &self,
        unit_id: UnitId,
        function_id: FunctionId,
        instruction_index: u32,
        hit: bool,
    ) {
        if !self.options.tiering.enabled
            || !self.options.quickening.enabled()
            || self.adaptive_tiny_unit_setup_skipped.get()
        {
            return;
        }
        let observation = self.quickening.borrow_mut().record_dense_specialized_guard(
            unit_id,
            function_id,
            instruction_index,
            hit,
        );
        self.record_counter_quickening_site(function_id, instruction_index, observation);
    }

    /// Drops the transient shared array handle left in `register` by a
    /// dimension fetch whose block-local last use this was (Runtime lever R3).
    /// The plan already proved the register is dead after this fetch; only
    /// *shared* array handles are released, so dropping merely decrements the
    /// refcount — no contents are freed and no destructors run — while the
    /// array's owning local regains sole ownership and its next write mutates
    /// in place instead of copy-on-write-separating. Non-arrays, sole-owned
    /// arrays, and non-register operands are left untouched (byte-identical to
    /// the flag-off path).
    fn release_dead_shared_array_register(&self, stack: &mut CallStack, register: u32) {
        let Some(frame) = stack.current_mut() else {
            return;
        };
        let reg = RegId::new(register);
        let is_shared_array = matches!(
            frame.registers.get(reg),
            Some(Value::Array(array)) if array.is_shared()
        );
        if !is_shared_array {
            return;
        }
        if let Ok(value) = frame.registers.take(reg) {
            drop(value);
            self.record_counter_last_use_array_read_release();
        }
    }

    /// Returns the memoized last-use move plan for a dense function, building it
    /// on first use. Returns `None` when the R3 flag is off, so callers keep the
    /// unchanged clone path.
    fn last_use_move_plan(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        dense_function: &DenseFunction,
    ) -> Option<Rc<crate::last_use::LastUseMovePlan>> {
        if !self.options.last_use_moves {
            return None;
        }
        let key = (compiled_unit_cache_key(compiled), function_id.raw());
        if let Some(plan) = self.last_use_move_plans.borrow().get(&key) {
            return (!plan.is_empty()
                && (plan.move_checks_enabled() || plan.has_array_release_reads()))
            .then(|| Rc::clone(plan));
        }
        let plan = Rc::new(crate::last_use::LastUseMovePlan::analyze(dense_function));
        self.record_counter_last_use_move_ineligible(&plan);
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_last_use_plan_built(plan.eligible_reads(), plan.array_release_reads());
        }
        self.last_use_move_plans
            .borrow_mut()
            .insert(key, Rc::clone(&plan));
        (!plan.is_empty() && (plan.move_checks_enabled() || plan.has_array_release_reads()))
            .then_some(plan)
    }

    /// Reads a dense source operand, moving the value out of its register when
    /// the last-use plan marks this exact `(instruction, register)` read as a
    /// provably-safe last use. With `move_plan` `None` (R3 off) this is exactly
    /// `read_dense_operand` (a clone). Only register operands are ever moved;
    /// locals/constants take the clone path.
    fn read_dense_operand_last_use(
        &self,
        compiled: &CompiledUnit,
        stack: &mut CallStack,
        operand: DenseOperand,
        move_plan: Option<&crate::last_use::LastUseMovePlan>,
        dense_instruction_index: u32,
    ) -> Result<Value, String> {
        if let Some(plan) = move_plan
            && operand.kind == DenseOperandKind::Register
            && plan.move_checks_enabled()
        {
            if let Some(counters) = self.counters.borrow_mut().as_mut() {
                counters.record_last_use_move_consultation();
            }
            if plan.is_move_eligible(dense_instruction_index, operand.index) {
                let value = self.take_consumed_dense_operand(compiled, stack, operand)?;
                self.record_counter_last_use_move_applied(value_clone_is_heap(&value));
                return Ok(value);
            }
        }
        self.read_dense_operand(compiled, stack, operand)
    }

    /// Memoized: can this function's body observe its argument vector?
    fn frame_args_elidable(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        function: &IrFunction,
    ) -> bool {
        !prepared_function_facts(compiled, function_id, function).observes_argument_vector
    }

    /// Returns the memoized frame-shape flags for a callee, scanning its body
    /// only on the first call to each (unit, function). Subsequent calls reuse
    /// the cached result instead of re-scanning the whole body per invocation.
    fn frame_shape_flags(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        function: &IrFunction,
    ) -> FrameShapeFlags {
        let facts = prepared_function_facts(compiled, function_id, function);
        FrameShapeFlags {
            has_try_or_finally: facts.has_try_or_finally,
            may_hold_destructor_sensitive_value: facts.may_hold_destructor_sensitive_value,
            has_inline_blocker: facts.has_inline_blocker,
        }
    }

    /// Returns shared normalized/display handles for a class-name spelling,
    /// allocating them only on its first sighting. `with_class_context`
    /// re-derives both forms per call; the forms are pure functions of the
    /// spelling, so reusing the handles is behavior-neutral.
    fn class_name_handles(&self, name: &str) -> ClassNameHandles {
        if let Some(handles) = self.class_name_handles.borrow().get(name) {
            return handles.clone();
        }
        let handles = ClassNameHandles {
            normalized: Arc::from(normalize_class_name(name)),
            display: Arc::from(display_class_name(name)),
        };
        self.class_name_handles
            .borrow_mut()
            .insert(name.to_owned(), handles.clone());
        handles
    }

    /// Returns the resolved runtime class entry for a class, building it only on
    /// the first instantiation within a class-table epoch and reusing the shared
    /// `Rc` afterward. When the class table changes (new class declared or
    /// autoloaded, tracked by `class_table_epoch`), the cache is dropped so
    /// lineage/property/constant resolution is recomputed against the new table.
    fn cached_runtime_class_entry(
        &self,
        class_owner: &CompiledUnit,
        state: &ExecutionState,
        class: &php_ir::module::ClassEntry,
    ) -> Result<Rc<RuntimeClassEntry>, RuntimeClassEntryError> {
        let epoch = state.class_table_epoch;
        let key = normalize_class_name(&class.name);
        {
            let mut cache = self.runtime_class_entry_cache.borrow_mut();
            if cache.epoch != epoch {
                cache.entries.clear();
                cache.epoch = epoch;
            } else if let Some(entry) = cache.entries.get(&key) {
                return Ok(Rc::clone(entry));
            }
        }
        let entry = runtime_class_entry(
            class_owner,
            state,
            class,
            &|value| self.constant_value(class_owner.unit(), value),
            &|reference| class_constant_reference_value(class_owner, state, reference),
            &|reference| named_constant_reference_value(class_owner, state, reference),
        )?;
        let entry = Rc::new(entry);
        self.runtime_class_entry_cache
            .borrow_mut()
            .entries
            .insert(key, Rc::clone(&entry));
        Ok(entry)
    }

    /// Returns the raw IR class entry for a name, shared via `Rc`, cloning the
    /// (possibly large) class definition out of the class table only on the
    /// first `new` of each class within a class-table epoch. Subsequent
    /// instantiations reuse the shared `Rc` instead of re-resolving the name and
    /// deep-cloning the entry out of the `Arc` `lookup_class_in_state` returns.
    fn cached_class_entry(
        &self,
        compiled: &CompiledUnit,
        state: &ExecutionState,
        class_name: &str,
    ) -> Option<Rc<php_ir::module::ClassEntry>> {
        let epoch = state.class_table_epoch;
        let key = normalize_class_name(class_name);
        {
            let mut cache = self.ir_class_entry_cache.borrow_mut();
            if cache.epoch != epoch {
                cache.entries.clear();
                cache.epoch = epoch;
            } else if let Some(entry) = cache.entries.get(&key) {
                return Some(Rc::clone(entry));
            }
        }
        let entry = Rc::new((*lookup_class_in_state(compiled, state, class_name)?).clone());
        self.ir_class_entry_cache
            .borrow_mut()
            .entries
            .insert(key, Rc::clone(&entry));
        Some(entry)
    }

    /// Returns the memoized default declared-slot template for a resolved
    /// runtime class, building it once per (class identity, class-table epoch)
    /// and reusing the shared `Rc` afterward. The hot `new C(...)` path clones
    /// the template into a fresh instance (see `ObjectRef::from_layout_slots`)
    /// instead of re-running the per-property default-materialization loop.
    ///
    /// The template is byte-identical to the `declared_slots`
    /// `ObjectRef::new_with_display_name` builds for the same class shape, and
    /// is independent of `display_name` (which only selects the debug-label
    /// layout variant). Keying by the class-table epoch means a redefinition
    /// (which bumps the epoch) rebuilds the template from the current entry, so
    /// stale defaults can never leak across a redeclaration.
    fn cached_default_slot_template(
        &self,
        state: &ExecutionState,
        runtime_class: &RuntimeClassEntry,
        display_name: &str,
    ) -> Rc<Vec<Option<Value>>> {
        let epoch = state.class_table_epoch;
        let key = normalize_class_name(&runtime_class.name);
        {
            let mut cache = self.default_slot_template_cache.borrow_mut();
            if cache.epoch != epoch {
                cache.entries.clear();
                cache.epoch = epoch;
            } else if let Some(template) = cache.entries.get(&key) {
                return Rc::clone(template);
            }
        }
        let template = Rc::new(ObjectRef::default_declared_slots(
            runtime_class,
            display_name,
        ));
        self.default_slot_template_cache
            .borrow_mut()
            .entries
            .insert(key, Rc::clone(&template));
        template
    }

    /// Returns the resolved `__construct` for a class as seen from `caller_scope`,
    /// running the inheritance + visibility method-resolution walk only on the
    /// first `new` of each (class, caller scope) pair within a class-table epoch
    /// and reusing the memoized outcome afterward.
    ///
    /// The outcome is exactly what `lookup_resolved_method_in_state` returns for
    /// `"__construct"` — `Ok(Some(resolved))`, `Ok(None)` (no constructor → default
    /// construction), or `Err(message)` (e.g. an inheritance-cycle diagnostic) — so
    /// a cache hit reproduces the same result byte-for-byte, including errors. The
    /// caller scope is part of the key because private/protected resolution depends
    /// on it, and it is normalized (as `lookup_resolved_method_in_state` compares
    /// scopes case-insensitively) so equivalent scopes share one entry. When the
    /// class table changes (redeclaration or autoload, both bump
    /// `class_table_epoch`), the cache is dropped so resolution is recomputed
    /// against the new table.
    ///
    /// Downstream visibility enforcement (`validate_constructor_callable_in_state_scope`,
    /// abstract-class instantiation checks) runs on the returned `ResolvedMethodOwned`
    /// exactly as before and is not memoized here.
    fn cached_constructor_resolution(
        &self,
        compiled: &CompiledUnit,
        state: &ExecutionState,
        class_name: &str,
        caller_scope: Option<&str>,
    ) -> Result<Option<ResolvedMethodOwned>, String> {
        let epoch = state.class_table_epoch;
        let key = (
            normalize_class_name(class_name),
            caller_scope.map(normalize_class_name),
        );
        {
            let mut cache = self.constructor_resolution_cache.borrow_mut();
            if cache.epoch != epoch {
                cache.entries.clear();
                cache.epoch = epoch;
            } else if let Some(outcome) = cache.entries.get(&key) {
                return outcome.clone();
            }
        }
        let outcome = lookup_resolved_method_in_state(
            compiled,
            state,
            class_name,
            "__construct",
            caller_scope,
        );
        self.constructor_resolution_cache
            .borrow_mut()
            .entries
            .insert(key, outcome.clone());
        outcome
    }

    fn record_quickened_concat_guard(
        &self,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        hit: bool,
    ) {
        self.record_quickening_guard(function_id, block_id, instruction_id, hit);
        self.record_counter_string_concat_fast_path(hit);
    }

    fn record_quickened_dense_concat_guard(
        &self,
        unit_id: UnitId,
        function_id: FunctionId,
        instruction_index: u32,
        hit: bool,
    ) {
        self.record_dense_quickening_guard(unit_id, function_id, instruction_index, hit);
        self.record_counter_string_concat_fast_path(hit);
    }

    fn try_array_shape_lookup(&self, array: &PhpArray, key: &ArrayKey) -> Option<Option<Value>> {
        let metadata = array.shape_metadata();
        self.record_counter_array_shape_observed(array);
        if metadata.numeric_string_key_ambiguity {
            self.record_counter_array_shape_lookup_fallback(
                PhpArrayShapeLookupFallback::KeyCoercion,
            );
            return None;
        }
        match metadata.kind {
            PhpArrayShapeKind::InternedStringKeyRecord
            | PhpArrayShapeKind::ShapeStableRecordLike => {
                let lookup = array.record_shape_string_key_lookup(key);
                self.record_counter_record_shape_lookup(&lookup);
                match lookup {
                    PhpArrayShapeLookup::Hit(value) => Some(Some(effective_value(value))),
                    PhpArrayShapeLookup::Miss => Some(None),
                    PhpArrayShapeLookup::Fallback(_) => None,
                }
            }
            PhpArrayShapeKind::SmallInlineMap => {
                let lookup = array.small_map_lookup(key);
                self.record_counter_small_map_lookup(&lookup);
                match lookup {
                    PhpArrayShapeLookup::Hit(value) => Some(Some(effective_value(value))),
                    PhpArrayShapeLookup::Miss => Some(None),
                    PhpArrayShapeLookup::Fallback(_) => None,
                }
            }
            PhpArrayShapeKind::CowOrReferenceFallback => {
                self.record_counter_array_shape_lookup_fallback(
                    PhpArrayShapeLookupFallback::CowOrReference,
                );
                None
            }
            PhpArrayShapeKind::PackedWithHoles | PhpArrayShapeKind::MixedHash => {
                self.record_counter_array_shape_lookup_fallback(
                    PhpArrayShapeLookupFallback::OrderSemantics,
                );
                None
            }
            PhpArrayShapeKind::Empty
            | PhpArrayShapeKind::Packed
            | PhpArrayShapeKind::SharedImmutableLiteralArray => None,
        }
    }

    fn record_array_count_fast_path_if_applicable(&self, name: &str, values: &[Value]) {
        if name != "count" {
            return;
        }
        let Some(first) = values.first() else {
            return;
        };
        let recursive_mode = values
            .get(1)
            .is_some_and(|value| matches!(effective_value(value), Value::Int(1)));
        if recursive_mode {
            return;
        }
        if matches!(effective_value(first), Value::Array(_)) {
            self.record_counter_array_count_fast_path_hit();
        }
    }

    fn lookup_internal_function_dispatch(&self, name: &str) -> Option<BuiltinEntry> {
        if !self.options.internal_function_dispatch_cache {
            return BuiltinRegistry::new().get(name);
        }
        let (entry, outcome) = self
            .internal_function_dispatch_cache
            .borrow_mut()
            .lookup(name);
        match outcome {
            InternalFunctionDispatchCacheOutcome::Hit => {
                self.record_counter_internal_function_dispatch_cache(true);
            }
            InternalFunctionDispatchCacheOutcome::Miss => {
                self.record_counter_internal_function_dispatch_cache(false);
            }
            InternalFunctionDispatchCacheOutcome::Uncached => {}
        }
        entry
    }

    fn typecheck_fast_path_context(&self) -> TypecheckFastPathContext<'_> {
        TypecheckFastPathContext::new(
            self.options.typecheck_fast_paths,
            self.options.collect_counters.then_some(&self.counters),
        )
    }

    fn record_quickened_packed_dim_guard(
        &self,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        hit: bool,
    ) {
        self.record_quickening_guard(function_id, block_id, instruction_id, hit);
        self.record_counter_packed_dim_fast_path(hit);
    }

    fn try_quickened_int_int_binary(
        &self,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        op: BinaryOp,
        lhs: &Value,
        rhs: &Value,
    ) -> Option<Value> {
        if !self.options.quickening.enabled() {
            return None;
        }
        let expected = int_int_specialization_for_binary_op(op)?;

        let specialization = {
            self.quickening
                .borrow()
                .specialization(function_id, block_id, instruction_id)
        };

        match specialization {
            Some(specialization) if specialization == expected => match (lhs, rhs) {
                (Value::Int(lhs), Value::Int(rhs)) => {
                    if let Some(value) = checked_int_binary(op, *lhs, *rhs) {
                        self.record_quickening_guard(function_id, block_id, instruction_id, true);
                        Some(Value::Int(value))
                    } else {
                        self.record_quickening_guard(function_id, block_id, instruction_id, false);
                        None
                    }
                }
                _ => {
                    self.record_quickening_guard(function_id, block_id, instruction_id, false);
                    None
                }
            },
            None => {
                if matches!(lhs, Value::Int(_)) && matches!(rhs, Value::Int(_)) {
                    let observation = match op {
                        BinaryOp::Add => self
                            .quickening
                            .borrow_mut()
                            .observe_add_int_int_candidate(function_id, block_id, instruction_id),
                        BinaryOp::Sub => self
                            .quickening
                            .borrow_mut()
                            .observe_sub_int_int_candidate(function_id, block_id, instruction_id),
                        BinaryOp::Mul => self
                            .quickening
                            .borrow_mut()
                            .observe_mul_int_int_candidate(function_id, block_id, instruction_id),
                        _ => return None,
                    };
                    self.record_counter_quickening_site(
                        function_id,
                        instruction_id.raw(),
                        observation,
                    );
                }
                None
            }
            Some(_) => None,
        }
    }

    fn try_quickened_concat_string_string(
        &self,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        lhs: &Value,
        rhs: &Value,
    ) -> Option<Value> {
        if !self.options.quickening.enabled() {
            return None;
        }

        let specialization = {
            self.quickening
                .borrow()
                .specialization(function_id, block_id, instruction_id)
        };

        match specialization {
            Some(QuickeningSpecialization::ConcatStringString) => match (lhs, rhs) {
                (Value::String(lhs), Value::String(rhs)) => {
                    if lhs.len().checked_add(rhs.len()).is_none() {
                        self.record_quickened_concat_guard(
                            function_id,
                            block_id,
                            instruction_id,
                            false,
                        );
                        return None;
                    }
                    self.record_quickened_concat_guard(function_id, block_id, instruction_id, true);
                    self.record_counter_concat_prealloc_hit();
                    Some(Value::String(PhpString::from_parts(&[
                        lhs.as_bytes(),
                        rhs.as_bytes(),
                    ])))
                }
                _ => {
                    self.record_quickened_concat_guard(
                        function_id,
                        block_id,
                        instruction_id,
                        false,
                    );
                    None
                }
            },
            None => {
                if matches!(lhs, Value::String(_)) && matches!(rhs, Value::String(_)) {
                    let observation = self
                        .quickening
                        .borrow_mut()
                        .observe_concat_string_string_candidate(
                            function_id,
                            block_id,
                            instruction_id,
                        );
                    self.record_counter_quickening_site(
                        function_id,
                        instruction_id.raw(),
                        observation,
                    );
                }
                None
            }
            Some(
                QuickeningSpecialization::AddIntInt
                | QuickeningSpecialization::SubIntInt
                | QuickeningSpecialization::MulIntInt
                | QuickeningSpecialization::PackedArrayIntKey
                | QuickeningSpecialization::BoolBranchCondition,
            ) => None,
        }
    }

    fn try_quickened_packed_array_int_key(
        &self,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: php_ir::ids::InstrId,
        array: &Value,
        key: &Value,
    ) -> Option<Value> {
        if !self.options.quickening.enabled() {
            return None;
        }

        let specialization = {
            self.quickening
                .borrow()
                .specialization(function_id, block_id, instruction_id)
        };

        match specialization {
            Some(QuickeningSpecialization::PackedArrayIntKey) => match (array, key) {
                (Value::Array(array), Value::Int(index)) if *index >= 0 => {
                    let metadata = array.packed_metadata();
                    if metadata.contains_references {
                        self.record_counter_dequickened_by_reference(
                            AliasState::PropertyOrArrayDimReference,
                        );
                        self.record_quickened_packed_dim_guard(
                            function_id,
                            block_id,
                            instruction_id,
                            false,
                        );
                        self.record_counter_cow_or_reference_fallback();
                        return None;
                    }
                    if metadata.kind != PhpArrayKind::PackedList {
                        self.record_quickened_packed_dim_guard(
                            function_id,
                            block_id,
                            instruction_id,
                            false,
                        );
                        if metadata.numeric_string_key_ambiguity {
                            self.record_counter_array_fast_path_fallback("numeric_string_key");
                        } else {
                            self.record_counter_packed_fetch_layout_exit();
                        }
                        return None;
                    }
                    if let Some(value) = array.packed_element_fast(*index as usize) {
                        self.record_quickened_packed_dim_guard(
                            function_id,
                            block_id,
                            instruction_id,
                            true,
                        );
                        self.record_counter_packed_fetch_fast_hit();
                        self.record_counter_array_packed_read_fast_path_hit();
                        Some(effective_value(value))
                    } else {
                        self.record_quickened_packed_dim_guard(
                            function_id,
                            block_id,
                            instruction_id,
                            false,
                        );
                        self.record_counter_packed_fetch_bounds_exit();
                        None
                    }
                }
                (Value::Array(array), _) => {
                    self.record_quickened_packed_dim_guard(
                        function_id,
                        block_id,
                        instruction_id,
                        false,
                    );
                    let metadata = array.packed_metadata();
                    if metadata.contains_references {
                        self.record_counter_dequickened_by_reference(
                            AliasState::PropertyOrArrayDimReference,
                        );
                        self.record_counter_cow_or_reference_fallback();
                    } else if metadata.numeric_string_key_ambiguity
                        || value_is_numeric_string_key_ambiguity(key)
                    {
                        self.record_counter_array_fast_path_fallback("numeric_string_key");
                    } else {
                        self.record_counter_packed_fetch_layout_exit();
                    }
                    None
                }
                _ => {
                    self.record_quickened_packed_dim_guard(
                        function_id,
                        block_id,
                        instruction_id,
                        false,
                    );
                    self.record_counter_packed_fetch_layout_exit();
                    None
                }
            },
            None => {
                if let (Value::Array(array), Value::Int(index)) = (array, key)
                    && *index >= 0
                    && !array.contains_references_fast()
                    && array.packed_element_fast(*index as usize).is_some()
                {
                    let observation = self
                        .quickening
                        .borrow_mut()
                        .observe_packed_array_int_key_candidate(
                            function_id,
                            block_id,
                            instruction_id,
                        );
                    self.record_counter_quickening_site(
                        function_id,
                        instruction_id.raw(),
                        observation,
                    );
                }
                None
            }
            Some(
                QuickeningSpecialization::AddIntInt
                | QuickeningSpecialization::SubIntInt
                | QuickeningSpecialization::MulIntInt
                | QuickeningSpecialization::ConcatStringString
                | QuickeningSpecialization::BoolBranchCondition,
            ) => None,
        }
    }

    fn try_quickened_dense_int_int_binary(
        &self,
        unit_id: UnitId,
        function_id: FunctionId,
        instruction_index: u32,
        op: BinaryOp,
        lhs: &Value,
        rhs: &Value,
    ) -> Option<Value> {
        if !self.options.quickening.enabled() {
            return None;
        }
        let expected = int_int_specialization_for_binary_op(op)?;
        let specialization = {
            self.quickening
                .borrow()
                .dense_specialization(unit_id, function_id, instruction_index)
        };

        match specialization {
            Some(specialization) if specialization == expected => match (lhs, rhs) {
                (Value::Int(lhs), Value::Int(rhs)) => {
                    if let Some(value) = checked_int_binary(op, *lhs, *rhs) {
                        self.record_dense_quickening_guard(
                            unit_id,
                            function_id,
                            instruction_index,
                            true,
                        );
                        Some(Value::Int(value))
                    } else {
                        self.record_dense_quickening_guard(
                            unit_id,
                            function_id,
                            instruction_index,
                            false,
                        );
                        None
                    }
                }
                _ => {
                    self.record_dense_quickening_guard(
                        unit_id,
                        function_id,
                        instruction_index,
                        false,
                    );
                    None
                }
            },
            None => {
                if matches!(lhs, Value::Int(_)) && matches!(rhs, Value::Int(_)) {
                    let observation = self
                        .quickening
                        .borrow_mut()
                        .observe_dense_int_int_candidate(
                            unit_id,
                            function_id,
                            instruction_index,
                            expected,
                        );
                    self.record_counter_quickening_site(
                        function_id,
                        instruction_index,
                        observation,
                    );
                }
                None
            }
            Some(_) => None,
        }
    }

    fn try_quickened_dense_concat_string_string(
        &self,
        unit_id: UnitId,
        function_id: FunctionId,
        instruction_index: u32,
        lhs: &Value,
        rhs: &Value,
    ) -> Option<Value> {
        if !self.options.quickening.enabled() {
            return None;
        }
        let specialization = {
            self.quickening
                .borrow()
                .dense_specialization(unit_id, function_id, instruction_index)
        };

        match specialization {
            Some(QuickeningSpecialization::ConcatStringString) => match (lhs, rhs) {
                (Value::String(lhs), Value::String(rhs)) => {
                    if lhs.len().checked_add(rhs.len()).is_none() {
                        self.record_quickened_dense_concat_guard(
                            unit_id,
                            function_id,
                            instruction_index,
                            false,
                        );
                        return None;
                    }
                    self.record_quickened_dense_concat_guard(
                        unit_id,
                        function_id,
                        instruction_index,
                        true,
                    );
                    self.record_counter_concat_prealloc_hit();
                    Some(Value::String(PhpString::from_parts(&[
                        lhs.as_bytes(),
                        rhs.as_bytes(),
                    ])))
                }
                _ => {
                    self.record_quickened_dense_concat_guard(
                        unit_id,
                        function_id,
                        instruction_index,
                        false,
                    );
                    None
                }
            },
            None => {
                if matches!(lhs, Value::String(_)) && matches!(rhs, Value::String(_)) {
                    let observation = self
                        .quickening
                        .borrow_mut()
                        .observe_dense_concat_string_string_candidate(
                            unit_id,
                            function_id,
                            instruction_index,
                        );
                    self.record_counter_quickening_site(
                        function_id,
                        instruction_index,
                        observation,
                    );
                }
                None
            }
            Some(_) => None,
        }
    }

    fn try_quickened_dense_bool_branch(
        &self,
        unit_id: UnitId,
        function_id: FunctionId,
        instruction_index: u32,
        value: &Value,
    ) -> Option<bool> {
        if !self.options.quickening.enabled() {
            return None;
        }
        let specialization = {
            self.quickening
                .borrow()
                .dense_specialization(unit_id, function_id, instruction_index)
        };

        match specialization {
            Some(QuickeningSpecialization::BoolBranchCondition) => match value {
                Value::Bool(value) => {
                    self.record_dense_quickening_guard(
                        unit_id,
                        function_id,
                        instruction_index,
                        true,
                    );
                    Some(*value)
                }
                _ => {
                    self.record_dense_quickening_guard(
                        unit_id,
                        function_id,
                        instruction_index,
                        false,
                    );
                    None
                }
            },
            None => {
                if matches!(value, Value::Bool(_)) {
                    let observation = self
                        .quickening
                        .borrow_mut()
                        .observe_dense_bool_branch_candidate(
                            unit_id,
                            function_id,
                            instruction_index,
                        );
                    self.record_counter_quickening_site(
                        function_id,
                        instruction_index,
                        observation,
                    );
                }
                None
            }
            Some(_) => None,
        }
    }

    fn dense_branch_truthy_from_value(
        &self,
        unit_id: UnitId,
        function_id: FunctionId,
        instruction_index: u32,
        value: &Value,
    ) -> Result<bool, String> {
        if let Some(value) =
            self.try_quickened_dense_bool_branch(unit_id, function_id, instruction_index, value)
        {
            Ok(value)
        } else {
            to_bool(value)
        }
    }

    fn read_dense_operand_branch_truthy(
        &self,
        compiled: &CompiledUnit,
        stack: &CallStack,
        operand: DenseOperand,
        unit_id: UnitId,
        function_id: FunctionId,
        instruction_index: u32,
    ) -> Result<bool, String> {
        match operand.kind {
            DenseOperandKind::Register => {
                let frame = stack.current().ok_or("no active frame")?;
                let Some(value) = frame.registers.get(RegId::new(operand.index)) else {
                    return Err(format!("invalid register r{}", operand.index));
                };
                if value.is_uninitialized() {
                    return Err(format!("read uninitialized register r{}", operand.index));
                }
                self.dense_branch_truthy_from_value(unit_id, function_id, instruction_index, value)
            }
            DenseOperandKind::Local => {
                let frame = stack.current().ok_or("no active frame")?;
                let Some(slot) = frame.locals.get_slot(LocalId::new(operand.index)) else {
                    return Err(format!("invalid local local:{}", operand.index));
                };
                match slot {
                    Slot::Value(value) if value.is_uninitialized() => self
                        .dense_branch_truthy_from_value(
                            unit_id,
                            function_id,
                            instruction_index,
                            &Value::Null,
                        ),
                    Slot::Value(value) => self.dense_branch_truthy_from_value(
                        unit_id,
                        function_id,
                        instruction_index,
                        value,
                    ),
                    Slot::Reference(cell) => {
                        let value = cell.borrow();
                        self.dense_branch_truthy_from_value(
                            unit_id,
                            function_id,
                            instruction_index,
                            &value,
                        )
                    }
                }
            }
            DenseOperandKind::Constant => {
                let value = self.cached_constant_value(compiled, ConstId::new(operand.index))?;
                self.dense_branch_truthy_from_value(unit_id, function_id, instruction_index, &value)
            }
        }
    }

    fn intern_bytes(&self, bytes: &[u8]) -> PhpString {
        let interned = self.literal_pool.borrow_mut().intern_bytes(bytes);
        let hit = interned.hit;
        let value = interned.value;
        self.record_counter_literal_intern(hit);
        value
    }

    fn intern_str(&self, value: &str) -> PhpString {
        self.intern_bytes(value.as_bytes())
    }

    fn warm_literal_pool(&self, unit: &IrUnit) {
        for constant in &unit.constants {
            match constant {
                IrConstant::String(value) => {
                    self.intern_str(value);
                }
                IrConstant::StringBytes(value) => {
                    self.intern_bytes(value);
                }
                _ => {}
            }
        }
        for function in &unit.functions {
            self.intern_str(&function.name);
            for local in &function.locals {
                self.intern_str(local);
            }
            for param in &function.params {
                self.intern_str(&param.name);
            }
            for capture in &function.captures {
                self.intern_str(&capture.name);
            }
        }
        for entry in &unit.function_table {
            self.intern_str(&entry.name);
        }
        for entry in &unit.constant_table {
            self.intern_str(&entry.name);
        }
        for class in &unit.classes {
            self.intern_str(&class.name);
            self.intern_str(&class.display_name);
            if let Some(parent) = &class.parent {
                self.intern_str(parent);
            }
            for interface in &class.interfaces {
                self.intern_str(interface);
            }
            for method in &class.methods {
                self.intern_str(&method.name);
                self.intern_str(&method.origin_class);
                for attribute in &method.attributes {
                    self.intern_attribute(attribute);
                }
            }
            for property in &class.properties {
                self.intern_str(&property.name);
                for attribute in &property.attributes {
                    self.intern_attribute(attribute);
                }
            }
            for constant in &class.constants {
                self.intern_str(&constant.name);
                for attribute in &constant.attributes {
                    self.intern_attribute(attribute);
                }
            }
            for case in &class.enum_cases {
                self.intern_str(&case.name);
                for attribute in &case.attributes {
                    self.intern_attribute(attribute);
                }
            }
            for attribute in &class.attributes {
                self.intern_attribute(attribute);
            }
        }
    }

    fn intern_attribute(&self, attribute: &php_ir::module::AttributeEntry) {
        self.intern_str(&attribute.name);
        if let Some(name) = &attribute.resolved_name {
            self.intern_str(name);
        }
        if let Some(name) = &attribute.fallback_name {
            self.intern_str(name);
        }
    }

    /// Resolves a materializable unit constant through the per-unit
    /// resolved-constant table: the first read interns/builds the value,
    /// every later read is an indexed refcount bump. Returns `None` for
    /// out-of-range ids and for constants that need per-read runtime
    /// resolution (named/class constants), so callers keep their exact
    /// existing error/fallback behavior for those.
    ///
    /// Sharing one value across reads is sound for the same reason the
    /// literal pool is: strings and arrays copy-on-write, so a mutation
    /// through any handle separates from the cached storage first.
    fn resolved_constant_value(&self, compiled: &CompiledUnit, constant: ConstId) -> Option<Value> {
        let key = compiled.cache_identity();
        // Hit path: one borrow, an indexed cell read, one value clone —
        // no IR-table touch and no `Rc` traffic.
        let mut tables = self.resolved_constants.borrow_mut();
        if !matches!(&tables.last, Some((last_key, _)) if *last_key == key) {
            let table = Rc::clone(tables.tables.entry(key).or_insert_with(|| {
                std::iter::repeat_with(std::cell::OnceCell::new)
                    .take(compiled.unit().constants.len())
                    .collect()
            }));
            tables.last = Some((key, table));
        }
        let (_, table) = tables.last.as_ref()?;
        let cell = table.get(constant.index())?;
        if let Some(value) = cell.get() {
            return Some(value.clone());
        }
        let table = Rc::clone(table);
        drop(tables);
        // Miss path (first read of this id): named/class constants keep
        // their per-read runtime resolution and never populate the cell.
        let ir_constant = compiled.unit().constants.get(constant.index())?;
        if matches!(
            ir_constant,
            IrConstant::NamedConstant(_) | IrConstant::ClassConstant { .. }
        ) {
            return None;
        }
        let value = self.inline_constant_value(ir_constant);
        let _ = table.get(constant.index())?.set(value.clone());
        Some(value)
    }

    /// Table-backed variant of [`Self::constant_value`] for hot dense
    /// sites: falls through to the interning path (and its exact error
    /// and null-mapping behavior) whenever the table declines the id.
    pub(super) fn cached_constant_value(
        &self,
        compiled: &CompiledUnit,
        constant: ConstId,
    ) -> Result<Value, String> {
        if let Some(value) = self.resolved_constant_value(compiled, constant) {
            return Ok(value);
        }
        self.constant_value(compiled.unit(), constant)
    }

    fn constant_value(&self, unit: &IrUnit, constant: ConstId) -> Result<Value, String> {
        let Some(value) = unit.constants.get(constant.index()) else {
            return Err(format!(
                "invalid constant const:{} for unit {} with {} constants",
                constant.raw(),
                unit.files
                    .first()
                    .map_or("<unknown>", |file| file.path.as_str()),
                unit.constants.len()
            ));
        };
        Ok(self.inline_constant_value(value))
    }

    fn inline_constant_value(&self, constant: &IrConstant) -> Value {
        match constant {
            IrConstant::Null => Value::Null,
            IrConstant::Bool(value) => Value::Bool(*value),
            IrConstant::Int(value) => Value::Int(*value),
            IrConstant::Float(value) => Value::float(*value),
            IrConstant::String(value) => Value::String(self.intern_str(value)),
            IrConstant::StringBytes(value) => Value::String(self.intern_bytes(value)),
            IrConstant::NamedConstant(_) | IrConstant::ClassConstant { .. } => Value::Null,
            IrConstant::Array(entries) => {
                let mut array = PhpArray::new();
                for entry in entries {
                    let value = self.inline_constant_value(&entry.value);
                    if let Some(key) = &entry.key {
                        let key_value = self.inline_constant_value(key);
                        if let Some(key) = ArrayKey::from_value(&key_value) {
                            array.insert(key, value);
                        } else {
                            array.append(value);
                        }
                    } else {
                        array.append(value);
                    }
                }
                Value::Array(array)
            }
        }
    }

    fn record_trace_event(
        &self,
        function_id: FunctionId,
        function: &IrFunction,
        stack: &mut CallStack,
        block_id: BlockId,
        instruction: &Instruction,
        output_len: usize,
    ) {
        let mut trace = self.trace.borrow_mut();
        let step = trace.len() + 1;
        trace.push(format!(
            "step={step} function={}({}) block={} instr={} kind={} stack_depth={} output_len={} locals=[{}] registers=[{}]",
            function.name,
            function_id.raw(),
            block_id.raw(),
            instruction.id.raw(),
            format_instruction_kind(&instruction.kind),
            stack.len(),
            output_len,
            format_locals(function, stack),
            format_registers(stack),
        ));
    }

    fn record_lvalue_trace_event(&self, operation: &str, local: LocalId, dims: &[ArrayKey]) {
        if !(self.options.trace || self.options.trace_runtime) {
            return;
        }
        let mut trace = self.trace.borrow_mut();
        let step = trace.len() + 1;
        trace.push(format!(
            "step={step} runtime lvalue operation={operation} local={} path=[{}]",
            local.raw(),
            dims.iter()
                .map(format_array_key_for_trace)
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }

    /// Lazily records one runtime trace line. The event closure only runs
    /// when tracing is enabled, so hot paths never pay for the string.
    fn record_runtime_trace_event(&self, event: impl FnOnce() -> String) {
        if !self.options.trace_runtime {
            return;
        }
        let mut trace = self.trace.borrow_mut();
        let step = trace.len() + 1;
        let event = event();
        trace.push(format!("step={step} runtime {event}"));
    }

    fn record_gc_root_trace_event(&self, stack: &CallStack, state: &ExecutionState) {
        if !self.options.trace_runtime {
            return;
        }
        let root_count = gc_root_count_from_vm_roots(stack, state);
        let snapshot = gc_snapshot_from_vm_roots(stack, state);
        self.record_runtime_trace_event(|| {
            format!(
                "gc-roots roots={} entities={} cycle_candidates={}",
                root_count,
                snapshot.nodes.len(),
                snapshot.cycle_candidates.len()
            )
        });
    }

    #[cfg(not(feature = "jit-cranelift"))]
    fn try_execute_jit_leaf(
        &self,
        _compiled: &CompiledUnit,
        _state: &ExecutionState,
        _function_id: FunctionId,
        _function: &IrFunction,
        tier: ExecutionTier,
        _call_shape_supported: bool,
        _args: &[PreparedArg],
    ) -> Option<Value> {
        if tier == ExecutionTier::Jit && matches!(self.options.jit, JitMode::Cranelift) {
            self.record_counter_native_candidate();
            self.record_counter_native_platform_unavailable();
        }
        None
    }

    /// Copy-and-patch native leaf tier (behind the default-on `jit-copy-patch`
    /// feature; disable per process via `PHRUST_JIT_COPY_PATCH=0` or per VM via
    /// `VmOptions::copy_patch_leaf_override`). Runs before the dense-dispatch and
    /// interpreter paths: if the callee is a recognized leaf called with plain
    /// positional value arguments, compile it once (cached), run it natively
    /// over the argument values, and return the result — otherwise `None` to
    /// fall through. An instance-method leaf (the `$this` property
    /// getter/setter shapes) marshals the call's receiver into slot `0` ahead
    /// of the declared parameters (`$this` is local `0` in method IR); the
    /// receiver's presence must match the function's methodness. Closures
    /// (captures), named arguments, by-reference arguments, and arity
    /// mismatches are rejected here; guard failures take the region's side exit
    /// (also `None`), so behavior is identical to interpreting the function.
    #[cfg(all(feature = "jit-copy-patch", unix, target_arch = "aarch64"))]
    fn try_execute_copy_patch_leaf(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        function: &IrFunction,
        call: &FunctionCall<'_>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Option<VmResult> {
        if !self
            .options
            .copy_patch_leaf_override
            .unwrap_or_else(crate::copy_patch_bridge::copy_patch_leaf_enabled)
        {
            return None;
        }
        // Plain positional value arguments only; the receiver's presence must
        // match the function's methodness (an instance-method leaf needs
        // `$this` for slot 0, a free function must not get one).
        if function.flags.is_method != call.this_value.is_some() || !call.captures.is_empty() {
            return None;
        }
        if call.arg_count() != function.params.len() {
            return None;
        }
        // Named args would misalign positional slots. By-reference *arg* fields
        // (`by_ref_local` etc.) only track a variable arg's source location for
        // potential write-back; they are set for any variable passed positionally
        // and are moot here because the recognizer already rejects functions with
        // by-reference *parameters* — the value is passed by value regardless.
        // (`positional_values` is positional by construction, so only the
        // `CallArgument` form can carry names.)
        if call.args.iter().any(|arg| arg.name.is_some()) {
            return None;
        }
        let leaf = crate::copy_patch_bridge::cached_leaf(
            compiled,
            function_id.raw(),
            function,
            &compiled.unit().constants,
        )?;
        // Buffer slot `i` is marshaled from `params[i]`; a method's `$this`
        // occupies local 0 in method IR, so the receiver leads and the
        // declared parameters follow at their local indices.
        let mut params: Vec<Value> =
            Vec::with_capacity(call.arg_count() + usize::from(call.this_value.is_some()));
        if let Some(this) = call.this_value.as_ref() {
            params.push(Value::Object(this.clone()));
        }
        if call.positional_values.is_empty() {
            params.extend(call.args.iter().map(|arg| arg.value.clone()));
        } else {
            params.extend(call.positional_values.iter().cloned());
        }
        // Return-and-resume call compositions need the VM to drive the
        // suspend/perform-call/re-enter loop rather than a single region run.
        if leaf.resume_plan().is_some() {
            return self.execute_copy_patch_resume_leaf(
                compiled,
                function_id,
                function,
                call,
                &leaf,
                &params,
                output,
                stack,
                state,
            );
        }
        match leaf.run_outcome(&params) {
            crate::copy_patch_bridge::LeafOutcome::Value(value) => {
                Some(VmResult::success_no_output(Some(value)))
            }
            crate::copy_patch_bridge::LeafOutcome::Fallback => None,
            // The native prefix computed the arguments and requested the userland
            // call. The bridge never re-enters the VM; the call runs here, on the
            // identical normal path, so behavior matches the interpreter exactly.
            crate::copy_patch_bridge::LeafOutcome::TailCall { callee_name, args } => self
                .execute_copy_patch_tailcall(
                    compiled,
                    function_id,
                    function,
                    call.call_span,
                    &params,
                    &callee_name,
                    args,
                    &call.running_fiber,
                    output,
                    stack,
                    state,
                ),
        }
    }

    /// Perform a copy-and-patch tail call: resolve `callee_name` exactly as the
    /// interpreter resolves an unqualified `CallFunction`, validate it is a plain
    /// userland function whose by-value arity matches the natively-computed
    /// `args`, then run it through the normal [`Self::execute_function`] path and
    /// return its result faithfully (exceptions/errors included).
    ///
    /// Materializes the leaf's own stack frame around the call so a throwing or
    /// stack-inspecting callee observes the identical call stack (name, arguments,
    /// and call-site spans) it would under the interpreter. The leaf is a free
    /// function whose parameters are all int-by-value, and the native region only
    /// requested the tail call after guarding every parameter as `Int`, so
    /// `leaf_args` are exactly the int values the leaf was called with — no
    /// argument-coercion divergence. The callee pops its own frame on every exit
    /// (return, runtime error, and `propagate_exception`), so popping the leaf
    /// frame afterward keeps the stack balanced, mirroring the interpreter popping
    /// the leaf once its body returns.
    ///
    /// Returns `None` — so the caller falls back to interpreting the *whole* leaf
    /// — when the callee is a builtin, a dynamic miss, a method/closure/generator,
    /// declared by-reference return, or has any by-reference/variadic parameter or
    /// a mismatched arity. A tail call to a userland scalar leaf simply re-enters
    /// `execute_function`, which may itself run natively.
    #[cfg(all(feature = "jit-copy-patch", unix, target_arch = "aarch64"))]
    #[allow(clippy::too_many_arguments)]
    fn execute_copy_patch_tailcall(
        &self,
        compiled: &CompiledUnit,
        leaf_function_id: FunctionId,
        leaf_function: &IrFunction,
        leaf_call_span: Option<php_ir::IrSpan>,
        leaf_args: &[Value],
        callee_name: &str,
        args: Vec<Value>,
        running_fiber: &Option<FiberRef>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Option<VmResult> {
        let normalized = normalize_function_name(callee_name);
        let (callee_unit, callee_id) =
            match self.resolve_function_call_target(compiled, state, &normalized)? {
                FunctionCallCacheTarget::CurrentUnit {
                    unit_identity,
                    function,
                } if unit_identity == compiled.cache_identity() => (compiled.clone(), function),
                FunctionCallCacheTarget::CurrentUnit { .. } => return None,
                FunctionCallCacheTarget::DynamicUnit {
                    unit_index,
                    unit_identity,
                    function,
                } => (
                    resolve_dynamic_unit_by_identity(state, unit_index, unit_identity)?,
                    function,
                ),
                // A builtin tail call is out of scope; interpret the whole leaf.
                FunctionCallCacheTarget::Builtin { .. } => return None,
            };
        let callee = callee_unit.unit().functions.get(callee_id.index())?;
        let flags = callee.flags;
        if flags.is_top_level || flags.is_closure || flags.is_method || flags.is_generator {
            return None;
        }
        if callee.returns_by_ref || callee.params.len() != args.len() {
            return None;
        }
        if callee
            .params
            .iter()
            .any(|param| param.by_ref || param.variadic)
        {
            return None;
        }

        // The tail call's call-site span (where the leaf calls the callee). The
        // leaf is single-block with exactly one `CallFunction` (the tail call), so
        // the last `CallFunction` instruction is it; used for the callee frame's
        // backtrace line.
        let callee_call_span = leaf_function
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .rev()
            .find_map(|instruction| match instruction.kind {
                InstructionKind::CallFunction { .. } => Some(instruction.span),
                _ => None,
            });

        // Materialize the leaf's frame (free function: no class scope) with its
        // guaranteed-int arguments, so the callee sees the same stack.
        stack.push_fresh_frame(
            leaf_function_id,
            leaf_function.register_count,
            leaf_function.local_count,
            FrameActivationContext {
                scope_class: None,
                called_class: None,
                declaring_class: None,
                call_span: leaf_call_span,
            },
        );
        if let Some(frame) = stack.current_mut() {
            // Native leaf frames never bind arguments into locals, so the
            // lazy trace reconstruction has no source — keep the eager
            // snapshot here (guaranteed-int args, so the clones are cheap).
            frame.trace_arguments = TraceArguments::Materialized(
                leaf_args
                    .iter()
                    .map(|value| FrameTraceArgument {
                        name: None,
                        value: value.clone(),
                    })
                    .collect(),
            );
            frame.arguments = leaf_args.to_vec();
        }

        let sub_args: Vec<CallArgument> = args.into_iter().map(CallArgument::positional).collect();
        let sub_call = FunctionCall::new(sub_args, Vec::new())
            .with_call_site_strict_types(compiled.unit().strict_types)
            .with_optional_call_span(callee_call_span)
            .inherit_fiber_context(running_fiber);
        let mut result =
            self.execute_function(&callee_unit, callee_id, sub_call, output, stack, state);
        // The interpreter coerces the callee's value against the *leaf's*
        // declared return type at the leaf's `return g(...)` site (weak-mode
        // `"5"` → `int(5)` through `: int`, or the exact `TypeError`); the
        // callee's own return coercion does not subsume it. Mirror that here on
        // the normal-return path only — an exception/exit/suspension result
        // never reaches the leaf's return, so it propagates untouched. The
        // leaf's frame is still on the stack, so a thrown `TypeError`
        // attributes exactly as under the interpreter.
        if result.status.is_success()
            && result.process_exit_code.is_none()
            && result.yielded.is_none()
            && result.fiber_suspension.is_none()
        {
            match coerce_return_value(
                compiled,
                state,
                leaf_function,
                result.return_value.take(),
                self.typecheck_fast_path_context(),
            ) {
                Ok(value) => result.return_value = value,
                Err(message) => {
                    let error = self.runtime_error(output, compiled, stack, message);
                    stack.pop_recycle();
                    return Some(error);
                }
            }
        }
        stack.pop_recycle();
        Some(result)
    }

    /// Drive a return-and-resume call-composition leaf: run the region until
    /// it suspends, perform each requested userland call through the normal
    /// interpreter path, write the `Int` result into the site's slot, and
    /// re-enter the region — repeating until the region completes.
    ///
    /// Soundness contract (mirrors `compile_scalar_int_resume_leaf`):
    ///
    /// - Before the first performed call nothing has run but pure native
    ///   prefix work, so any mismatch (side exit, resolution change) falls
    ///   back to interpreting the whole leaf.
    /// - After a call has been performed, re-running is unsound (the callee's
    ///   side effects happened). Every anomaly past that point is an engine
    ///   invariant violation surfaced as a deterministic runtime error — by
    ///   construction none is reachable: arguments are guarded/proven `Int`,
    ///   callees are compile-time-resolved unit functions whose names cannot
    ///   be legally redeclared, and their declared `: int` return coercion
    ///   guarantees an `Int` result or a throw (which propagates instead of
    ///   resuming).
    /// - Generator/fiber/continuation contexts are rejected up front: a
    ///   suspension inside a callee could otherwise abandon the region with
    ///   the call half-performed. Outside a fiber, `Fiber::suspend()` inside
    ///   the callee is PHP error behavior and propagates as such.
    ///
    /// The leaf's own frame is materialized around the whole loop (exactly
    /// like the tail-call path) so throwing or stack-inspecting callees
    /// observe the interpreter-identical stack, and the final result runs
    /// through the leaf's return-site coercion.
    #[cfg(all(feature = "jit-copy-patch", unix, target_arch = "aarch64"))]
    #[allow(clippy::too_many_arguments)]
    fn execute_copy_patch_resume_leaf(
        &self,
        compiled: &CompiledUnit,
        leaf_function_id: FunctionId,
        leaf_function: &IrFunction,
        call: &FunctionCall<'_>,
        leaf: &std::rc::Rc<crate::copy_patch_bridge::NativeLeaf>,
        params: &[Value],
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Option<VmResult> {
        use crate::copy_patch_bridge::ResumeStep;

        if call.resume_continuation.is_some()
            || call.resume_fiber_continuation.is_some()
            || call.running_generator.is_some()
            || call.running_fiber.is_some()
        {
            return None;
        }
        let plan = leaf.resume_plan()?;
        let (mut session, mut step) = leaf.begin_resume(params)?;
        let mut frame_pushed = false;
        let mut calls_performed = false;
        let resume_invariant = |this: &Self,
                                output: &mut OutputBuffer,
                                stack: &mut CallStack,
                                detail: &str|
         -> VmResult {
            this.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_COPY_PATCH_RESUME_INVARIANT: {detail}"),
            )
        };
        loop {
            match step {
                ResumeStep::Fallback => {
                    if calls_performed {
                        // Unreachable by construction; never re-run a leaf
                        // whose callee side effects already happened.
                        let result = resume_invariant(
                            self,
                            output,
                            stack,
                            "post-call side exit or unrepresentable result",
                        );
                        stack.pop_recycle();
                        return Some(result);
                    }
                    // Pure native prefix only — interpreting the whole leaf is
                    // sound (no frame was pushed yet).
                    return None;
                }
                ResumeStep::CallRequest { site } => {
                    let expected = plan.targets.get(site).copied()?;
                    let normalized = plan.normalized_names.get(site)?;
                    let resolved = self.resolve_function_call_target(compiled, state, normalized);
                    let matches_expected = matches!(
                        resolved,
                        Some(FunctionCallCacheTarget::CurrentUnit {
                            unit_identity,
                            function,
                        }) if unit_identity == compiled.cache_identity() && function == expected
                    );
                    if !matches_expected {
                        if calls_performed {
                            let result = resume_invariant(
                                self,
                                output,
                                stack,
                                "resume callee resolution changed mid-region",
                            );
                            stack.pop_recycle();
                            return Some(result);
                        }
                        // Nothing ran yet; the interpreter handles whatever
                        // the divergent resolution means.
                        return None;
                    }
                    let Some(args) = leaf.resume_args(&session, site) else {
                        if calls_performed {
                            let result = resume_invariant(
                                self,
                                output,
                                stack,
                                "non-int marshaled argument slot",
                            );
                            stack.pop_recycle();
                            return Some(result);
                        }
                        return None;
                    };
                    if !frame_pushed {
                        // Materialize the leaf's frame around the whole loop
                        // (mirrors `execute_copy_patch_tailcall`).
                        stack.push_fresh_frame(
                            leaf_function_id,
                            leaf_function.register_count,
                            leaf_function.local_count,
                            FrameActivationContext {
                                scope_class: None,
                                called_class: None,
                                declaring_class: None,
                                call_span: call.call_span,
                            },
                        );
                        if let Some(frame) = stack.current_mut() {
                            // Native leaf frame: see the tail-call arm above.
                            frame.trace_arguments = TraceArguments::Materialized(
                                params
                                    .iter()
                                    .map(|value| FrameTraceArgument {
                                        name: None,
                                        value: value.clone(),
                                    })
                                    .collect(),
                            );
                            frame.arguments = params.to_vec();
                        }
                        frame_pushed = true;
                    }
                    let sub_args: Vec<CallArgument> =
                        args.into_iter().map(CallArgument::positional).collect();
                    let sub_call = FunctionCall::new(sub_args, Vec::new())
                        .with_call_site_strict_types(compiled.unit().strict_types)
                        .with_optional_call_span(plan.call_spans.get(site).copied().flatten())
                        .inherit_fiber_context(&call.running_fiber);
                    let result =
                        self.execute_function(compiled, expected, sub_call, output, stack, state);
                    calls_performed = true;
                    if !result.status.is_success()
                        || result.process_exit_code.is_some()
                        || result.yielded.is_some()
                        || result.fiber_suspension.is_some()
                    {
                        // Exception/exit/suspension: the leaf's return never
                        // completes; propagate faithfully.
                        stack.pop_recycle();
                        return Some(result);
                    }
                    let value = result.return_value.unwrap_or(Value::Null);
                    if !matches!(value, Value::Int(_)) {
                        let result = resume_invariant(
                            self,
                            output,
                            stack,
                            "resume callee returned a non-int despite a declared int return",
                        );
                        stack.pop_recycle();
                        return Some(result);
                    }
                    step = leaf.resume(&mut session, site, &value);
                }
                ResumeStep::Value(value) => {
                    // The interpreter coerces at the leaf's return site; mirror
                    // it exactly (identity for the proven-`Int` result, and the
                    // frame is still pushed for error attribution).
                    match coerce_return_value(
                        compiled,
                        state,
                        leaf_function,
                        Some(value),
                        self.typecheck_fast_path_context(),
                    ) {
                        Ok(value) => {
                            if frame_pushed {
                                stack.pop_recycle();
                            }
                            return Some(VmResult::success_no_output(value));
                        }
                        Err(message) => {
                            let result = self.runtime_error(output, compiled, stack, message);
                            if frame_pushed {
                                stack.pop_recycle();
                            }
                            return Some(result);
                        }
                    }
                }
            }
        }
    }

    /// Hosts without the copy-patch emitter always fall back to the interpreter.
    #[cfg(all(feature = "jit-copy-patch", not(all(unix, target_arch = "aarch64"))))]
    fn try_execute_copy_patch_leaf(
        &self,
        _compiled: &CompiledUnit,
        _function_id: FunctionId,
        _function: &IrFunction,
        _call: &FunctionCall<'_>,
        _output: &mut OutputBuffer,
        _stack: &mut CallStack,
        _state: &mut ExecutionState,
    ) -> Option<VmResult> {
        None
    }

    #[cfg(feature = "jit-cranelift")]
    // Audited native-tier helper boundary (docs/performance/cranelift/
    // safety-audit.md): reconstitutes Box<Value> pointers produced by JIT
    // helpers for this synchronous call.
    #[allow(unsafe_code)]
    fn try_execute_jit_leaf(
        &self,
        compiled: &CompiledUnit,
        state: &ExecutionState,
        function_id: FunctionId,
        function: &IrFunction,
        tier: ExecutionTier,
        call_shape_supported: bool,
        args: &[PreparedArg],
    ) -> Option<Value> {
        if tier != ExecutionTier::Jit || !self.options.tiering.enabled {
            return None;
        }
        if self.options.jit != JitMode::Cranelift {
            return None;
        }
        self.record_counter_native_candidate();
        if !jit_leaf_call_shape_is_supported(function, call_shape_supported, args) {
            let reason = native_leaf_rejection_reason(function, call_shape_supported, args);
            self.record_counter_native_eligibility_rejection(reason);
            return None;
        }

        let key = JitFunctionKey {
            unit: compiled_unit_cache_key(compiled),
            function: function_id,
        };
        let cache_key = jit_compile_cache_key(function_id, function, &self.options);
        let runtime_layout_epoch = state.lookup_epoch().raw();
        {
            let mut jit = self.jit.borrow_mut();
            let entry = jit.functions.entry(key).or_default();
            if (self.options.jit_blacklist.enabled() && entry.blacklisted) || entry.disabled {
                self.record_counter_jit_tiering_blacklist_rejection();
                return None;
            }
            entry.calls = entry.calls.saturating_add(1);
        }

        let cache_lookup = self
            .jit
            .borrow_mut()
            .lookup_compile_cache(&cache_key, runtime_layout_epoch);
        let handle = match cache_lookup {
            JitCompileCacheLookup::Hit(handle) => {
                self.record_counter_jit_compile_cache_hit();
                if let Some(entry) = self.jit.borrow_mut().functions.get_mut(&key) {
                    entry.compiled = true;
                    entry.handle = Some(handle.clone());
                }
                handle
            }
            JitCompileCacheLookup::Invalidated => {
                self.record_counter_jit_compile_cache_invalidations(1);
                self.record_counter_jit_compile_cache_miss();
                if let Some(entry) = self.jit.borrow_mut().functions.get_mut(&key) {
                    entry.compiled = false;
                    entry.handle = None;
                }
                self.compile_cranelift_jit_leaf(
                    compiled,
                    function_id,
                    function,
                    key,
                    cache_key,
                    runtime_layout_epoch,
                )?
            }
            JitCompileCacheLookup::Miss => {
                self.record_counter_jit_compile_cache_miss();
                self.compile_cranelift_jit_leaf(
                    compiled,
                    function_id,
                    function,
                    key,
                    cache_key,
                    runtime_layout_epoch,
                )?
            }
        };

        if handle.expects_value_metadata() {
            let [object_arg] = args else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                self.record_counter_property_load_guard_exit();
                self.record_counter_property_load_slow_call();
                return None;
            };
            let Some(metadata) = handle.property_load_metadata() else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::GuardFailed),
                );
                self.record_counter_jit_guard_failure();
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                self.record_counter_property_load_guard_exit();
                self.record_counter_property_load_slow_call();
                return None;
            };
            if let Some(status) =
                property_load_pre_guard_status(compiled, state, &object_arg.value, metadata)
            {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::GuardFailed)
                        .with_status(status),
                );
                self.record_counter_jit_guard_failure();
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                self.record_counter_property_load_guard_exit();
                self.record_counter_property_load_slow_call();
                if status == JIT_PROPERTY_LOAD_STATUS_LAYOUT_EXIT {
                    self.record_counter_property_load_layout_exit();
                }
                return None;
            }
            let value_ptr = &object_arg.value as *const Value as usize;
            let metadata_ptr = metadata as *const php_jit::JitPropertyLoadMetadata as usize;
            match handle.invoke_value_metadata(
                value_ptr,
                metadata_ptr,
                php_jit::JIT_RUNTIME_ABI_HASH,
            ) {
                Ok(value_ptr) if value_ptr != 0 => {
                    // SAFETY: Successful property-load helpers return a pointer
                    // created with `Box::into_raw(Box<Value>)` specifically for
                    // this synchronous VM call.
                    let value = unsafe { *Box::from_raw(value_ptr as *mut Value) };
                    self.record_counter_jit_helper_calls(handle.helper_calls_per_invocation());
                    self.record_counter_jit_fast_path_hits(handle.fast_path_hits_per_invocation());
                    self.record_counter_property_load_fast_hit();
                    self.record_counter_jit_executed();
                    return Some(value);
                }
                Ok(_) => {
                    self.record_jit_side_exit_for_key(
                        key,
                        php_jit::JitSideExit::new(php_jit::SideExitReason::HelperStatus),
                    );
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    self.record_counter_property_load_guard_exit();
                    self.record_counter_property_load_slow_call();
                    return None;
                }
                Err(error) => {
                    let status = error.native_status();
                    let side_exit = match status {
                        Some(
                            JIT_PROPERTY_LOAD_STATUS_CLASS_EXIT
                            | JIT_PROPERTY_LOAD_STATUS_LAYOUT_EXIT
                            | JIT_PROPERTY_LOAD_STATUS_UNINITIALIZED_EXIT
                            | JIT_PROPERTY_LOAD_STATUS_STORAGE_EXIT,
                        ) => php_jit::JitSideExit::new(php_jit::SideExitReason::GuardFailed)
                            .with_status(status.unwrap()),
                        _ => error.side_exit(),
                    };
                    self.record_jit_side_exit_for_key(key, side_exit);
                    self.record_counter_jit_guard_failure();
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    self.record_counter_property_load_guard_exit();
                    self.record_counter_property_load_slow_call();
                    if status == Some(JIT_PROPERTY_LOAD_STATUS_LAYOUT_EXIT) {
                        self.record_counter_property_load_layout_exit();
                    }
                    if status == Some(JIT_PROPERTY_LOAD_STATUS_UNINITIALIZED_EXIT) {
                        self.record_counter_property_load_uninitialized_exit();
                    }
                    return None;
                }
            }
        }

        if handle.expects_value() {
            let [array_arg] = args else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                return None;
            };
            let value_ptr = &array_arg.value as *const Value as usize;
            match handle.invoke_value(value_ptr, php_jit::JIT_RUNTIME_ABI_HASH) {
                Ok(value) => {
                    self.record_counter_jit_helper_calls(handle.helper_calls_per_invocation());
                    self.record_counter_jit_fast_path_hits(handle.fast_path_hits_per_invocation());
                    match handle.specialization() {
                        php_jit::JitNativeSpecialization::PackedForeachIntSum => {
                            self.record_counter_packed_foreach_sum_fast_hit();
                        }
                        php_jit::JitNativeSpecialization::KnownCallStrlen
                        | php_jit::JitNativeSpecialization::KnownCallCount => {
                            self.record_counter_known_call_fast_hit();
                        }
                        php_jit::JitNativeSpecialization::StringConcat
                        | php_jit::JitNativeSpecialization::PropertyLoad
                        | php_jit::JitNativeSpecialization::RecordArrayLookup
                        | php_jit::JitNativeSpecialization::Generic => {}
                    }
                    self.record_counter_jit_executed();
                    return Some(Value::Int(value));
                }
                Err(error) => {
                    let side_exit = error.side_exit();
                    match handle.specialization() {
                        php_jit::JitNativeSpecialization::PackedForeachIntSum => {
                            match error.native_status() {
                                Some(status) if status == php_jit::JIT_HELPER_STATUS_OVERFLOW => {
                                    self.record_counter_jit_overflow_exit();
                                    self.record_counter_packed_foreach_sum_overflow_exit();
                                }
                                Some(status)
                                    if status == php_runtime::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT
                                        || status == php_runtime::PHP_JIT_ARRAY_STATUS_FALLBACK =>
                                {
                                    self.record_counter_packed_foreach_sum_layout_exit();
                                }
                                _ => {}
                            }
                        }
                        php_jit::JitNativeSpecialization::KnownCallStrlen
                        | php_jit::JitNativeSpecialization::KnownCallCount => {
                            self.record_counter_known_call_guard_exit();
                            self.record_counter_known_call_slow_call();
                        }
                        php_jit::JitNativeSpecialization::StringConcat
                        | php_jit::JitNativeSpecialization::PropertyLoad
                        | php_jit::JitNativeSpecialization::RecordArrayLookup
                        | php_jit::JitNativeSpecialization::Generic => {}
                    }
                    self.record_jit_side_exit_for_key(key, side_exit);
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    return None;
                }
            }
        }

        if handle.expects_value_value() {
            let [lhs_arg, rhs_arg] = args else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                self.record_counter_string_concat_fast_path(false);
                return None;
            };
            let lhs_ptr = &lhs_arg.value as *const Value as usize;
            let rhs_ptr = &rhs_arg.value as *const Value as usize;
            match handle.invoke_value_value(lhs_ptr, rhs_ptr, php_jit::JIT_RUNTIME_ABI_HASH) {
                Ok(value_ptr) if value_ptr != 0 => {
                    // SAFETY: Successful value/value helpers return a pointer
                    // created with `Box::into_raw(Box<Value>)` specifically for
                    // this synchronous VM call.
                    let value = unsafe { *Box::from_raw(value_ptr as *mut Value) };
                    self.record_counter_jit_helper_calls(handle.helper_calls_per_invocation());
                    self.record_counter_jit_fast_path_hits(handle.fast_path_hits_per_invocation());
                    match handle.specialization() {
                        php_jit::JitNativeSpecialization::StringConcat => {
                            self.record_counter_string_concat_fast_path(true);
                        }
                        php_jit::JitNativeSpecialization::RecordArrayLookup => {
                            self.record_counter_record_lookup_fast_hit();
                        }
                        _ => {}
                    }
                    self.record_counter_jit_executed();
                    return Some(value);
                }
                Ok(_) => {
                    self.record_jit_side_exit_for_key(
                        key,
                        php_jit::JitSideExit::new(php_jit::SideExitReason::HelperStatus),
                    );
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    if handle.specialization() == php_jit::JitNativeSpecialization::StringConcat {
                        self.record_counter_string_concat_fast_path(false);
                    }
                    return None;
                }
                Err(error) => {
                    let side_exit = error.side_exit();
                    self.record_jit_side_exit_for_key(key, side_exit);
                    if matches!(
                        error.native_status(),
                        Some(status) if status == php_jit::JIT_HELPER_STATUS_OVERFLOW
                    ) {
                        self.record_counter_jit_overflow_exit();
                    }
                    match handle.specialization() {
                        php_jit::JitNativeSpecialization::StringConcat => {
                            self.record_counter_string_concat_fast_path(false);
                        }
                        php_jit::JitNativeSpecialization::RecordArrayLookup => {
                            match error.native_status() {
                                Some(status)
                                    if status
                                        == php_runtime::PHP_JIT_ARRAY_STATUS_KEY_MISS_EXIT =>
                                {
                                    self.record_counter_record_lookup_key_miss_exit();
                                }
                                Some(status)
                                    if status == php_runtime::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT =>
                                {
                                    self.record_counter_record_lookup_layout_exit();
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    return None;
                }
            }
        }

        if handle.expects_value_i64() {
            let [array_arg, index_arg] = args else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                return None;
            };
            let Value::Int(index) = index_arg.value else {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                return None;
            };
            let value_ptr = &array_arg.value as *const Value as usize;
            match handle.invoke_value_i64(value_ptr, index, php_jit::JIT_RUNTIME_ABI_HASH) {
                Ok(value) => {
                    self.record_counter_jit_helper_calls(handle.helper_calls_per_invocation());
                    self.record_counter_jit_fast_path_hits(handle.fast_path_hits_per_invocation());
                    self.record_counter_packed_fetch_fast_hit();
                    self.record_counter_jit_executed();
                    return Some(Value::Int(value));
                }
                Err(error) => {
                    let mut side_exit = error.side_exit();
                    match error.native_status() {
                        Some(status) if status == php_runtime::PHP_JIT_ARRAY_STATUS_BOUNDS_EXIT => {
                            self.record_counter_packed_fetch_bounds_exit();
                            side_exit =
                                php_jit::JitSideExit::new(php_jit::SideExitReason::HelperStatus)
                                    .with_status(status);
                        }
                        Some(status) if status == php_runtime::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT => {
                            self.record_counter_packed_fetch_layout_exit();
                        }
                        _ => {}
                    }
                    self.record_jit_side_exit_for_key(key, side_exit);
                    self.record_counter_jit_bailout();
                    self.record_counter_jit_slow_path_call();
                    return None;
                }
            }
        }

        let native_args = match args
            .iter()
            .map(|arg| value_as_jit_int(&arg.value))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(args) => args,
            Err(()) => {
                self.record_jit_side_exit_for_key(
                    key,
                    php_jit::JitSideExit::new(php_jit::SideExitReason::TypeMismatch),
                );
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                return None;
            }
        };
        match handle.invoke_i64(&native_args, php_jit::JIT_RUNTIME_ABI_HASH) {
            Ok(value) => {
                self.record_counter_jit_helper_calls(handle.helper_calls_per_invocation());
                self.record_counter_jit_fast_path_hits(handle.fast_path_hits_per_invocation());
                self.record_counter_jit_executed();
                Some(Value::Int(value))
            }
            Err(error) => {
                let side_exit = error.side_exit();
                if side_exit.reason == php_jit::SideExitReason::Overflow {
                    self.record_counter_jit_overflow_exit();
                }
                self.record_jit_side_exit_for_key(key, side_exit);
                self.record_counter_jit_bailout();
                self.record_counter_jit_slow_path_call();
                None
            }
        }
    }

    #[cfg(feature = "jit-cranelift")]
    fn compile_cranelift_jit_leaf(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        function: &IrFunction,
        key: JitFunctionKey,
        cache_key: JitCompileCacheKey,
        runtime_layout_epoch: u64,
    ) -> Option<php_jit::JitFunctionHandle> {
        if !self.jit_compile_budget_allows_attempt() {
            self.record_counter_jit_tiering_budget_rejection();
            return None;
        }
        self.record_counter_jit_compile_attempt();
        let mut engine = php_jit::JitEngine::with_options(php_jit::JitOptions {
            enabled: true,
            allow_native_execution: true,
        });
        let compile_result = engine.compile_function_with_runtime_helpers(
            compiled.unit(),
            function_id,
            php_jit::JitCompileRequest::new(format!("function.{}", function.name))
                .with_function_name(function.name.clone())
                .with_ir_fingerprint(format!("{:016x}", cache_key.ir_fingerprint)),
            php_jit::JitRuntimeHelperAddresses {
                packed_array_len: jit_array_len_abi as *const () as usize,
                packed_array_fetch_int_slow: jit_array_fetch_int_slow_abi as *const () as usize,
                known_strlen: jit_strlen_known_abi as *const () as usize,
                known_count: jit_count_known_abi as *const () as usize,
                string_concat: jit_concat_string_string_fast as *const () as usize,
                property_load: jit_property_load_monomorphic_fast as *const () as usize,
                record_array_lookup: jit_record_array_lookup_abi as *const () as usize,
            },
        );
        match compile_result {
            Ok(result) if result.status == php_jit::JitCompileStatus::Compiled => {
                let Some(handle) = result.handle else {
                    self.record_jit_compile_failure_for_key(key);
                    self.record_counter_jit_bailout();
                    return None;
                };
                let descriptor = JitCompileDescriptor {
                    function_id: function_id.raw(),
                    function_name: function.name.clone(),
                    ir_fingerprint: format!("{:016x}", cache_key.ir_fingerprint),
                    code_bytes: result.stats.native_code_bytes,
                    compile_time_nanos: result.stats.native_compile_time_nanos,
                    target_isa: cache_key.target_isa.clone(),
                    abi_hash: cache_key.abi_hash,
                    config_hash: cache_key.config_hash,
                };
                {
                    let mut jit = self.jit.borrow_mut();
                    if let Some(entry) = jit.functions.get_mut(&key) {
                        entry.compiled = true;
                        entry.handle = Some(handle.clone());
                    }
                    jit.insert_compile_cache(cache_key, handle.clone(), runtime_layout_epoch);
                }
                self.record_counter_jit_compiled();
                self.record_counter_jit_compile_metadata(
                    result.stats.native_code_bytes,
                    result.stats.native_compile_time_nanos,
                );
                self.record_counter_jit_compile_descriptor(descriptor);
                self.maybe_write_cranelift_clif_dump(compiled, function_id);
                self.record_jit_compile_budget_spent(result.stats.native_compile_time_nanos);
                Some(handle)
            }
            Ok(_) | Err(_) => {
                self.record_jit_compile_failure_for_key(key);
                self.record_counter_jit_bailout();
                None
            }
        }
    }

    fn try_execute_bytecode_entry(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> BytecodeEntryAttempt {
        match self.try_execute_dense_function_entry(
            compiled,
            compiled.unit().entry,
            FunctionCall::new(Vec::new(), Vec::new()),
            output,
            stack,
            state,
        ) {
            BytecodeFunctionAttempt::Executed(result, _) => BytecodeEntryAttempt::Executed(result),
            BytecodeFunctionAttempt::Unsupported(message, _) => {
                BytecodeEntryAttempt::Unsupported(message)
            }
        }
    }

    fn dense_execution_artifact_key(&self) -> DenseExecutionArtifactKey {
        DenseExecutionArtifactKey {
            mode: if self.options.execution_format.is_strict_bytecode() {
                DenseExecutionArtifactMode::Strict
            } else {
                DenseExecutionArtifactMode::Mixed
            },
            superinstructions: self.options.superinstructions.is_enabled(),
            profiled_layout: self.options.bytecode_layout.is_profiled(),
            layout_profile_entries: if self.options.bytecode_layout.is_profiled() {
                self.options
                    .bytecode_layout_profile
                    .as_ref()
                    .map(|profile| {
                        profile
                            .block_entries
                            .iter()
                            .map(|(key, value)| (key.clone(), *value))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            },
            dense_jump_threading: self.options.dense_jump_threading.is_enabled(),
        }
    }

    fn get_or_build_dense_execution_plan(
        &self,
        compiled: &CompiledUnit,
    ) -> Result<Arc<DenseExecutionPlan>, String> {
        let key = DenseExecutionPlanThreadCacheKey {
            compiled_identity: compiled.cache_identity(),
            artifact: self.dense_execution_artifact_key(),
        };
        if let Some(plan) =
            DENSE_EXECUTION_PLAN_THREAD_CACHE.with(|cache| cache.borrow().get(&key).cloned())
        {
            self.record_counter_dense_execution_plan_cache_hit();
            self.record_counter_dense_execution_plan(plan.as_ref());
            return Ok(plan);
        }

        #[allow(clippy::arc_with_non_send_sync)] // plan sharing predates a Send-safe design
        let plan = Arc::new({
            let mut plan = self.build_dense_execution_plan(compiled)?;
            plan.call_shape_meta = dense_call_shape_meta_for_unit(compiled.unit());
            plan
        });
        DENSE_EXECUTION_PLAN_THREAD_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() >= DENSE_EXECUTION_PLAN_THREAD_CACHE_MAX {
                cache.clear();
            }
            cache.insert(key, Arc::clone(&plan));
        });
        self.record_counter_dense_execution_plan_cache_miss();
        self.record_counter_dense_execution_plan(plan.as_ref());
        Ok(plan)
    }

    fn build_dense_execution_plan(
        &self,
        compiled: &CompiledUnit,
    ) -> Result<DenseExecutionPlan, String> {
        self.record_counter_bytecode_lower_attempt();
        if !self.options.execution_format.is_strict_bytecode() {
            let mut plan = DenseBytecodeUnit::mixed_plan_from_ir(compiled.unit());
            if let Err(errors) = plan.unit.verify() {
                return Err(format!(
                    "E_PHP_VM_DENSE_BYTECODE_VERIFY: mixed dense bytecode verifier rejected unit with {} error(s)",
                    errors.len()
                ));
            }
            if self.options.superinstructions.is_enabled() {
                let report = plan.unit.select_superinstructions();
                self.record_counter_superinstruction_selection(&report);
                if let Err(errors) = plan.unit.verify() {
                    return Err(format!(
                        "E_PHP_VM_DENSE_SUPERINSTRUCTION_VERIFY: selected mixed dense bytecode failed verification with {} error(s)",
                        errors.len()
                    ));
                }
            }
            if self.options.bytecode_layout.is_profiled() {
                let _report = plan
                    .unit
                    .apply_profiled_layout(self.options.bytecode_layout_profile.as_ref());
                if let Err(errors) = plan.unit.verify() {
                    return Err(format!(
                        "E_PHP_VM_DENSE_LAYOUT_VERIFY: profiled mixed dense bytecode layout failed verification with {} error(s)",
                        errors.len()
                    ));
                }
            }
            if self.options.dense_jump_threading.is_enabled() && plan.unit.has_jump_trampolines() {
                // Verifier-bracketed with rollback: a threading result that
                // fails verification restores the pre-pass unit instead of
                // dropping the whole plan. The trampoline pre-scan keeps the
                // snapshot clone off the common no-trampoline path.
                let snapshot = plan.unit.clone();
                let report = plan.unit.thread_jump_chains();
                if report.threaded_edges > 0 && plan.unit.verify().is_err() {
                    plan.unit = snapshot;
                    self.record_counter_dense_jump_threading(&report, true);
                } else {
                    self.record_counter_dense_jump_threading(&report, false);
                }
            }
            self.record_counter_bytecode_lowered_families(&plan.unit);
            self.record_counter_bytecode_lower_success();
            return Ok(plan);
        }

        let mut dense = DenseBytecodeUnit::lower_from_ir(compiled.unit())
            .map_err(|error| format!("E_PHP_VM_DENSE_BYTECODE_UNSUPPORTED: {}", error.message))?;
        if let Err(errors) = dense.verify() {
            return Err(format!(
                "E_PHP_VM_DENSE_BYTECODE_VERIFY: dense bytecode verifier rejected unit with {} error(s)",
                errors.len()
            ));
        }
        if self.options.superinstructions.is_enabled() {
            let report = dense.select_superinstructions();
            self.record_counter_superinstruction_selection(&report);
            if let Err(errors) = dense.verify() {
                return Err(format!(
                    "E_PHP_VM_DENSE_SUPERINSTRUCTION_VERIFY: selected dense bytecode failed verification with {} error(s)",
                    errors.len()
                ));
            }
        }
        if self.options.bytecode_layout.is_profiled() {
            let _report =
                dense.apply_profiled_layout(self.options.bytecode_layout_profile.as_ref());
            if let Err(errors) = dense.verify() {
                return Err(format!(
                    "E_PHP_VM_DENSE_LAYOUT_VERIFY: profiled dense bytecode layout failed verification with {} error(s)",
                    errors.len()
                ));
            }
        }
        if self.options.dense_jump_threading.is_enabled() && dense.has_jump_trampolines() {
            let snapshot = dense.clone();
            let report = dense.thread_jump_chains();
            if report.threaded_edges > 0 && dense.verify().is_err() {
                dense = snapshot;
                self.record_counter_dense_jump_threading(&report, true);
            } else {
                self.record_counter_dense_jump_threading(&report, false);
            }
        }
        self.record_counter_bytecode_lowered_families(&dense);
        self.record_counter_bytecode_lower_success();
        let functions = dense
            .functions
            .iter()
            .map(|_| DenseFunctionPlan::Dense)
            .collect();
        Ok(DenseExecutionPlan {
            unit: dense,
            functions,
            call_shape_meta: Vec::new(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn try_execute_dense_function_entry<'a>(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        call: FunctionCall<'a>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> BytecodeFunctionAttempt<'a> {
        let mut call = Some(call);
        if self.options.trace || self.options.trace_runtime {
            return BytecodeFunctionAttempt::Unsupported(
                "E_PHP_VM_DENSE_BYTECODE_TRACE_UNSUPPORTED: dense bytecode execution does not support tracing yet"
                    .to_string(),
                call.expect("call should be available before execution starts"),
            );
        }
        let Some(ir_function) = compiled.unit().functions.get(function_id.index()) else {
            return BytecodeFunctionAttempt::Unsupported(
                "E_PHP_VM_DENSE_BYTECODE_ENTRY: IR entry function is missing".to_string(),
                call.expect("call should be available before execution starts"),
            );
        };
        let plan = match self.get_or_build_dense_execution_plan(compiled) {
            Ok(plan) => plan,
            Err(message) => {
                return BytecodeFunctionAttempt::Unsupported(
                    message,
                    call.expect("call should be available before execution starts"),
                );
            }
        };
        match plan.function_plan(function_id.index()) {
            Some(DenseFunctionPlan::Dense) => {
                let Some(dense_function) = plan.unit.functions.get(function_id.index()) else {
                    return BytecodeFunctionAttempt::Unsupported(
                        "E_PHP_VM_DENSE_BYTECODE_ENTRY: dense bytecode entry function is missing"
                            .to_string(),
                        call.expect("call should be available before execution starts"),
                    );
                };
                BytecodeFunctionAttempt::Executed(
                    Box::new(self.execute_bytecode_function(
                        DenseExecutionRequest {
                            compiled,
                            dense: &plan.unit,
                            plan: Some(plan.as_ref()),
                            dense_function,
                            ir_function,
                            function_id,
                            call: call.take().expect("call should be consumed exactly once"),
                        },
                        output,
                        stack,
                        state,
                    )),
                    BytecodeFunctionTier::Dense,
                )
            }
            Some(DenseFunctionPlan::RichFallback { reason }) => {
                self.record_counter_rich_fallback_function_executed(reason, &ir_function.name);
                BytecodeFunctionAttempt::Executed(
                    Box::new(self.execute_function(
                        compiled,
                        function_id,
                        call.take().expect("call should be consumed exactly once"),
                        output,
                        stack,
                        state,
                    )),
                    BytecodeFunctionTier::RichFallback(reason.clone()),
                )
            }
            None => BytecodeFunctionAttempt::Unsupported(
                "E_PHP_VM_DENSE_BYTECODE_ENTRY: dense execution plan entry is missing".to_string(),
                call.expect("call should be available before execution starts"),
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn try_execute_cached_dense_function_dispatch<'a>(
        &self,
        compiled: &CompiledUnit,
        function_id: FunctionId,
        function: &IrFunction,
        call: FunctionCall<'a>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> CachedDenseFunctionDispatch<'a> {
        if !self.options.execution_format.attempts_bytecode()
            || self.options.trace
            || self.options.trace_runtime
            || function.flags.is_generator
            || call.resume_continuation.is_some()
            || call.resume_fiber_continuation.is_some()
            || call.running_generator.is_some()
            || call.running_fiber.is_some()
        {
            return CachedDenseFunctionDispatch::Continue(call);
        }

        let plan = match self.get_or_build_dense_execution_plan(compiled) {
            Ok(plan) => plan,
            Err(message) => {
                let reason = dense_bytecode_unsupported_reason(&message);
                self.record_counter_bytecode_unsupported_reason(reason);
                if self.options.execution_format.is_strict_bytecode() {
                    return CachedDenseFunctionDispatch::Executed(Box::new(VmResult::unsupported(
                        output.clone(),
                        message,
                    )));
                }
                self.record_counter_bytecode_unsupported_fallback();
                self.record_counter_bytecode_auto_fallback_reason(reason);
                return CachedDenseFunctionDispatch::Continue(call);
            }
        };

        match plan.function_plan(function_id.index()) {
            Some(DenseFunctionPlan::Dense) => {
                let Some(dense_function) = plan.unit.functions.get(function_id.index()) else {
                    let message =
                        "E_PHP_VM_DENSE_BYTECODE_ENTRY: dense bytecode function is missing"
                            .to_string();
                    if self.options.execution_format.is_strict_bytecode() {
                        return CachedDenseFunctionDispatch::Executed(Box::new(
                            VmResult::unsupported(output.clone(), message),
                        ));
                    }
                    self.record_counter_bytecode_unsupported_reason(
                        dense_bytecode_unsupported_reason(&message),
                    );
                    return CachedDenseFunctionDispatch::Continue(call);
                };
                CachedDenseFunctionDispatch::Executed(Box::new(self.execute_bytecode_function(
                    DenseExecutionRequest {
                        compiled,
                        dense: &plan.unit,
                        plan: Some(plan.as_ref()),
                        dense_function,
                        ir_function: function,
                        function_id,
                        call,
                    },
                    output,
                    stack,
                    state,
                )))
            }
            Some(DenseFunctionPlan::RichFallback { .. }) => {
                CachedDenseFunctionDispatch::Continue(call)
            }
            None => {
                let message =
                    "E_PHP_VM_DENSE_BYTECODE_ENTRY: dense execution plan entry is missing"
                        .to_string();
                if self.options.execution_format.is_strict_bytecode() {
                    CachedDenseFunctionDispatch::Executed(Box::new(VmResult::unsupported(
                        output.clone(),
                        message,
                    )))
                } else {
                    self.record_counter_bytecode_unsupported_reason(
                        dense_bytecode_unsupported_reason(&message),
                    );
                    CachedDenseFunctionDispatch::Continue(call)
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn load_local_value(
        &self,
        compiled: &CompiledUnit,
        function: &IrFunction,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
        local: LocalId,
        span: IrSpan,
        pre_call_by_ref_out_param: bool,
    ) -> Result<Value, VmResult> {
        if is_globals_local(function, local) {
            self.record_counter_local_slot_fast_path(false);
            return Ok(Value::Array(state.globals.globals_array()));
        }
        self.record_counter_local_slot_fast_path(local_slot_is_in_bounds(stack, local));
        let local_value = stack.current().expect("frame was pushed").locals.get(local);
        match local_value {
            Some(Value::Uninitialized) if is_this_local(function, local) => {
                let result = self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_THIS_OUTSIDE_METHOD: Using $this when not in object context"
                        .to_owned(),
                );
                if let Some(throwable) = runtime_error_throwable(&result) {
                    tag_throwable_location(&throwable, compiled, span);
                    state.pending_trace = Some(capture_backtrace_string(compiled, stack));
                    state.pending_throw = Some(throwable);
                    Err(VmResult::propagating_exception(output.clone()))
                } else {
                    Err(result)
                }
            }
            Some(Value::Uninitialized) if pre_call_by_ref_out_param => Ok(Value::Null),
            Some(Value::Uninitialized) => {
                let local_name = function
                    .locals
                    .get(local.index())
                    .cloned()
                    .unwrap_or_else(|| format!("local:{}", local.raw()));
                let diagnostic = if is_auto_global_name(&local_name) {
                    undefined_global_variable_warning(
                        local_name,
                        runtime_source_span(compiled, span),
                        stack_trace(compiled, stack),
                    )
                } else {
                    undefined_variable_warning(
                        local_name,
                        runtime_source_span(compiled, span),
                        stack_trace(compiled, stack),
                    )
                };
                let handled = self.dispatch_error_handler(
                    compiled,
                    output,
                    stack,
                    state,
                    php_runtime::PHP_E_WARNING,
                    &diagnostic,
                )?;
                if !handled && error_reporting_allows(state, php_runtime::PHP_E_WARNING) {
                    emit_vm_diagnostic(
                        output,
                        state,
                        &diagnostic,
                        php_runtime::PhpDiagnosticChannel::Warning,
                        php_runtime::PHP_E_WARNING,
                    );
                    diagnostics.push(diagnostic);
                }
                Ok(Value::Null)
            }
            Some(value) => Ok(value),
            None => Err(self.runtime_error(
                output,
                compiled,
                stack,
                format!("invalid local local:{}", local.raw()),
            )),
        }
    }

    #[cold]
    #[inline(never)]
    fn dense_runtime_error(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &CallStack,
        state: &mut ExecutionState,
        span: IrSpan,
        message: String,
    ) -> VmResult {
        // Match the rich raise path: the emitted diagnostic carries the
        // source span, not just the throwable location tag.
        let result = self.runtime_error_with_bringup_context(
            output,
            compiled,
            stack,
            state,
            runtime_source_span(compiled, span),
            message,
            BringupDiagnosticInput {
                autoload_enabled: Some(true),
                ..BringupDiagnosticInput::default()
            },
        );
        if let Some(throwable) = runtime_error_throwable(&result) {
            tag_throwable_location(&throwable, compiled, span);
            state.pending_trace = Some(capture_backtrace_string(compiled, stack));
            state.pending_throw = Some(throwable);
            VmResult::propagating_exception(output.clone())
        } else {
            result
        }
    }

    #[cold]
    #[inline(never)]
    fn runtime_error_at_optional_span(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &CallStack,
        state: &mut ExecutionState,
        span: Option<IrSpan>,
        message: String,
    ) -> VmResult {
        if let Some(span) = span {
            self.dense_runtime_error(compiled, output, stack, state, span, message)
        } else {
            self.runtime_error(output, compiled, stack, message)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn dense_fetch_property_value(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: InstrId,
        cache_id: Option<InlineCacheId>,
        property: &str,
        object_value: Value,
        span: IrSpan,
    ) -> Result<Value, VmResult> {
        let object = match object_value {
            Value::Object(object) => object,
            Value::Reference(cell) => match cell.get() {
                Value::Object(object) => object,
                other => {
                    self.record_counter_dense_property_fallback("non_object");
                    return Err(self.dense_runtime_error(
                        compiled,
                        output,
                        stack,
                        state,
                        span,
                        format!(
                            "E_PHP_VM_PROPERTY_FETCH_NON_OBJECT: cannot fetch property {property} from {}",
                            value_type_name(&other)
                        ),
                    ));
                }
            },
            other => {
                self.record_counter_dense_property_fallback("non_object");
                return Err(self.dense_runtime_error(
                    compiled,
                    output,
                    stack,
                    state,
                    span,
                    format!(
                        "E_PHP_VM_PROPERTY_FETCH_NON_OBJECT: cannot fetch property {property} from {}",
                        value_type_name(&other)
                    ),
                ));
            }
        };

        let class_handle = object.class_name_handle();
        if internal_throwable_instanceof(&class_handle, "throwable").is_some()
            || is_php_token_runtime_class(&class_handle)
            || is_std_class_runtime_class(&class_handle)
            || is_date_time_runtime_class(&class_handle)
            || is_pdo_runtime_class(&class_handle)
        {
            self.record_counter_dense_property_fallback("internal_runtime_object");
            return Ok(object.get_property(property).unwrap_or(Value::Null));
        }
        if class_name_is(&class_handle, &["simplexmlelement"]) {
            self.record_counter_dense_property_fallback("simplexml_property");
            return Ok(php_runtime::xml::simplexml_property(&object, property));
        }
        if spl_array_object_uses_array_as_props(&object) {
            // ARRAY_AS_PROPS routes property reads through the container's
            // array storage; the rich arm takes the same branch.
            self.record_counter_dense_property_fallback("spl_array_as_props");
            return spl_container_offset_get(
                &object,
                &Value::String(PhpString::from_test_str(property)),
            )
            .map_err(|message| {
                self.dense_runtime_error(compiled, output, stack, state, span, message)
            });
        }

        let scope = current_scope_class(compiled, stack);
        let normalized_scope = scope.as_deref().map(normalize_class_name);
        let receiver_class = normalize_class_name(&class_handle);
        let lookup_epoch = state.lookup_epoch();

        let cached_target = if let Some(id) = cache_id {
            self.lookup_dense_property_fetch_inline_cache(
                id,
                function_id,
                instruction_id,
                property,
                &receiver_class,
                normalized_scope.as_deref(),
                lookup_epoch,
            )
        } else {
            self.observe_dense_property_inline_cache(
                compiled,
                function_id,
                block_id,
                instruction_id,
                InlineCacheKind::PropertyFetch,
            );
            self.lookup_property_fetch_inline_cache(
                compiled,
                function_id,
                block_id,
                instruction_id,
                property,
                &receiver_class,
                normalized_scope.as_deref(),
                lookup_epoch,
            )
        };
        if let Some(target) = cached_target {
            match self.read_property_fetch_target(compiled, target, &object, stack, state) {
                Ok(PropertyFetchCacheRead::Value(value)) => {
                    self.record_counter_dense_property_ic_reuse();
                    self.record_counter_dense_property_fetch_hit();
                    return Ok(value);
                }
                Ok(PropertyFetchCacheRead::Fallback) => {
                    self.record_counter_dense_property_fallback("inline_cache_guard");
                }
                Err(message) => {
                    return Err(
                        self.dense_runtime_error(compiled, output, stack, state, span, message)
                    );
                }
            }
        }

        // Only the resolve/miss path needs the class entry and the magic-get
        // flag; keep the class-table lookup and hierarchy walk off the
        // cache-hit path.
        let class = match lookup_class_in_state(compiled, state, &class_handle) {
            Some(class) => class,
            None => {
                self.record_counter_dense_property_fallback("receiver_class_missing");
                return Err(self.dense_runtime_error(
                    compiled,
                    output,
                    stack,
                    state,
                    span,
                    format!("E_PHP_VM_UNKNOWN_CLASS: class {class_handle} is not defined"),
                ));
            }
        };
        let receiver_has_magic_get = class_has_public_magic_get(compiled, &class);
        let resolved = match lookup_resolved_property_in_state(
            compiled,
            state,
            &class,
            property,
            scope.as_deref(),
        ) {
            Ok(Some(resolved)) => resolved,
            Ok(None) => {
                self.record_counter_dense_property_fallback("dynamic_property");
                let property_callsite =
                    property_fetch_callsite(compiled, function_id, block_id, instruction_id);
                if let Some(value) = object.get_property(property) {
                    self.record_counter_property_fetch_profile(property_fetch_profile_observation(
                        &property_callsite,
                        property,
                        &receiver_class,
                        &class,
                        None,
                        normalized_scope.as_deref(),
                        lookup_epoch,
                        receiver_has_magic_get,
                        false,
                        true,
                        false,
                        false,
                        Vec::new(),
                    ));
                    return Ok(value);
                }
                self.record_counter_property_fetch_profile(property_fetch_profile_observation(
                    &property_callsite,
                    property,
                    &receiver_class,
                    &class,
                    None,
                    normalized_scope.as_deref(),
                    lookup_epoch,
                    receiver_has_magic_get,
                    false,
                    false,
                    false,
                    false,
                    Vec::new(),
                ));
                match self.call_magic_property_method(
                    compiled,
                    object.clone(),
                    "__get",
                    property,
                    vec![CallArgument::positional(Value::String(
                        PhpString::from_test_str(property),
                    ))],
                    output,
                    stack,
                    state,
                ) {
                    Ok(Some(value)) => {
                        self.record_counter_dense_property_fallback("magic_get");
                        return Ok(value);
                    }
                    Ok(None) => {}
                    Err(result) => return Err(result),
                }
                self.emit_undefined_property_warning(
                    compiled,
                    output,
                    stack,
                    state,
                    diagnostics,
                    &object.display_name(),
                    property,
                    span,
                )?;
                return Ok(Value::Null);
            }
            Err(message) => {
                return Err(self.dense_runtime_error(compiled, output, stack, state, span, message));
            }
        };
        let resolved_class = &resolved.class;
        let resolved_property = &resolved.property;

        if let Err(access_error) = validate_property_access_in_state(
            compiled,
            state,
            stack,
            resolved_class,
            resolved_property,
        ) {
            self.record_counter_dense_property_fallback("visibility_mismatch");
            match self.call_magic_property_method(
                compiled,
                object.clone(),
                "__get",
                property,
                vec![CallArgument::positional(Value::String(
                    PhpString::from_test_str(property),
                ))],
                output,
                stack,
                state,
            ) {
                Ok(Some(value)) => {
                    self.record_counter_dense_property_fallback("magic_get");
                    return Ok(value);
                }
                Ok(None) => {
                    return Err(self.dense_runtime_error(
                        compiled,
                        output,
                        stack,
                        state,
                        span,
                        access_error,
                    ));
                }
                Err(result) => return Err(result),
            }
        }

        if resolved.property.flags.is_static {
            self.record_counter_dense_property_fallback("static_property");
            emit_static_property_as_non_static_notice(
                compiled,
                output,
                stack,
                state,
                resolved_class,
                resolved_property,
                span,
            );
        }

        if !property_hook_is_active(state, &object, resolved_class, resolved_property)
            && let Some(function) = resolved.property.hooks.get
        {
            self.record_counter_dense_property_fallback("property_hook");
            return self.call_property_hook(
                compiled,
                object,
                resolved_class,
                resolved_property,
                function,
                Vec::new(),
                output,
                stack,
                state,
            );
        }

        let storage_name = property_storage_name(resolved_class, resolved_property);
        let Some(value) = object.get_property(&storage_name) else {
            self.record_counter_dense_property_fallback("storage_missing");
            match self.call_magic_property_method(
                compiled,
                object.clone(),
                "__get",
                property,
                vec![CallArgument::positional(Value::String(
                    PhpString::from_test_str(property),
                ))],
                output,
                stack,
                state,
            ) {
                Ok(Some(value)) => {
                    self.record_counter_dense_property_fallback("magic_get");
                    return Ok(value);
                }
                Ok(None) => {}
                Err(result) => return Err(result),
            }
            self.emit_undefined_property_warning(
                compiled,
                output,
                stack,
                state,
                diagnostics,
                &object.display_name(),
                property,
                span,
            )?;
            return Ok(Value::Null);
        };
        if matches!(value, Value::Uninitialized) {
            self.record_counter_dense_property_fallback("typed_property_uninitialized");
            return Err(self.dense_runtime_error(
                compiled,
                output,
                stack,
                state,
                span,
                format!(
                    "E_PHP_VM_UNINITIALIZED_PROPERTY: Typed property {}::${property} must not be accessed before initialization",
                    resolved.class.display_name
                ),
            ));
        }
        self.maybe_install_property_fetch_inline_cache_target(
            compiled,
            function_id,
            block_id,
            instruction_id,
            property,
            &receiver_class,
            &class,
            resolved_class,
            resolved_property,
            &storage_name,
            normalized_scope.as_deref(),
            lookup_epoch,
            receiver_has_magic_get,
            state,
            &object,
            cache_id,
        );
        self.record_counter_dense_property_fetch_hit();
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    fn dense_assign_property_value(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        function_id: FunctionId,
        block_id: BlockId,
        instruction_id: InstrId,
        cache_id: Option<InlineCacheId>,
        property: &str,
        object_value: Value,
        value: Value,
        span: IrSpan,
    ) -> Result<Value, VmResult> {
        let object = match object_value {
            Value::Object(object) => object,
            Value::Reference(cell) => match cell.get() {
                Value::Object(object) => object,
                other => {
                    self.record_counter_dense_property_fallback("non_object");
                    return Err(self.dense_runtime_error(
                        compiled,
                        output,
                        stack,
                        state,
                        span,
                        format!(
                            "E_PHP_VM_PROPERTY_ASSIGN_NON_OBJECT: cannot assign property {property} on {}",
                            value_type_name(&other)
                        ),
                    ));
                }
            },
            Value::Callable(_) => {
                self.record_counter_dense_property_fallback("dynamic_property");
                return Err(self.dense_runtime_error(
                    compiled,
                    output,
                    stack,
                    state,
                    span,
                    format!(
                        "E_PHP_VM_DYNAMIC_PROPERTY_ERROR: Cannot create dynamic property Closure::${property}"
                    ),
                ));
            }
            other => {
                self.record_counter_dense_property_fallback("non_object");
                return Err(self.dense_runtime_error(
                    compiled,
                    output,
                    stack,
                    state,
                    span,
                    format!(
                        "E_PHP_VM_PROPERTY_ASSIGN_NON_OBJECT: cannot assign property {property} on {}",
                        value_type_name(&other)
                    ),
                ));
            }
        };

        if spl_array_object_uses_array_as_props(&object) {
            // ARRAY_AS_PROPS routes property writes into the container's
            // array storage; the rich arms take the same branch before any
            // declared/dynamic property handling.
            self.record_counter_dense_property_fallback("spl_array_as_props");
            if let Err(message) = spl_container_offset_set(
                &object,
                Value::String(PhpString::from_test_str(property)),
                value.clone(),
            ) {
                return Err(self.dense_runtime_error(compiled, output, stack, state, span, message));
            }
            return Ok(value);
        }
        let class_handle = object.class_name_handle();
        if is_std_class_runtime_class(&class_handle) {
            self.record_counter_dense_property_fallback("dynamic_property");
            object.set_property(property, value.clone());
            return Ok(value);
        }

        let scope = current_scope_class(compiled, stack);
        let normalized_scope = scope.as_deref().map(normalize_class_name);
        let receiver_class = normalize_class_name(&class_handle);
        let lookup_epoch = state.lookup_epoch();

        let cached_target = if let Some(id) = cache_id {
            self.lookup_dense_property_assign_inline_cache(
                id,
                function_id,
                instruction_id,
                property,
                &receiver_class,
                normalized_scope.as_deref(),
                lookup_epoch,
            )
        } else {
            self.observe_dense_property_inline_cache(
                compiled,
                function_id,
                block_id,
                instruction_id,
                InlineCacheKind::PropertyAssign,
            );
            self.lookup_property_assign_inline_cache(
                compiled,
                function_id,
                block_id,
                instruction_id,
                property,
                &receiver_class,
                normalized_scope.as_deref(),
                lookup_epoch,
            )
        };
        if let Some(target) = cached_target {
            match self.write_property_assign_target(
                compiled,
                target,
                &object,
                value.clone(),
                stack,
                state,
            ) {
                Ok(PropertyAssignCacheWrite::Written(value)) => {
                    self.record_counter_dense_property_ic_reuse();
                    self.record_counter_dense_property_assignment_hit();
                    return Ok(value);
                }
                Ok(PropertyAssignCacheWrite::Fallback) => {
                    self.record_counter_dense_property_fallback("inline_cache_guard");
                }
                Err(message) => {
                    return Err(
                        self.dense_runtime_error(compiled, output, stack, state, span, message)
                    );
                }
            }
        }

        // Same as the fetch arm: the class entry and magic-set flag only
        // matter on the resolve/miss path.
        let class = match lookup_class_in_state(compiled, state, &class_handle) {
            Some(class) => class,
            None => {
                self.record_counter_dense_property_fallback("receiver_class_missing");
                return Err(self.dense_runtime_error(
                    compiled,
                    output,
                    stack,
                    state,
                    span,
                    format!("E_PHP_VM_UNKNOWN_CLASS: class {class_handle} is not defined"),
                ));
            }
        };
        let receiver_has_magic_set = class_has_public_magic_set(compiled, &class);
        let resolved = match lookup_resolved_property_in_state(
            compiled,
            state,
            &class,
            property,
            scope.as_deref(),
        ) {
            Ok(Some(resolved)) => resolved,
            Ok(None) => {
                self.record_counter_dense_property_fallback("dynamic_property");
                match self.call_magic_property_method(
                    compiled,
                    object.clone(),
                    "__set",
                    property,
                    vec![
                        CallArgument::positional(Value::String(PhpString::from_test_str(property))),
                        CallArgument::positional(value.clone()),
                    ],
                    output,
                    stack,
                    state,
                ) {
                    Ok(Some(_)) => {
                        self.record_counter_dense_property_fallback("magic_set");
                        return Ok(value);
                    }
                    Ok(None) => {}
                    Err(result) => return Err(result),
                }
                if let Some(diagnostic) = dynamic_property_deprecation_diagnostic(
                    compiled, state, &class, &object, property, stack,
                ) {
                    state.diagnostics.push(diagnostic);
                }
                object.set_property(property, value.clone());
                return Ok(value);
            }
            Err(message) => {
                return Err(self.dense_runtime_error(compiled, output, stack, state, span, message));
            }
        };
        let resolved_class = &resolved.class;
        let resolved_property = &resolved.property;

        if resolved.property.flags.is_static {
            self.record_counter_dense_property_fallback("static_property");
            emit_static_property_as_non_static_notice(
                compiled,
                output,
                stack,
                state,
                resolved_class,
                resolved_property,
                span,
            );
        }

        if let Err(message) = validate_property_access_in_state(
            compiled,
            state,
            stack,
            resolved_class,
            resolved_property,
        )
        .and_then(|()| {
            validate_property_set_access_in_state(
                compiled,
                state,
                stack,
                resolved_class,
                resolved_property,
            )
        }) {
            self.record_counter_dense_property_fallback("visibility_mismatch");
            match self.call_magic_property_method(
                compiled,
                object.clone(),
                "__set",
                property,
                vec![
                    CallArgument::positional(Value::String(PhpString::from_test_str(property))),
                    CallArgument::positional(value.clone()),
                ],
                output,
                stack,
                state,
            ) {
                Ok(Some(_)) => {
                    self.record_counter_dense_property_fallback("magic_set");
                    return Ok(value);
                }
                Ok(None) => {
                    return Err(
                        self.dense_runtime_error(compiled, output, stack, state, span, message)
                    );
                }
                Err(result) => return Err(result),
            }
        }

        let property_type = ir_runtime_type(resolved.property.type_.as_ref());
        if let Err(message) = check_property_type(
            compiled,
            Some(state),
            resolved_class.display_name.as_str(),
            property,
            &property_type,
            &value,
            self.typecheck_fast_path_context(),
        ) {
            self.record_counter_dense_property_fallback("typed_property_validation");
            return Err(self.dense_runtime_error(compiled, output, stack, state, span, message));
        }
        if let Err(message) =
            validate_property_write(resolved_class, resolved_property, &object, stack, compiled)
        {
            self.record_counter_dense_property_fallback("readonly_or_init_only");
            return Err(self.dense_runtime_error(compiled, output, stack, state, span, message));
        }
        if !property_hook_is_active(state, &object, resolved_class, resolved_property)
            && let Some(function) = resolved.property.hooks.set
        {
            self.record_counter_dense_property_fallback("property_hook");
            self.call_property_hook(
                compiled,
                object.clone(),
                resolved_class,
                resolved_property,
                function,
                vec![CallArgument::positional(value.clone())],
                output,
                stack,
                state,
            )?;
            return Ok(value);
        }
        if !resolved.property.hooks.backed
            && (resolved.property.hooks.get.is_some() || resolved.property.hooks.set.is_some())
        {
            self.record_counter_dense_property_fallback("property_hook");
            return Err(self.dense_runtime_error(
                compiled,
                output,
                stack,
                state,
                span,
                format!(
                    "E_PHP_VM_VIRTUAL_PROPERTY_WRITE: property {}::${} has no backing storage",
                    resolved_class.name, resolved_property.name
                ),
            ));
        }

        let storage_name = property_storage_name(resolved_class, resolved_property);
        if matches!(
            object.get_property(&storage_name),
            Some(Value::Reference(_))
        ) {
            self.record_counter_dense_property_fallback("reference_slot");
        }
        write_property_storage_value(&object, &storage_name, value.clone());
        self.maybe_install_property_assign_inline_cache_target(
            compiled,
            function_id,
            block_id,
            instruction_id,
            property,
            &receiver_class,
            &class,
            resolved_class,
            resolved_property,
            &storage_name,
            normalized_scope.as_deref(),
            lookup_epoch,
            receiver_has_magic_set,
            state,
            &object,
            cache_id,
        );
        self.record_counter_dense_property_assignment_hit();
        Ok(value)
    }

    fn try_dense_array_fetch_dim_borrowed(
        &self,
        array: &Value,
        key_value: &Value,
        quiet: bool,
    ) -> Option<Value> {
        let Value::Array(array) = array else {
            return None;
        };
        let Ok(key) = array_key_from_value(key_value) else {
            return None;
        };
        let result = match array.get(&key) {
            Some(value) => {
                let _source = layout_source::enter(layout_source::ARRAY_ELEMENT_READ);
                Some(effective_value(value))
            }
            None if quiet => Some(Value::Null),
            None => None,
        };
        if result.is_some()
            && self.options.collect_counters
            && let Some(counters) = self.counters.borrow_mut().as_mut()
        {
            counters.record_array_read_borrow_hit();
        }
        result
    }

    fn read_dense_dim_operands(
        &self,
        compiled: &CompiledUnit,
        stack: &CallStack,
        dims: &[DenseOperand],
    ) -> Result<Vec<ArrayKey>, String> {
        dims.iter()
            .map(|operand| {
                self.read_dense_operand(compiled, stack, *operand)
                    .and_then(|value| array_key_from_value(&value))
            })
            .collect()
    }

    fn read_dense_dim_values(
        &self,
        compiled: &CompiledUnit,
        stack: &CallStack,
        dims: &[DenseOperand],
    ) -> Result<Vec<Value>, String> {
        dims.iter()
            .map(|operand| self.read_dense_operand(compiled, stack, *operand))
            .collect()
    }

    /// Dense mirror of `evaluate_closure_captures`: by-ref captures bind the
    /// enclosing local's reference cell, by-value captures read the operand.
    fn evaluate_dense_closure_captures(
        &self,
        dense: &DenseBytecodeUnit,
        compiled: &CompiledUnit,
        stack: &mut CallStack,
        captures: &[DenseClosureCapture],
    ) -> Result<Vec<ClosureCaptureValue>, String> {
        let mut values = Vec::with_capacity(captures.len());
        for capture in captures {
            let name = dense
                .names
                .get(capture.name as usize)
                .ok_or_else(|| format!("invalid dense bytecode capture name n{}", capture.name))?;
            if capture.by_ref {
                if capture.src.kind != DenseOperandKind::Local {
                    return Err(format!(
                        "E_PHP_VM_BY_REF_CAPTURE_NOT_REFERENCEABLE: closure capture ${name} is not a local variable"
                    ));
                }
                let _source = layout_source::enter(layout_source::CLOSURE_CAPTURE_BINDING);
                let cell = stack
                    .current_mut()
                    .ok_or("no active frame")?
                    .locals
                    .ensure_reference_cell(LocalId::new(capture.src.index))?;
                values.push(ClosureCaptureValue::by_reference(name.clone(), cell));
                continue;
            }
            values.push(ClosureCaptureValue::by_value(
                name.clone(),
                self.read_dense_operand(compiled, stack, capture.src)?,
            ));
        }
        Ok(values)
    }

    fn read_dense_call_args(
        &self,
        dense: &DenseBytecodeUnit,
        compiled: &CompiledUnit,
        stack: &mut CallStack,
        args: &[DenseCallArg],
    ) -> Result<Vec<CallArgument>, String> {
        self.read_dense_call_args_with_value_policy(dense, compiled, stack, args, |_, _| false)
    }

    fn read_dense_call_args_for_function(
        &self,
        dense: &DenseBytecodeUnit,
        compiled: &CompiledUnit,
        stack: &mut CallStack,
        function: &str,
        args: &[DenseCallArg],
    ) -> Result<Vec<CallArgument>, String> {
        self.read_dense_call_args_with_value_policy(dense, compiled, stack, args, |index, arg| {
            is_quiet_dense_by_ref_internal_builtin_arg(dense, function, index, arg)
        })
    }

    fn read_dense_call_args_with_value_policy(
        &self,
        dense: &DenseBytecodeUnit,
        compiled: &CompiledUnit,
        stack: &mut CallStack,
        args: &[DenseCallArg],
        mut use_null_placeholder: impl FnMut(usize, &DenseCallArg) -> bool,
    ) -> Result<Vec<CallArgument>, String> {
        let mut out = Vec::with_capacity(args.len());
        for (index, arg) in args.iter().enumerate() {
            let value = if use_null_placeholder(index, arg) {
                Value::Null
            } else {
                let value = self.read_dense_operand_with_source(
                    compiled,
                    stack,
                    arg.value,
                    layout_source::CALL_ARGUMENT_SNAPSHOT,
                )?;
                self.record_counter_value_clone_reason(
                    layout_source::CALL_ARGUMENT_SNAPSHOT.name(),
                );
                value
            };
            let by_ref_dim = arg
                .by_ref_dim
                .as_ref()
                .map(|target| {
                    self.read_dense_dim_operands(compiled, stack, &target.dims)
                        .map(|dims| CallDimTarget {
                            local: LocalId::new(target.local),
                            dims,
                        })
                })
                .transpose()?;
            let by_ref_property = arg
                .by_ref_property
                .as_ref()
                .map(
                    |target| match self.read_dense_operand(compiled, stack, target.object)? {
                        Value::Object(object) => Ok(CallPropertyTarget {
                            object,
                            property: dense
                                .names
                                .get(target.property as usize)
                                .ok_or_else(|| {
                                    format!(
                                        "invalid dense bytecode property name n{}",
                                        target.property
                                    )
                                })?
                                .clone(),
                        }),
                        other => Err(format!(
                            "E_PHP_VM_BY_REF_PROPERTY_NON_OBJECT: cannot bind property n{} on {}",
                            target.property,
                            value_type_name(&other)
                        )),
                    },
                )
                .transpose()?;
            let by_ref_property_dim = arg
                .by_ref_property_dim
                .as_ref()
                .map(
                    |target| match self.read_dense_operand(compiled, stack, target.object)? {
                        Value::Object(object) => {
                            let dims =
                                self.read_dense_dim_operands(compiled, stack, &target.dims)?;
                            Ok(CallPropertyDimTarget {
                                object,
                                property: dense
                                    .names
                                    .get(target.property as usize)
                                    .ok_or_else(|| {
                                        format!(
                                            "invalid dense bytecode property name n{}",
                                            target.property
                                        )
                                    })?
                                    .clone(),
                                dims,
                            })
                        }
                        other => Err(format!(
                            "E_PHP_VM_BY_REF_PROPERTY_DIM_NON_OBJECT: cannot bind property dimension n{} on {}",
                            target.property,
                            value_type_name(&other)
                        )),
                    },
                )
                .transpose()?;
            out.push(CallArgument {
                name: arg
                    .name
                    .map(|name| {
                        dense
                            .names
                            .get(name as usize)
                            .cloned()
                            .ok_or_else(|| format!("invalid dense bytecode argument name n{name}"))
                    })
                    .transpose()?,
                value,
                value_kind: arg.value_kind,
                by_ref_local: arg.by_ref_local.map(LocalId::new),
                by_ref_dim,
                by_ref_property,
                by_ref_property_dim,
            });
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_array_offset_on_scalar_warning(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
        base: &Value,
        span: IrSpan,
    ) -> Result<(), VmResult> {
        let diagnostic = array_offset_on_scalar_warning(
            base,
            runtime_source_span(compiled, span),
            stack_trace(compiled, stack),
        );
        match self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::PHP_E_WARNING,
            &diagnostic,
        ) {
            Ok(false) if error_reporting_allows(state, php_runtime::PHP_E_WARNING) => {
                emit_vm_diagnostic(
                    output,
                    state,
                    &diagnostic,
                    php_runtime::PhpDiagnosticChannel::Warning,
                    php_runtime::PHP_E_WARNING,
                );
                diagnostics.push(diagnostic);
            }
            Ok(_) => {}
            Err(result) => return Err(result),
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn fetch_dim_value(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
        array: &Value,
        key_value: &Value,
        quiet: bool,
        span: IrSpan,
    ) -> Result<Value, VmResult> {
        let base = effective_value(array);
        if let Value::Object(object) = &base
            && spl_runtime_marker(object)
                .is_some_and(|class| is_spl_array_access_runtime_class(&class))
        {
            return self.call_array_access_dim_method(
                compiled,
                object.clone(),
                "offsetGet",
                key_value.clone(),
                Some(span),
                output,
                stack,
                state,
            );
        }
        if let Some(object) = match userland_arrayaccess_object(compiled, state, &base) {
            Ok(object) => object,
            Err(message) => {
                return Err(self.runtime_error(output, compiled, stack, message));
            }
        } {
            return self.call_userland_arrayaccess_method(
                compiled,
                output,
                stack,
                state,
                object,
                "offsetGet",
                vec![CallArgument::positional(key_value.clone())],
                span,
            );
        }
        let key = array_key_from_value(key_value)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        if let Value::Object(object) = &base
            && normalize_class_name(&object.class_name()) == "simplexmlelement"
        {
            return Ok(php_runtime::xml::simplexml_dimension(object, &key));
        }
        if let Value::String(string) = &base {
            return match string_offset_for_read(string, &key) {
                StringOffsetRead::Byte(value) => Ok(value),
                StringOffsetRead::Illegal { value, key_bytes } => {
                    if !quiet {
                        let diagnostic = illegal_string_offset_warning(
                            &key_bytes,
                            runtime_source_span(compiled, span),
                            stack_trace(compiled, stack),
                        );
                        match self.dispatch_error_handler(
                            compiled,
                            output,
                            stack,
                            state,
                            php_runtime::PHP_E_WARNING,
                            &diagnostic,
                        ) {
                            Ok(false)
                                if error_reporting_allows(state, php_runtime::PHP_E_WARNING) =>
                            {
                                emit_vm_diagnostic(
                                    output,
                                    state,
                                    &diagnostic,
                                    php_runtime::PhpDiagnosticChannel::Warning,
                                    php_runtime::PHP_E_WARNING,
                                );
                            }
                            Ok(_) => {}
                            Err(result) => return Err(result),
                        }
                    }
                    Ok(value)
                }
                StringOffsetRead::OutOfRange(index) => {
                    if quiet {
                        Ok(Value::Null)
                    } else {
                        let diagnostic = uninitialized_string_offset_warning(
                            index,
                            runtime_source_span(compiled, span),
                            stack_trace(compiled, stack),
                        );
                        match self.dispatch_error_handler(
                            compiled,
                            output,
                            stack,
                            state,
                            php_runtime::PHP_E_WARNING,
                            &diagnostic,
                        ) {
                            Ok(false)
                                if error_reporting_allows(state, php_runtime::PHP_E_WARNING) =>
                            {
                                emit_vm_diagnostic(
                                    output,
                                    state,
                                    &diagnostic,
                                    php_runtime::PhpDiagnosticChannel::Warning,
                                    php_runtime::PHP_E_WARNING,
                                );
                            }
                            Ok(_) => {}
                            Err(result) => return Err(result),
                        }
                        Ok(Value::string(Vec::new()))
                    }
                }
                StringOffsetRead::NonNumeric => {
                    if quiet {
                        Ok(Value::Null)
                    } else {
                        Err(self.runtime_error_with_source_span(
                            output,
                            compiled,
                            stack,
                            runtime_source_span(compiled, span),
                            "E_PHP_VM_STRING_OFFSET_TYPE: Cannot access offset of type string on string"
                                .to_owned(),
                        ))
                    }
                }
            };
        }
        if quiet && quiet_dim_fetch_scalar_returns_null(&base) {
            return Ok(Value::Null);
        }
        if quiet_dim_fetch_scalar_returns_null(&base) {
            self.emit_array_offset_on_scalar_warning(
                compiled,
                output,
                stack,
                state,
                diagnostics,
                &base,
                span,
            )?;
            return Ok(Value::Null);
        }

        match fetch_dim_value(array, &key) {
            Ok(Some(value)) => Ok(value),
            Ok(None) if quiet => Ok(Value::Null),
            Ok(None) => {
                diagnostics.push(undefined_array_key_warning(
                    &key,
                    runtime_source_span(compiled, span),
                    stack_trace(compiled, stack),
                ));
                Ok(Value::Null)
            }
            Err(message) => Err(self.runtime_error_with_source_span(
                output,
                compiled,
                stack,
                runtime_source_span(compiled, span),
                message,
            )),
        }
    }

    fn call_userland_arrayaccess_method(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        object: ObjectRef,
        method: &str,
        args: Vec<CallArgument>,
        span: IrSpan,
    ) -> Result<Value, VmResult> {
        let result = self.call_object_method_callable(
            compiled,
            object,
            method,
            args,
            Some(span),
            output,
            stack,
            state,
        );
        if !result.status.is_success()
            || result.yielded.is_some()
            || result.fiber_suspension.is_some()
        {
            return Err(result);
        }
        Ok(result.return_value.unwrap_or(Value::Null))
    }

    fn call_array_access_dim_method(
        &self,
        compiled: &CompiledUnit,
        object: ObjectRef,
        method: &str,
        key: Value,
        call_span: Option<IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
        let span = call_span.unwrap_or_default();
        self.call_userland_arrayaccess_method(
            compiled,
            output,
            stack,
            state,
            object,
            method,
            vec![CallArgument::positional(key)],
            span,
        )
    }

    fn try_userland_arrayaccess_offset_set_local(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        local: LocalId,
        dim_values: &[Value],
        append: bool,
        // Borrowed: the common non-ArrayAccess target returns `Ok(false)`
        // without ever needing the value, so the clone happens only on the
        // actual `offsetSet` dispatch below.
        value: &Value,
        span: IrSpan,
    ) -> Result<bool, VmResult> {
        let Some(object) = local_effective_object(stack, local) else {
            return Ok(false);
        };
        let object = match userland_arrayaccess_object_from_object(compiled, state, object) {
            Ok(Some(object)) => object,
            Ok(None) => return Ok(false),
            Err(message) => {
                return Err(self.runtime_error(output, compiled, stack, message));
            }
        };
        let key = if append {
            if !dim_values.is_empty() {
                return Err(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_ARRAYACCESS_NESTED_DIM: nested ArrayAccess writes are not implemented",
                ));
            }
            Value::Null
        } else {
            let [key] = dim_values else {
                return Err(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_ARRAYACCESS_DIM: ArrayAccess writes require exactly one dimension",
                ));
            };
            key.clone()
        };
        self.call_userland_arrayaccess_method(
            compiled,
            output,
            stack,
            state,
            object,
            "offsetSet",
            vec![
                CallArgument::positional(key),
                CallArgument::positional(value.clone()),
            ],
            span,
        )?;
        Ok(true)
    }

    fn try_userland_arrayaccess_offset_set_value(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        base: &Value,
        dims: &[ArrayKey],
        append: bool,
        value: Value,
        span: IrSpan,
    ) -> Result<bool, VmResult> {
        let object = match userland_arrayaccess_object(compiled, state, base) {
            Ok(Some(object)) => object,
            Ok(None) => return Ok(false),
            Err(message) => {
                return Err(self.runtime_error(output, compiled, stack, message));
            }
        };
        let key = if append {
            if !dims.is_empty() {
                return Err(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_ARRAYACCESS_NESTED_DIM: nested ArrayAccess writes are not implemented",
                ));
            }
            Value::Null
        } else {
            let [key] = dims else {
                return Err(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_VM_ARRAYACCESS_DIM: ArrayAccess writes require exactly one dimension",
                ));
            };
            array_key_to_value(key.clone())
        };
        self.call_userland_arrayaccess_method(
            compiled,
            output,
            stack,
            state,
            object,
            "offsetSet",
            vec![
                CallArgument::positional(key),
                CallArgument::positional(value),
            ],
            span,
        )?;
        Ok(true)
    }

    fn try_userland_arrayaccess_offset_exists_local(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        local: LocalId,
        dims: &[ArrayKey],
        span: IrSpan,
    ) -> Result<Option<bool>, VmResult> {
        let Some(local_value) = read_local_value(stack, local) else {
            return Ok(None);
        };
        let object = match userland_arrayaccess_object(compiled, state, &local_value) {
            Ok(Some(object)) => object,
            Ok(None) => return Ok(None),
            Err(message) => {
                return Err(self.runtime_error(output, compiled, stack, message));
            }
        };
        self.arrayaccess_dim_isset_value(
            compiled,
            output,
            stack,
            state,
            Value::Object(object),
            dims,
            span,
        )
        .map(Some)
    }

    fn try_userland_arrayaccess_offset_empty_local(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        local: LocalId,
        dims: &[ArrayKey],
        span: IrSpan,
    ) -> Result<Option<bool>, VmResult> {
        let Some(local_value) = read_local_value(stack, local) else {
            return Ok(None);
        };
        let object = match userland_arrayaccess_object(compiled, state, &local_value) {
            Ok(Some(object)) => object,
            Ok(None) => return Ok(None),
            Err(message) => {
                return Err(self.runtime_error(output, compiled, stack, message));
            }
        };
        self.arrayaccess_dim_empty_value(
            compiled,
            output,
            stack,
            state,
            Value::Object(object),
            dims,
            span,
        )
        .map(Some)
    }

    #[allow(clippy::too_many_arguments)]
    fn arrayaccess_dim_isset_value(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        value: Value,
        dims: &[ArrayKey],
        span: IrSpan,
    ) -> Result<bool, VmResult> {
        let Some((first, rest)) = dims.split_first() else {
            return Ok(!matches!(
                effective_value(&value),
                Value::Uninitialized | Value::Null
            ));
        };
        let base = effective_value(&value);
        if let Some(object) = match arrayaccess_object(compiled, state, &base) {
            Ok(object) => object,
            Err(message) => return Err(self.runtime_error(output, compiled, stack, message)),
        } {
            let key_value = array_key_to_value(first.clone());
            let exists = self.call_array_access_dim_method(
                compiled,
                object.clone(),
                "offsetExists",
                key_value.clone(),
                Some(span),
                output,
                stack,
                state,
            )?;
            if !to_bool(&exists)
                .map_err(|message| self.runtime_error(output, compiled, stack, message))?
            {
                return Ok(false);
            }
            if rest.is_empty() {
                return Ok(true);
            }
            let child = self.call_array_access_dim_method(
                compiled,
                object,
                "offsetGet",
                key_value,
                Some(span),
                output,
                stack,
                state,
            )?;
            return self
                .arrayaccess_dim_isset_value(compiled, output, stack, state, child, rest, span);
        }
        let value = fetch_dim_path_value(&base, dims).ok().flatten();
        Ok(!matches!(value, None | Some(Value::Null)))
    }

    #[allow(clippy::too_many_arguments)]
    fn arrayaccess_dim_empty_value(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        value: Value,
        dims: &[ArrayKey],
        span: IrSpan,
    ) -> Result<bool, VmResult> {
        let Some((first, rest)) = dims.split_first() else {
            return php_empty(&value)
                .map_err(|message| self.runtime_error(output, compiled, stack, message));
        };
        let base = effective_value(&value);
        if let Some(object) = match arrayaccess_object(compiled, state, &base) {
            Ok(object) => object,
            Err(message) => return Err(self.runtime_error(output, compiled, stack, message)),
        } {
            let key_value = array_key_to_value(first.clone());
            let exists = self.call_array_access_dim_method(
                compiled,
                object.clone(),
                "offsetExists",
                key_value.clone(),
                Some(span),
                output,
                stack,
                state,
            )?;
            if !to_bool(&exists)
                .map_err(|message| self.runtime_error(output, compiled, stack, message))?
            {
                return Ok(true);
            }
            let child = self.call_array_access_dim_method(
                compiled,
                object,
                "offsetGet",
                key_value,
                Some(span),
                output,
                stack,
                state,
            )?;
            if rest.is_empty() {
                return php_empty_access_value(&child)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message));
            }
            return self
                .arrayaccess_dim_empty_value(compiled, output, stack, state, child, rest, span);
        }
        let value = fetch_dim_path_value(&base, dims)
            .ok()
            .flatten()
            .unwrap_or(Value::Uninitialized);
        php_empty_access_value(&value)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))
    }

    fn try_userland_arrayaccess_offset_unset_local(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        local: LocalId,
        dims: &[ArrayKey],
        span: IrSpan,
    ) -> Result<bool, VmResult> {
        let Some(local_value) = read_local_value(stack, local) else {
            return Ok(false);
        };
        let object = match userland_arrayaccess_object(compiled, state, &local_value) {
            Ok(Some(object)) => object,
            Ok(None) => return Ok(false),
            Err(message) => {
                return Err(self.runtime_error(output, compiled, stack, message));
            }
        };
        let [key] = dims else {
            return Err(self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_ARRAYACCESS_NESTED_DIM: nested ArrayAccess unset is not implemented",
            ));
        };
        self.call_userland_arrayaccess_method(
            compiled,
            output,
            stack,
            state,
            object,
            "offsetUnset",
            vec![CallArgument::positional(array_key_to_value(key.clone()))],
            span,
        )?;
        Ok(true)
    }

    #[cold]
    #[inline(never)]
    fn invalid_bytecode_operand_shape(
        &self,
        output: &mut OutputBuffer,
        compiled: &CompiledUnit,
        stack: &CallStack,
        instruction: &DenseInstruction,
    ) -> VmResult {
        self.runtime_error(
            output,
            compiled,
            stack,
            format!(
                "E_PHP_VM_DENSE_BYTECODE_OPERAND_SHAPE: opcode {} has invalid operand payload",
                instruction.opcode.as_str()
            ),
        )
    }

    fn record_tiering_backedge(&self, function_id: FunctionId, current: BlockId, target: BlockId) {
        self.tiering
            .borrow_mut()
            .record_loop_backedge(function_id, current, target);
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
        match state.class_relation_cache.lookup(&key, epochs) {
            ClassRelationCacheLookup::Hit(target) => {
                self.record_counter_class_relation_cache_hit();
                self.record_counter_instanceof_cache_hit();
                return Ok(target.matches);
            }
            ClassRelationCacheLookup::Invalidated => {
                self.record_counter_class_relation_cache_invalidation();
                self.record_counter_instanceof_cache_miss();
            }
            ClassRelationCacheLookup::Miss => {
                self.record_counter_class_relation_cache_miss();
                self.record_counter_instanceof_cache_miss();
            }
        }
        let matches = object_instanceof_in_state(compiled, state, value, class_name)?;
        state.class_relation_cache.install(
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
    ) -> Result<(), VmResult> {
        for diagnostic in diagnostics {
            let (level, channel) = match diagnostic.severity() {
                RuntimeSeverity::Warning => (
                    php_runtime::PHP_E_WARNING,
                    php_runtime::PhpDiagnosticChannel::Warning,
                ),
                RuntimeSeverity::Deprecation => (
                    php_runtime::PHP_E_DEPRECATED,
                    php_runtime::PhpDiagnosticChannel::Deprecated,
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
        compiled: &CompiledUnit,
        class_name: &str,
        method: &str,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        allow_by_ref_value_warnings: bool,
        by_ref_warning_callable_name: Option<String>,
    ) -> VmResult {
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
                return result;
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
            compiled,
            class_name,
            call_span.unwrap_or_default(),
            None,
            output,
            stack,
            state,
        ) {
            return result;
        }
        let class = match resolve_static_class_name(compiled, state, stack, class_name) {
            Ok(class) => class,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        if let Err(result) =
            self.autoload_class_parents_if_missing(compiled, &class, output, stack, state)
        {
            return result;
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
                return result;
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
                    compiled,
                    &class,
                    "__callStatic",
                    method,
                    args,
                    called_class,
                    call_span,
                    output,
                    stack,
                    state,
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
                    Err(result) => result,
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
                compiled,
                &class,
                "__callStatic",
                method,
                args,
                called_class,
                call_span,
                output,
                stack,
                state,
            ) {
                Ok(Some(result)) => result,
                Ok(None) => self.runtime_error(output, compiled, stack, inaccessible),
                Err(result) => result,
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

#[derive(Clone, Debug)]
struct RuntimeClassEntryError {
    message: String,
    constant_initializer_span: Option<IrSpan>,
}

impl RuntimeClassEntryError {
    fn new(message: String) -> Self {
        Self {
            message,
            constant_initializer_span: None,
        }
    }

    fn with_constant_initializer_span(message: String, span: IrSpan) -> Self {
        Self {
            message,
            constant_initializer_span: Some(span),
        }
    }

    fn into_message(self) -> String {
        self.message
    }
}

impl From<String> for RuntimeClassEntryError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<RuntimeClassEntryError> for String {
    fn from(error: RuntimeClassEntryError) -> Self {
        error.into_message()
    }
}

#[derive(Clone, Debug)]
enum PhpTokenStaticMethodError {
    RuntimeClass(RuntimeClassEntryError),
    Runtime(String),
}

impl PhpTokenStaticMethodError {
    fn into_message(self) -> String {
        match self {
            Self::RuntimeClass(error) => error.into_message(),
            Self::Runtime(message) => message,
        }
    }
}

/// Activation-context handles for one class-name spelling; see
/// `Vm::class_name_handles`.
#[derive(Clone, Debug)]
struct ClassNameHandles {
    /// `normalize_class_name` form for scope/declaring-class fields.
    normalized: Arc<str>,
    /// `display_class_name` form for the late-static-binding called class.
    display: Arc<str>,
}

/// Late-static-binding handle for a receiver object. The stored display name
/// already carries the PHP-visible spelling, so the shared handle is reused
/// directly unless a leading root slash still needs stripping.
/// Call-site strictness resolved at the caller: per-file when the site has a
/// span (linked multi-file units mix strict and weak files in one unit), the
/// unit flag otherwise. Spans are unit-local, so this must only ever be
/// called with the unit that produced the span.
fn call_site_strictness(compiled: &CompiledUnit, span: Option<IrSpan>) -> bool {
    span.map_or(compiled.unit().strict_types, |span| {
        compiled.unit().strict_types_for_span(span)
    })
}

fn object_called_class_handle(object: &ObjectRef) -> Arc<str> {
    let display = object.display_name_handle();
    if display.starts_with('\\') {
        Arc::from(display_class_name(&display))
    } else {
        display
    }
}

/// Cache of resolved runtime class entries, keyed by normalized class name and
/// guarded by the class-table epoch. Rebuilding a `RuntimeClassEntry` walks the
/// lineage, evaluates property/constant defaults, and maps every method, so a
/// hot instantiation site would otherwise pay that on every `new`. Entries are
/// shared via `Rc` (the object model copies only the property defaults it needs
/// and never retains the entry); property defaults are scalar/string/array-COW
/// or shared enum-case singletons, so sharing a cached entry is behavior-neutral.
#[derive(Clone, Debug, Default)]
struct RuntimeClassEntryCache {
    epoch: u64,
    entries: HashMap<String, Rc<RuntimeClassEntry>>,
}

/// Cache of resolved raw IR class entries, keyed by normalized class name and
/// guarded by the class-table epoch. `lookup_class_in_state` returns a shared
/// `Arc<ClassEntry>` (a cheap refcount bump), but a hot `new` site still needs an
/// owned `ClassEntry` and would deep-clone the whole class definition out of that
/// `Arc` per instantiation. Sharing the owned entry via `Rc` is behavior-neutral:
/// within a class-table epoch a class definition is immutable (redeclaration is a
/// fatal), and the cache is dropped when the epoch changes.
#[derive(Clone, Debug, Default)]
struct IrClassEntryCache {
    epoch: u64,
    entries: HashMap<String, Rc<php_ir::module::ClassEntry>>,
}

/// Cache of default declared-slot templates, keyed by normalized class name and
/// guarded by the class-table epoch. Each template is the slot-index-aligned
/// default vector a fresh instance starts from; cloning it into a new object
/// skips the per-property iterate + filter + `slot_by_name` hash-lookup loop
/// that `ObjectRef::new_with_display_name` runs. Sharing it via `Rc` is
/// behavior-neutral within a class-table epoch (a class definition is immutable
/// until redeclaration, which bumps the epoch and drops the cache), and the
/// template is byte-identical to the defaults the slow path builds.
#[derive(Clone, Debug, Default)]
struct DefaultSlotTemplateCache {
    epoch: u64,
    entries: HashMap<String, Rc<Vec<Option<Value>>>>,
}

/// Cache of `__construct` resolution outcomes, keyed by (normalized class name,
/// normalized caller scope) and guarded by the class-table epoch. Resolving a
/// constructor runs a private-scope probe plus the full inheritance + visibility
/// walk (`lookup_resolved_method_in_state`); a hot instantiation site would
/// otherwise pay that on every `new`. Caching the whole `Result` — `Ok(Some)`,
/// `Ok(None)`, and `Err` alike — reproduces the exact outcome on a hit (a
/// visibility/cycle diagnostic is replayed, never re-walked or bypassed). The
/// caller scope is part of the key because private/protected resolution differs
/// by calling scope. Cloning the cached `ResolvedMethodOwned` bumps the shared
/// `Arc<ClassEntry>` and clones the resolved method entry, which is exactly what
/// the walk's terminal step does, so a hit is behavior-neutral; the cache is
/// dropped when the epoch changes.
#[derive(Clone, Debug, Default)]
struct ConstructorResolutionCache {
    epoch: u64,
    entries: HashMap<(String, Option<String>), Result<Option<ResolvedMethodOwned>, String>>,
}

fn is_reflection_runtime_class(name: &str) -> bool {
    let name = normalize_class_name(name);
    [
        "reflectionclass",
        "reflectionfunction",
        "reflectionmethod",
        "reflectionproperty",
        "reflectionclassconstant",
        "reflectionenum",
        "reflectionenumunitcase",
        "reflectionenumbackedcase",
        "reflectionparameter",
        "reflectionattribute",
        "reflectionnamedtype",
        "reflectionextension",
    ]
    .contains(&name.as_str())
}

pub(crate) fn normalize_function_name(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}

fn normalize_stream_wrapper_protocol(protocol: &str) -> String {
    protocol.trim().trim_end_matches("://").to_ascii_lowercase()
}

fn stream_uri_protocol(uri: &str) -> Option<String> {
    uri.find("://")
        .map(|index| normalize_stream_wrapper_protocol(&uri[..index]))
        .filter(|protocol| !protocol.is_empty())
}

fn normalize_exit_code(code: i64) -> i32 {
    code.clamp(0, 255) as i32
}

fn script_exit_result(output: &OutputBuffer, state: &ExecutionState, code: i32) -> VmResult {
    VmResult::script_exit(
        output.clone(),
        code,
        state.builtins.pcntl_state.is_fork_child(),
    )
}

fn compiled_unit_cache_key(compiled: &CompiledUnit) -> u64 {
    compiled.cache_identity()
}

/// True when cloning `value` would allocate or bump a refcount (a refcounted
/// heap value), i.e. a last-use move genuinely avoided clone work. Scalars are
/// `Copy`-like and moving them saves nothing observable.
fn value_clone_is_heap(value: &Value) -> bool {
    matches!(
        value,
        Value::String(_)
            | Value::Array(_)
            | Value::Object(_)
            | Value::Resource(_)
            | Value::Fiber(_)
            | Value::Generator(_)
            | Value::Callable(_)
            | Value::Reference(_)
    )
}

fn instruction_runtime_error_context(
    message: String,
    unit: &IrUnit,
    function: &IrFunction,
    block_id: BlockId,
    instruction_index: usize,
    instruction: &Instruction,
) -> String {
    let source = unit
        .files
        .first()
        .map(|file| file.path.as_str())
        .unwrap_or("<unknown>");
    format!(
        "{message} in {source} function={} block:{} instruction:{} {:?} constants={}",
        function.name,
        block_id.raw(),
        instruction_index,
        instruction.kind,
        unit.constants.len()
    )
}

#[cfg(feature = "jit-cranelift")]
fn jit_compile_cache_key(
    function_id: FunctionId,
    function: &IrFunction,
    options: &VmOptions,
) -> JitCompileCacheKey {
    JitCompileCacheKey {
        function: function_id.raw(),
        ir_fingerprint: stable_hash_bytes(format!("{function:?}").as_bytes()),
        abi_hash: php_jit::JIT_RUNTIME_ABI_HASH,
        config_hash: jit_config_hash(options),
        target_isa: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
    }
}

#[cfg(feature = "jit-cranelift")]
fn jit_config_hash(options: &VmOptions) -> u64 {
    let config = format!(
        "jit={};quickening={};inline_caches={};blacklist={};typecheck={};threshold={};loop_threshold={}",
        options.jit.as_str(),
        options.quickening.enabled(),
        options.inline_caches.enabled(),
        options.jit_blacklist.as_str(),
        options.typecheck_fast_paths,
        options.tiering.function_entry_threshold,
        options.tiering.loop_backedge_threshold
    );
    stable_hash_bytes(config.as_bytes())
}

#[cfg(feature = "jit-cranelift")]
fn stable_hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn empty_array_value() -> Value {
    Value::Array(PhpArray::new())
}

fn enum_case_object(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    class: &php_ir::module::ClassEntry,
    case: &php_ir::module::ClassEnumCaseEntry,
    constant_value: &impl Fn(ConstId) -> Result<Value, String>,
) -> Result<ObjectRef, String> {
    let key = (
        normalize_class_name(&class.name),
        case.name.to_ascii_lowercase(),
    );
    if let Some(object) = state.enum_cases.get(&key) {
        return Ok(object.clone());
    }
    let runtime_class = runtime_class_entry(
        compiled,
        state,
        class,
        constant_value,
        &|reference| class_constant_reference_value(compiled, state, reference),
        &|reference| named_constant_reference_value(compiled, state, reference),
    )
    .map_err(RuntimeClassEntryError::into_message)?;
    let object = ObjectRef::new_with_display_name(&runtime_class, class.display_name.clone());
    object.set_property("name", Value::String(PhpString::from_test_str(&case.name)));
    if runtime_class.enum_backing_type.is_some() {
        let value = case.value.map(constant_value).transpose()?.ok_or_else(|| {
            format!(
                "E_PHP_VM_ENUM_CASE_MISSING_VALUE: backed enum case {}::{} has no value",
                class.name, case.name
            )
        })?;
        object.set_property("value", value);
    }
    state.enum_cases.insert(key, object.clone());
    Ok(object)
}

fn enum_static_method(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    class: &php_ir::module::ClassEntry,
    method: &str,
    args: Vec<CallArgument>,
    constant_value: &impl Fn(ConstId) -> Result<Value, String>,
) -> Result<Value, String> {
    match normalize_method_name(method).as_str() {
        "cases" => {
            if !args.is_empty() {
                return Err(format!(
                    "E_PHP_VM_TOO_MANY_ARGS: enum {}::cases expects no arguments",
                    class.name
                ));
            }
            let mut array = PhpArray::new();
            for case in &class.enum_cases {
                array.append(Value::Object(enum_case_object(
                    compiled,
                    state,
                    class,
                    case,
                    constant_value,
                )?));
            }
            Ok(Value::Array(array))
        }
        "from" | "tryfrom" => {
            enum_backed_lookup(compiled, state, class, method, args, constant_value)
        }
        _ => unreachable!("enum_static_method called for non-enum method"),
    }
}

fn enum_backed_lookup(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    class: &php_ir::module::ClassEntry,
    method: &str,
    args: Vec<CallArgument>,
    constant_value: &impl Fn(ConstId) -> Result<Value, String>,
) -> Result<Value, String> {
    let normalized_method = normalize_method_name(method);
    if args.len() != 1 {
        return Err(format!(
            "E_PHP_VM_ENUM_LOOKUP_ARITY: enum {}::{} expects exactly one argument",
            class.name, method
        ));
    }
    if class.enum_backing_type.is_none() {
        return Err(format!(
            "E_PHP_VM_ENUM_LOOKUP_ON_UNIT_ENUM: enum {} has no backing values",
            class.name
        ));
    }
    let needle = &args[0].value;
    for case in &class.enum_cases {
        let Some(value_id) = case.value else {
            continue;
        };
        let value = constant_value(value_id)?;
        if identical(&value, needle) {
            return Ok(Value::Object(enum_case_object(
                compiled,
                state,
                class,
                case,
                constant_value,
            )?));
        }
    }
    if normalized_method == "tryfrom" {
        Ok(Value::Null)
    } else {
        Err(format!(
            "E_PHP_VM_ENUM_VALUE_ERROR: value is not a valid backing value for enum {}",
            class.name
        ))
    }
}

fn callable_resolve_reference(value: Value) -> Value {
    match value {
        Value::Reference(cell) => callable_resolve_reference(cell.get()),
        value => value,
    }
}

fn callable_string_value(value: Value) -> Option<String> {
    match callable_resolve_reference(value) {
        Value::String(value) => Some(value.to_string_lossy()),
        _ => None,
    }
}

fn callable_string_ref(value: &Value) -> Option<String> {
    match value {
        Value::Reference(cell) => callable_string_value(cell.get()),
        Value::String(value) => Some(value.to_string_lossy()),
        _ => None,
    }
}

fn magic_args_array(args: Vec<CallArgument>) -> Value {
    let mut array = PhpArray::new();
    for arg in args {
        if let Some(name) = arg.name {
            array.insert(ArrayKey::String(PhpString::from_test_str(&name)), arg.value);
        } else {
            array.append(arg.value);
        }
    }
    Value::Array(array)
}

fn debug_info_object(source: &ObjectRef, properties: PhpArray) -> ObjectRef {
    let properties = properties
        .iter()
        .map(|(key, value)| match key {
            ArrayKey::Int(index) => (index.to_string(), index.to_string(), value.clone()),
            ArrayKey::String(name) => {
                let name = name.to_string_lossy();
                (name.clone(), format!("\"{name}\""), value.clone())
            }
        })
        .collect();
    ObjectRef::debug_view_with_properties(source, properties)
}

fn spl_internal_debug_info_object(source: &ObjectRef) -> Option<ObjectRef> {
    let runtime_class =
        spl_runtime_marker(source).unwrap_or_else(|| normalize_class_name(&source.class_name()));
    match runtime_class.as_str() {
        "hashcontext" => hash_context_debug_info_object(source),
        "arrayiterator" | "recursivearrayiterator" => {
            let storage =
                spl_entries_to_debug_php_array_excluding(spl_entries(source), source.id());
            Some(ObjectRef::debug_view_with_properties(
                source,
                vec![(
                    "storage".to_owned(),
                    "\"storage\":\"ArrayIterator\":private".to_owned(),
                    Value::Array(storage),
                )],
            ))
        }
        "arrayobject" => {
            let storage =
                spl_entries_to_debug_php_array_excluding(spl_entries(source), source.id());
            Some(ObjectRef::debug_view_with_properties(
                source,
                vec![(
                    "storage".to_owned(),
                    "\"storage\":\"ArrayObject\":private".to_owned(),
                    Value::Array(storage),
                )],
            ))
        }
        "splfixedarray" => Some(debug_info_object(
            source,
            spl_entries_to_debug_php_array_excluding(spl_entries(source), source.id()),
        )),
        "spldoublylinkedlist" | "splstack" | "splqueue" => Some(
            spl_doubly_linked_list_debug_info_object(source, &runtime_class),
        ),
        "splobjectstorage" => Some(ObjectRef::debug_view_with_properties(
            source,
            vec![(
                "storage".to_owned(),
                "\"storage\":\"SplObjectStorage\":private".to_owned(),
                Value::Array(spl_object_storage_debug_records_array(source)),
            )],
        )),
        "splheap" | "splmaxheap" | "splminheap" | "splpriorityqueue" => {
            Some(spl_heap_debug_info_object(source, &runtime_class))
        }
        "splfileinfo" | "splfileobject" | "spltempfileobject" => {
            Some(spl_file_info_debug_info_object(source))
        }
        "phar" => Some(phar_debug_info_object(source)),
        "ziparchive" => Some(zip_archive_debug_info_object(source)),
        _ => None,
    }
}

fn hash_context_debug_info_object(source: &ObjectRef) -> Option<ObjectRef> {
    Some(debug_info_object(
        source,
        hash_context_debug_info_array(source)?,
    ))
}

fn hash_context_debug_info_array(source: &ObjectRef) -> Option<PhpArray> {
    let Some(Value::String(algorithm)) = source.get_property(HASH_CONTEXT_ALGORITHM_PROPERTY)
    else {
        return None;
    };
    let mut properties = PhpArray::new();
    properties.insert(
        ArrayKey::String(PhpString::from_test_str("algo")),
        Value::String(algorithm),
    );
    Some(properties)
}

fn hash_context_method_is_supported(method: &str) -> bool {
    matches!(
        normalize_method_name(method).as_str(),
        "__debuginfo" | "__serialize" | "__unserialize"
    )
}

fn validate_hash_context_arg_count(
    method: &str,
    args: &[CallArgument],
    expected: usize,
) -> Result<(), String> {
    if args.len() == expected {
        return Ok(());
    }
    Err(format!(
        "E_PHP_VM_ARGUMENT_COUNT: HashContext::{method}() expects exactly {expected} argument{}, {} given",
        if expected == 1 { "" } else { "s" },
        args.len()
    ))
}

fn hash_context_runtime_exception(message: impl AsRef<str>) -> String {
    format!("E_PHP_VM_SPL_RUNTIME_EXCEPTION: {}", message.as_ref())
}

fn hash_context_object_is_initialized(object: &ObjectRef) -> bool {
    object
        .get_property(HASH_CONTEXT_ALGORITHM_PROPERTY)
        .is_some()
}

fn hash_context_serialize_array(object: &ObjectRef) -> Result<PhpArray, String> {
    let algorithm = match object.get_property(HASH_CONTEXT_ALGORITHM_PROPERTY) {
        Some(Value::String(algorithm)) => algorithm,
        _ => {
            return Err("E_PHP_VM_INVALID_HASH_CONTEXT: invalid HashContext state".to_owned());
        }
    };
    let flags = match object.get_property(HASH_CONTEXT_FLAGS_PROPERTY) {
        Some(Value::Int(flags)) => flags,
        _ => {
            return Err("E_PHP_VM_INVALID_HASH_CONTEXT: invalid HashContext state".to_owned());
        }
    };
    if flags & HASH_HMAC_FLAG != 0 {
        return Err(hash_context_runtime_exception(
            "HashContext with HASH_HMAC option cannot be serialized",
        ));
    }
    if matches!(
        object.get_property(HASH_CONTEXT_FINALIZED_PROPERTY),
        Some(Value::Bool(true))
    ) {
        return Err(hash_context_runtime_exception(format!(
            "HashContext for algorithm \"{}\" cannot be serialized",
            algorithm.to_string_lossy()
        )));
    }

    let mut internals = PhpArray::new();
    internals.insert(
        ArrayKey::Int(0),
        object
            .get_property(HASH_CONTEXT_DATA_PROPERTY)
            .unwrap_or_else(|| Value::string(Vec::new())),
    );

    let mut payload = PhpArray::new();
    payload.insert(ArrayKey::Int(0), Value::String(algorithm));
    payload.insert(ArrayKey::Int(1), Value::Int(flags));
    payload.insert(ArrayKey::Int(2), Value::Array(internals));
    payload.insert(ArrayKey::Int(3), Value::Int(2));
    payload.insert(ArrayKey::Int(4), Value::Array(PhpArray::new()));
    Ok(payload)
}

fn spl_file_info_debug_info_object(source: &ObjectRef) -> ObjectRef {
    let path = spl_file_path(source);
    ObjectRef::debug_view_with_properties(
        source,
        vec![
            (
                "pathName".to_owned(),
                "\"pathName\":\"SplFileInfo\":private".to_owned(),
                Value::string(path.as_bytes().to_vec()),
            ),
            (
                "fileName".to_owned(),
                "\"fileName\":\"SplFileInfo\":private".to_owned(),
                Value::string(spl_file_basename(&path).into_bytes()),
            ),
        ],
    )
}

fn phar_debug_info_object(source: &ObjectRef) -> ObjectRef {
    ObjectRef::debug_view_with_properties(
        source,
        vec![
            (
                "pathName".to_owned(),
                "\"pathName\":\"SplFileInfo\":private".to_owned(),
                Value::string(Vec::new()),
            ),
            (
                "fileName".to_owned(),
                "\"fileName\":\"SplFileInfo\":private".to_owned(),
                Value::string(Vec::new()),
            ),
            (
                "glob".to_owned(),
                "\"glob\":\"DirectoryIterator\":private".to_owned(),
                Value::Bool(false),
            ),
            (
                "subPathName".to_owned(),
                "\"subPathName\":\"RecursiveDirectoryIterator\":private".to_owned(),
                Value::string(Vec::new()),
            ),
        ],
    )
}

fn zip_archive_debug_info_object(source: &ObjectRef) -> ObjectRef {
    ObjectRef::debug_view_with_properties(
        source,
        vec![
            zip_archive_debug_property(source, "lastId"),
            zip_archive_debug_property(source, "status"),
            zip_archive_debug_property(source, "statusSys"),
            zip_archive_debug_property(source, "numFiles"),
            zip_archive_debug_property(source, "filename"),
            zip_archive_debug_property(source, "comment"),
        ],
    )
}

fn zip_archive_debug_property(source: &ObjectRef, name: &str) -> (String, String, Value) {
    (
        name.to_owned(),
        format!("\"{name}\""),
        source.get_property(name).unwrap_or(Value::Null),
    )
}

fn spl_doubly_linked_list_debug_info_object(source: &ObjectRef, runtime_class: &str) -> ObjectRef {
    ObjectRef::debug_view_with_properties(
        source,
        vec![
            (
                "flags".to_owned(),
                "\"flags\":\"SplDoublyLinkedList\":private".to_owned(),
                Value::Int(spl_doubly_linked_list_flags(source, runtime_class)),
            ),
            (
                "dllist".to_owned(),
                "\"dllist\":\"SplDoublyLinkedList\":private".to_owned(),
                Value::Array(spl_entries_to_debug_php_array_excluding(
                    spl_entries(source),
                    source.id(),
                )),
            ),
        ],
    )
}

fn spl_heap_debug_info_object(source: &ObjectRef, runtime_class: &str) -> ObjectRef {
    let property_owner = if normalize_class_name(runtime_class) == "splpriorityqueue" {
        "SplPriorityQueue"
    } else {
        "SplHeap"
    };
    let mut properties = spl_object_user_debug_properties(source);
    properties.extend([
        (
            "flags".to_owned(),
            format!("\"flags\":\"{property_owner}\":private"),
            Value::Int(spl_heap_debug_flags(source, runtime_class)),
        ),
        (
            "isCorrupted".to_owned(),
            format!("\"isCorrupted\":\"{property_owner}\":private"),
            Value::Bool(spl_heap_is_corrupted(source)),
        ),
        (
            "heap".to_owned(),
            format!("\"heap\":\"{property_owner}\":private"),
            Value::Array(spl_entries_to_debug_php_array_excluding(
                spl_entries(source),
                source.id(),
            )),
        ),
    ]);
    ObjectRef::debug_view_with_properties(source, properties)
}

fn spl_debug_view_value(value: Value, excluded_object_id: Option<u64>) -> Value {
    match value {
        Value::Object(object) => {
            if excluded_object_id.is_some_and(|id| id == object.id()) {
                return Value::Object(object);
            }
            spl_internal_debug_info_object(&object)
                .map(Value::Object)
                .unwrap_or(Value::Object(object))
        }
        Value::Array(array) => {
            let mut debug_array = PhpArray::new();
            for (key, value) in array.iter() {
                debug_array.insert(
                    key.clone(),
                    spl_debug_view_value(value.clone(), excluded_object_id),
                );
            }
            Value::Array(debug_array)
        }
        value => value,
    }
}

fn emit_zip_open_empty_file_deprecation(
    compiled: &CompiledUnit,
    output: &mut OutputBuffer,
    stack: &CallStack,
    state: &mut ExecutionState,
    source_span: RuntimeSourceSpan,
) {
    if !error_reporting_allows(state, php_runtime::PHP_E_DEPRECATED) {
        return;
    }
    let diagnostic = RuntimeDiagnostic::new(
        "E_PHP_VM_ZIP_EMPTY_FILE_DEPRECATED",
        RuntimeSeverity::Deprecation,
        "ZipArchive::open(): Using empty file as ZipArchive is deprecated",
        source_span,
        stack_trace(compiled, stack),
        None,
    );
    emit_vm_diagnostic(
        output,
        state,
        &diagnostic,
        php_runtime::PhpDiagnosticChannel::Deprecated,
        php_runtime::PHP_E_DEPRECATED,
    );
    state.diagnostics.push(diagnostic);
}

#[cold]
fn is_autoload_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "spl_autoload"
            | "spl_autoload_extensions"
            | "spl_autoload_register"
            | "spl_autoload_unregister"
            | "spl_autoload_functions"
            | "spl_autoload_call"
    )
}

fn is_symbol_introspection_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "define"
            | "defined"
            | "constant"
            | "extension_loaded"
            | "function_exists"
            | "compact"
            | "clone"
            | "class_exists"
            | "class_alias"
            | "call_user_func"
            | "call_user_func_array"
            | "forward_static_call"
            | "debug_backtrace"
            | "debug_print_backtrace"
            | "func_get_arg"
            | "func_get_args"
            | "func_num_args"
            | "get_called_class"
            | "interface_exists"
            | "trait_exists"
            | "enum_exists"
            | "method_exists"
            | "property_exists"
            | "is_callable"
            | "is_a"
            | "is_subclass_of"
            | "get_class"
            | "get_class_methods"
            | "get_class_vars"
            | "get_parent_class"
            | "class_parents"
            | "class_implements"
            | "get_declared_classes"
            | "get_declared_interfaces"
            | "get_declared_traits"
            | "get_defined_functions"
            | "get_defined_constants"
            | "get_defined_vars"
            | "get_extension_funcs"
            | "get_included_files"
            | "get_loaded_extensions"
            | "get_required_files"
            | "phpversion"
            | "zend_version"
            | "get_mangled_object_vars"
            | "get_object_vars"
    )
}

fn is_config_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "ignore_user_abort" | "ini_get" | "ini_set" | "ini_get_all" | "get_cfg_var"
    )
}

fn is_error_handling_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "error_reporting"
            | "error_log"
            | "set_error_handler"
            | "get_error_handler"
            | "restore_error_handler"
            | "error_get_last"
            | "register_shutdown_function"
            | "trigger_error"
            | "user_error"
            | "set_exception_handler"
            | "get_exception_handler"
            | "restore_exception_handler"
    )
}

fn is_output_buffering_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "ob_start"
            | "ob_get_contents"
            | "ob_get_clean"
            | "ob_get_flush"
            | "ob_get_length"
            | "ob_get_level"
            | "ob_end_clean"
            | "ob_end_flush"
            | "flush"
    )
}

fn is_environment_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "getenv"
            | "putenv"
            | "php_sapi_name"
            | "php_uname"
            | "get_current_user"
            | "getmyuid"
            | "getmygid"
    )
}

fn is_process_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "proc_open"
            | "proc_close"
            | "proc_get_status"
            | "popen"
            | "pclose"
            | "shell_exec"
            | "exec"
            | "passthru"
            | "system"
    )
}

fn error_handler_callback_from_value(
    compiled: &CompiledUnit,
    value: Value,
) -> Result<CallableValue, String> {
    match value {
        Value::Callable(callable) => match *callable {
            CallableValue::UserFunction { name } => {
                let normalized = normalize_function_name(&name);
                if compiled.lookup_function(&normalized).is_some() {
                    Ok(CallableValue::UserFunction { name: normalized })
                } else if BuiltinRegistry::new().contains(&normalized) {
                    Ok(CallableValue::InternalBuiltin { name: normalized })
                } else {
                    Err(format!(
                        "E_PHP_VM_ERROR_INVALID_CALLBACK: function {name} is not callable"
                    ))
                }
            }
            CallableValue::Closure(payload) => Ok(CallableValue::Closure(payload)),
            CallableValue::InternalBuiltin { name } => {
                if BuiltinRegistry::new().contains(&name) {
                    Ok(CallableValue::InternalBuiltin { name })
                } else {
                    Err(format!(
                        "E_PHP_VM_ERROR_INVALID_CALLBACK: builtin {name} is not callable"
                    ))
                }
            }
            other_callable => Err(format!(
                "E_PHP_VM_ERROR_INVALID_CALLBACK: value of type {} is not callable",
                value_type_name(&Value::Callable(Box::new(other_callable)))
            )),
        },
        Value::String(name) => {
            let name = normalize_function_name(&name.to_string_lossy());
            if compiled.lookup_function(&name).is_some() {
                Ok(CallableValue::UserFunction { name })
            } else if BuiltinRegistry::new().contains(&name) {
                Ok(CallableValue::InternalBuiltin { name })
            } else {
                Err(format!(
                    "E_PHP_VM_ERROR_INVALID_CALLBACK: function {name} is not callable"
                ))
            }
        }
        other => Err(format!(
            "E_PHP_VM_ERROR_INVALID_CALLBACK: value of type {} is not callable",
            value_type_name(&other)
        )),
    }
}

fn autoload_callback_from_value(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    value: Value,
) -> Result<CallableValue, String> {
    match value {
        Value::Callable(callable) => match *callable {
            CallableValue::UserFunction { name } => {
                let normalized = normalize_function_name(&name);
                if compiled.lookup_function(&normalized).is_some()
                    || dynamic_function_in_state(state, &normalized).is_some()
                    || BuiltinRegistry::new().contains(&normalized)
                {
                    Ok(CallableValue::UserFunction { name: normalized })
                } else {
                    Err(format!(
                        "function \"{name}\" not found or invalid function name"
                    ))
                }
            }
            CallableValue::Closure(payload) => Ok(CallableValue::Closure(payload)),
            CallableValue::InternalBuiltin { name } => {
                if BuiltinRegistry::new().contains(&name) {
                    Ok(CallableValue::InternalBuiltin { name })
                } else {
                    Err(format!("builtin {name} is not callable"))
                }
            }
            other_callable => Err(format!(
                "value of type {} is not callable",
                value_type_name(&Value::Callable(Box::new(other_callable)))
            )),
        },
        Value::String(name) => {
            let name = name.to_string_lossy();
            if let Some((class_name, method)) = name.split_once("::") {
                autoload_class_method_callback(compiled, state, class_name, method, true)
            } else {
                let normalized = normalize_function_name(&name);
                if compiled.lookup_function(&normalized).is_some()
                    || dynamic_function_in_state(state, &normalized).is_some()
                {
                    Ok(CallableValue::UserFunction { name: normalized })
                } else if BuiltinRegistry::new().contains(&normalized)
                    || is_autoload_builtin_name(&normalized)
                {
                    Ok(CallableValue::InternalBuiltin { name: normalized })
                } else {
                    Err(format!(
                        "function \"{name}\" not found or invalid function name"
                    ))
                }
            }
        }
        Value::Array(array) => {
            let elements = array
                .iter()
                .map(|(_, value)| value.clone())
                .collect::<Vec<_>>();
            let [target, method]: [Value; 2] = match elements.try_into() {
                Ok(elements) => elements,
                Err(_) => {
                    return Err("callable arrays must contain exactly target and method".to_owned());
                }
            };
            let Some(method) = callable_string_value(method) else {
                return Err("callable array method must be string".to_owned());
            };
            match callable_resolve_reference(target) {
                Value::Object(object) => {
                    let class_name = object.class_name();
                    let resolved = autoload_resolve_method(compiled, state, &class_name, &method)?;
                    if resolved.method.flags.is_static {
                        Ok(CallableValue::BoundMethod {
                            target: CallableMethodTarget::Class(object.display_name()),
                            method,
                            scope: Some(normalize_class_name(&class_name)),
                        })
                    } else {
                        Ok(CallableValue::BoundMethod {
                            target: CallableMethodTarget::Object(object),
                            method,
                            scope: None,
                        })
                    }
                }
                Value::String(class_name) => {
                    let class_name = class_name.to_string_lossy();
                    autoload_class_method_callback(compiled, state, &class_name, &method, true)
                }
                other => Err(format!(
                    "callable array target must be object or class string, got {}",
                    value_type_name(&other)
                )),
            }
        }
        Value::Object(object) => {
            if lookup_method_in_state(compiled, state, &object.class_name(), "__invoke")
                .map(|method| method.is_some())?
            {
                Ok(CallableValue::BoundMethod {
                    target: CallableMethodTarget::Object(object),
                    method: "__invoke".to_owned(),
                    scope: None,
                })
            } else {
                Err(format!(
                    "object of class {} is not callable",
                    object.class_name()
                ))
            }
        }
        other => Err(format!(
            "value of type {} is not callable",
            value_type_name(&other)
        )),
    }
}

fn autoload_invalid_callback_error(function_name: &str, reason: &str) -> String {
    format!(
        "E_PHP_VM_AUTOLOAD_INVALID_CALLBACK: {function_name}(): Argument #1 ($callback) must be a valid callback or null, {reason}"
    )
}

fn autoload_class_method_callback(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    method: &str,
    require_static: bool,
) -> Result<CallableValue, String> {
    let resolved = autoload_resolve_method(compiled, state, class_name, method)?;
    if require_static && !resolved.method.flags.is_static {
        return Err(format!(
            "non-static method {}::{}() cannot be called statically",
            resolved.class.display_name, method
        ));
    }
    let target_display = lookup_class_in_state(compiled, state, class_name)
        .map(|class| class.display_name.clone())
        .unwrap_or_else(|| display_class_name(class_name));
    Ok(CallableValue::BoundMethod {
        target: CallableMethodTarget::Class(target_display),
        method: method.to_owned(),
        scope: Some(normalize_class_name(&resolved.class.name)),
    })
}

fn autoload_resolve_method(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    method: &str,
) -> Result<ResolvedMethodOwned, String> {
    let display_name = callable_class_display_name(compiled, state, class_name);
    let Some(class) = lookup_class_in_state(compiled, state, class_name) else {
        return Err(format!("class {display_name} does not exist"));
    };
    let Some(resolved) =
        lookup_resolved_method_in_state(compiled, state, &class.name, method, None)?
    else {
        return Err(format!(
            "class {} does not have a method \"{method}\"",
            class.display_name
        ));
    };
    if resolved.method.flags.is_private || resolved.method.flags.is_protected {
        return Err(format!(
            "cannot access {} method {}::{}()",
            method_visibility_name(resolved.method.flags),
            resolved.class.display_name,
            method
        ));
    }
    Ok(resolved)
}

fn class_like_exists_direct(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    kind: AutoloadClassLookupKind,
) -> bool {
    if lookup_class_in_state(compiled, state, class_name).is_some_and(|class| match kind {
        AutoloadClassLookupKind::ClassLike => true,
        AutoloadClassLookupKind::Interface => class.flags.is_interface,
        AutoloadClassLookupKind::Enum => class.flags.is_enum,
        AutoloadClassLookupKind::Trait => class.flags.is_trait,
        AutoloadClassLookupKind::Class => !class.flags.is_interface && !class.flags.is_trait,
    }) {
        return true;
    }

    php_std::ExtensionRegistry::standard_library()
        .enabled_class(class_name)
        .is_some_and(|class| match kind {
            AutoloadClassLookupKind::ClassLike => true,
            AutoloadClassLookupKind::Interface => class.kind() == php_std::ClassKind::Interface,
            AutoloadClassLookupKind::Enum => class.kind() == php_std::ClassKind::Enum,
            AutoloadClassLookupKind::Trait => class.kind() == php_std::ClassKind::Trait,
            AutoloadClassLookupKind::Class => class.kind() == php_std::ClassKind::Class,
        })
}

fn class_dependency_display_name(
    compiled: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
    normalized_dependency: &str,
) -> String {
    let normalized_dependency = normalize_class_name(normalized_dependency);
    if class
        .parent
        .as_deref()
        .is_some_and(|parent| normalize_class_name(parent) == normalized_dependency)
        && let Some(display) = class.parent_display_name.as_deref()
    {
        return display.to_owned();
    }
    let Some(file) = compiled.unit().files.get(class.span.file.index()) else {
        return normalized_dependency;
    };
    let Ok(source) = std::fs::read_to_string(&file.path) else {
        return normalized_dependency;
    };
    if let Some(display) =
        class_dependency_import_display_name(&source, class, &normalized_dependency)
    {
        return display;
    }
    let start = (class.span.start as usize).min(source.len());
    let end = (class.span.end as usize).min(source.len()).max(start);
    let declaration = source[start..end]
        .split_once('{')
        .map_or(&source[start..end], |(head, _)| head);
    for token in php_name_tokens(declaration) {
        if normalize_class_name(&token) == normalized_dependency {
            return display_class_name(&token);
        }
        if class_name_tail(&token).eq_ignore_ascii_case(class_name_tail(&normalized_dependency))
            && let Some(display) = class_dependency_namespace_display_name(
                &source,
                class,
                &token,
                &normalized_dependency,
            )
        {
            return display;
        }
    }
    normalized_dependency
}

fn class_dependency_import_display_name(
    source: &str,
    class: &php_ir::module::ClassEntry,
    normalized_dependency: &str,
) -> Option<String> {
    let header_end = (class.span.start as usize).min(source.len());
    let header = source.get(..header_end)?;
    let dependency_tail = class_name_tail(normalized_dependency);
    for line in header.lines() {
        let line = line.trim();
        let Some(imports) = line.strip_prefix("use ") else {
            continue;
        };
        if let Some(display) = class_dependency_import_display_name_from_imports(
            imports,
            dependency_tail,
            normalized_dependency,
        ) {
            return Some(display);
        }
    }
    for statement in header.split(';') {
        let statement = statement.trim();
        let Some(imports) = statement.strip_prefix("use ") else {
            continue;
        };
        if let Some(display) = class_dependency_import_display_name_from_imports(
            imports,
            dependency_tail,
            normalized_dependency,
        ) {
            return Some(display);
        }
    }
    None
}

fn class_dependency_import_display_name_from_imports(
    imports: &str,
    dependency_tail: &str,
    normalized_dependency: &str,
) -> Option<String> {
    let imports = imports.trim().trim_end_matches(';').trim();
    if imports.starts_with("function ") || imports.starts_with("const ") {
        return None;
    }
    for import in imports.split(',') {
        let import = import.trim();
        if import.contains('{') || import.contains('}') {
            continue;
        }
        let (name, alias) = split_import_alias(import);
        let name = name.trim().trim_start_matches('\\');
        if name.is_empty() {
            continue;
        }
        let alias = alias
            .map(str::trim)
            .filter(|alias| !alias.is_empty())
            .unwrap_or_else(|| class_name_tail(name));
        if alias.eq_ignore_ascii_case(dependency_tail)
            && normalize_class_name(name) == normalized_dependency
        {
            return Some(name.to_owned());
        }
    }
    None
}

fn class_dependency_namespace_display_name(
    source: &str,
    class: &php_ir::module::ClassEntry,
    token: &str,
    normalized_dependency: &str,
) -> Option<String> {
    if token.contains('\\') {
        return None;
    }
    let namespace = class_declaration_namespace_display_name(source, class)?;
    let candidate = format!("{namespace}\\{token}");
    (normalize_class_name(&candidate) == normalized_dependency).then_some(candidate)
}

fn class_declaration_namespace_display_name(
    source: &str,
    class: &php_ir::module::ClassEntry,
) -> Option<String> {
    let header_end = (class.span.start as usize).min(source.len());
    let header = source.get(..header_end)?;
    for statement in header.split(';') {
        let statement = statement.trim();
        let marker = "namespace ";
        let Some(index) = statement.find(marker) else {
            continue;
        };
        let namespace = statement[index + marker.len()..].trim();
        if namespace.is_empty() || namespace.starts_with('{') {
            continue;
        }
        let namespace = namespace
            .split_whitespace()
            .next()
            .unwrap_or(namespace)
            .trim_matches('{')
            .trim();
        if !namespace.is_empty() {
            return Some(namespace.trim_start_matches('\\').to_owned());
        }
    }
    None
}

fn split_import_alias(import: &str) -> (&str, Option<&str>) {
    let lower = import.to_ascii_lowercase();
    if let Some(index) = lower.rfind(" as ") {
        (&import[..index], Some(&import[index + 4..]))
    } else {
        (import, None)
    }
}

fn class_name_tail(name: &str) -> &str {
    name.trim_start_matches('\\')
        .rsplit('\\')
        .next()
        .unwrap_or(name)
}

fn should_defer_class_dependency_validation(class: &php_ir::module::ClassEntry) -> bool {
    class.name.starts_with("__phrust_anonymous_") || class.display_name.starts_with("anonymous#")
}

fn php_name_tokens(source: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in source.chars() {
        if ch == '\\' || ch == '_' || ch.is_alphanumeric() {
            current.push(ch);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn is_valid_autoload_class_name(class_name: &str) -> bool {
    let name = class_name.strip_prefix('\\').unwrap_or(class_name);
    if name.is_empty() {
        return false;
    }
    name.split('\\')
        .all(|segment| is_valid_autoload_class_name_segment(segment.as_bytes()))
}

fn is_valid_autoload_class_name_segment(segment: &[u8]) -> bool {
    let Some((&first, rest)) = segment.split_first() else {
        return false;
    };
    is_php_name_start_byte(first) && rest.iter().copied().all(is_php_name_byte)
}

fn is_php_name_start_byte(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic() || byte >= 0x80
}

fn is_php_name_byte(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric() || byte >= 0x80
}

impl AutoloadClassLookupKind {
    const fn exists_function_name(self) -> &'static str {
        match self {
            Self::ClassLike => "class_exists",
            Self::Class => "class_exists",
            Self::Interface => "interface_exists",
            Self::Trait => "trait_exists",
            Self::Enum => "enum_exists",
        }
    }
}

fn autoload_trace_origin_from_call_site(
    compiled: &CompiledUnit,
    function_name: &'static str,
    call_site: Option<(u64, FunctionId, BlockId, InstrId)>,
) -> Option<AutoloadTraceOrigin> {
    let (_, function_id, block_id, instruction_id) = call_site?;
    let function = compiled.unit().functions.get(function_id.index())?;
    let block = function.blocks.get(block_id.index())?;
    let instruction = block
        .instructions
        .iter()
        .find(|instruction| instruction.id == instruction_id)?;
    Some(AutoloadTraceOrigin {
        function_name,
        span: instruction.span,
    })
}

fn capture_autoload_trace(
    compiled: &CompiledUnit,
    stack: &CallStack,
    callback: &CallableValue,
    class_name: &str,
    origin: AutoloadTraceOrigin,
) -> String {
    let class_arg = format_trace_arg(&Value::string(class_name.as_bytes().to_vec()));
    let callback = autoload_trace_callback_name(callback);
    let mut lines = vec![format!("#0 [internal function]: {callback}({class_arg})")];
    let file = compiled
        .unit()
        .files
        .get(origin.span.file.index())
        .map(|file| file.path.clone())
        .unwrap_or_default();
    let line = source_span_display_line(compiled, origin.span, false)
        .unwrap_or_else(|| i64::from(origin.span.start));
    lines.push(format!(
        "#1 {file}({line}): {}({class_arg})",
        origin.function_name
    ));
    let rest = capture_backtrace_string_from_index(compiled, stack, 2);
    if !rest.is_empty() {
        lines.push(rest);
    }
    lines.join("\n")
}

fn autoload_trace_callback_name(callback: &CallableValue) -> String {
    match callback {
        CallableValue::UserFunction { name } | CallableValue::InternalBuiltin { name } => {
            name.clone()
        }
        CallableValue::Closure(_) => "{closure}".to_owned(),
        CallableValue::BoundMethod { target, method, .. } => {
            let target = match target {
                CallableMethodTarget::Object(object) => object.display_name(),
                CallableMethodTarget::Class(class_name) => class_name.clone(),
            };
            format!("{target}->{method}")
        }
        CallableValue::MethodPlaceholder { target }
        | CallableValue::UnresolvedDynamic { target } => target.clone(),
    }
}

fn dynamic_class_owner_in_state(state: &ExecutionState, class_name: &str) -> Option<CompiledUnit> {
    let unit_index = dynamic_class_owner_index_in_state(state, class_name)?;
    state.dynamic_units.get(unit_index).cloned()
}

fn class_owner_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
) -> CompiledUnit {
    dynamic_class_owner_in_state(state, class_name).unwrap_or_else(|| compiled.clone())
}

fn destructor_entry_owner(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    entry: &DestructorEntry,
) -> CompiledUnit {
    entry
        .owner_dynamic_unit_index
        .and_then(|unit_index| state.dynamic_units.get(unit_index).cloned())
        .unwrap_or_else(|| compiled.clone())
}

fn dynamic_class_owner_index_in_state(state: &ExecutionState, class_name: &str) -> Option<usize> {
    let normalized = normalize_class_name(class_name);
    if let Some(entry) = dynamic_class_entry_by_normalized_name(state, &normalized)
        && state.dynamic_units.get(entry.unit_index).is_some()
    {
        return Some(entry.unit_index);
    }
    state.dynamic_units.iter().rposition(|unit| {
        unit.lookup_class(&normalized)
            .is_some_and(|class| normalize_class_name(&class.name) == normalized)
    })
}

fn ini_option_name(value: &Value) -> Result<String, String> {
    to_string(value).map(|name| name.to_string_lossy())
}

fn session_ini_cannot_change_when_active(option: &str) -> bool {
    matches!(
        option.to_ascii_lowercase().as_str(),
        "session.save_path"
            | "session.name"
            | "session.save_handler"
            | "session.gc_probability"
            | "session.gc_divisor"
            | "session.gc_maxlifetime"
            | "session.serialize_handler"
            | "session.sid_length"
            | "session.sid_bits_per_character"
            | "session.use_strict_mode"
            | "session.cookie_lifetime"
            | "session.cookie_path"
            | "session.cookie_domain"
            | "session.cookie_secure"
            | "session.cookie_partitioned"
            | "session.cookie_httponly"
            | "session.cookie_samesite"
            | "session.use_cookies"
            | "session.use_only_cookies"
            | "session.referer_check"
            | "session.cache_expire"
            | "session.cache_limiter"
            | "session.use_trans_sid"
            | "session.lazy_write"
    )
}

fn session_sid_ini_deprecation(option: &str, value: &str) -> Option<String> {
    let (canonical, default) = if option.eq_ignore_ascii_case("session.sid_length") {
        ("session.sid_length", 32)
    } else if option.eq_ignore_ascii_case("session.sid_bits_per_character") {
        ("session.sid_bits_per_character", 4)
    } else {
        return None;
    };
    let parsed = value.trim().parse::<i64>().unwrap_or(0);
    (parsed != default).then(|| format!("ini_set(): {canonical} INI setting is deprecated"))
}

fn session_serialize_handler_ini_error(option: &str, value: &str) -> Option<String> {
    if !option.eq_ignore_ascii_case("session.serialize_handler") {
        return None;
    }
    match value {
        "php" | "php_binary" | "php_serialize" => None,
        _ => Some(format!(
            "ini_set(): Serialization handler \"{value}\" cannot be found"
        )),
    }
}

fn ini_set_effective_value(option: &str, value: String, cwd: &Path) -> String {
    if option.eq_ignore_ascii_case("open_basedir") {
        return normalize_open_basedir_ini_value(&value, cwd);
    }
    value
}

fn normalize_open_basedir_ini_value(value: &str, cwd: &Path) -> String {
    value
        .split(open_basedir_separator())
        .map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                String::new()
            } else {
                canonicalize_open_basedir_path(entry, cwd)
                    .to_string_lossy()
                    .into_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(&open_basedir_separator().to_string())
}

fn session_save_path_open_basedir_ini_error(
    option: &str,
    value: &str,
    cwd: &Path,
    registry: &IniRegistry,
) -> Option<String> {
    if !option.eq_ignore_ascii_case("session.save_path") {
        return None;
    }
    let save_path = session_save_path_directory(value)?;
    let open_basedir = registry.get("open_basedir")?.trim();
    if open_basedir.is_empty() || open_basedir_allows_path(&save_path, open_basedir, cwd) {
        return None;
    }
    Some(format!(
        "ini_set(): open_basedir restriction in effect. File({save_path}) is not within the allowed path(s): ({open_basedir})"
    ))
}

fn session_save_path_directory(raw_path: &str) -> Option<String> {
    let path = raw_path
        .split(';')
        .next_back()
        .unwrap_or(raw_path)
        .trim()
        .to_owned();
    (!path.is_empty()).then_some(path)
}

fn open_basedir_allows_path(path: &str, open_basedir: &str, cwd: &Path) -> bool {
    let candidate = canonicalize_open_basedir_path(path, cwd);
    open_basedir
        .split(open_basedir_separator())
        .filter_map(|entry| {
            let entry = entry.trim();
            (!entry.is_empty()).then(|| canonicalize_open_basedir_path(entry, cwd))
        })
        .any(|allowed| candidate == allowed || candidate.starts_with(&allowed))
}

fn canonicalize_open_basedir_path(path: &str, cwd: &Path) -> PathBuf {
    let path = Path::new(path);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    fs::canonicalize(&absolute).unwrap_or_else(|_| normalize_open_basedir_path(&absolute))
}

fn normalize_open_basedir_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn open_basedir_separator() -> char {
    if cfg!(windows) { ';' } else { ':' }
}

fn apply_float_string_precision(registry: &IniRegistry) {
    if let Some(precision) = registry
        .get("precision")
        .and_then(|value| value.trim().parse::<i32>().ok())
    {
        set_float_string_precision(precision);
    }
}

fn ini_get_all_array(registry: &IniRegistry, details: bool, extension: Option<&str>) -> PhpArray {
    let mut output = PhpArray::new();
    let entries = match extension {
        Some(extension) => registry.entries_for_extension(extension),
        None => registry.entries(),
    };
    for entry in entries {
        let value = if details {
            let mut detail = PhpArray::new();
            detail.insert(
                php_string_key("global_value"),
                Value::string(entry.global_value),
            );
            detail.insert(
                php_string_key("local_value"),
                Value::string(entry.local_value),
            );
            detail.insert(php_string_key("access"), Value::Int(entry.access));
            Value::Array(detail)
        } else {
            Value::string(entry.local_value)
        };
        output.insert(php_string_key(entry.name), value);
    }
    output
}

fn trim_error_handler_args(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    callback: &CallableValue,
    values: Vec<Value>,
) -> Vec<Value> {
    let Some(max_args) = error_handler_callback_max_args(compiled, state, callback) else {
        return values;
    };
    values.into_iter().take(max_args).collect()
}

fn error_handler_callback_max_args(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    callback: &CallableValue,
) -> Option<usize> {
    match callback {
        CallableValue::UserFunction { name } => {
            if let Some(function) = compiled
                .lookup_function(name)
                .and_then(|function| compiled.unit().functions.get(function.index()))
            {
                return Some(user_function_max_positional_args(function));
            }
            dynamic_function_in_state(state, name).and_then(|(owner, function)| {
                owner
                    .unit()
                    .functions
                    .get(function.index())
                    .map(user_function_max_positional_args)
            })
        }
        CallableValue::Closure(payload) => compiled
            .unit()
            .functions
            .get(FunctionId::new(payload.function).index())
            .map(user_function_max_positional_args),
        CallableValue::InternalBuiltin { .. }
        | CallableValue::BoundMethod { .. }
        | CallableValue::MethodPlaceholder { .. }
        | CallableValue::UnresolvedDynamic { .. } => None,
    }
}

fn user_function_max_positional_args(function: &IrFunction) -> usize {
    if function.params.iter().any(|param| param.variadic) {
        usize::MAX
    } else {
        function.params.len()
    }
}

fn state_include_path(state: &ExecutionState) -> Arc<Vec<PathBuf>> {
    Arc::clone(&state.parsed_include_path)
}

fn parse_ini_include_path(ini: &IniRegistry) -> Arc<Vec<PathBuf>> {
    Arc::new(
        ini.get("include_path")
            .unwrap_or(".")
            .split(':')
            .filter(|entry| !entry.is_empty())
            .map(PathBuf::from)
            .collect(),
    )
}

fn php_string_key(value: &str) -> ArrayKey {
    ArrayKey::String(PhpString::from_test_str(value))
}

fn php_object_vars_key(value: &str) -> ArrayKey {
    ArrayKey::from_php_string(PhpString::from_test_str(value))
}

fn object_from_value(value: &Value) -> Option<ObjectRef> {
    match value {
        Value::Object(object) => Some(object.clone()),
        Value::Reference(cell) => object_from_value(&cell.get()),
        _ => None,
    }
}

fn class_name_for_is_a_subject(
    value: &Value,
    allow_string: bool,
) -> Result<Option<String>, String> {
    match effective_value(value) {
        Value::Object(object) => Ok(Some(object.class_name())),
        Value::Callable(_) => Ok(Some("Closure".to_owned())),
        value if allow_string => to_string(&value).map(|name| Some(name.to_string_lossy())),
        _ => Ok(None),
    }
}

fn object_vars_array(
    compiled: &CompiledUnit,
    stack: &CallStack,
    object: &ObjectRef,
    mangled: bool,
) -> PhpArray {
    let mut array = PhpArray::new();
    let class = compiled.lookup_class(&object.class_name());
    let scope = current_scope_class(compiled, stack);

    for (storage_name, value) in object.properties_snapshot() {
        if !mangled && is_spl_internal_storage_property(object, &storage_name) {
            continue;
        }
        if let Some((declaring_class, property)) = private_storage_parts(&storage_name) {
            if mangled {
                let display_class =
                    class_display_name(compiled, &declaring_class).unwrap_or(declaring_class);
                array.insert(
                    ArrayKey::String(PhpString::from_test_str(&format!(
                        "\0{display_class}\0{property}"
                    ))),
                    value,
                );
            } else if scope.as_deref().is_some_and(|scope| {
                normalize_class_name(scope) == normalize_class_name(&declaring_class)
            }) {
                array.insert(php_string_key(&property), value);
            }
            continue;
        }

        let property = class.and_then(|class| {
            lookup_property_in_hierarchy(compiled, class, &storage_name, None)
                .ok()
                .flatten()
        });
        if mangled {
            let key = property
                .as_ref()
                .and_then(|resolved| {
                    if resolved.property.flags.is_protected {
                        Some(ArrayKey::String(PhpString::from_test_str(&format!(
                            "\0*\0{}",
                            resolved.property.name
                        ))))
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| php_string_key(&storage_name));
            array.insert(key, value);
            continue;
        }

        let visible = property.as_ref().is_none_or(|resolved| {
            class_member_visible(
                compiled,
                scope.as_deref(),
                resolved.class,
                resolved.property.flags.is_private,
                resolved.property.flags.is_protected,
            )
        });
        if visible {
            let key = property
                .as_ref()
                .map(|resolved| php_string_key(resolved.property.name.as_str()))
                .unwrap_or_else(|| php_object_vars_key(storage_name.as_str()));
            array.insert(key, value);
        }
    }

    array
}

fn is_spl_internal_storage_property(object: &ObjectRef, storage_name: &str) -> bool {
    if spl_runtime_marker(object).is_none() {
        return false;
    }
    matches!(
        storage_name,
        SPL_RUNTIME_CLASS_PROPERTY
            | "__append_entry_iterator_indices"
            | "__append_iterators"
            | "__attached_iterator_ids"
            | "__attached_iterators"
            | "__entries"
            | "__entry_depths"
            | "__extract_flags"
            | "__file_info_class"
            | "__flags"
            | "__inner_iterator"
            | "__iterator_class"
            | "__iterator_count"
            | "__limit_count"
            | "__limit_offset"
            | "__position"
            | "__regex_accept_pre_parent"
            | "__regex_flags"
            | "__regex_last_accept_result"
            | "__regex_mode"
            | "__regex_pattern"
            | "__rii_array_string_warning_positions"
            | "__rii_checked_child_results"
            | "__rii_checked_child_positions"
            | "__rii_direct_at_root"
            | "__rii_direct_root_consumed"
            | "__rii_end_iteration_called"
            | "__rii_entered_child_positions"
            | "__rii_flags"
            | "__rii_hook_depth"
            | "__rii_hook_iterators"
            | "__rii_iteration_active"
            | "__rii_last_call_has_children"
            | "__rii_mode"
            | "__rii_notified_position"
            | "__rii_pruned_branches"
            | "__rti_flags"
            | "__rti_prefix_parts"
            | "__snapshot_source_id"
            | "__storage"
            | "__sub_iterators"
    )
}

fn private_storage_parts(storage_name: &str) -> Option<(String, String)> {
    storage_name
        .strip_prefix("private:")
        .and_then(|rest| rest.split_once(':'))
        .map(|(class, property)| (class.to_owned(), property.to_owned()))
}

fn sleep_property_value(
    properties: &[(String, Value)],
    selected_name: &str,
) -> Option<(String, Value)> {
    properties.iter().find_map(|(storage_name, value)| {
        if storage_name == selected_name {
            return Some((storage_name.clone(), value.clone()));
        }
        if let Some((owner, property)) = private_storage_parts(storage_name)
            && (property == selected_name || selected_name == format!("\0{owner}\0{property}"))
        {
            return Some((storage_name.clone(), value.clone()));
        }
        if selected_name == format!("\0*\0{storage_name}") {
            return Some((storage_name.clone(), value.clone()));
        }
        None
    })
}

fn eval_failure(
    output: &OutputBuffer,
    message: impl Into<String>,
    stack_trace: Vec<RuntimeStackFrame>,
) -> VmResult {
    let message = message.into();
    VmResult::runtime_error_with_diagnostic(
        output.clone(),
        message.clone(),
        RuntimeDiagnostic::new(
            eval_failure_id(&message).to_owned(),
            RuntimeSeverity::FatalError,
            message,
            RuntimeSourceSpan::default(),
            stack_trace,
            None,
        ),
    )
}

fn eval_failure_id(message: &str) -> &str {
    message
        .split_once(':')
        .and_then(|(id, _)| id.starts_with("E_").then_some(id))
        .unwrap_or("E_PHP_VM_EVAL_ERROR")
}

fn current_source_path(compiled: &CompiledUnit, stack: &CallStack) -> Option<PathBuf> {
    let frame = stack.current()?;
    let function = compiled.unit().functions.get(frame.function.index())?;
    let file = compiled.unit().files.get(function.span.file.index())?;
    Some(PathBuf::from(&file.path))
}

fn dense_instruction_span(dense: &DenseBytecodeUnit, instruction: &DenseInstruction) -> IrSpan {
    dense
        .spans
        .get(instruction.span.index())
        .copied()
        .unwrap_or_default()
}

fn is_synthetic_eof_return(
    function: &IrFunction,
    terminator_span: IrSpan,
    return_value: Option<&Value>,
) -> bool {
    function.flags.is_top_level
        && terminator_span == function.span
        && matches!(return_value, None | Some(Value::Null))
}

fn include_return_value(return_value: Option<Value>, returned_explicitly: bool) -> Option<Value> {
    if returned_explicitly {
        Some(return_value.unwrap_or(Value::Null))
    } else {
        None
    }
}

fn shared_locals_from_current_frame(
    compiled: &CompiledUnit,
    stack: &CallStack,
) -> HashMap<String, Slot> {
    let Some(frame) = stack.current() else {
        return HashMap::new();
    };
    let Some(function) = compiled.unit().functions.get(frame.function.index()) else {
        return HashMap::new();
    };
    function
        .locals
        .iter()
        .enumerate()
        .filter_map(|(index, name)| {
            frame
                .locals
                .get_slot(LocalId::new(index as u32))
                .map(|slot| (name.clone(), slot.clone()))
        })
        .collect()
}

fn import_shared_locals(
    function: &IrFunction,
    stack: &mut CallStack,
    state: &mut ExecutionState,
    shared: &HashMap<String, Slot>,
    bind_missing_globals: bool,
) {
    let Some(frame) = stack.current_mut() else {
        return;
    };
    for (index, name) in function.locals.iter().enumerate() {
        if let Some(slot) = shared.get(name) {
            let _ = frame
                .locals
                .set_slot(LocalId::new(index as u32), slot.clone());
        } else if bind_missing_globals && name != "GLOBALS" {
            let cell = state
                .globals
                .ensure_slot(name.clone(), Value::Uninitialized);
            let _ = frame
                .locals
                .bind_reference_cell(LocalId::new(index as u32), cell);
        }
    }
}

fn current_frame_is_top_level(compiled: &CompiledUnit, stack: &CallStack) -> bool {
    let Some(frame) = stack.current() else {
        return false;
    };
    compiled
        .unit()
        .functions
        .get(frame.function.index())
        .is_some_and(|function| function.flags.is_top_level)
}

fn auto_start_session_if_configured(state: &mut ExecutionState, source_span: RuntimeSourceSpan) {
    if !ini_bool(&state.ini, "session.auto_start")
        || state.request.session.status() == php_runtime::PHP_SESSION_ACTIVE
    {
        return;
    }
    if state.request.session.needs_lazy_load() {
        let id = state.request.session.id().to_owned();
        if let Some(loader) = &state.request.session_loader
            && let Ok(data) = loader.load(&id)
        {
            state.request.session.load_data(data);
        }
    }
    let id_length = session_sid_length_from_ini(&state.ini);
    let strict_mode = ini_bool(&state.ini, "session.use_strict_mode");
    state
        .request
        .session
        .start_with_policy(id_length, strict_mode);
    state.request.session.mark_started_automatically();
    let location = php_runtime::PhpDiagnosticLocation::from_span(&source_span);
    state
        .request
        .session
        .record_start_location(location.file, location.line);
    state
        .globals
        .set("_SESSION", state.request.session.data_value());
}

fn session_sid_length_from_ini(ini: &IniRegistry) -> usize {
    ini.get("session.sid_length")
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| (22..=256).contains(value))
        .unwrap_or(32)
}

fn ini_bool(ini: &IniRegistry, name: &str) -> bool {
    ini.get(name).is_some_and(|value| {
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "off" | "no"
        )
    })
}

fn seed_runtime_globals(globals: &mut GlobalSymbolTable, context: &RuntimeContext) {
    for name in [
        "argc", "argv", "_SERVER", "_ENV", "_GET", "_POST", "_COOKIE", "_FILES", "_REQUEST",
        "_SESSION",
    ] {
        if let Some(value) = context.global_value(name) {
            globals.set(name, value);
        }
    }
}

fn sync_session_state_from_globals(state: &mut ExecutionState) {
    let Some(Value::Array(array)) = state.globals.get("_SESSION") else {
        return;
    };
    state.request.session.set_data(array);
}

fn env_entries_array(entries: &[(String, String)]) -> PhpArray {
    let mut array = PhpArray::new();
    for (key, value) in entries {
        array.insert(
            ArrayKey::String(PhpString::from_test_str(key)),
            Value::string(value.clone()),
        );
    }
    array
}

fn set_env_entry(entries: &mut Vec<(String, String)>, key: String, value: Option<String>) {
    entries.retain(|(entry_key, _)| entry_key != &key);
    if let Some(value) = value {
        entries.push((key, value));
        entries.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    }
}

fn php_uname_value(mode: &str) -> String {
    match mode.chars().next().unwrap_or('a').to_ascii_lowercase() {
        's' => "Phrust".to_string(),
        'n' => "localhost".to_string(),
        'r' => php_source::reference_php_version().to_string(),
        'v' => "Stdlib".to_string(),
        'm' => "generic".to_string(),
        _ => format!(
            "Phrust localhost {} Stdlib generic",
            php_source::reference_php_version()
        ),
    }
}

fn script_owner_uid(script_path: Option<&str>) -> i64 {
    script_path
        .and_then(|path| fs::metadata(path).ok())
        .map_or_else(current_directory_uid, |metadata| {
            metadata_owner_uid(&metadata)
        })
}

fn script_owner_gid(script_path: Option<&str>) -> i64 {
    script_path
        .and_then(|path| fs::metadata(path).ok())
        .map_or_else(current_directory_gid, |metadata| {
            metadata_owner_gid(&metadata)
        })
}

#[cfg(unix)]
fn metadata_owner_uid(metadata: &fs::Metadata) -> i64 {
    use std::os::unix::fs::MetadataExt as _;

    metadata.uid() as i64
}

#[cfg(not(unix))]
fn metadata_owner_uid(_metadata: &fs::Metadata) -> i64 {
    0
}

#[cfg(unix)]
fn metadata_owner_gid(metadata: &fs::Metadata) -> i64 {
    use std::os::unix::fs::MetadataExt as _;

    metadata.gid() as i64
}

#[cfg(not(unix))]
fn metadata_owner_gid(_metadata: &fs::Metadata) -> i64 {
    0
}

fn current_directory_uid() -> i64 {
    fs::metadata(".").map_or(0, |metadata| metadata_owner_uid(&metadata))
}

fn current_directory_gid() -> i64 {
    fs::metadata(".").map_or(0, |metadata| metadata_owner_gid(&metadata))
}

fn validate_process_arity(name: &str, argc: usize) -> Option<String> {
    let valid = match name {
        "proc_open" => (3..=6).contains(&argc),
        "proc_close" | "proc_get_status" | "pclose" => argc == 1,
        "popen" => argc == 2,
        "shell_exec" | "system" => argc == 1,
        "exec" => (1..=3).contains(&argc),
        "passthru" => (1..=2).contains(&argc),
        _ => false,
    };
    if valid {
        None
    } else {
        Some(format!(
            "E_PHP_VM_PROCESS_ARITY: {name} received {argc} argument(s)"
        ))
    }
}

fn process_disabled_result(
    output: &OutputBuffer,
    name: &str,
    stack_trace: Vec<RuntimeStackFrame>,
) -> VmResult {
    process_warning_result(
        output,
        name,
        "E_PHP_VM_PROCESS_CAPABILITY_DISABLED",
        format!("{name}(): process execution is disabled by runtime capabilities"),
        process_failure_value(name),
        stack_trace,
    )
}

fn process_unsupported_mock_result(
    output: &OutputBuffer,
    name: &str,
    stack_trace: Vec<RuntimeStackFrame>,
) -> VmResult {
    process_warning_result(
        output,
        name,
        "E_PHP_VM_PROCESS_RESOURCE_MOCK_UNSUPPORTED",
        format!("{name}(): process resource APIs are not implemented by the standard-library mock"),
        process_failure_value(name),
        stack_trace,
    )
}

fn process_warning_result(
    _output: &OutputBuffer,
    _name: &str,
    id: &'static str,
    message: String,
    return_value: Value,
    stack_trace: Vec<RuntimeStackFrame>,
) -> VmResult {
    VmResult::success_with_diagnostics_no_output(
        Some(return_value),
        vec![RuntimeDiagnostic::new(
            id,
            RuntimeSeverity::Warning,
            message,
            RuntimeSourceSpan::default(),
            stack_trace,
            Some(php_runtime::PhpReferenceClassification::Warning),
        )],
    )
}

fn process_failure_value(name: &str) -> Value {
    match name {
        "shell_exec" | "passthru" => Value::Bool(false),
        _ => Value::Bool(false),
    }
}

fn process_output_lines_array(output: &str) -> Value {
    Value::packed_array(
        output
            .lines()
            .map(|line| Value::string(line.to_owned()))
            .collect(),
    )
}

fn process_last_output_line(output: &str) -> String {
    output.lines().last().unwrap_or_default().to_owned()
}

fn assign_process_ref_arg(
    stack: &mut CallStack,
    arg: &CallArgument,
    value: Value,
) -> Result<(), String> {
    let Some(local) = arg.by_ref_local else {
        return Ok(());
    };
    let frame = stack.current_mut().ok_or_else(|| {
        "E_PHP_VM_NO_ACTIVE_FRAME: cannot bind process reference argument".to_owned()
    })?;
    let _source = layout_source::enter(layout_source::BY_REF_ARGUMENT_BINDING);
    frame.locals.ensure_reference_cell(local)?.set(value);
    Ok(())
}

fn should_skip_top_level_auto_global_bind(
    function: &IrFunction,
    instruction: &Instruction,
) -> bool {
    let InstructionKind::BindGlobal { local, name } = &instruction.kind else {
        return false;
    };
    function.flags.is_top_level
        && is_auto_global_name(name)
        && function
            .locals
            .get(local.index())
            .is_some_and(|local_name| local_name == name)
}

fn is_auto_global_name(name: &str) -> bool {
    matches!(
        name,
        "argc"
            | "argv"
            | "_SERVER"
            | "_ENV"
            | "_GET"
            | "_POST"
            | "_COOKIE"
            | "_FILES"
            | "_REQUEST"
            | "_SESSION"
    )
}

fn bind_top_level_global_locals(
    function: &IrFunction,
    stack: &mut CallStack,
    state: &mut ExecutionState,
) {
    let Some(frame) = stack.current_mut() else {
        return;
    };
    for (index, name) in function.locals.iter().enumerate() {
        if name == "GLOBALS" {
            continue;
        }
        let cell = state
            .globals
            .ensure_slot(name.clone(), Value::Uninitialized);
        let _ = frame
            .locals
            .bind_reference_cell(LocalId::new(index as u32), cell);
    }
}

fn export_shared_locals_at_frame(
    function: &IrFunction,
    stack: &CallStack,
    frame_index: usize,
    shared: &mut HashMap<String, Slot>,
) {
    let Some(frame) = stack.frames().get(frame_index) else {
        return;
    };
    for (index, name) in function.locals.iter().enumerate() {
        if let Some(slot) = frame.locals.get_slot(LocalId::new(index as u32)) {
            shared.insert(name.clone(), slot.clone());
        }
    }
}

fn export_shared_locals(
    function: &IrFunction,
    stack: &CallStack,
    shared: &mut HashMap<String, Slot>,
) {
    let Some(frame_index) = stack.len().checked_sub(1) else {
        return;
    };
    export_shared_locals_at_frame(function, stack, frame_index, shared);
}

fn write_shared_locals_to_current_frame(
    compiled: &CompiledUnit,
    stack: &mut CallStack,
    shared: &HashMap<String, Slot>,
) {
    let Some(frame) = stack.current_mut() else {
        return;
    };
    let Some(function) = compiled.unit().functions.get(frame.function.index()) else {
        return;
    };
    for (index, name) in function.locals.iter().enumerate() {
        if let Some(slot) = shared.get(name) {
            let _ = frame
                .locals
                .set_slot(LocalId::new(index as u32), slot.clone());
        }
    }
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

fn constant_value(unit: &IrUnit, constant: ConstId) -> Result<Value, String> {
    let Some(value) = unit.constants.get(constant.index()) else {
        return Err(format!("invalid constant const:{}", constant.raw()));
    };
    Ok(match value {
        IrConstant::Null => Value::Null,
        IrConstant::Bool(value) => Value::Bool(*value),
        IrConstant::Int(value) => Value::Int(*value),
        IrConstant::Float(value) => Value::float(*value),
        IrConstant::String(value) => Value::String(PhpString::from_test_str(value)),
        IrConstant::StringBytes(value) => Value::String(PhpString::from_bytes(value.clone())),
        IrConstant::NamedConstant(name) => {
            return Err(format!(
                "E_PHP_VM_UNRESOLVED_CONSTANT_EXPR: constant {name} requires runtime resolution"
            ));
        }
        IrConstant::ClassConstant {
            class_name,
            constant_name,
        } => {
            return Err(format!(
                "E_PHP_VM_UNRESOLVED_CONSTANT_EXPR: constant {class_name}::{constant_name} requires runtime resolution"
            ));
        }
        IrConstant::Array(entries) => {
            let mut array = PhpArray::new();
            for entry in entries {
                let value = inline_constant_value(&entry.value);
                if let Some(key) = &entry.key {
                    let key_value = inline_constant_value(key);
                    if let Some(key) = ArrayKey::from_value(&key_value) {
                        array.insert(key, value);
                    } else {
                        array.append(value);
                    }
                } else {
                    array.append(value);
                }
            }
            Value::Array(array)
        }
    })
}

fn inline_constant_value(constant: &IrConstant) -> Value {
    match constant {
        IrConstant::Null => Value::Null,
        IrConstant::Bool(value) => Value::Bool(*value),
        IrConstant::Int(value) => Value::Int(*value),
        IrConstant::Float(value) => Value::float(*value),
        IrConstant::String(value) => Value::String(PhpString::from_test_str(value)),
        IrConstant::StringBytes(value) => Value::String(PhpString::from_bytes(value.clone())),
        IrConstant::NamedConstant(_) | IrConstant::ClassConstant { .. } => Value::Null,
        IrConstant::Array(entries) => {
            let mut array = PhpArray::new();
            for entry in entries {
                let value = inline_constant_value(&entry.value);
                if let Some(key) = &entry.key {
                    let key_value = inline_constant_value(key);
                    if let Some(key) = ArrayKey::from_value(&key_value) {
                        array.insert(key, value);
                    } else {
                        array.append(value);
                    }
                } else {
                    array.append(value);
                }
            }
            Value::Array(array)
        }
    }
}

fn array_key_from_value(value: &Value) -> Result<ArrayKey, String> {
    ArrayKey::from_value(value).ok_or_else(|| {
        if let Value::Object(object) = effective_value(value) {
            return format!(
                "E_PHP_VM_ARRAY_KEY_CONVERSION: Cannot access offset of type {} on array",
                object.display_name()
            );
        }
        format!(
            "E_PHP_VM_ARRAY_KEY_CONVERSION: cannot use {} as array key",
            value_type_name(value)
        )
    })
}

fn array_key_to_value(key: ArrayKey) -> Value {
    match key {
        ArrayKey::Int(value) => Value::Int(value),
        ArrayKey::String(value) => Value::String(value),
    }
}

fn clone_with_property_name(key: &ArrayKey) -> Result<String, String> {
    let ArrayKey::String(value) = key else {
        return Err(
            "E_PHP_VM_CLONE_WITH_PROPERTY_KEY: clone-with property names must be strings"
                .to_owned(),
        );
    };
    String::from_utf8(value.as_bytes().to_vec()).map_err(|_| {
        "E_PHP_VM_CLONE_WITH_PROPERTY_KEY: clone-with property name is not valid UTF-8".to_owned()
    })
}

/// Parse the leading PHP integer of a non-canonical string array key, used to
/// resolve string offsets like `$s["0foo"]`. Returns `None` for keys with no
/// leading integer (e.g. `"foo"`), which the caller treats as a non-numeric
/// offset.
fn leading_int_offset(bytes: &[u8]) -> Option<i64> {
    let mut index = 0;
    let mut negative = false;
    if matches!(bytes.first(), Some(b'-' | b'+')) {
        negative = bytes[0] == b'-';
        index = 1;
    }
    let digits_start = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == digits_start {
        return None;
    }
    let digits = std::str::from_utf8(&bytes[digits_start..index]).ok()?;
    let magnitude = digits.parse::<i64>().ok()?;
    Some(if negative { -magnitude } else { magnitude })
}

/// Read a single-byte string offset following PHP semantics: integer keys may be
/// negative (counted from the end), and string keys use their leading integer.
/// Returns `None` for out-of-range or non-numeric offsets.
fn string_offset_byte(string: &PhpString, key: &ArrayKey) -> Option<Value> {
    let index = match key {
        ArrayKey::Int(value) => *value,
        ArrayKey::String(value) => leading_int_offset(value.as_bytes())?,
    };
    let length = string.len() as i64;
    let resolved = if index < 0 { index + length } else { index };
    if resolved < 0 || resolved >= length {
        return None;
    }
    Some(Value::string(vec![string.as_bytes()[resolved as usize]]))
}

/// Outcome of reading a string offset, distinguishing the diagnostics PHP emits.
enum StringOffsetRead {
    /// In-range read with an integer (or canonical integer string) key.
    Byte(Value),
    /// Integer offset outside the string; PHP warns "Uninitialized string offset".
    OutOfRange(i64),
    /// Leading-integer string key (e.g. `"0foo"`); PHP warns "Illegal string offset".
    Illegal { value: Value, key_bytes: Vec<u8> },
    /// Non-numeric string key; PHP throws TypeError on read, false on isset.
    NonNumeric,
}

fn string_offset_for_read(string: &PhpString, key: &ArrayKey) -> StringOffsetRead {
    let (index, illegal_key) = match key {
        ArrayKey::Int(value) => (*value, None),
        ArrayKey::String(value) => match leading_int_offset(value.as_bytes()) {
            Some(index) => (index, Some(value.as_bytes().to_vec())),
            None => return StringOffsetRead::NonNumeric,
        },
    };
    let length = string.len() as i64;
    let resolved = if index < 0 { index + length } else { index };
    let byte = if resolved < 0 || resolved >= length {
        None
    } else {
        Some(Value::string(vec![string.as_bytes()[resolved as usize]]))
    };
    match (illegal_key, byte) {
        (Some(key_bytes), value) => StringOffsetRead::Illegal {
            value: value.unwrap_or_else(|| Value::string(Vec::new())),
            key_bytes,
        },
        (None, Some(value)) => StringOffsetRead::Byte(value),
        (None, None) => StringOffsetRead::OutOfRange(index),
    }
}

/// Rich-IR instruction kinds with a quickening candidate or guard arm in the
/// dispatch loop (int add/sub/mul, string concat, packed-array int-key fetch).
/// Observing any other kind cannot lead to a specialization: it only grows the
/// per-site ordered map with write-only entries on the dispatch hot path and
/// reports phantom `specialized` events once a site crosses the execution
/// threshold, so per-instruction observation is limited to these kinds.
fn rich_quickening_candidate_kind(kind: &InstructionKind) -> bool {
    matches!(
        kind,
        InstructionKind::Binary {
            op: BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Concat,
            ..
        } | InstructionKind::FetchDim { .. }
    )
}

/// Dense opcodes with a quickening candidate or guard arm in the dispatch
/// loop. Keeping this exhaustive allowlist next to the rich classifier avoids
/// creating and updating quickening entries for bytecode that can never
/// specialize.
fn dense_quickening_candidate_opcode(opcode: DenseOpcode) -> bool {
    matches!(
        opcode,
        DenseOpcode::BinaryAdd
            | DenseOpcode::BinarySub
            | DenseOpcode::BinaryMul
            | DenseOpcode::BinaryConcat
            | DenseOpcode::BinaryConcatEcho
            | DenseOpcode::JumpIfFalse
            | DenseOpcode::JumpIfTrue
            | DenseOpcode::JumpIf
    )
}

/// True when a function body can observe its call-argument vector: a
/// direct call to a func_get_args-style builtin, or any dynamic dispatch
/// or include/eval that could reach one. Bodies that cannot observe the
/// vector let calls skip the per-call argument snapshot entirely
/// (backtraces read the separately kept trace arguments).
fn function_body_observes_argument_vector(function: &IrFunction) -> bool {
    function.blocks.iter().any(|block| {
        block
            .instructions
            .iter()
            .any(|instruction| match &instruction.kind {
                InstructionKind::CallFunction { name, .. } => matches!(
                    normalize_function_name(name).as_str(),
                    "func_get_args" | "func_num_args" | "func_get_arg"
                ),
                InstructionKind::CallCallable { .. }
                | InstructionKind::Pipe { .. }
                | InstructionKind::AcquireCallable { .. }
                | InstructionKind::ResolveCallable { .. }
                | InstructionKind::Include { .. }
                | InstructionKind::Eval { .. } => true,
                _ => false,
            })
    })
}

/// Inline plan for a trivial method body.
#[derive(Clone, Debug, Eq, PartialEq)]
enum TrivialMethodPlan {
    /// `return $this->prop;`
    Getter { property: String },
    /// `$this->prop = $x;` optionally returning `$this`.
    Setter {
        property: String,
        returns_this: bool,
    },
}

/// Classifies bodies that are exactly a declared-property read or write on
/// `$this`, with plain positional untyped parameters and no control flow.
/// Everything else stays on generic dispatch.
fn classify_trivial_method(function: &IrFunction) -> Option<TrivialMethodPlan> {
    if !function.flags.is_method
        || function.flags.is_static
        || function.flags.is_generator
        || function.flags.is_closure
        || function.returns_by_ref
        || function.return_type.is_some()
        || !function.captures.is_empty()
        || function.blocks.len() != 1
        || function
            .params
            .iter()
            .any(|param| param.by_ref || param.variadic || param.type_.is_some())
    {
        return None;
    }
    let block = function.blocks.first()?;
    let terminator = block.terminator.as_ref()?;
    let instructions = &block.instructions;
    let this_local = LocalId::new(0);
    match (function.params.len(), instructions.as_slice()) {
        // LoadLocal $this; FetchProperty; return value
        (0, [load_this, fetch]) => {
            let InstructionKind::LoadLocal {
                dst: this_reg,
                local,
            } = &load_this.kind
            else {
                return None;
            };
            if *local != this_local {
                return None;
            }
            let InstructionKind::FetchProperty {
                dst: value_reg,
                object: Operand::Register(object_reg),
                property,
            } = &fetch.kind
            else {
                return None;
            };
            if object_reg != this_reg {
                return None;
            }
            let TerminatorKind::Return {
                value: Some(Operand::Register(returned)),
                by_ref_local: None,
            } = &terminator.kind
            else {
                return None;
            };
            (returned == value_reg).then(|| TrivialMethodPlan::Getter {
                property: property.clone(),
            })
        }
        // LoadLocal $this; LoadLocal $x; AssignProperty; Discard
        // [; LoadLocal $this] ; return [$this]
        (1, [load_this, load_param, assign, discard, rest @ ..]) => {
            let InstructionKind::LoadLocal {
                dst: this_reg,
                local,
            } = &load_this.kind
            else {
                return None;
            };
            if *local != this_local {
                return None;
            }
            let InstructionKind::LoadLocal {
                dst: param_reg,
                local: param_local,
            } = &load_param.kind
            else {
                return None;
            };
            if *param_local != LocalId::new(1) {
                return None;
            }
            let InstructionKind::AssignProperty {
                dst: assign_reg,
                object: Operand::Register(object_reg),
                property,
                value: Operand::Register(value_reg),
            } = &assign.kind
            else {
                return None;
            };
            if object_reg != this_reg || value_reg != param_reg {
                return None;
            }
            let InstructionKind::Discard {
                src: Operand::Register(discarded),
            } = &discard.kind
            else {
                return None;
            };
            if discarded != assign_reg {
                return None;
            }
            match (rest, &terminator.kind) {
                (
                    [],
                    TerminatorKind::Return {
                        value: None,
                        by_ref_local: None,
                    },
                ) => Some(TrivialMethodPlan::Setter {
                    property: property.clone(),
                    returns_this: false,
                }),
                (
                    [load_this_again],
                    TerminatorKind::Return {
                        value: Some(Operand::Register(returned)),
                        by_ref_local: None,
                    },
                ) => {
                    let InstructionKind::LoadLocal {
                        dst: this_again_reg,
                        local: this_again_local,
                    } = &load_this_again.kind
                    else {
                        return None;
                    };
                    (*this_again_local == this_local && returned == this_again_reg).then(|| {
                        TrivialMethodPlan::Setter {
                            property: property.clone(),
                            returns_this: true,
                        }
                    })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn fetch_dim_value(array: &Value, key: &ArrayKey) -> Result<Option<Value>, String> {
    if let Value::Reference(cell) = array {
        return fetch_dim_value(&cell.borrow(), key);
    }
    if let Value::Object(object) = array
        && spl_runtime_marker(object).is_some_and(|class| is_spl_caching_iterator_class(&class))
    {
        spl_caching_iterator_require_full_cache(object, &object.display_name())?;
        return spl_caching_iterator_offset_get(object, &array_key_to_value(key.clone())).map(Some);
    }
    if let Value::Object(object) = array
        && spl_runtime_marker(object).is_some_and(|class| is_spl_array_access_runtime_class(&class))
    {
        return spl_container_offset_get(object, &array_key_to_value(key.clone())).map(Some);
    }
    if let Value::String(string) = array {
        return Ok(string_offset_byte(string, key));
    }
    let Value::Array(array) = array else {
        return Err("E_PHP_VM_ARRAY_FETCH_TYPE: value is not an array".to_owned());
    };
    Ok(array.get(key).map(effective_value))
}

fn quiet_dim_fetch_scalar_returns_null(value: &Value) -> bool {
    matches!(
        value,
        Value::Null
            | Value::Bool(_)
            | Value::Int(_)
            | Value::Float(_)
            | Value::Uninitialized
            | Value::Resource(_)
    )
}

fn effective_value(value: &Value) -> Value {
    match value {
        Value::Reference(cell) => {
            let _source = layout_source::enter_default(layout_source::REFERENCE_DEREFERENCE);
            cell.get()
        }
        value => {
            let _source = layout_source::enter_default(layout_source::STACK_REGISTER_LOCAL_MOVE);
            value.clone()
        }
    }
}

fn effective_is_null_or_false(value: &Value) -> bool {
    match value {
        Value::Reference(cell) => effective_is_null_or_false(&cell.borrow()),
        Value::Null | Value::Bool(false) => true,
        _ => false,
    }
}

fn effective_is_uninitialized_or_null(value: &Value) -> bool {
    match value {
        Value::Reference(cell) => effective_is_uninitialized_or_null(&cell.borrow()),
        Value::Uninitialized | Value::Null => true,
        _ => false,
    }
}

fn effective_is_array(value: &Value) -> bool {
    match value {
        Value::Reference(cell) => effective_is_array(&cell.borrow()),
        Value::Array(_) => true,
        _ => false,
    }
}

fn curl_callback_is_enabled(value: &Value) -> bool {
    !effective_is_null_or_false(value)
}

fn collect_compact_variable_names(value: &Value, names: &mut Vec<String>) {
    match effective_value(value) {
        Value::Array(array) => {
            for (_, element) in array.iter() {
                collect_compact_variable_names(element, names);
            }
        }
        Value::String(name) if !name.is_empty() => names.push(name.to_string_lossy()),
        value => {
            if let Ok(name) = to_string(&value)
                && !name.is_empty()
            {
                names.push(name.to_string_lossy());
            }
        }
    }
}

fn cast_value_to_object(value: &Value) -> Value {
    match effective_value(value) {
        Value::Object(object) => Value::Object(object),
        Value::Null | Value::Uninitialized => Value::Object(ObjectRef::new_with_display_name(
            &std_class_entry(),
            "stdClass",
        )),
        Value::Array(array) => {
            let object = ObjectRef::new_with_display_name(&std_class_entry(), "stdClass");
            for (key, element) in array.iter() {
                object.set_property(object_cast_property_name(&key), effective_value(element));
            }
            Value::Object(object)
        }
        scalar => {
            let object = ObjectRef::new_with_display_name(&std_class_entry(), "stdClass");
            object.set_property("scalar", scalar);
            Value::Object(object)
        }
    }
}

fn cast_value_to_array(compiled: &CompiledUnit, stack: &CallStack, value: &Value) -> Value {
    match effective_value(value) {
        Value::Array(array) => Value::Array(array),
        Value::Null | Value::Uninitialized => Value::Array(PhpArray::new()),
        Value::Object(object) => Value::Array(object_vars_array(compiled, stack, &object, true)),
        Value::Callable(callable) if matches!(callable.as_ref(), CallableValue::Closure(_)) => {
            Value::packed_array(vec![Value::Callable(callable)])
        }
        scalar => Value::packed_array(vec![scalar]),
    }
}

fn object_cast_property_name(key: &ArrayKey) -> String {
    match key {
        ArrayKey::Int(value) => value.to_string(),
        ArrayKey::String(value) => value.to_string_lossy(),
    }
}

/// Borrowed mirror of [`fetch_dim_path_value`] for read-only predicates.
///
/// Walks `value[dims...]` by reference and applies `f` to the leaf (`None`
/// when a dimension is missing, mirroring `fetch_dim_path_value`'s `Ok(None)`
/// results). Returns `None` — caller must use the cloning path — only for
/// shapes whose reads have side effects or interior borrows the borrowed walk
/// cannot model: SimpleXML dimension access and reference cells that are
/// currently mutably borrowed. Cloning containers for mere isset/empty
/// probes is what shares array handles and forces copy-on-write separations
/// on the next write, so hot registry checks must stay on this path.
fn with_borrowed_dim_path<R>(
    value: &Value,
    dims: &[ArrayKey],
    f: &mut dyn FnMut(Option<&Value>) -> R,
) -> Option<R> {
    match value {
        Value::Reference(cell) => cell
            .try_with_value(|inner| with_borrowed_dim_path(inner, dims, f))
            .ok()
            .flatten(),
        _ => {
            let Some((first, rest)) = dims.split_first() else {
                return Some(f(Some(value)));
            };
            match value {
                Value::Array(array) => match array.get(first) {
                    Some(child) => with_borrowed_dim_path(child, rest, f),
                    None => Some(f(None)),
                },
                Value::String(string) => match string_offset_byte(string, first) {
                    Some(byte) => with_borrowed_dim_path(&byte, rest, f),
                    None => Some(f(None)),
                },
                Value::Object(object) if is_simplexml_object(object) => None,
                _ => Some(f(None)),
            }
        }
    }
}

fn fetch_dim_path_value(value: &Value, dims: &[ArrayKey]) -> Result<Option<Value>, String> {
    let mut current = effective_value(value);
    for key in dims {
        match &current {
            Value::Array(array) => {
                let Some(next) = array.get(key) else {
                    return Ok(None);
                };
                current = effective_value(next);
            }
            Value::String(string) => {
                let Some(next) = string_offset_byte(string, key) else {
                    return Ok(None);
                };
                current = next;
            }
            Value::Object(object) if is_simplexml_object(object) => {
                let next = php_runtime::xml::simplexml_dimension(object, key);
                if matches!(next, Value::Null) {
                    return Ok(None);
                }
                current = effective_value(&next);
            }
            _ => return Ok(None),
        }
    }
    Ok(Some(current))
}

fn spl_array_access_dim_target(value: &Value, dims: &[ArrayKey]) -> Option<(ObjectRef, Value)> {
    let [key] = dims else {
        return None;
    };
    let Value::Object(object) = effective_value(value) else {
        return None;
    };
    spl_runtime_marker(&object)
        .is_some_and(|class| is_spl_array_access_runtime_class(&class))
        .then(|| (object, array_key_to_value(key.clone())))
}

fn read_dim_operands_at_frame(
    unit: &IrUnit,
    stack: &CallStack,
    frame_index: usize,
    dims: &[Operand],
) -> Result<Vec<ArrayKey>, String> {
    let values = read_dim_operand_values_at_frame(unit, stack, frame_index, dims)?;
    dim_values_to_array_keys(&values)
}

fn read_dim_operand_values_at_frame(
    unit: &IrUnit,
    stack: &CallStack,
    frame_index: usize,
    dims: &[Operand],
) -> Result<Vec<Value>, String> {
    dims.iter()
        .map(|operand| read_operand_at_frame(unit, stack, frame_index, *operand))
        .collect()
}

fn dim_values_to_array_keys(values: &[Value]) -> Result<Vec<ArrayKey>, String> {
    values.iter().map(array_key_from_value).collect()
}

fn spl_object_storage_local_object(stack: &CallStack, local: LocalId) -> Option<ObjectRef> {
    let Value::Object(object) = effective_value(&read_local_value(stack, local)?) else {
        return None;
    };
    (normalize_class_name(&object.class_name()) == "splobjectstorage").then_some(object)
}

fn spl_object_storage_local_object_at_frame(
    stack: &CallStack,
    frame_index: usize,
    local: LocalId,
) -> Option<ObjectRef> {
    let Value::Object(object) =
        effective_value(&read_local_value_at_frame(stack, frame_index, local)?)
    else {
        return None;
    };
    (normalize_class_name(&object.class_name()) == "splobjectstorage").then_some(object)
}

fn spl_multiple_iterator_local_object(stack: &CallStack, local: LocalId) -> Option<ObjectRef> {
    let Value::Object(object) = effective_value(&read_local_value(stack, local)?) else {
        return None;
    };
    (spl_runtime_marker(&object).as_deref() == Some("multipleiterator")).then_some(object)
}

fn spl_multiple_iterator_local_object_at_frame(
    stack: &CallStack,
    frame_index: usize,
    local: LocalId,
) -> Option<ObjectRef> {
    let Value::Object(object) =
        effective_value(&read_local_value_at_frame(stack, frame_index, local)?)
    else {
        return None;
    };
    (spl_runtime_marker(&object).as_deref() == Some("multipleiterator")).then_some(object)
}

fn spl_array_access_local_object_at_frame(
    stack: &CallStack,
    frame_index: usize,
    local: LocalId,
) -> Option<ObjectRef> {
    let Value::Object(object) =
        effective_value(&read_local_value_at_frame(stack, frame_index, local)?)
    else {
        return None;
    };
    spl_runtime_marker(&object)
        .is_some_and(|class| is_spl_array_access_runtime_class(&class))
        .then_some(object)
}

fn read_local_value(stack: &CallStack, local: LocalId) -> Option<Value> {
    stack.current()?.locals.get(local)
}

fn read_local_value_at_frame(
    stack: &CallStack,
    frame_index: usize,
    local: LocalId,
) -> Option<Value> {
    stack.frames().get(frame_index)?.locals.get(local)
}

fn local_slot_is_in_bounds(stack: &CallStack, local: LocalId) -> bool {
    stack
        .current()
        .is_some_and(|frame| frame.locals.contains(local))
}

fn local_slot_is_in_bounds_at_frame(stack: &CallStack, frame_index: usize, local: LocalId) -> bool {
    stack
        .frames()
        .get(frame_index)
        .is_some_and(|frame| frame.locals.contains(local))
}

fn local_alias_state(stack: &CallStack, local: LocalId) -> AliasState {
    stack
        .current()
        .and_then(|frame| frame.locals.get_slot(local))
        .map(slot_alias_state)
        .unwrap_or(AliasState::UnknownAliasing)
}

fn local_alias_state_at_frame(stack: &CallStack, frame_index: usize, local: LocalId) -> AliasState {
    stack
        .frames()
        .get(frame_index)
        .and_then(|frame| frame.locals.get_slot(local))
        .map(slot_alias_state)
        .unwrap_or(AliasState::UnknownAliasing)
}

fn local_array_is_packed_fast(stack: &CallStack, local: LocalId) -> bool {
    stack
        .current()
        .and_then(|frame| frame.locals.get_slot(local))
        .is_some_and(slot_effective_array_is_packed_fast)
}

fn local_array_is_packed_fast_at_frame(
    stack: &CallStack,
    frame_index: usize,
    local: LocalId,
) -> bool {
    stack
        .frames()
        .get(frame_index)
        .and_then(|frame| frame.locals.get_slot(local))
        .is_some_and(slot_effective_array_is_packed_fast)
}

/// Checks packed-array layout through a local slot without cloning its array
/// handle. The prior read/effective-value path cloned ordinary arrays twice
/// around every dimension write merely to inspect their layout.
fn slot_effective_array_is_packed_fast(slot: &Slot) -> bool {
    fn value_is_packed_after_one_deref(value: &Value) -> bool {
        match value {
            Value::Array(array) => array.is_packed_fast(),
            Value::Reference(cell) => cell
                .try_with_value(
                    |value| matches!(value, Value::Array(array) if array.is_packed_fast()),
                )
                .unwrap_or(false),
            _ => false,
        }
    }

    match slot {
        Slot::Value(value) => value_is_packed_after_one_deref(value),
        Slot::Reference(cell) => cell
            .try_with_value(value_is_packed_after_one_deref)
            .unwrap_or(false),
    }
}

/// Returns a local's effective object handle without cloning non-object values.
/// Array-dimension writes use this to probe for userland `ArrayAccess`; normal
/// arrays and scalars stay borrowed and allocate no transient value handle.
fn local_effective_object(stack: &CallStack, local: LocalId) -> Option<ObjectRef> {
    fn object_after_one_deref(value: &Value) -> Option<ObjectRef> {
        match value {
            Value::Object(object) => Some(object.clone()),
            Value::Reference(cell) => cell
                .try_with_value(|value| match value {
                    Value::Object(object) => Some(object.clone()),
                    _ => None,
                })
                .ok()
                .flatten(),
            _ => None,
        }
    }

    match stack.current()?.locals.get_slot(local)? {
        Slot::Value(value) => object_after_one_deref(value),
        Slot::Reference(cell) => cell.try_with_value(object_after_one_deref).ok().flatten(),
    }
}

fn local_array_has_cow_or_reference_fallback(stack: &CallStack, local: LocalId) -> bool {
    let Some(slot) = stack
        .current()
        .and_then(|frame| frame.locals.get_slot(local))
    else {
        return false;
    };
    match slot {
        Slot::Value(Value::Array(array)) => array.is_shared() || array.contains_references_fast(),
        Slot::Reference(cell) => cell
            .try_with_value(|value| match value {
                Value::Array(array) => array.is_shared() || array.contains_references_fast(),
                _ => true,
            })
            .unwrap_or(true),
        _ => false,
    }
}

fn local_array_has_cow_or_reference_fallback_at_frame(
    stack: &CallStack,
    frame_index: usize,
    local: LocalId,
) -> bool {
    let Some(slot) = stack
        .frames()
        .get(frame_index)
        .and_then(|frame| frame.locals.get_slot(local))
    else {
        return false;
    };
    match slot {
        Slot::Value(Value::Array(array)) => array.is_shared() || array.contains_references_fast(),
        Slot::Reference(cell) => cell
            .try_with_value(|value| match value {
                Value::Array(array) => array.is_shared() || array.contains_references_fast(),
                _ => true,
            })
            .unwrap_or(true),
        _ => false,
    }
}

fn is_this_local(function: &IrFunction, local: LocalId) -> bool {
    function
        .locals
        .get(local.index())
        .is_some_and(|name| name == "this")
}

fn is_globals_local(function: &IrFunction, local: LocalId) -> bool {
    function
        .locals
        .get(local.index())
        .is_some_and(|name| name == "GLOBALS")
}

enum ExactEchoBatchPart {
    Bytes(Vec<u8>),
    Empty,
}

fn exact_echo_batch_part(value: &Value) -> Option<ExactEchoBatchPart> {
    match value {
        Value::String(value) => Some(ExactEchoBatchPart::Bytes(value.as_bytes().to_vec())),
        Value::Int(value) => Some(ExactEchoBatchPart::Bytes(value.to_string().into_bytes())),
        Value::Bool(true) => Some(ExactEchoBatchPart::Bytes(b"1".to_vec())),
        Value::Bool(false) | Value::Null => Some(ExactEchoBatchPart::Empty),
        Value::Float(_)
        | Value::Array(_)
        | Value::Object(_)
        | Value::Resource(_)
        | Value::Reference(_)
        | Value::Callable(_)
        | Value::Fiber(_)
        | Value::Generator(_)
        | Value::Uninitialized => None,
    }
}

fn collect_exact_echo_batch_at_frame(
    vm: &Vm,
    unit: &IrUnit,
    stack: &CallStack,
    frame_index: usize,
    instructions: &[Instruction],
    instruction_index: usize,
    first_value: &Value,
) -> Option<(Vec<ExactEchoBatchPart>, usize)> {
    let mut parts = vec![exact_echo_batch_part(first_value)?];
    let mut next_index = instruction_index + 1;
    while let Some(instruction) = instructions.get(next_index) {
        match &instruction.kind {
            InstructionKind::Echo { src } => {
                let Ok(value) = read_operand_ref_at_frame(unit, stack, frame_index, *src) else {
                    break;
                };
                let Some(part) = exact_echo_batch_part(value.as_value()) else {
                    break;
                };
                parts.push(part);
                next_index += 1;
            }
            InstructionKind::LoadConst { dst, constant } => {
                let Some(next) = instructions.get(next_index + 1) else {
                    break;
                };
                let InstructionKind::Echo { src } = &next.kind else {
                    break;
                };
                if !matches!(src, Operand::Register(register) if *register == *dst) {
                    break;
                }
                let Ok(value) = vm.constant_value(unit, *constant) else {
                    break;
                };
                let Some(part) = exact_echo_batch_part(&value) else {
                    break;
                };
                parts.push(part);
                next_index += 2;
            }
            _ => break,
        }
    }
    Some((parts, next_index))
}

fn write_exact_echo_batch(output: &mut OutputBuffer, parts: &[ExactEchoBatchPart]) {
    let slices = parts
        .iter()
        .filter_map(|part| match part {
            ExactEchoBatchPart::Bytes(bytes) if !bytes.is_empty() => Some(bytes.as_slice()),
            ExactEchoBatchPart::Bytes(_) | ExactEchoBatchPart::Empty => None,
        })
        .collect::<Vec<_>>();
    output.write_fast_slices(&slices);
}

#[cold]
fn concat_fallback_reason(lhs: &Value, rhs: &Value) -> Option<&'static str> {
    concat_operand_fallback_reason(lhs).or_else(|| concat_operand_fallback_reason(rhs))
}

#[cold]
fn concat_operand_fallback_reason(value: &Value) -> Option<&'static str> {
    match value {
        Value::String(_) => None,
        Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) => Some("scalar_conversion"),
        Value::Array(_) => Some("array_conversion_warning"),
        Value::Object(_) | Value::Fiber(_) | Value::Generator(_) => Some("object_to_string"),
        Value::Resource(_) => Some("resource_conversion"),
        Value::Reference(_) => Some("reference_deref"),
        Value::Callable(_) => Some("callable_conversion_error"),
        Value::Uninitialized => Some("uninitialized_conversion_error"),
    }
}

fn try_execute_dense_pcre_ascii_offset_block_fast_path(
    compiled: &CompiledUnit,
    dense: &DenseBytecodeUnit,
    instructions: &[DenseInstruction],
    stack: &mut CallStack,
    state: &mut ExecutionState,
) -> Result<Option<(u32, bool)>, String> {
    let mut active = [None; 8];
    let mut active_len = 0_usize;
    for instruction in instructions {
        if instruction.opcode == DenseOpcode::Nop {
            continue;
        }
        if active_len == active.len() {
            return Ok(None);
        }
        active[active_len] = Some(instruction);
        active_len += 1;
    }
    if !(5..=8).contains(&active_len) {
        return Ok(None);
    }
    let Some(first_active) = active[0] else {
        return Ok(None);
    };
    let Some((pattern_reg, pattern_const)) = dense_load_const_register(first_active) else {
        return Ok(None);
    };
    if !dense_constant_string_bytes_eq(compiled, pattern_const, br"/\G\w/u") {
        return Ok(None);
    }

    let mut cursor = 1;
    let Some(subject_active) = active[cursor] else {
        return Ok(None);
    };
    let Some((subject_reg, subject_local, fused_flags)) =
        dense_load_local_register_with_optional_const(subject_active)
    else {
        return Ok(None);
    };
    cursor += 1;

    let flags_reg = if let Some((flags_reg, flags_const)) = fused_flags {
        if !dense_constant_exact_int(compiled, flags_const, 0) {
            return Ok(None);
        }
        flags_reg
    } else {
        let Some((flags_reg, flags_const)) = active[cursor].and_then(dense_load_const_register)
        else {
            return Ok(None);
        };
        if !dense_constant_exact_int(compiled, flags_const, 0) {
            return Ok(None);
        }
        cursor += 1;
        flags_reg
    };

    let Some((offset_reg, offset_local, None)) =
        active[cursor].and_then(dense_load_local_register_with_optional_const)
    else {
        return Ok(None);
    };
    cursor += 1;

    let Some((call_dst, name, args)) = active[cursor].and_then(dense_call_function_operands) else {
        return Ok(None);
    };
    cursor += 1;

    while let Some(instruction) = active.get(cursor).and_then(|instruction| *instruction) {
        if instruction.opcode != DenseOpcode::Discard {
            break;
        }
        cursor += 1;
    }

    let Some((condition, if_true, if_false)) = active[cursor].and_then(dense_jump_if_operands)
    else {
        return Ok(None);
    };
    if condition.kind != DenseOperandKind::Register || condition.index != call_dst {
        return Ok(None);
    }

    let Some(name) = dense.names.get(name as usize) else {
        return Ok(None);
    };
    if !name.eq_ignore_ascii_case("preg_match")
        || name.contains('\\')
        || args.len() != 5
        || args.iter().any(|arg| arg.name.is_some())
    {
        return Ok(None);
    }

    if !dense_operand_is_register(args[0].value, pattern_reg)
        || !dense_operand_is_register(args[1].value, subject_reg)
        || !dense_operand_is_register(args[3].value, flags_reg)
        || !dense_operand_is_register(args[4].value, offset_reg)
    {
        return Ok(None);
    }
    let Some(call_subject_local) = dense_plain_by_ref_local(&args[1]) else {
        return Ok(None);
    };
    if call_subject_local != subject_local {
        return Ok(None);
    }
    let Some(matches_local) = dense_plain_by_ref_local(&args[2]) else {
        return Ok(None);
    };
    let Some(call_offset_local) = dense_plain_by_ref_local(&args[4]) else {
        return Ok(None);
    };
    if call_offset_local != offset_local {
        return Ok(None);
    }

    let Some(offset) = dense_local_exact_int(stack, LocalId::new(offset_local)) else {
        return Ok(None);
    };
    if offset < 0 {
        return Ok(None);
    }
    let start = offset as usize;
    let Some(match_result) =
        with_dense_local_string(stack, LocalId::new(subject_local), |subject| {
            let subject_bytes = subject.as_bytes();
            if start > subject_bytes.len() {
                return Ok(None);
            }
            if !state
                .builtins
                .pcre_state_mut()
                .cache_mut()
                .validate_utf8_ascii_subject_at_offset(subject, start)
                .map_err(|error| error.message().to_owned())?
            {
                return Ok(None);
            }
            Ok(Some(match subject_bytes.get(start).copied() {
                Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_' => {
                    DensePcreAsciiOffsetBlockMatch::Matched(byte)
                }
                _ => DensePcreAsciiOffsetBlockMatch::NoMatch,
            }))
        })?
    else {
        return Ok(None);
    };

    let matches = stack
        .current_mut()
        .ok_or_else(|| "no active frame".to_owned())?
        .locals
        .ensure_reference_cell(LocalId::new(matches_local))?;
    state.builtins.pcre_state_mut().last_error_mut().clear();
    let truthy = match match_result {
        DensePcreAsciiOffsetBlockMatch::Matched(byte) => {
            builtin_intrinsics::set_preg_match_single_byte_match(&matches, &[byte]);
            true
        }
        DensePcreAsciiOffsetBlockMatch::NoMatch => {
            builtin_intrinsics::set_preg_match_empty_matches(&matches);
            false
        }
    };
    let next_block = if truthy { if_true } else { if_false };
    Ok(Some((next_block, truthy)))
}

fn dense_load_const_register(instruction: &DenseInstruction) -> Option<(u32, u32)> {
    if instruction.opcode != DenseOpcode::LoadConst {
        return None;
    }
    let DenseOperands::RegConst { dst, constant } = instruction.operands else {
        return None;
    };
    Some((dst, constant))
}

type DenseLoadLocalInfo = (u32, u32, Option<(u32, u32)>);

fn dense_load_local_register_with_optional_const(
    instruction: &DenseInstruction,
) -> Option<DenseLoadLocalInfo> {
    match instruction.opcode {
        DenseOpcode::LoadLocal => {
            let DenseOperands::RegOperand { dst, src } = instruction.operands else {
                return None;
            };
            if src.kind != DenseOperandKind::Local {
                return None;
            }
            Some((dst, src.index, None))
        }
        DenseOpcode::LoadLocalLoadConst => {
            let DenseOperands::LoadLocalLoadConst {
                first_dst,
                local,
                second_dst,
                constant,
            } = instruction.operands
            else {
                return None;
            };
            Some((first_dst, local.index, Some((second_dst, constant))))
        }
        _ => None,
    }
}

fn dense_call_function_operands(
    instruction: &DenseInstruction,
) -> Option<(u32, u32, &[DenseCallArg])> {
    if instruction.opcode != DenseOpcode::CallFunction {
        return None;
    }
    let DenseOperands::Call {
        dst,
        name,
        ref args,
    } = instruction.operands
    else {
        return None;
    };
    Some((dst, name, args))
}

fn dense_jump_if_operands(instruction: &DenseInstruction) -> Option<(DenseOperand, u32, u32)> {
    if instruction.opcode != DenseOpcode::JumpIf {
        return None;
    }
    let DenseOperands::JumpIfElse {
        condition,
        if_true,
        if_false,
    } = instruction.operands
    else {
        return None;
    };
    Some((condition, if_true, if_false))
}

fn dense_operand_is_register(operand: DenseOperand, register: u32) -> bool {
    operand.kind == DenseOperandKind::Register && operand.index == register
}

fn dense_constant_string_bytes_eq(compiled: &CompiledUnit, constant: u32, expected: &[u8]) -> bool {
    compiled
        .unit()
        .constants
        .get(constant as usize)
        .is_some_and(|constant| match constant {
            IrConstant::String(value) => value.as_bytes() == expected,
            IrConstant::StringBytes(value) => value.as_slice() == expected,
            _ => false,
        })
}

fn dense_constant_exact_int(compiled: &CompiledUnit, constant: u32, expected: i64) -> bool {
    compiled
        .unit()
        .constants
        .get(constant as usize)
        .is_some_and(|constant| matches!(constant, IrConstant::Int(value) if *value == expected))
}

fn with_dense_local_string<T>(
    stack: &CallStack,
    local: LocalId,
    f: impl FnOnce(&PhpString) -> Result<Option<T>, String>,
) -> Result<Option<T>, String> {
    let Some(slot) = stack
        .current()
        .and_then(|frame| frame.locals.get_slot(local))
    else {
        return Ok(None);
    };
    match slot {
        Slot::Value(Value::String(value)) => f(value),
        Slot::Reference(cell) => cell
            .try_with_value(|value| match value {
                Value::String(value) => f(value),
                _ => Ok(None),
            })
            .unwrap_or_else(|message| Err(message.to_string())),
        _ => Ok(None),
    }
}

enum DensePcreAsciiOffsetBlockMatch {
    Matched(u8),
    NoMatch,
}

fn dense_plain_by_ref_local(arg: &DenseCallArg) -> Option<u32> {
    if arg.by_ref_dim.is_none()
        && arg.by_ref_property.is_none()
        && arg.by_ref_property_dim.is_none()
    {
        arg.by_ref_local
    } else {
        None
    }
}

fn dense_local_exact_int(stack: &CallStack, local: LocalId) -> Option<i64> {
    stack
        .current()
        .and_then(|frame| frame.locals.get_slot(local))
        .and_then(slot_exact_int)
}

fn slot_exact_int(slot: &Slot) -> Option<i64> {
    match slot {
        Slot::Value(Value::Int(value)) => Some(*value),
        Slot::Reference(cell) => cell
            .try_with_value(|value| match value {
                Value::Int(value) => Some(*value),
                _ => None,
            })
            .unwrap_or(None),
        _ => None,
    }
}

fn emit_spl_array_access_bind_reference_notice(
    compiled: &CompiledUnit,
    output: &mut OutputBuffer,
    stack: &CallStack,
    state: &mut ExecutionState,
    object: &ObjectRef,
    span: php_ir::IrSpan,
) {
    let diagnostic = RuntimeDiagnostic::new(
        "E_PHP_VM_ARRAY_ACCESS_BIND_REFERENCE_NOTICE",
        RuntimeSeverity::Notice,
        format!(
            "Indirect modification of overloaded element of {} has no effect",
            object.display_name()
        ),
        runtime_source_span(compiled, span),
        stack_trace(compiled, stack),
        Some(php_runtime::PhpReferenceClassification::Warning),
    );
    if error_reporting_allows(state, php_runtime::PHP_E_NOTICE) {
        let leading_newline = !output.as_bytes().is_empty();
        emit_vm_diagnostic_with_options(
            output,
            state,
            &diagnostic,
            php_runtime::PhpDiagnosticChannel::Notice,
            php_runtime::PHP_E_NOTICE,
            leading_newline,
        );
        state.diagnostics.push(diagnostic);
    }
}

fn emit_static_property_as_non_static_notice(
    compiled: &CompiledUnit,
    output: &mut OutputBuffer,
    stack: &CallStack,
    state: &mut ExecutionState,
    class: &php_ir::module::ClassEntry,
    property: &php_ir::module::ClassPropertyEntry,
    span: php_ir::IrSpan,
) {
    let diagnostic = RuntimeDiagnostic::new(
        "E_PHP_VM_STATIC_PROPERTY_AS_NON_STATIC_NOTICE",
        RuntimeSeverity::Notice,
        format!(
            "Accessing static property {}::${} as non static",
            class.display_name, property.name
        ),
        runtime_source_span(compiled, span),
        stack_trace(compiled, stack),
        Some(php_runtime::PhpReferenceClassification::Warning),
    );
    if error_reporting_allows(state, php_runtime::PHP_E_NOTICE) {
        emit_vm_diagnostic(
            output,
            state,
            &diagnostic,
            php_runtime::PhpDiagnosticChannel::Notice,
            php_runtime::PHP_E_NOTICE,
        );
        state.diagnostics.push(diagnostic);
    }
}

/// How `assign_dim_local` reached the container, for slot-fast counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AssignDimLocalPath {
    /// Mutated the slot (or its reference cell) in place — no transient
    /// handle clone, so copy-on-write only separates for real sharing.
    InPlace,
    /// The reference cell was already borrowed; used the clone-based
    /// read/mutate/write-back path.
    ClonedReferenceFallback,
}

fn assign_dim_local(
    stack: &mut CallStack,
    local: LocalId,
    dims: &[ArrayKey],
    value: Value,
    append: bool,
) -> Result<AssignDimLocalPath, String> {
    let frame = stack.current_mut().ok_or("no active frame")?;
    let Some(slot) = frame.locals.get_slot_mut(local) else {
        return Err(format!("invalid local local:{}", local.raw()));
    };
    let mut pending = Some(value);
    let in_place = slot.try_with_effective_value_mut(|current| {
        if matches!(current, Value::Uninitialized | Value::Null) {
            *current = Value::Array(PhpArray::new());
        }
        assign_dim_value(
            current,
            dims,
            pending.take().expect("assign value consumed once"),
            append,
        )
    });
    if let Some(result) = in_place {
        return result.map(|()| AssignDimLocalPath::InPlace);
    }
    let mut current = slot.read();
    if matches!(current, Value::Uninitialized | Value::Null) {
        current = Value::Array(PhpArray::new());
    }
    assign_dim_value(
        &mut current,
        dims,
        pending.take().expect("assign value still pending"),
        append,
    )?;
    slot.write(current);
    Ok(AssignDimLocalPath::ClonedReferenceFallback)
}

fn assign_globals_dim(
    globals: &mut GlobalSymbolTable,
    dims: &[ArrayKey],
    value: Value,
    append: bool,
) -> Result<(), String> {
    if append {
        return Err(
            "E_PHP_VM_GLOBALS_APPEND_GAP: appending directly to $GLOBALS is not implemented"
                .to_owned(),
        );
    }
    let Some((first, rest)) = dims.split_first() else {
        return Err("E_PHP_VM_GLOBALS_ASSIGN_DIM: missing $GLOBALS key".to_owned());
    };
    let ArrayKey::String(name) = first else {
        return Err(
            "E_PHP_VM_GLOBALS_ASSIGN_KEY: $GLOBALS keys must be strings in runtime-semantics"
                .to_owned(),
        );
    };
    let name = name.to_string();
    if rest.is_empty() {
        globals.set(name, value);
        return Ok(());
    }
    let cell = globals.ensure_slot(name, Value::Array(PhpArray::new()));
    let mut current = cell.get();
    if matches!(current, Value::Uninitialized | Value::Null) {
        current = Value::Array(PhpArray::new());
    }
    assign_dim_value(&mut current, rest, value, false)?;
    cell.set(current);
    Ok(())
}

fn unset_globals_dim(globals: &mut GlobalSymbolTable, dims: &[ArrayKey]) -> Result<(), String> {
    let Some((first, rest)) = dims.split_first() else {
        return Ok(());
    };
    let ArrayKey::String(name) = first else {
        return Ok(());
    };
    let Some(cell) = globals.get_slot(&name.to_string()) else {
        return Ok(());
    };
    if rest.is_empty() {
        cell.set(Value::Uninitialized);
        return Ok(());
    }
    let mut current = cell.get();
    unset_dim_value(&mut current, rest);
    cell.set(current);
    Ok(())
}

/// Write a single byte into a string offset, following PHP semantics: the first
/// byte of the value replaces the byte at the index, the string is padded with
/// spaces when the index is past the end, and only a single integer dimension is
/// allowed.
fn write_string_offset(
    mut bytes: Vec<u8>,
    dims: &[ArrayKey],
    value: Value,
    append: bool,
) -> Result<Vec<u8>, String> {
    if append {
        return Err("E_PHP_VM_STRING_APPEND: [] operator not supported for strings".to_owned());
    }
    let [key] = dims else {
        return Err(
            "E_PHP_VM_STRING_OFFSET_NESTED: cannot use a nested write on a string offset"
                .to_owned(),
        );
    };
    let index = match key {
        ArrayKey::Int(value) => *value,
        ArrayKey::String(value) => leading_int_offset(value.as_bytes()).ok_or_else(|| {
            "E_PHP_VM_STRING_OFFSET_TYPE: Cannot access offset of type string on string".to_owned()
        })?,
    };
    let length = bytes.len() as i64;
    let resolved = if index < 0 { index + length } else { index };
    if resolved < 0 {
        return Err(format!(
            "E_PHP_VM_STRING_OFFSET_NEGATIVE: Illegal string offset {index}"
        ));
    }
    let replacement =
        to_string(&value).map_err(|message| format!("E_PHP_VM_STRING_OFFSET_VALUE: {message}"))?;
    let Some(&first) = replacement.as_bytes().first() else {
        return Err(
            "E_PHP_VM_STRING_OFFSET_EMPTY: Cannot assign an empty string to a string offset"
                .to_owned(),
        );
    };
    let index = resolved as usize;
    if index >= bytes.len() {
        bytes.resize(index + 1, b' ');
    }
    bytes[index] = first;
    Ok(bytes)
}

/// Outcome of attempting an in-place property dimension assignment.
enum PropertyDimInPlace {
    /// The property held an array and `assign_dim_value` ran directly on
    /// the stored value; carries that call's result.
    Applied(Result<(), String>),
    /// The property is missing, holds a non-array value, or the object
    /// storage is unavailable; the caller must run the generic
    /// read-clone → assign → write-back path.
    NotEligible,
}

/// Assigns `object->property[dims...] = value` directly on the stored
/// property value when it currently holds an array. This avoids the generic
/// path's property read (which shares the array handle) followed by
/// `assign_dim_value` on the copy (which then deep-copies the entire array
/// storage through a COW separation) on every nested write.
///
/// Callers gate on property shape first: untyped, non-readonly,
/// non-hooked properties only, with visibility already validated. Non-array
/// slot values (objects, references, strings, scalars, null) report
/// `NotEligible` so ArrayAccess dispatch, string offsets, vivification and
/// error behavior stay on the generic path unchanged.
fn try_assign_property_dim_in_place(
    object: &ObjectRef,
    storage_name: &str,
    dims: &[ArrayKey],
    value: Value,
    append: bool,
) -> PropertyDimInPlace {
    let mut pending = Some(value);
    match object.try_modify_property_value(storage_name, |slot| {
        if !matches!(slot, Value::Array(_)) {
            return None;
        }
        Some(assign_dim_value(
            slot,
            dims,
            pending.take().expect("assign value consumed once"),
            append,
        ))
    }) {
        Ok(Some(Some(result))) => PropertyDimInPlace::Applied(result),
        Ok(Some(None)) | Ok(None) | Err(_) => PropertyDimInPlace::NotEligible,
    }
}

fn assign_dim_value(
    container: &mut Value,
    dims: &[ArrayKey],
    value: Value,
    append: bool,
) -> Result<(), String> {
    if let Value::Reference(cell) = container {
        let mut pending = Some(value);
        if let Ok(result) = cell.try_with_value_mut(|current| {
            assign_dim_value(
                current,
                dims,
                pending.take().expect("assign value consumed once"),
                append,
            )
        }) {
            return result;
        }
        let mut current = cell.get();
        assign_dim_value(
            &mut current,
            dims,
            pending.take().expect("assign value still pending"),
            append,
        )?;
        cell.set(current);
        return Ok(());
    }
    if let Value::Object(object) = container
        && spl_runtime_marker(object).is_some_and(|class| is_spl_caching_iterator_class(&class))
    {
        if dims.len() > 1 {
            return Err(
                "E_PHP_VM_SPL_CONTAINER_NESTED_DIM: nested ArrayAccess writes are not implemented"
                    .to_owned(),
            );
        }
        let key = if append {
            Value::Null
        } else {
            let Some(key) = dims.first() else {
                return Err("E_PHP_VM_ARRAY_ASSIGN_DIM: missing array dimension".to_owned());
            };
            array_key_to_value(key.clone())
        };
        spl_caching_iterator_require_full_cache(object, &object.display_name())?;
        spl_caching_iterator_offset_set(object, &key, value)?;
        return Ok(());
    }
    if let Value::Object(object) = container
        && spl_runtime_marker(object).is_some_and(|class| is_spl_array_access_runtime_class(&class))
    {
        if dims.len() > 1 {
            return Err(
                "E_PHP_VM_SPL_CONTAINER_NESTED_DIM: nested ArrayAccess writes are not implemented"
                    .to_owned(),
            );
        }
        let key = if append {
            Value::Null
        } else {
            let Some(key) = dims.first() else {
                return Err("E_PHP_VM_ARRAY_ASSIGN_DIM: missing array dimension".to_owned());
            };
            array_key_to_value(key.clone())
        };
        spl_container_offset_set(object, key, value)?;
        return Ok(());
    }
    if let Value::String(string) = container {
        let updated = write_string_offset(string.as_bytes().to_vec(), dims, value, append)?;
        *container = Value::string(updated);
        return Ok(());
    }
    let Value::Array(array) = container else {
        return Err(format!(
            "E_PHP_VM_ARRAY_ASSIGN_TYPE: cannot assign dimension on {}",
            value_type_name(container)
        ));
    };
    let Some((first, rest)) = dims.split_first() else {
        if append {
            array.append(value);
            return Ok(());
        }
        return Err("E_PHP_VM_ARRAY_ASSIGN_DIM: missing array dimension".to_owned());
    };
    if rest.is_empty() && !append {
        if let Some(mut existing) = array.get_mut(first) {
            write_lvalue(&mut existing, value);
        } else {
            array.insert(first.clone(), value);
        }
        return Ok(());
    }
    if array.get(first).is_none() {
        array.insert(first.clone(), Value::Array(PhpArray::new()));
    }
    let Some(mut child) = array.get_mut(first) else {
        return Err("E_PHP_VM_ARRAY_ASSIGN_DIM: failed to create nested array".to_owned());
    };
    if matches!(*child, Value::Uninitialized | Value::Null) {
        *child = Value::Array(PhpArray::new());
    }
    assign_dim_value(&mut child, rest, value, append)
}

fn bind_dim_local_to_reference_cell(
    stack: &mut CallStack,
    local: LocalId,
    dims: &[ArrayKey],
    append: bool,
    cell: ReferenceCell,
) -> Result<(), String> {
    let frame = stack.current_mut().ok_or("no active frame")?;
    let Some(slot) = frame.locals.get_slot_mut(local) else {
        return Err(format!("invalid local local:{}", local.raw()));
    };
    let mut current = slot.read();
    if matches!(current, Value::Uninitialized | Value::Null) {
        current = Value::Array(PhpArray::new());
    }
    bind_dim_value_to_reference_cell(&mut current, dims, append, cell)?;
    slot.write(current);
    Ok(())
}

fn bind_dim_value_to_reference_cell(
    container: &mut Value,
    dims: &[ArrayKey],
    append: bool,
    cell: ReferenceCell,
) -> Result<(), String> {
    if let Value::Reference(container_cell) = container {
        let mut current = container_cell.get();
        bind_dim_value_to_reference_cell(&mut current, dims, append, cell)?;
        container_cell.set(current);
        return Ok(());
    }
    let Value::Array(array) = container else {
        return Err(format!(
            "E_PHP_VM_ARRAY_BIND_DIM_TYPE: cannot bind dimension on {}",
            value_type_name(container)
        ));
    };
    let Some((first, rest)) = dims.split_first() else {
        if append {
            array.append(Value::Reference(cell));
            return Ok(());
        }
        return Err("E_PHP_VM_ARRAY_BIND_DIM: missing array dimension".to_owned());
    };
    if rest.is_empty() && !append {
        array.insert(first.clone(), Value::Reference(cell));
        return Ok(());
    }
    if array.get(first).is_none() {
        array.insert(first.clone(), Value::Array(PhpArray::new()));
    }
    let Some(mut child) = array.get_mut(first) else {
        return Err("E_PHP_VM_ARRAY_BIND_DIM: failed to create nested array".to_owned());
    };
    if matches!(*child, Value::Uninitialized | Value::Null) {
        *child = Value::Array(PhpArray::new());
    }
    bind_dim_value_to_reference_cell(&mut child, rest, append, cell)
}

fn bind_property_dim_to_reference_cell(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    object: &ObjectRef,
    property: &str,
    dims: &[ArrayKey],
    append: bool,
    cell: ReferenceCell,
) -> Result<(), String> {
    let storage_name =
        property_dimension_storage_name(compiled, state, stack, object, property, true)?;
    let mut current = object
        .get_property(&storage_name)
        .or_else(|| object.get_property(property))
        .unwrap_or(Value::Null);
    if matches!(current, Value::Uninitialized | Value::Null) {
        current = Value::Array(PhpArray::new());
    }
    bind_dim_value_to_reference_cell(&mut current, dims, append, cell)?;
    object.set_property(storage_name, current);
    Ok(())
}

fn ensure_dim_reference_cell(
    stack: &mut CallStack,
    local: LocalId,
    dims: &[ArrayKey],
) -> Result<ReferenceCell, String> {
    let frame = stack.current_mut().ok_or("no active frame")?;
    let Some(slot) = frame.locals.get_slot_mut(local) else {
        return Err(format!("invalid local local:{}", local.raw()));
    };
    let mut current = slot.read();
    if matches!(current, Value::Uninitialized | Value::Null) {
        current = Value::Array(PhpArray::new());
    }
    let cell = ensure_dim_reference_cell_value(&mut current, dims)?;
    slot.write(current);
    Ok(cell)
}

fn ensure_dim_reference_cell_value(
    container: &mut Value,
    dims: &[ArrayKey],
) -> Result<ReferenceCell, String> {
    if let Value::Reference(container_cell) = container {
        let mut current = container_cell.get();
        let cell = ensure_dim_reference_cell_value(&mut current, dims)?;
        container_cell.set(current);
        return Ok(cell);
    }
    let Value::Array(array) = container else {
        return Err(format!(
            "E_PHP_VM_ARRAY_REF_DIM_TYPE: cannot reference dimension on {}",
            value_type_name(container)
        ));
    };
    let Some((first, rest)) = dims.split_first() else {
        return Err("E_PHP_VM_ARRAY_REF_DIM: missing array dimension".to_owned());
    };
    if array.get(first).is_none() {
        array.insert(first.clone(), Value::Null);
    }
    let Some(mut child) = array.get_mut(first) else {
        return Err("E_PHP_VM_ARRAY_REF_DIM: failed to create array element".to_owned());
    };
    if rest.is_empty() {
        return Ok(ensure_value_reference_cell(&mut child));
    }
    if matches!(*child, Value::Uninitialized | Value::Null) {
        *child = Value::Array(PhpArray::new());
    }
    ensure_dim_reference_cell_value(&mut child, rest)
}

fn ensure_value_reference_cell(value: &mut Value) -> ReferenceCell {
    Lvalue::value(value, LvalueKind::ArrayElement)
        .ensure_reference_cell()
        .expect("array element lvalue can become a reference cell")
}

fn write_property_storage_value(object: &ObjectRef, storage_name: &str, value: Value) {
    Lvalue::object_property(object.clone(), storage_name, LvalueKind::ObjectProperty)
        .write_value(value)
        .expect("object property lvalue writes are supported")
}

fn write_lvalue(target: &mut Value, value: Value) {
    Lvalue::value(target, LvalueKind::ArrayElement)
        .write_value(value)
        .expect("array element lvalue writes are supported")
}

fn unset_dim_local(stack: &mut CallStack, local: LocalId, dims: &[ArrayKey]) -> Result<(), String> {
    let frame = stack.current_mut().ok_or("no active frame")?;
    let Some(slot) = frame.locals.get_slot_mut(local) else {
        return Err(format!("invalid local local:{}", local.raw()));
    };
    let mut current = slot.read();
    unset_dim_value(&mut current, dims);
    slot.write(current);
    Ok(())
}

fn unset_dim_value(container: &mut Value, dims: &[ArrayKey]) {
    if let Value::Reference(cell) = container {
        let mut current = cell.get();
        unset_dim_value(&mut current, dims);
        cell.set(current);
        return;
    }
    let Some((first, rest)) = dims.split_first() else {
        return;
    };
    if let Value::Object(object) = container
        && spl_runtime_marker(object).is_some_and(|class| is_spl_caching_iterator_class(&class))
        && rest.is_empty()
    {
        let _ = spl_caching_iterator_require_full_cache(object, &object.display_name()).and_then(
            |()| spl_caching_iterator_offset_unset(object, &array_key_to_value(first.clone())),
        );
        return;
    }
    if let Value::Object(object) = container
        && spl_runtime_marker(object).is_some_and(|class| is_spl_array_access_runtime_class(&class))
        && rest.is_empty()
    {
        let _ = spl_container_offset_unset(object, &array_key_to_value(first.clone()));
        return;
    }
    let Value::Array(array) = container else {
        return;
    };
    if rest.is_empty() {
        array.remove(first);
        return;
    }
    if let Some(mut child) = array.get_mut(first) {
        unset_dim_value(&mut child, rest);
    }
}

fn php_empty(value: &Value) -> Result<bool, String> {
    match value {
        Value::Reference(cell) => php_empty(&cell.borrow()),
        Value::Uninitialized | Value::Null => Ok(true),
        Value::Bool(value) => Ok(!*value),
        Value::Int(value) => Ok(*value == 0),
        Value::Float(value) => {
            let value = value.to_f64();
            Ok(value == 0.0 || value.is_nan())
        }
        Value::String(value) => Ok(value.is_empty() || value.as_bytes() == b"0"),
        Value::Array(array) => Ok(array.is_empty()),
        Value::Object(_)
        | Value::Resource(_)
        | Value::Fiber(_)
        | Value::Generator(_)
        | Value::Callable(_) => Ok(false),
    }
}

fn php_empty_access_value(value: &Value) -> Result<bool, String> {
    match effective_value(value) {
        Value::Object(object) if is_simplexml_object(&object) => {
            Ok(php_runtime::xml::simplexml_empty_access(&object))
        }
        value => php_empty(&value),
    }
}

fn is_simplexml_object(object: &ObjectRef) -> bool {
    normalize_class_name(&object.class_name()) == "simplexmlelement"
}

fn illegal_string_offset_warning(
    key_bytes: &[u8],
    span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    let key = String::from_utf8_lossy(key_bytes);
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_ILLEGAL_STRING_OFFSET",
        RuntimeSeverity::Warning,
        format!("Illegal string offset \"{key}\""),
        span,
        stack_trace,
        Some(php_runtime::PhpReferenceClassification::Warning),
    )
}

fn uninitialized_string_offset_warning(
    index: i64,
    span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_UNINITIALIZED_STRING_OFFSET",
        RuntimeSeverity::Warning,
        format!("Uninitialized string offset {index}"),
        span,
        stack_trace,
        Some(php_runtime::PhpReferenceClassification::Warning),
    )
}

fn undefined_array_key_warning(
    key: &ArrayKey,
    span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    let key = match key {
        ArrayKey::Int(value) => value.to_string(),
        ArrayKey::String(value) => format!("\"{}\"", value.to_string_lossy()),
    };
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_UNDEFINED_ARRAY_KEY_WARNING",
        RuntimeSeverity::Warning,
        format!("Undefined array key {key}"),
        span,
        stack_trace,
        Some(php_runtime::PhpReferenceClassification::Warning),
    )
}

fn undefined_array_string_key_warning(
    key: &PhpString,
    span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_UNDEFINED_ARRAY_KEY_WARNING",
        RuntimeSeverity::Warning,
        format!("Undefined array key \"{}\"", key.to_string_lossy()),
        span,
        stack_trace,
        Some(php_runtime::PhpReferenceClassification::Warning),
    )
}

fn array_offset_on_scalar_warning(
    value: &Value,
    span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_ARRAY_OFFSET_ON_SCALAR_WARNING",
        RuntimeSeverity::Warning,
        format!(
            "Trying to access array offset on {}",
            array_offset_scalar_type_name(value)
        ),
        span,
        stack_trace,
        Some(php_runtime::PhpReferenceClassification::Warning),
    )
}

fn array_offset_scalar_type_name(value: &Value) -> &'static str {
    match value {
        Value::Reference(cell) => array_offset_scalar_type_name(&cell.borrow()),
        Value::Null | Value::Uninitialized => "null",
        Value::Bool(false) => "false",
        Value::Bool(true) => "true",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::Resource(_) => "resource",
        other => value_type_name(other),
    }
}

#[cfg(test)]
mod tests;
