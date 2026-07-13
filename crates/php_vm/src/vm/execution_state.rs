//! Core request-local execution state and invalidation epochs.

use super::prelude::*;

#[derive(Debug, Default)]
pub(super) struct ExecutionState {
    /// Worker-stable symbol epochs enabled (VmOptions::worker_symbol_epoch).
    pub(super) worker_symbol_epoch: bool,
    pub(super) globals: GlobalSymbolTable,
    pub(super) included_once: Vec<PathBuf>,
    pub(super) included_once_set: HashSet<PathBuf>,
    pub(super) include_stack: Vec<PathBuf>,
    pub(super) cwd: PathBuf,
    /// Request-invariant: network builtins explicitly enabled via env.
    /// Precomputed once so builtin dispatch does not rescan the env table.
    pub(super) network_requests_enabled: bool,
    pub(super) static_locals: HashMap<(u32, String), ReferenceCell>,
    pub(super) static_properties: HashMap<(String, String), Value>,
    pub(super) enum_cases: HashMap<(String, String), ObjectRef>,
    pub(super) destructor_queue: DestructorQueue,
    pub(super) magic_property_stack: Vec<MagicPropertyCall>,
    pub(super) magic_method_stack: Vec<MagicMethodCall>,
    pub(super) property_hook_stack: Vec<PropertyHookCall>,
    pub(super) generator_continuations: HashMap<u64, GeneratorContinuation>,
    pub(super) fiber_continuations: HashMap<u64, Vec<FiberContinuation>>,
    pub(super) yield_from_delegations: HashMap<YieldFromKey, YieldFromDelegation>,
    pub(super) eval_depth: usize,
    pub(super) eval_counter: usize,
    pub(super) eval_diagnostic_spans: Vec<RuntimeSourceSpan>,
    pub(super) function_table_epoch: u64,
    pub(super) autoload_stack_epoch: u64,
    pub(super) class_table_epoch: u64,
    pub(super) include_config_epoch: u64,
    pub(super) parsed_include_path: Arc<Vec<PathBuf>>,
    pub(super) class_relation_cache: Rc<RefCell<ClassRelationCache>>,
    pub(super) autoload_registry: AutoloadRegistry,
    pub(super) autoload_stack: Vec<String>,
    pub(super) spl_autoload_extensions: String,
    /// Composer autoload-map fingerprint observed once per request on first
    /// autoload-cache use. Outer `None` = not yet computed; inner `None` = no
    /// map detected (unknown, blocks persistent reuse keyed on it).
    pub(super) composer_map_fingerprint: Option<Option<Arc<str>>>,
    pub(super) dynamic_units: Vec<CompiledUnit>,
    pub(super) dynamic_unit_index: HashMap<u64, usize>,
    pub(super) dynamic_functions: Vec<DynamicFunctionEntry>,
    pub(super) dynamic_function_index: HashMap<String, usize>,
    pub(super) dynamic_classes: Vec<DynamicClassEntry>,
    pub(super) dynamic_class_index: HashMap<String, usize>,
    pub(super) dynamic_constants: Vec<DynamicConstantEntry>,
    pub(super) dynamic_constant_index: HashMap<String, usize>,
    pub(super) validated_class_dependencies: HashSet<String>,
    pub(super) failed_class_declarations: HashSet<String>,
    pub(super) user_constants: HashMap<String, Value>,
    pub(super) shutdown_functions: Vec<ShutdownFunctionEntry>,
    pub(super) ini: IniRegistry,
    pub(super) default_timezone: String,
    pub(super) env: Arc<Vec<(String, String)>>,
    pub(super) filter_input_arrays: Rc<BTreeMap<i64, PhpArray>>,
    pub(super) resources: ResourceTable,
    pub(super) stdin: Option<php_runtime::api::ResourceRef>,
    pub(super) stdout: Option<php_runtime::api::ResourceRef>,
    pub(super) stderr: Option<php_runtime::api::ResourceRef>,
    pub(super) builtins: BuiltinAdapterState,
    pub(super) last_error: Option<LastErrorEntry>,
    pub(super) request: RequestLifecycleState,
    pub(super) error_handlers: Vec<ErrorHandlerEntry>,
    pub(super) exception_handlers: Vec<CallableValue>,
    pub(super) diagnostics: Vec<RuntimeDiagnostic>,
    pub(super) suppress_array_to_string_warnings: usize,
    pub(super) execution_deadline_at: Option<Instant>,
    pub(super) execution_deadline_mutable: bool,
    pub(super) process_exit_code: Option<i32>,
    /// Throwable propagating up the call stack toward an enclosing handler.
    ///
    /// Set when a frame cannot handle a throw locally; each caller frame gets a
    /// chance to catch it before the entry point renders it as uncaught.
    pub(super) pending_throw: Option<Value>,
    /// Stack trace captured at the throw origin (before unwinding), rendered as
    /// PHP's `Stack trace:` body for the uncaught-error message.
    pub(super) pending_trace: Option<String>,
}

impl ExecutionState {
    pub(super) fn has_included(&self, path: &Path) -> bool {
        self.included_once_set.contains(path)
    }

    pub(super) fn record_included(&mut self, path: PathBuf) -> bool {
        if !self.included_once_set.insert(path.clone()) {
            return false;
        }
        self.included_once.push(path);
        true
    }
}

impl ExecutionState {
    pub(super) fn push_dynamic_unit(&mut self, unit: CompiledUnit) -> usize {
        let index = self.dynamic_units.len();
        let identity = unit.cache_identity();
        self.dynamic_units.push(unit);
        self.dynamic_unit_index.insert(identity, index);
        index
    }

    pub(super) fn push_dynamic_function(&mut self, entry: DynamicFunctionEntry) {
        let index = self.dynamic_functions.len();
        self.dynamic_function_index
            .entry(entry.name.clone())
            .or_insert(index);
        self.dynamic_functions.push(entry);
    }

    pub(super) fn push_dynamic_class(&mut self, entry: DynamicClassEntry) {
        let index = self.dynamic_classes.len();
        self.dynamic_class_index
            .entry(entry.lookup_name.clone())
            .or_insert(index);
        self.dynamic_classes.push(entry);
    }

    pub(super) fn push_dynamic_constant(&mut self, entry: DynamicConstantEntry) {
        let index = self.dynamic_constants.len();
        self.dynamic_constant_index
            .entry(entry.name.clone())
            .or_insert(index);
        self.dynamic_constants.push(entry);
    }

    pub(super) fn lookup_epoch(&self) -> InvalidationEpoch {
        InvalidationEpoch::new(self.function_table_epoch)
    }

    pub(super) fn bump_lookup_epoch(&mut self) {
        if self.worker_symbol_epoch {
            // Advance the worker ledger so the epoch stays monotonic across
            // requests on this thread; per-request state re-seeds from it.
            self.function_table_epoch = WORKER_SYMBOL_LEDGER.with(|ledger| {
                let next = ledger.epoch.get().saturating_add(1);
                ledger.epoch.set(next);
                next
            });
        } else {
            self.function_table_epoch = self.function_table_epoch.saturating_add(1);
        }
    }

    pub(super) fn autoload_class_lookup_epochs(&self) -> AutoloadClassLookupEpochs {
        AutoloadClassLookupEpochs {
            autoload_stack_epoch: self.autoload_stack_epoch,
            class_table_epoch: self.class_table_epoch,
            include_config_epoch: self.include_config_epoch,
        }
    }

    pub(super) fn class_relation_epochs(&self) -> ClassRelationEpochs {
        ClassRelationEpochs {
            class_table_epoch: self.class_table_epoch,
            autoload_epoch: self.autoload_stack_epoch,
            include_eval_epoch: self.include_config_epoch.wrapping_mul(1_000_003)
                ^ self.eval_counter as u64,
            trait_interface_map_version: self.class_table_epoch,
            method_table_version: self.function_table_epoch,
        }
    }

    pub(super) fn bump_autoload_stack_epoch(&mut self) {
        self.autoload_stack_epoch = self.autoload_stack_epoch.saturating_add(1);
        self.bump_lookup_epoch();
    }

    pub(super) fn bump_class_table_epoch(&mut self) {
        self.class_table_epoch = self.class_table_epoch.saturating_add(1);
        self.bump_lookup_epoch();
    }

    pub(super) fn bump_include_config_epoch(&mut self) {
        self.include_config_epoch = self.include_config_epoch.saturating_add(1);
        self.parsed_include_path = parse_ini_include_path(&self.ini);
        self.bump_lookup_epoch();
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DynamicFunctionEntry {
    pub(super) name: String,
    pub(super) unit_index: usize,
    pub(super) function: FunctionId,
    pub(super) origin: DeclarationOrigin,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct DynamicClassEntry {
    pub(super) lookup_name: String,
    pub(super) class: CompiledClass,
    pub(super) unit_index: usize,
    pub(super) origin: DeclarationOrigin,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DynamicConstantEntry {
    pub(super) name: String,
    pub(super) unit_index: usize,
    pub(super) value: ConstId,
    pub(super) origin: DeclarationOrigin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DeclarationKind {
    Function,
    ClassLike,
    GlobalConstant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DeclarationLoadKind {
    Main,
    Include,
    Eval,
    Conditional,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DeclarationOrigin {
    pub(super) source_path: String,
    pub(super) line: i64,
    pub(super) span: IrSpan,
    pub(super) namespace: Option<String>,
    pub(super) kind: DeclarationKind,
    pub(super) load_kind: DeclarationLoadKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ErrorHandlerEntry {
    pub(super) callback: CallableValue,
    pub(super) levels: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct LastErrorEntry {
    pub(super) level: i64,
    pub(super) message: String,
    pub(super) file: String,
    pub(super) line: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ShutdownFunctionEntry {
    pub(super) callback: Value,
    pub(super) args: Vec<CallArgument>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct DestructorQueue {
    pub(super) entries: Vec<DestructorEntry>,
}

impl DestructorQueue {
    pub(super) fn register(
        &mut self,
        object: ObjectRef,
        class_name: String,
        function: FunctionId,
        owner_dynamic_unit_index: Option<usize>,
        visibility: DestructorVisibility,
    ) {
        if self
            .entries
            .iter()
            .any(|entry| entry.object.id() == object.id())
        {
            return;
        }
        self.entries.push(DestructorEntry {
            object,
            class_name,
            function,
            owner_dynamic_unit_index,
            visibility,
        });
    }

    pub(super) fn take_for_object(&mut self, object_id: u64) -> Option<DestructorEntry> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.object.id() == object_id)?;
        Some(self.entries.remove(index))
    }

    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(super) fn objects_snapshot(&self) -> Vec<ObjectRef> {
        self.entries
            .iter()
            .map(|entry| entry.object.clone())
            .collect()
    }

    pub(super) fn drain_reverse(&mut self) -> Vec<DestructorEntry> {
        let mut entries = std::mem::take(&mut self.entries);
        entries.reverse();
        entries
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DestructorEntry {
    pub(super) object: ObjectRef,
    pub(super) class_name: String,
    pub(super) function: FunctionId,
    pub(super) owner_dynamic_unit_index: Option<usize>,
    pub(super) visibility: DestructorVisibility,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DestructorVisibility {
    Public,
    Protected,
    Private,
}

pub(super) fn gc_snapshot_from_vm_roots(stack: &CallStack, state: &ExecutionState) -> GcSnapshot {
    scan_roots(gc_roots_from_vm(stack, state))
}

pub(super) fn gc_root_count_from_vm_roots(stack: &CallStack, state: &ExecutionState) -> usize {
    gc_roots_from_vm(stack, state).len()
}

fn gc_roots_from_vm(stack: &CallStack, state: &ExecutionState) -> Vec<GcRoot> {
    let mut roots = Vec::new();
    for (frame_index, frame) in stack.frames().iter().enumerate() {
        for (index, value) in frame.registers.iter() {
            roots.push(GcRoot::value(
                GcRootKind::FrameRegister,
                format!("frame{frame_index}.r{index}"),
                value.clone(),
            ));
        }
        for (index, slot) in frame.locals.iter() {
            roots.push(GcRoot::slot(
                GcRootKind::FrameLocal,
                format!("frame{frame_index}.local{index}"),
                slot,
            ));
        }
    }
    for ((function, name), cell) in &state.static_locals {
        roots.push(GcRoot::value(
            GcRootKind::StaticLocal,
            format!("static-local:{function}:{name}"),
            Value::Reference(cell.clone()),
        ));
    }
    for ((class_name, property), value) in &state.static_properties {
        roots.push(GcRoot::value(
            GcRootKind::ClassTable,
            format!("static-property:{class_name}::{property}"),
            value.clone(),
        ));
    }
    for ((class_name, case_name), object) in &state.enum_cases {
        roots.push(GcRoot::value(
            GcRootKind::ClassTable,
            format!("enum-case:{class_name}::{case_name}"),
            Value::Object(object.clone()),
        ));
    }
    for (index, entry) in state.destructor_queue.entries.iter().enumerate() {
        roots.push(GcRoot::value(
            GcRootKind::DestructorQueue,
            format!("destructor-queue:{index}"),
            Value::Object(entry.object.clone()),
        ));
    }
    for (index, entry) in state.shutdown_functions.iter().enumerate() {
        roots.push(GcRoot::value(
            GcRootKind::Temporary,
            format!("shutdown-function:{index}:callback"),
            entry.callback.clone(),
        ));
        for (arg_index, arg) in entry.args.iter().enumerate() {
            roots.push(GcRoot::value(
                GcRootKind::Temporary,
                format!("shutdown-function:{index}:arg{arg_index}"),
                arg.value.clone(),
            ));
        }
    }
    for (index, callback) in state.autoload_registry.callbacks().iter().enumerate() {
        roots.push(GcRoot::value(
            GcRootKind::Temporary,
            format!("autoload-callback:{index}"),
            Value::Callable(Box::new(callback.clone())),
        ));
    }
    for (fiber_id, continuations) in &state.fiber_continuations {
        for (continuation_index, continuation) in continuations.iter().enumerate() {
            for (index, value) in continuation.frame.registers.iter() {
                roots.push(GcRoot::value(
                    GcRootKind::FiberStack,
                    format!("fiber{fiber_id}.continuation{continuation_index}.r{index}"),
                    value.clone(),
                ));
            }
            for (index, slot) in continuation.frame.locals.iter() {
                roots.push(GcRoot::slot(
                    GcRootKind::FiberStack,
                    format!("fiber{fiber_id}.continuation{continuation_index}.local{index}"),
                    slot,
                ));
            }
        }
    }
    roots
}

pub(super) fn php_visible_root_object_ids(
    stack: &CallStack,
    state: &ExecutionState,
) -> GcObjectIdSet {
    collect_root_object_ids(stack, state, true)
}

pub(super) fn php_visible_non_register_root_object_ids(
    stack: &CallStack,
    state: &ExecutionState,
) -> GcObjectIdSet {
    collect_root_object_ids(stack, state, false)
}

/// Runs `f` with the thread-local scan scratch (seen set + work list),
/// cleared but with capacity retained across scans. The scans run on every
/// object-overwriting store, so re-growing fresh tables per scan is pure
/// allocator/rehash overhead. Traversal never re-enters PHP, so the
/// scratch borrow never nests.
fn with_gc_scan_scratch<R>(f: impl FnOnce(&mut GcSeenSet, &mut Vec<GcPendingEntity>) -> R) -> R {
    thread_local! {
        static SCRATCH: std::cell::RefCell<(GcSeenSet, Vec<GcPendingEntity>)> =
            std::cell::RefCell::new((GcSeenSet::default(), Vec::new()));
    }
    SCRATCH.with(|scratch| {
        let mut guard = scratch.borrow_mut();
        let (seen, pending) = &mut *guard;
        seen.clear();
        pending.clear();
        f(seen, pending)
    })
}

fn collect_root_object_ids(
    stack: &CallStack,
    state: &ExecutionState,
    include_current_registers: bool,
) -> GcObjectIdSet {
    // The root scan walks every live register, local, and object graph. It
    // traverses borrowed values and only refcount-bumps container handles
    // into the work list; per-value deep clones are scan overhead.
    let _source = layout_source::enter(layout_source::GC_ROOT_SCAN);
    let mut object_ids = GcObjectIdSet::default();
    with_gc_scan_scratch(|seen, pending| {
        collect_root_object_ids_into(
            stack,
            state,
            include_current_registers,
            seen,
            &mut object_ids,
            pending,
        );
    });
    object_ids
}

fn collect_root_object_ids_into(
    stack: &CallStack,
    state: &ExecutionState,
    include_current_registers: bool,
    seen: &mut GcSeenSet,
    object_ids: &mut GcObjectIdSet,
    pending: &mut Vec<GcPendingEntity>,
) {
    for frame in stack.frames() {
        if include_current_registers {
            for (_, value) in frame.registers.iter() {
                gc_note_value(value, seen, object_ids, pending);
            }
        }
        for (_, slot) in frame.locals.iter() {
            gc_note_slot(slot, seen, object_ids, pending);
        }
    }
    for cell in state.static_locals.values() {
        gc_note_reference(cell, seen, pending);
    }
    for value in state.static_properties.values() {
        gc_note_value(value, seen, object_ids, pending);
    }
    for object in state.enum_cases.values() {
        object_ids.insert(object.id());
    }
    for entry in &state.shutdown_functions {
        gc_note_value(&entry.callback, seen, object_ids, pending);
        for arg in &entry.args {
            gc_note_value(&arg.value, seen, object_ids, pending);
        }
    }
    for callback in state.autoload_registry.callbacks() {
        gc_note_callable(callback, seen, object_ids, pending);
    }
    gc_note_array(&state.globals.globals_array(), seen, pending);
    for continuation in state.generator_continuations.values() {
        for (_, value) in continuation.frame.registers.iter() {
            gc_note_value(value, seen, object_ids, pending);
        }
        for (_, slot) in continuation.frame.locals.iter() {
            gc_note_slot(slot, seen, object_ids, pending);
        }
    }
    for continuations in state.fiber_continuations.values() {
        for continuation in continuations {
            for (_, value) in continuation.frame.registers.iter() {
                gc_note_value(value, seen, object_ids, pending);
            }
            for (_, slot) in continuation.frame.locals.iter() {
                gc_note_slot(slot, seen, object_ids, pending);
            }
        }
    }
    gc_drain_reachable(seen, object_ids, pending);
}

pub(super) fn preserved_destructor_object_ids(preserved: &[Value]) -> GcObjectIdSet {
    let mut object_ids = GcObjectIdSet::default();
    with_gc_scan_scratch(|seen, pending| {
        for value in preserved {
            gc_note_value(value, seen, &mut object_ids, pending);
        }
        gc_drain_reachable(seen, &mut object_ids, pending);
    });
    object_ids
}

/// Collects destructor candidates plus the shared-container flag from the
/// scratch-backed walk (see [`collect_destructor_candidates_with_share_flag`]).
fn destructor_candidates_with_share_flag(value: &Value) -> (Vec<ObjectRef>, bool) {
    let mut candidates = Vec::new();
    let mut saw_shared_container = false;
    DESTRUCTOR_SEEN_SCRATCH.with(|scratch| {
        let mut seen = scratch.borrow_mut();
        seen.clear();
        collect_destructor_candidates_with_share_flag(
            value,
            &mut seen,
            &mut candidates,
            &mut saw_shared_container,
        );
    });
    (candidates, saw_shared_container)
}

// The candidate scan runs on every object-overwriting store; reusing one
// scratch set keeps its capacity across calls instead of re-growing a fresh
// table each time. The walk is a pure value-graph traversal (no PHP
// re-entry), so the borrow never nests.
thread_local! {
    static DESTRUCTOR_SEEN_SCRATCH: std::cell::RefCell<GcSeenSet> =
        std::cell::RefCell::new(GcSeenSet::default());
}

pub(super) fn destructor_candidates_for_value(value: &Value) -> Vec<ObjectRef> {
    destructor_candidates_with_share_flag(value).0
}

/// Multiply-shift hasher for the GC scan's process-local entity ids. The
/// scan visits ~10^6 nodes per WordPress request and SipHash on the seen-sets
/// shows up in clean profiles; sequential ids need only multiplicative
/// dispersion.
#[derive(Default)]
pub(super) struct GcIdHasher(u64);

impl std::hash::Hasher for GcIdHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.0 = (self.0 ^ u64::from(value)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0 ^ value).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

pub(super) type GcIdBuildHasher = std::hash::BuildHasherDefault<GcIdHasher>;
type GcSeenSet = HashSet<GcEntityId, GcIdBuildHasher>;
pub(super) type GcObjectIdSet = HashSet<u64, GcIdBuildHasher>;

/// Container handle queued by the reachability walk. Holding the handle is a
/// refcount bump; contents are traversed borrowed when the entry is drained.
enum GcPendingEntity {
    Array(PhpArray),
    Object(ObjectRef),
    Reference(ReferenceCell),
}

/// Notes one value for the reachability walk: records object ids, and queues
/// unseen containers for traversal. Scalars never clone.
fn gc_note_value(
    value: &Value,
    seen: &mut GcSeenSet,
    object_ids: &mut GcObjectIdSet,
    pending: &mut Vec<GcPendingEntity>,
) {
    match value {
        Value::Array(array) => gc_note_array(array, seen, pending),
        Value::Object(object) => gc_note_object(object, seen, object_ids, pending),
        Value::Reference(cell) => gc_note_reference(cell, seen, pending),
        Value::Callable(callable) => gc_note_callable(callable, seen, object_ids, pending),
        Value::Null
        | Value::Bool(_)
        | Value::Int(_)
        | Value::Float(_)
        | Value::String(_)
        | Value::Resource(_)
        | Value::Generator(_)
        | Value::Fiber(_)
        | Value::Uninitialized => {}
    }
}

fn gc_note_slot(
    slot: &Slot,
    seen: &mut GcSeenSet,
    object_ids: &mut GcObjectIdSet,
    pending: &mut Vec<GcPendingEntity>,
) {
    match slot {
        Slot::Value(value) => gc_note_value(value, seen, object_ids, pending),
        Slot::Reference(cell) => gc_note_reference(cell, seen, pending),
    }
}

fn gc_note_array(array: &PhpArray, seen: &mut GcSeenSet, pending: &mut Vec<GcPendingEntity>) {
    let id = GcEntityId::new(GcEntityKind::Array, array.gc_debug_id());
    if seen.insert(id) {
        pending.push(GcPendingEntity::Array(array.clone()));
    }
}

fn gc_note_object(
    object: &ObjectRef,
    seen: &mut GcSeenSet,
    object_ids: &mut GcObjectIdSet,
    pending: &mut Vec<GcPendingEntity>,
) {
    object_ids.insert(object.id());
    let id = GcEntityId::new(GcEntityKind::Object, object.id());
    if seen.insert(id) {
        pending.push(GcPendingEntity::Object(object.clone()));
    }
}

fn gc_note_reference(
    cell: &ReferenceCell,
    seen: &mut GcSeenSet,
    pending: &mut Vec<GcPendingEntity>,
) {
    let id = GcEntityId::new(GcEntityKind::Reference, cell.gc_debug_id());
    if seen.insert(id) {
        pending.push(GcPendingEntity::Reference(cell.clone()));
    }
}

fn gc_note_callable(
    callable: &CallableValue,
    seen: &mut GcSeenSet,
    object_ids: &mut GcObjectIdSet,
    pending: &mut Vec<GcPendingEntity>,
) {
    match callable {
        CallableValue::Closure(payload) => {
            for capture in &payload.captures {
                if let Some(value) = &capture.value {
                    gc_note_value(value, seen, object_ids, pending);
                }
                if let Some(cell) = &capture.reference {
                    gc_note_reference(cell, seen, pending);
                }
            }
            if let Some(bound_this) = &payload.bound_this {
                gc_note_object(bound_this, seen, object_ids, pending);
            }
        }
        CallableValue::BoundMethod {
            target: CallableMethodTarget::Object(object),
            ..
        } => {
            gc_note_object(object, seen, object_ids, pending);
        }
        CallableValue::UserFunction { .. }
        | CallableValue::InternalBuiltin { .. }
        | CallableValue::BoundMethod {
            target: CallableMethodTarget::Class(_),
            ..
        }
        | CallableValue::MethodPlaceholder { .. }
        | CallableValue::UnresolvedDynamic { .. } => {}
    }
}

/// Drains queued containers, traversing their contents borrowed.
fn gc_drain_reachable(
    seen: &mut GcSeenSet,
    object_ids: &mut GcObjectIdSet,
    pending: &mut Vec<GcPendingEntity>,
) {
    while let Some(entity) = pending.pop() {
        match entity {
            GcPendingEntity::Array(array) => {
                for (_, element) in array.iter() {
                    gc_note_value(element, seen, object_ids, pending);
                }
            }
            GcPendingEntity::Object(object) => {
                object.visit_property_values(|value| {
                    gc_note_value(value, seen, object_ids, pending);
                });
            }
            GcPendingEntity::Reference(cell) => {
                let value = cell.borrow();
                gc_note_value(&value, seen, object_ids, pending);
            }
        }
    }
}

/// Walks `value` collecting every reachable object, and reports whether any
/// traversed container (array storage, reference cell, callable payload)
/// might also be held outside this graph. When the flag stays false, the
/// only strong paths into the graph run through `value` itself, so a
/// candidate whose handle count is exactly graph + candidate list is
/// provably unreachable from the roots without scanning them.
fn collect_destructor_candidates_with_share_flag(
    value: &Value,
    seen: &mut GcSeenSet,
    candidates: &mut Vec<ObjectRef>,
    saw_shared_container: &mut bool,
) {
    match value {
        Value::Array(array) => {
            let id = GcEntityId::new(GcEntityKind::Array, array.gc_debug_id());
            if !seen.insert(id) {
                return;
            }
            if array.is_shared() {
                *saw_shared_container = true;
            }
            for (_, value) in array.iter() {
                collect_destructor_candidates_with_share_flag(
                    value,
                    seen,
                    candidates,
                    saw_shared_container,
                );
            }
        }
        Value::Object(object) => {
            let id = GcEntityId::new(GcEntityKind::Object, object.id());
            if !seen.insert(id) {
                return;
            }
            candidates.push(object.clone());
            object.visit_property_values(|value| {
                collect_destructor_candidates_with_share_flag(
                    value,
                    seen,
                    candidates,
                    saw_shared_container,
                );
            });
        }
        Value::Reference(cell) => {
            let id = GcEntityId::new(GcEntityKind::Reference, cell.gc_debug_id());
            if !seen.insert(id) {
                return;
            }
            if cell.gc_refcount_estimate() > 1 {
                *saw_shared_container = true;
            }
            let value = cell.borrow();
            collect_destructor_candidates_with_share_flag(
                &value,
                seen,
                candidates,
                saw_shared_container,
            );
        }
        Value::Callable(callable) => match callable.as_ref() {
            CallableValue::Closure(payload) => {
                // Callable payload sharing is not directly observable here;
                // stay conservative so captured objects never release
                // scan-free while another closure handle can reach them.
                *saw_shared_container = true;
                for capture in &payload.captures {
                    if let Some(value) = capture.value() {
                        collect_destructor_candidates_with_share_flag(
                            value,
                            seen,
                            candidates,
                            saw_shared_container,
                        );
                    }
                    if let Some(cell) = capture.reference() {
                        collect_destructor_candidates_with_share_flag(
                            &Value::Reference(cell),
                            seen,
                            candidates,
                            saw_shared_container,
                        );
                    }
                }
                if let Some(bound_this) = &payload.bound_this {
                    collect_destructor_candidates_with_share_flag(
                        &Value::Object(bound_this.clone()),
                        seen,
                        candidates,
                        saw_shared_container,
                    );
                }
            }
            CallableValue::BoundMethod {
                target: CallableMethodTarget::Object(object),
                ..
            } => {
                *saw_shared_container = true;
                collect_destructor_candidates_with_share_flag(
                    &Value::Object(object.clone()),
                    seen,
                    candidates,
                    saw_shared_container,
                );
            }
            CallableValue::UserFunction { .. }
            | CallableValue::InternalBuiltin { .. }
            | CallableValue::BoundMethod {
                target: CallableMethodTarget::Class(_),
                ..
            }
            | CallableValue::MethodPlaceholder { .. }
            | CallableValue::UnresolvedDynamic { .. } => {}
        },
        Value::Null
        | Value::Bool(_)
        | Value::Int(_)
        | Value::Float(_)
        | Value::String(_)
        | Value::Resource(_)
        | Value::Generator(_)
        | Value::Fiber(_)
        | Value::Uninitialized => {}
    }
}

pub(super) struct DestructorSweep {
    pub(super) outcome: Option<RaiseOutcome>,
}

/// Historical hook for eager PHP object-id recycling on value drops.
///
/// The previous implementation deep-walked the dropped value and then the
/// entire PHP-visible heap (every local, global, static, and destructor
/// entry, cloning every array element along the way) on every overwrite of
/// an object-bearing value. On container-heavy workloads that turned each
/// store into an O(heap) scan and dominated whole requests.
///
/// Object ids are recycled naturally when the last handle drops
/// (`ObjectIdGuard` frees the id from the storage drop). Eager scanning only
/// tightens id-reuse timing for objects kept alive by stale VM temporaries
/// or reference cycles. Keep that eager release local to the dropped object
/// graph, and avoid the previous full-heap root traversal. Destructor timing
/// is unaffected: it runs through the separate
/// `run_destructors_for_unreferenced_value` path, which is gated on the
/// destructor queue.
pub(super) fn release_unrooted_object_handles(value: &Value) {
    // The overwritten value drops immediately after this hook. Its natural
    // drop recursively releases unshared container contents and recycles
    // their object ids, while shared or cyclic graphs must stay alive. Walking
    // every nested array/reference/callable here only duplicates that work.
    // Keep the constant-time direct-object case for stale VM temporaries, and
    // leave every container shape to the drop/destructor paths.
    if let Value::Object(object) = value {
        if object.gc_refcount_estimate() <= 1 {
            object.release_php_handle();
        }
    }
}

pub(super) fn release_unrooted_direct_object_handle(value: &Value) {
    // Same contract as `release_unrooted_object_handles`: eager release only
    // tightens id-reuse timing, so instead of walking the full root set the
    // handle is released exactly when this value is provably its only
    // holder. A reference-wrapped object defers unconditionally — the cell
    // may be aliased from a root without bumping the object's own count.
    let Value::Object(object) = value else {
        return;
    };
    if object.gc_refcount_estimate() <= 1 {
        object.release_php_handle();
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct MagicPropertyCall {
    pub(super) object_id: u64,
    pub(super) method: String,
    pub(super) property: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct MagicMethodCall {
    pub(super) receiver: String,
    pub(super) magic_method: String,
    pub(super) called_method: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PropertyHookCall {
    pub(super) object_id: u64,
    pub(super) class_name: String,
    pub(super) property: String,
}

impl DeclarationOrigin {
    pub(super) fn display_site(&self) -> String {
        let namespace = self
            .namespace
            .as_deref()
            .map_or_else(|| "global".to_string(), ToOwned::to_owned);
        format!(
            "{}:{} ({} {:?} {:?} bytes {}..{})",
            self.source_path,
            self.line,
            namespace,
            self.load_kind,
            self.kind,
            self.span.start,
            self.span.end
        )
    }
}
