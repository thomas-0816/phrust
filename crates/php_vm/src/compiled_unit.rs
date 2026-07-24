//! VM-facing wrapper around verified IR units.

use php_ir::IrUnit;
use php_ir::constants::IrConstant;
use php_ir::ids::FunctionId;
use php_ir::module::{ClassEntry, normalize_class_name, normalized_class_name};
use php_ir::source_map::IrSpan;
use php_ir::verify::verify_unit;
use php_source::{BytePos, LineIndex};
use std::{
    collections::{BTreeMap, HashMap},
    ops::Deref,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

static NEXT_COMPILED_UNIT_CACHE_ID: AtomicU64 = AtomicU64::new(1);
static PREPARED_BUILTIN_DEFINITIONS: OnceLock<Box<[PreparedBuiltinDefinition]>> = OnceLock::new();

/// Authoritative IR unit handed to the native execution coordinator.
#[derive(Clone)]
pub struct CompiledUnit {
    inner: Arc<CompiledUnitInner>,
}

/// Invalid source-repository input supplied while constructing an artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompiledUnitBuildError {
    /// A source references no file in the IR source map.
    UnknownSourceFile(php_ir::ids::FileId),
    /// More than one source was supplied for the same file ID.
    DuplicateSourceFile(php_ir::ids::FileId),
}

struct CompiledUnitInner {
    cache_id: u64,
    artifact_identity: u64,
    unit: IrUnit,
    class_table: Box<[usize]>,
    function_lookup: SymbolIndex,
    constant_lookup: SymbolIndex,
    class_lookup: SymbolIndex,
    unit_class_lookup: SymbolIndex,
    sources: CompiledSourceRepository,
    prepared: PreparedUnit,
}

/// Immutable handle to one canonical class definition.
#[derive(Clone)]
pub struct CompiledClass {
    storage: CompiledClassStorage,
}

#[derive(Clone)]
enum CompiledClassStorage {
    Unit { owner: CompiledUnit, index: usize },
    Owned(Arc<ClassEntry>),
}

impl CompiledClass {
    fn in_unit(owner: CompiledUnit, index: usize) -> Self {
        Self {
            storage: CompiledClassStorage::Unit { owner, index },
        }
    }

    /// Wraps runtime-produced class metadata that has no compiled-unit owner.
    #[must_use]
    pub fn owned(class: ClassEntry) -> Self {
        Self {
            storage: CompiledClassStorage::Owned(Arc::new(class)),
        }
    }

    /// Returns true when both handles refer to the same canonical allocation.
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        match (&self.storage, &other.storage) {
            (
                CompiledClassStorage::Unit {
                    owner: left_owner,
                    index: left_index,
                },
                CompiledClassStorage::Unit {
                    owner: right_owner,
                    index: right_index,
                },
            ) => left_index == right_index && left_owner.ptr_eq(right_owner),
            (CompiledClassStorage::Owned(left), CompiledClassStorage::Owned(right)) => {
                Arc::ptr_eq(left, right)
            }
            _ => false,
        }
    }
}

impl Deref for CompiledClass {
    type Target = ClassEntry;

    fn deref(&self) -> &Self::Target {
        match &self.storage {
            CompiledClassStorage::Unit { owner, index } => &owner.inner.unit.classes[*index],
            CompiledClassStorage::Owned(class) => class,
        }
    }
}

impl AsRef<ClassEntry> for CompiledClass {
    fn as_ref(&self) -> &ClassEntry {
        self
    }
}

impl std::fmt::Debug for CompiledClass {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.deref().fmt(formatter)
    }
}

impl PartialEq for CompiledClass {
    fn eq(&self, other: &Self) -> bool {
        self.deref() == other.deref()
    }
}

#[derive(Debug)]
struct CompiledSourceRepository {
    entries: Box<[Option<CompiledSource>]>,
}

#[derive(Debug)]
struct CompiledSource {
    text: Arc<str>,
    lines: LineIndex,
}

#[derive(Debug)]
struct SymbolIndex {
    buckets: HashMap<u64, Box<[usize]>>,
}

impl SymbolIndex {
    fn new(names: impl Iterator<Item = (usize, u64)>) -> Self {
        let mut buckets = HashMap::<u64, Vec<usize>>::new();
        for (index, hash) in names {
            buckets.entry(hash).or_default().push(index);
        }
        Self {
            buckets: buckets
                .into_iter()
                .map(|(hash, indexes)| (hash, indexes.into_boxed_slice()))
                .collect(),
        }
    }

    fn candidates(&self, name: &str) -> impl Iterator<Item = usize> + '_ {
        self.buckets
            .get(&stable_hash(name.as_bytes()))
            .into_iter()
            .flat_map(|indexes| indexes.iter().copied())
    }
}

#[derive(Debug)]
struct PreparedUnit {
    ir_verification_errors: OnceLock<usize>,
    class_validation: OnceLock<PreparedClassValidation>,
    native_indexes: OnceLock<PreparedNativeIndexes>,
    ir_fingerprint: OnceLock<String>,
    function_ir_fingerprint_context: OnceLock<php_jit::StableFunctionIrFingerprintContext>,
    function_ir_fingerprints: Box<[OnceLock<String>]>,
    dependency_identity: OnceLock<String>,
    external_function_calls: OnceLock<PreparedExternalFunctionCalls>,
    native_function_metadata: OnceLock<Box<[Arc<PreparedNativeFunctionMetadata>]>>,
    deployment_image: OnceLock<PreparedDeploymentNativeImage>,
    ir_verification_runs: AtomicU64,
    continuation_index_runs: AtomicU64,
    ir_fingerprint_runs: AtomicU64,
    function_ir_fingerprint_runs: AtomicU64,
    dependency_identity_runs: AtomicU64,
    class_validation_runs: AtomicU64,
}

#[derive(Debug)]
pub(crate) struct PreparedDeploymentNativeImage {
    pub function_exports: Arc<std::collections::HashMap<Arc<str>, FunctionId>>,
    pub exported_classes: Arc<std::collections::HashSet<Arc<str>>>,
    pub native_call_argument_capacity: usize,
    /// Immutable source-unit constants in a numeric C-layout view. Generated
    /// code uses these records for literal string keys without decoding a
    /// Rust `Value` or allocating a request-local string handle.
    pub constant_views: Box<[php_jit::JitNativeConstantView]>,
    /// Dense baseline publication cells indexed by `FunctionId`. Generated
    /// code uses these only for an exact continuation after an optimizing
    /// callee side exit.
    pub native_function_entries: Box<[std::sync::atomic::AtomicUsize]>,
    /// Dense ordinary-call cells indexed by `FunctionId`. Every published
    /// baseline initializes its cell and an optimizing publication atomically
    /// replaces that target, so generated calls never select a tier.
    pub preferred_function_entries: Box<[std::sync::atomic::AtomicUsize]>,
}

impl PreparedUnit {
    fn new(function_count: usize, function_ir_fingerprints: Option<Box<[String]>>) -> Self {
        let function_ir_fingerprint_runs = u64::from(function_ir_fingerprints.is_some());
        let mut fingerprint_slots = (0..function_count)
            .map(|_| OnceLock::new())
            .collect::<Vec<_>>();
        if let Some(fingerprints) = function_ir_fingerprints {
            for (slot, fingerprint) in fingerprint_slots.iter_mut().zip(fingerprints) {
                let _ = slot.set(fingerprint);
            }
        }
        Self {
            ir_verification_errors: OnceLock::new(),
            class_validation: OnceLock::new(),
            native_indexes: OnceLock::new(),
            ir_fingerprint: OnceLock::new(),
            function_ir_fingerprint_context: OnceLock::new(),
            function_ir_fingerprints: fingerprint_slots.into_boxed_slice(),
            dependency_identity: OnceLock::new(),
            external_function_calls: OnceLock::new(),
            native_function_metadata: OnceLock::new(),
            deployment_image: OnceLock::new(),
            ir_verification_runs: AtomicU64::new(0),
            continuation_index_runs: AtomicU64::new(0),
            ir_fingerprint_runs: AtomicU64::new(0),
            function_ir_fingerprint_runs: AtomicU64::new(function_ir_fingerprint_runs),
            dependency_identity_runs: AtomicU64::new(0),
            class_validation_runs: AtomicU64::new(0),
        }
    }
}

/// Immutable result of validating the class graph owned by a compiled unit.
///
/// The result belongs to the published unit rather than to a request. Warm
/// execution reads this once-initialized value and never traverses the class
/// hierarchy again.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PreparedClassValidation {
    Valid,
    Invalid(Arc<str>),
}

#[derive(Debug)]
struct PreparedExternalFunctionCalls {
    by_function: Box<[Box<[PreparedExternalFunctionCall]>]>,
    whole_unit: Box<[PreparedExternalFunctionCall]>,
}

/// A statically named call that may resolve to a function in another unit.
///
/// Whether the target is currently visible and has by-reference parameters is
/// intentionally resolved at runtime. Only the source-IR scan and name
/// normalization are prepared here because those are immutable.
#[derive(Debug)]
pub(crate) struct PreparedExternalFunctionCall {
    pub normalized_name: Box<str>,
    pub source_name: Box<str>,
}

/// Immutable userland-call metadata shared by every invocation of a function.
#[derive(Debug)]
pub(crate) struct PreparedNativeFunctionMetadata {
    pub name: Arc<str>,
    pub params: Arc<[php_ir::IrParam]>,
    pub span: IrSpan,
    pub trace_function: Arc<str>,
    pub trace_class: Option<Arc<str>>,
    pub trace_call_type: Option<&'static str>,
    pub trace_file: Option<Arc<str>>,
    pub trace_line: i64,
    pub capture_count: usize,
    pub implicit_closure_this: bool,
    pub instance_method: bool,
}

#[derive(Debug)]
struct PreparedNativeIndexes {
    continuation_instructions: Arc<Vec<Vec<Option<Arc<php_ir::Instruction>>>>>,
    callsites: Arc<Vec<Vec<Option<Arc<NativeCallSiteDescriptor>>>>>,
    property_sites: Arc<Vec<Vec<Option<PreparedNativePropertySite>>>>,
    closure_sites: Arc<Vec<Vec<Option<Arc<PreparedNativeClosureSite>>>>>,
    global_sites: Arc<Vec<Vec<Option<Arc<str>>>>>,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedNativePropertySite {
    pub class_index: u32,
    pub property: Arc<str>,
    pub required_state: u32,
}

/// Immutable exact allocation metadata for one `MakeClosure` continuation.
/// Capture values remain native and are supplied by generated code; this
/// record owns only source/debug descriptors and the target identity.
#[derive(Debug)]
pub(crate) struct PreparedNativeClosureSite {
    pub function: FunctionId,
    pub capture_descriptors: Arc<[(String, bool)]>,
    pub debug: Option<php_runtime::api::ClosureDebugInfo>,
    pub binds_this: bool,
}

/// Typed operation selected by one native callsite descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeCallSiteKind {
    Function,
    Method,
    StaticMethod,
    Closure,
    Callable,
    Pipe,
    Constructor,
    DynamicConstructor,
    Semantic,
}

/// Immutable callsite metadata prepared once with the compiled unit.
///
/// Generated code passes the stable continuation ID. The runtime resolves it
/// directly to this descriptor, so it never scans blocks or reconstructs an IR
/// instruction on the request path. `semantic_instruction` is retained only as
/// the typed semantic payload for operation helpers that have not yet split
/// their source-location argument from their operation-specific metadata.
#[derive(Debug)]
pub(crate) struct NativeCallSiteDescriptor {
    pub kind: NativeCallSiteKind,
    pub span: IrSpan,
    pub target_symbol: Option<Arc<str>>,
    pub target_class: Option<Arc<str>>,
    pub target_function: Option<FunctionId>,
    /// Stable builtin entry resolved once with immutable callsite metadata.
    /// Generated code and this immutable table are published together, so the
    /// warm path needs neither a registry lookup nor redundant ID validation.
    pub direct_builtin: Option<PreparedNativeBuiltin>,
    pub arguments: Arc<[php_ir::instruction::IrCallArg]>,
    pub argument_operand_offset: usize,
    pub pic_slot: u64,
    semantic_instruction: Arc<php_ir::Instruction>,
    method_pic: PersistentNativeMethodPic,
}

/// Immutable builtin metadata resolved while preparing a native callsite.
///
/// Direct builtin calls use this record for dispatch and validation without
/// repeating registry and generated-arginfo hash lookups on every invocation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PreparedNativeBuiltin {
    pub entry: php_runtime::api::BuiltinEntry,
    pub metadata: Option<&'static php_std::generated::arginfo::GeneratedFunctionMetadata>,
    pub type_info: Option<&'static php_std::arginfo::FunctionArgInfo>,
    /// The immutable positional callsite was checked against the published
    /// arginfo once. PHP-visible value/type checks still run where required,
    /// but warm execution does not recount the same signature parameters.
    pub fixed_arity_validated: bool,
}

#[derive(Clone, Copy)]
struct PreparedBuiltinDefinition {
    entry: php_runtime::api::BuiltinEntry,
    metadata: Option<&'static php_std::generated::arginfo::GeneratedFunctionMetadata>,
    type_info: Option<&'static php_std::arginfo::FunctionArgInfo>,
}

impl PreparedNativeBuiltin {
    pub(crate) fn for_entry(
        entry: php_runtime::api::BuiltinEntry,
        argument_count: usize,
        positional: bool,
    ) -> Self {
        let definition = prepared_builtin_definitions()
            .get(entry.dense_id() as usize)
            .copied()
            .filter(|definition| definition.entry.dense_id() == entry.dense_id())
            .expect("published builtin dense ID must resolve to its prepared definition");
        Self::from_definition(definition, argument_count, positional)
    }

    pub(crate) fn for_dense_id(
        dense_id: u32,
        argument_count: usize,
        positional: bool,
    ) -> Option<Self> {
        let definition = prepared_builtin_definitions()
            .get(dense_id as usize)
            .copied()?;
        (definition.entry.dense_id() == dense_id)
            .then(|| Self::from_definition(definition, argument_count, positional))
    }

    fn from_definition(
        definition: PreparedBuiltinDefinition,
        argument_count: usize,
        positional: bool,
    ) -> Self {
        let PreparedBuiltinDefinition {
            entry,
            metadata,
            type_info,
        } = definition;
        Self {
            entry,
            metadata,
            type_info,
            fixed_arity_validated: positional
                && prepared_builtin_arity_is_valid(entry.name(), metadata, argument_count),
        }
    }
}

fn prepared_builtin_definitions() -> &'static [PreparedBuiltinDefinition] {
    PREPARED_BUILTIN_DEFINITIONS.get_or_init(|| {
        php_runtime::api::BuiltinRegistry::new()
            .entries()
            .iter()
            .copied()
            .map(|entry| {
                let metadata = php_std::arginfo::function_metadata_indexed(entry.name());
                let type_info = metadata
                    .filter(|metadata| {
                        matches!(metadata.extension, "hash" | "json" | "pcre" | "tokenizer")
                            && !metadata.params.iter().any(|parameter| {
                                parameter
                                    .type_decl
                                    .split('|')
                                    .any(|atom| atom.trim() == "callable")
                            })
                    })
                    .and_then(|_| php_std::arginfo::function_arginfo_indexed(entry.name()));
                PreparedBuiltinDefinition {
                    entry,
                    metadata,
                    type_info,
                }
            })
            .collect::<Vec<_>>()
            .into_boxed_slice()
    })
}

impl NativeCallSiteDescriptor {
    pub(crate) fn semantic_instruction(&self) -> &php_ir::Instruction {
        self.semantic_instruction.as_ref()
    }

    pub(crate) fn lookup_method_pic(
        &self,
        receiver_class: &str,
        method: &str,
        class_layout_epoch: u64,
        method_table_epoch: u64,
    ) -> Option<(FunctionId, bool)> {
        self.method_pic.lookup(
            receiver_class,
            method,
            class_layout_epoch,
            method_table_epoch,
        )
    }

    pub(crate) fn install_method_pic(
        &self,
        receiver_class: &str,
        method: &str,
        class_layout_epoch: u64,
        method_table_epoch: u64,
        function: FunctionId,
        is_static: bool,
    ) -> bool {
        self.method_pic.install(PersistentNativeMethodPicEntry {
            receiver_class: Arc::from(receiver_class),
            method: Arc::from(method),
            class_layout_epoch,
            method_table_epoch,
            function,
            is_static,
        })
    }
}

const PERSISTENT_NATIVE_METHOD_PIC_LIMIT: usize = 4;

#[derive(Debug)]
struct PersistentNativeMethodPicEntry {
    receiver_class: Arc<str>,
    method: Arc<str>,
    class_layout_epoch: u64,
    method_table_epoch: u64,
    function: FunctionId,
    is_static: bool,
}

#[derive(Debug)]
struct PersistentNativeMethodPic {
    entries:
        [std::sync::OnceLock<PersistentNativeMethodPicEntry>; PERSISTENT_NATIVE_METHOD_PIC_LIMIT],
    megamorphic: std::sync::atomic::AtomicBool,
}

impl Default for PersistentNativeMethodPic {
    fn default() -> Self {
        Self {
            entries: std::array::from_fn(|_| std::sync::OnceLock::new()),
            megamorphic: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl PersistentNativeMethodPic {
    fn matches(
        entry: &PersistentNativeMethodPicEntry,
        receiver_class: &str,
        method: &str,
        class_layout_epoch: u64,
        method_table_epoch: u64,
    ) -> bool {
        entry.receiver_class.eq_ignore_ascii_case(receiver_class)
            && entry.method.eq_ignore_ascii_case(method)
            && entry.class_layout_epoch == class_layout_epoch
            && entry.method_table_epoch == method_table_epoch
    }

    fn lookup(
        &self,
        receiver_class: &str,
        method: &str,
        class_layout_epoch: u64,
        method_table_epoch: u64,
    ) -> Option<(FunctionId, bool)> {
        if self.megamorphic.load(std::sync::atomic::Ordering::Acquire) {
            return None;
        }
        self.entries.iter().find_map(|entry| {
            let entry = entry.get()?;
            Self::matches(
                entry,
                receiver_class,
                method,
                class_layout_epoch,
                method_table_epoch,
            )
            .then_some((entry.function, entry.is_static))
        })
    }

    fn install(&self, mut candidate: PersistentNativeMethodPicEntry) -> bool {
        if self.megamorphic.load(std::sync::atomic::Ordering::Acquire) {
            return false;
        }
        loop {
            for entry in &self.entries {
                if let Some(entry) = entry.get()
                    && Self::matches(
                        entry,
                        &candidate.receiver_class,
                        &candidate.method,
                        candidate.class_layout_epoch,
                        candidate.method_table_epoch,
                    )
                {
                    return true;
                }
            }
            let Some(empty) = self.entries.iter().find(|entry| entry.get().is_none()) else {
                self.megamorphic
                    .store(true, std::sync::atomic::Ordering::Release);
                return false;
            };
            match empty.set(candidate) {
                Ok(()) => return true,
                Err(returned) => candidate = returned,
            }
        }
    }
}

/// Number of immutable preparation passes performed for a compiled unit.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PreparedUnitStats {
    /// IR verification passes.
    pub ir_verification_runs: u64,
    /// Native continuation-source indexes built.
    pub continuation_index_runs: u64,
    /// Stable full-IR fingerprints computed.
    pub ir_fingerprint_runs: u64,
    /// Batched function-scoped fingerprint passes.
    pub function_ir_fingerprint_runs: u64,
    /// Stable dependency identities computed.
    pub dependency_identity_runs: u64,
    /// Static class-table validation passes.
    pub class_validation_runs: u64,
}

/// Measurable ownership and retention properties of a compiled artifact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompiledUnitLayoutStats {
    /// Number of source-map files in the IR.
    pub source_files: usize,
    /// Number of files whose exact source text is retained.
    pub retained_source_files: usize,
    /// Bytes retained for runtime diagnostics.
    pub retained_source_bytes: usize,
    /// Canonical class entries owned by the IR unit.
    pub canonical_classes: usize,
    /// Deep class copies owned by `CompiledUnit` (always zero).
    pub duplicated_classes: usize,
    /// Symbol-table entries indexed without copying their names.
    pub indexed_symbols: usize,
    /// Name bytes duplicated by lookup indexes (always zero).
    pub duplicated_symbol_name_bytes: usize,
}

impl CompiledUnit {
    pub(crate) fn prepared_deployment_image(&self) -> &PreparedDeploymentNativeImage {
        self.inner.prepared.deployment_image.get_or_init(|| {
            let unit = self.unit();
            let mut function_exports =
                std::collections::HashMap::with_capacity(unit.function_table.len());
            for entry in &unit.function_table {
                function_exports
                    .entry(Arc::<str>::from(entry.name.to_ascii_lowercase()))
                    .or_insert(entry.function);
            }
            PreparedDeploymentNativeImage {
                function_exports: Arc::new(function_exports),
                exported_classes: Arc::new(
                    unit.classes
                        .iter()
                        .filter(|class| class.span.start != 0 || class.span.end != 0)
                        .map(|class| Arc::<str>::from(class.name.as_str()))
                        .collect(),
                ),
                native_call_argument_capacity: unit
                    .functions
                    .iter()
                    .map(|function| function.params.len() + function.captures.len() + 1)
                    .max()
                    .unwrap_or(0),
                constant_views: unit
                    .constants
                    .iter()
                    .map(|constant| match constant {
                        php_ir::IrConstant::Null => php_jit::JitNativeConstantView {
                            kind: php_jit::JIT_NATIVE_CONSTANT_VIEW_NULL,
                            ..php_jit::JitNativeConstantView::default()
                        },
                        php_ir::IrConstant::Bool(value) => php_jit::JitNativeConstantView {
                            kind: php_jit::JIT_NATIVE_CONSTANT_VIEW_BOOL,
                            length: u64::from(*value),
                            ..php_jit::JitNativeConstantView::default()
                        },
                        php_ir::IrConstant::Int(value) => php_jit::JitNativeConstantView {
                            kind: php_jit::JIT_NATIVE_CONSTANT_VIEW_INT,
                            length: *value as u64,
                            ..php_jit::JitNativeConstantView::default()
                        },
                        php_ir::IrConstant::Float(value) => php_jit::JitNativeConstantView {
                            kind: php_jit::JIT_NATIVE_CONSTANT_VIEW_FLOAT,
                            length: value.to_bits(),
                            ..php_jit::JitNativeConstantView::default()
                        },
                        php_ir::IrConstant::String(value) => php_jit::JitNativeConstantView {
                            kind: php_jit::JIT_NATIVE_CONSTANT_VIEW_STRING,
                            reserved: 0,
                            length: value.len() as u64,
                            bytes: value.as_ptr() as usize as u64,
                        },
                        php_ir::IrConstant::StringBytes(value) => php_jit::JitNativeConstantView {
                            kind: php_jit::JIT_NATIVE_CONSTANT_VIEW_STRING,
                            reserved: 0,
                            length: value.len() as u64,
                            bytes: value.as_ptr() as usize as u64,
                        },
                        _ => php_jit::JitNativeConstantView::default(),
                    })
                    .collect(),
                native_function_entries: (0..unit.functions.len())
                    .map(|_| std::sync::atomic::AtomicUsize::new(0))
                    .collect(),
                preferred_function_entries: (0..unit.functions.len())
                    .map(|_| std::sync::atomic::AtomicUsize::new(0))
                    .collect(),
            }
        })
    }

    /// Wraps an IR unit and snapshots all source files that are currently readable.
    #[must_use]
    pub fn new(unit: IrUnit) -> Self {
        let sources = unit
            .files
            .iter()
            .map(|file| {
                std::fs::read_to_string(&file.path)
                    .ok()
                    .map(Arc::<str>::from)
            })
            .collect();
        Self::with_source_slots(unit, sources)
    }

    /// Wraps an IR unit with exact source text captured by the compiler.
    pub fn try_with_sources(
        unit: IrUnit,
        sources: impl IntoIterator<Item = (php_ir::ids::FileId, Arc<str>)>,
    ) -> Result<Self, CompiledUnitBuildError> {
        let mut source_slots = vec![None; unit.files.len()];
        for (file, source) in sources {
            let Some(slot) = source_slots.get_mut(file.index()) else {
                return Err(CompiledUnitBuildError::UnknownSourceFile(file));
            };
            if slot.is_some() {
                return Err(CompiledUnitBuildError::DuplicateSourceFile(file));
            }
            *slot = Some(source);
        }
        Ok(Self::with_source_slots(unit, source_slots))
    }

    /// Wraps compiler-owned sources already ordered like `IrUnit::files`.
    #[must_use]
    pub fn with_ordered_sources(unit: IrUnit, sources: impl IntoIterator<Item = Arc<str>>) -> Self {
        let mut source_slots = sources.into_iter().map(Some).collect::<Vec<_>>();
        source_slots.truncate(unit.files.len());
        source_slots.resize_with(unit.files.len(), || None);
        Self::with_source_slots(unit, source_slots)
    }

    fn with_source_slots(unit: IrUnit, sources: Vec<Option<Arc<str>>>) -> Self {
        let function_lookup = SymbolIndex::new(
            unit.function_table
                .iter()
                .enumerate()
                .map(|(index, entry)| (index, stable_hash(entry.name.as_bytes()))),
        );
        let constant_lookup = SymbolIndex::new(
            unit.constant_table
                .iter()
                .enumerate()
                .map(|(index, entry)| (index, stable_hash(entry.name.as_bytes()))),
        );
        let class_table = unit
            .classes
            .iter()
            .enumerate()
            .filter(|(_, entry)| !entry.flags.is_conditional)
            .map(|(index, _)| index)
            .collect::<Box<[_]>>();
        let class_lookup = SymbolIndex::new(class_table.iter().copied().map(|index| {
            (
                index,
                stable_hash(normalize_class_name(&unit.classes[index].name).as_bytes()),
            )
        }));
        let unit_class_lookup =
            SymbolIndex::new(unit.classes.iter().enumerate().map(|(index, entry)| {
                (
                    index,
                    stable_hash(normalize_class_name(&entry.name).as_bytes()),
                )
            }));
        let sources = CompiledSourceRepository {
            entries: sources
                .into_iter()
                .map(|source| {
                    source.map(|text| CompiledSource {
                        lines: LineIndex::new(&text),
                        text,
                    })
                })
                .collect(),
        };
        let has_complete_source_identity = !sources.entries.is_empty()
            && sources.entries.len() == unit.files.len()
            && sources.entries.iter().all(Option::is_some);
        let function_ir_fingerprints = (!has_complete_source_identity)
            .then(|| php_jit::stable_function_ir_fingerprints(&unit).into_boxed_slice());
        let artifact_identity =
            artifact_identity(&unit, &sources, function_ir_fingerprints.as_deref());
        let function_count = unit.functions.len();
        Self {
            inner: Arc::new(CompiledUnitInner {
                cache_id: NEXT_COMPILED_UNIT_CACHE_ID.fetch_add(1, Ordering::Relaxed),
                artifact_identity,
                unit,
                class_table,
                function_lookup,
                constant_lookup,
                class_lookup,
                unit_class_lookup,
                sources,
                prepared: PreparedUnit::new(function_count, function_ir_fingerprints),
            }),
        }
    }

    /// Returns the underlying IR unit.
    #[must_use]
    pub fn unit(&self) -> &IrUnit {
        &self.inner.unit
    }

    /// Stable identity for VM-local artifact caches.
    #[must_use]
    pub fn cache_identity(&self) -> u64 {
        self.inner.cache_id
    }

    /// Stable identity derived from unit, path, and retained source contents.
    #[must_use]
    pub fn artifact_identity(&self) -> u64 {
        self.inner.artifact_identity
    }

    /// Returns ownership counters used by architecture and memory benchmarks.
    #[must_use]
    pub fn layout_stats(&self) -> CompiledUnitLayoutStats {
        CompiledUnitLayoutStats {
            source_files: self.inner.sources.entries.len(),
            retained_source_files: self
                .inner
                .sources
                .entries
                .iter()
                .filter(|source| source.is_some())
                .count(),
            retained_source_bytes: self
                .inner
                .sources
                .entries
                .iter()
                .flatten()
                .map(|source| source.text.len())
                .sum(),
            canonical_classes: self.inner.unit.classes.len(),
            duplicated_classes: 0,
            indexed_symbols: self.inner.unit.function_table.len()
                + self.inner.unit.constant_table.len()
                + self.inner.unit.classes.len(),
            duplicated_symbol_name_bytes: 0,
        }
    }

    /// Serializes stable cache/debug metadata without serializing executable IR.
    #[must_use]
    pub fn metadata_json(&self) -> String {
        let stats = self.layout_stats();
        format!(
            concat!(
                "{{\"schema\":\"phrust.compiled-unit.v1\",",
                "\"unit_id\":{},\"artifact_identity\":\"{:016x}\",",
                "\"source_files\":{},\"retained_source_files\":{},",
                "\"retained_source_bytes\":{},\"canonical_classes\":{},",
                "\"duplicated_classes\":{},\"indexed_symbols\":{},",
                "\"duplicated_symbol_name_bytes\":{}}}"
            ),
            self.inner.unit.id.raw(),
            self.inner.artifact_identity,
            stats.source_files,
            stats.retained_source_files,
            stats.retained_source_bytes,
            stats.canonical_classes,
            stats.duplicated_classes,
            stats.indexed_symbols,
            stats.duplicated_symbol_name_bytes,
        )
    }

    /// Returns true when two handles point at the same compiled unit allocation.
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Returns the cached immutable IR verification result.
    pub(crate) fn prepared_ir_verification_errors(&self) -> usize {
        *self.inner.prepared.ir_verification_errors.get_or_init(|| {
            self.inner
                .prepared
                .ir_verification_runs
                .fetch_add(1, Ordering::Relaxed);
            verify_unit(&self.inner.unit).map_or_else(|errors| errors.len(), |()| 0)
        })
    }

    pub(crate) fn prepared_class_validation(&self) -> &PreparedClassValidation {
        self.inner.prepared.class_validation.get_or_init(|| {
            self.inner
                .prepared
                .class_validation_runs
                .fetch_add(1, Ordering::Relaxed);
            validate_native_class_table(&self.inner.unit).map_or_else(
                |diagnostic| PreparedClassValidation::Invalid(Arc::from(diagnostic)),
                |()| PreparedClassValidation::Valid,
            )
        })
    }

    fn prepared_native_indexes(&self) -> &PreparedNativeIndexes {
        self.inner.prepared.native_indexes.get_or_init(|| {
            self.inner
                .prepared
                .continuation_index_runs
                .fetch_add(1, Ordering::Relaxed);
            let mut instructions = Vec::with_capacity(self.inner.unit.functions.len());
            let mut callsites = Vec::with_capacity(self.inner.unit.functions.len());
            let mut property_sites = Vec::with_capacity(self.inner.unit.functions.len());
            let mut closure_sites = Vec::with_capacity(self.inner.unit.functions.len());
            let mut global_sites = Vec::with_capacity(self.inner.unit.functions.len());
            let metadata = php_jit::region_ir::CompileMetadata::default();
            for function_index in 0..self.inner.unit.functions.len() {
                let function = FunctionId::new(function_index as u32);
                let mut function_instructions = Vec::new();
                let mut function_callsites = Vec::new();
                let mut function_property_sites = Vec::new();
                let mut function_closure_sites = Vec::new();
                let mut function_global_sites = Vec::new();
                if let Ok(region) = php_jit::region_ir::BaselineRegionBuilder::build(
                    &self.inner.unit,
                    function,
                    &metadata,
                ) {
                    for block in &region.blocks {
                        for instruction in &block.instructions {
                            let semantic_instruction = Arc::new(php_ir::Instruction {
                                id: instruction.id,
                                span: instruction.span,
                                kind: instruction.source_kind.clone(),
                            });
                            let continuation = instruction.continuation_id as usize;
                            if function_instructions.len() <= continuation {
                                function_instructions.resize_with(continuation + 1, || None);
                            }
                            function_instructions[continuation] =
                                Some(Arc::clone(&semantic_instruction));
                            if let Some(name) = instruction.native_global_name.as_deref() {
                                if function_global_sites.len() <= continuation {
                                    function_global_sites.resize_with(continuation + 1, || None);
                                }
                                function_global_sites[continuation] = Some(Arc::from(name));
                            }
                            let property_site = match &instruction.kind {
                                php_jit::region_ir::RegionInstructionKind::FetchProperty {
                                    property,
                                    prepared_class: Some(class_index),
                                    ..
                                } => Some(PreparedNativePropertySite {
                                    class_index: *class_index,
                                    property: Arc::from(property.as_str()),
                                    required_state:
                                        php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_PUBLISHED,
                                }),
                                php_jit::region_ir::RegionInstructionKind::AssignProperty {
                                    property,
                                    prepared_class: Some(class_index),
                                    ..
                                } => Some(PreparedNativePropertySite {
                                    class_index: *class_index,
                                    property: Arc::from(property.as_str()),
                                    required_state:
                                        php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_WRITABLE,
                                }),
                                php_jit::region_ir::RegionInstructionKind::BindReferenceProperty {
                                    property,
                                    prepared_class: Some(class_index),
                                    ..
                                }
                                | php_jit::region_ir::RegionInstructionKind::BindReferenceFromProperty {
                                    property,
                                    prepared_class: Some(class_index),
                                    ..
                                }
                                | php_jit::region_ir::RegionInstructionKind::BindReferenceDimFromProperty {
                                    property,
                                    prepared_class: Some(class_index),
                                    ..
                                } => Some(PreparedNativePropertySite {
                                    class_index: *class_index,
                                    property: Arc::from(property.as_str()),
                                    required_state:
                                        php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_REFERENCEABLE,
                                }),
                                php_jit::region_ir::RegionInstructionKind::BindReferenceIntoPropertyDim {
                                    property,
                                    prepared_class: Some(class_index),
                                    ..
                                }
                                | php_jit::region_ir::RegionInstructionKind::BindReferenceFromPropertyDim {
                                    property,
                                    prepared_class: Some(class_index),
                                    ..
                                } => Some(PreparedNativePropertySite {
                                    class_index: *class_index,
                                    property: Arc::from(property.as_str()),
                                    required_state:
                                        php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_DIMENSION_WRITABLE,
                                }),
                                _ => None,
                            };
                            if let Some(property_site) = property_site {
                                if function_property_sites.len() <= continuation {
                                    function_property_sites.resize_with(continuation + 1, || None);
                                }
                                function_property_sites[continuation] = Some(property_site);
                            }
                            if let php_jit::region_ir::RegionInstructionKind::NativeDynamicCode(
                                php_jit::region_ir::RegionNativeDynamicCode::MakeClosure {
                                    function: closure_function,
                                    captures,
                                    binds_this,
                                    ..
                                },
                            ) = &instruction.kind
                            {
                                let debug = self
                                    .inner
                                    .unit
                                    .functions
                                    .get(closure_function.index())
                                    .and_then(|function| {
                                        let file =
                                            self.inner.unit.files.get(function.span.file.index())?;
                                        let line = self
                                            .source_display_line(function.span, false)
                                            .unwrap_or(1);
                                        Some(php_runtime::api::ClosureDebugInfo {
                                            name: format!("{{closure:{}:{line}}}", file.path),
                                            file: file.path.clone(),
                                            line,
                                            parameters: function
                                                .params
                                                .iter()
                                                .map(|parameter| {
                                                    php_runtime::api::ClosureDebugParameter {
                                                        name: parameter.name.clone(),
                                                        required: parameter.required,
                                                    }
                                                })
                                                .collect(),
                                        })
                                    });
                                if function_closure_sites.len() <= continuation {
                                    function_closure_sites
                                        .resize_with(continuation + 1, || None);
                                }
                                function_closure_sites[continuation] =
                                    Some(Arc::new(PreparedNativeClosureSite {
                                        function: *closure_function,
                                        capture_descriptors: Arc::from(
                                            captures
                                                .iter()
                                                .map(|capture| {
                                                    (capture.name.clone(), capture.by_ref)
                                                })
                                                .collect::<Vec<_>>(),
                                        ),
                                        debug,
                                        binds_this: *binds_this,
                                    }));
                            }
                            if let php_jit::region_ir::RegionInstructionKind::NativeCall(call) =
                                &instruction.kind
                            {
                                let (kind, target_symbol, target_function) =
                                    native_callsite_target(&call.target);
                                let target_class = match &call.target {
                                    php_jit::region_ir::RegionCallTarget::StaticMethod {
                                        class_name,
                                        ..
                                    }
                                    | php_jit::region_ir::RegionCallTarget::Constructor {
                                        class_name,
                                        ..
                                    } => Some(Arc::from(class_name.as_str())),
                                    _ => None,
                                };
                                let direct_builtin = if kind == NativeCallSiteKind::Function
                                    && target_function.is_none()
                                {
                                    target_symbol.as_deref().and_then(|name| {
                                        let normalized = name.trim_start_matches('\\');
                                        (!normalized.contains('\\'))
                                            .then(|| normalized.to_ascii_lowercase())
                                            .and_then(|normalized| {
                                                php_runtime::api::BuiltinRegistry::new()
                                                    .get(&normalized)
                                            })
                                            .filter(|entry| entry.helper_id() != 0)
                                            .map(|entry| {
                                                PreparedNativeBuiltin::for_entry(
                                                    entry,
                                                    call.args.len(),
                                                    call.args.iter().all(|argument| {
                                                        argument.name.is_none() && !argument.unpack
                                                    }),
                                                )
                                            })
                                    })
                                } else {
                                    None
                                };
                                let pic_slot = (u64::from(function.raw()) << 32)
                                    | u64::from(instruction.continuation_id);
                                let continuation = instruction.continuation_id as usize;
                                if function_callsites.len() <= continuation {
                                    function_callsites.resize_with(continuation + 1, || None);
                                }
                                function_callsites[continuation] =
                                    Some(Arc::new(NativeCallSiteDescriptor {
                                        kind,
                                        span: instruction.span,
                                        target_symbol,
                                        target_class,
                                        target_function,
                                        direct_builtin,
                                        arguments: Arc::from(call.args.clone()),
                                        argument_operand_offset: call.argument_operand_offset,
                                        pic_slot,
                                        semantic_instruction,
                                        method_pic: PersistentNativeMethodPic::default(),
                                    }));
                            }
                        }
                    }
                }
                instructions.push(function_instructions);
                callsites.push(function_callsites);
                property_sites.push(function_property_sites);
                closure_sites.push(function_closure_sites);
                global_sites.push(function_global_sites);
            }
            PreparedNativeIndexes {
                continuation_instructions: Arc::new(instructions),
                callsites: Arc::new(callsites),
                property_sites: Arc::new(property_sites),
                closure_sites: Arc::new(closure_sites),
                global_sites: Arc::new(global_sites),
            }
        })
    }

    pub(crate) fn prepared_continuation_instructions(
        &self,
    ) -> Arc<Vec<Vec<Option<Arc<php_ir::Instruction>>>>> {
        Arc::clone(&self.prepared_native_indexes().continuation_instructions)
    }

    pub(crate) fn prepared_native_callsites(
        &self,
    ) -> Arc<Vec<Vec<Option<Arc<NativeCallSiteDescriptor>>>>> {
        Arc::clone(&self.prepared_native_indexes().callsites)
    }

    pub(crate) fn prepared_native_property_sites(
        &self,
    ) -> Arc<Vec<Vec<Option<PreparedNativePropertySite>>>> {
        Arc::clone(&self.prepared_native_indexes().property_sites)
    }

    pub(crate) fn prepared_native_closure_sites(
        &self,
    ) -> Arc<Vec<Vec<Option<Arc<PreparedNativeClosureSite>>>>> {
        Arc::clone(&self.prepared_native_indexes().closure_sites)
    }

    pub(crate) fn prepared_native_global_sites(&self) -> Arc<Vec<Vec<Option<Arc<str>>>>> {
        Arc::clone(&self.prepared_native_indexes().global_sites)
    }

    fn prepared_external_function_call_index(&self) -> &PreparedExternalFunctionCalls {
        self.inner.prepared.external_function_calls.get_or_init(|| {
            let local_functions = self
                .inner
                .unit
                .function_table
                .iter()
                .map(|entry| entry.name.to_ascii_lowercase())
                .collect::<std::collections::HashSet<_>>();
            let mut whole_unit = BTreeMap::<String, String>::new();
            let by_function = self
                .inner
                .unit
                .functions
                .iter()
                .map(|function| {
                    let mut calls = BTreeMap::<String, String>::new();
                    for instruction in function.blocks.iter().flat_map(|block| &block.instructions)
                    {
                        let name = match &instruction.kind {
                            php_ir::InstructionKind::CallFunction { name, .. }
                            | php_ir::InstructionKind::BindReferenceFromCall { name, .. } => name,
                            _ => continue,
                        };
                        let normalized = name.to_ascii_lowercase();
                        if local_functions.contains(&normalized) {
                            continue;
                        }
                        calls.insert(normalized.clone(), name.clone());
                        whole_unit.insert(normalized, name.clone());
                    }
                    calls
                        .into_iter()
                        .map(
                            |(normalized_name, source_name)| PreparedExternalFunctionCall {
                                normalized_name: normalized_name.into_boxed_str(),
                                source_name: source_name.into_boxed_str(),
                            },
                        )
                        .collect::<Vec<_>>()
                        .into_boxed_slice()
                })
                .collect::<Vec<_>>()
                .into_boxed_slice();
            let whole_unit = whole_unit
                .into_iter()
                .map(
                    |(normalized_name, source_name)| PreparedExternalFunctionCall {
                        normalized_name: normalized_name.into_boxed_str(),
                        source_name: source_name.into_boxed_str(),
                    },
                )
                .collect::<Vec<_>>()
                .into_boxed_slice();
            PreparedExternalFunctionCalls {
                by_function,
                whole_unit,
            }
        })
    }

    pub(crate) fn prepared_external_function_calls(
        &self,
        function: FunctionId,
    ) -> &[PreparedExternalFunctionCall] {
        self.prepared_external_function_call_index()
            .by_function
            .get(function.index())
            .map_or(&[], Box::as_ref)
    }

    pub(crate) fn prepared_unit_external_function_calls(&self) -> &[PreparedExternalFunctionCall] {
        &self.prepared_external_function_call_index().whole_unit
    }

    pub(crate) fn prepared_native_function_metadata_ptr(
        &self,
        function: FunctionId,
    ) -> Option<*const PreparedNativeFunctionMetadata> {
        self.inner
            .prepared
            .native_function_metadata
            .get_or_init(|| {
                let method_metadata = self
                    .inner
                    .unit
                    .classes
                    .iter()
                    .flat_map(|class| {
                        class.methods.iter().map(move |method| {
                            (
                                method.function,
                                (
                                    Arc::<str>::from(class.display_name.as_str()),
                                    if method.flags.is_static { "::" } else { "->" },
                                ),
                            )
                        })
                    })
                    .collect::<std::collections::HashMap<_, _>>();
                self.inner
                    .unit
                    .functions
                    .iter()
                    .enumerate()
                    .map(|(index, function)| {
                        let function_id = FunctionId::new(
                            u32::try_from(index).expect("function index exceeds u32"),
                        );
                        let trace_function = function
                            .name
                            .rsplit_once("::")
                            .map_or(function.name.as_str(), |(_, method)| method);
                        let (trace_class, trace_call_type) = method_metadata
                            .get(&function_id)
                            .map_or((None, None), |(class, call_type)| {
                                (Some(Arc::clone(class)), Some(*call_type))
                            });
                        Arc::new(PreparedNativeFunctionMetadata {
                            name: Arc::from(function.name.as_str()),
                            params: Arc::from(function.params.clone()),
                            span: function.span,
                            trace_function: Arc::from(trace_function),
                            trace_class,
                            trace_call_type,
                            trace_file: self
                                .inner
                                .unit
                                .files
                                .get(function.span.file.index())
                                .map(|file| Arc::from(file.path.as_str())),
                            trace_line: self.source_display_line(function.span, false).unwrap_or(0),
                            capture_count: function.captures.len(),
                            implicit_closure_this: function.implicit_closure_this_local().is_some(),
                            instance_method: method_metadata
                                .get(&function_id)
                                .is_some_and(|(_, call_type)| *call_type == "->"),
                        })
                    })
                    .collect::<Vec<_>>()
                    .into_boxed_slice()
            })
            .get(function.index())
            .map(Arc::as_ptr)
    }

    pub(crate) fn prepared_ir_fingerprint(&self) -> &str {
        self.inner.prepared.ir_fingerprint.get_or_init(|| {
            self.inner
                .prepared
                .ir_fingerprint_runs
                .fetch_add(1, Ordering::Relaxed);
            // Native linkage needs a source-sensitive deployment namespace,
            // not a second serialization of the complete IR. The artifact
            // identity was already computed while constructing this unit and
            // includes its retained source contents and declaration tables.
            format!(
                "php-compiled-artifact-v1-{:016x}",
                self.inner.artifact_identity
            )
        })
    }

    pub(crate) fn prepared_function_ir_fingerprint(&self, function: FunctionId) -> Option<&str> {
        let prepared = &self.inner.prepared;
        Some(
            prepared
                .function_ir_fingerprints
                .get(function.index())?
                .get_or_init(|| {
                    prepared
                        .function_ir_fingerprint_runs
                        .fetch_add(1, Ordering::Relaxed);
                    let context = *prepared.function_ir_fingerprint_context.get_or_init(|| {
                        php_jit::stable_function_ir_fingerprint_context(&self.inner.unit)
                    });
                    php_jit::stable_function_ir_fingerprint_in_context(
                        &self.inner.unit,
                        function,
                        context,
                    )
                })
                .as_str(),
        )
    }

    pub(crate) fn prepared_dependency_identity(&self) -> &str {
        self.inner.prepared.dependency_identity.get_or_init(|| {
            self.inner
                .prepared
                .dependency_identity_runs
                .fetch_add(1, Ordering::Relaxed);
            php_jit::stable_dependency_identity(&self.inner.unit)
        })
    }

    /// Preparation counters for validation and diagnostics.
    #[must_use]
    pub fn prepared_unit_stats(&self) -> PreparedUnitStats {
        PreparedUnitStats {
            ir_verification_runs: self
                .inner
                .prepared
                .ir_verification_runs
                .load(Ordering::Relaxed),
            continuation_index_runs: self
                .inner
                .prepared
                .continuation_index_runs
                .load(Ordering::Relaxed),
            ir_fingerprint_runs: self
                .inner
                .prepared
                .ir_fingerprint_runs
                .load(Ordering::Relaxed),
            function_ir_fingerprint_runs: self
                .inner
                .prepared
                .function_ir_fingerprint_runs
                .load(Ordering::Relaxed),
            dependency_identity_runs: self
                .inner
                .prepared
                .dependency_identity_runs
                .load(Ordering::Relaxed),
            class_validation_runs: self
                .inner
                .prepared
                .class_validation_runs
                .load(Ordering::Relaxed),
        }
    }

    /// Finds a user function by normalized name.
    #[must_use]
    pub fn lookup_function(&self, name: &str) -> Option<FunctionId> {
        php_runtime::experimental::layout_stats::record_symbol_map_lookup();
        self.inner
            .function_lookup
            .candidates(name)
            .find_map(|index| {
                let entry = self.inner.unit.function_table.get(index)?;
                (entry.name == name).then_some(entry.function)
            })
    }

    /// Finds a user constant by canonical name.
    #[must_use]
    pub fn lookup_constant(&self, name: &str) -> Option<&IrConstant> {
        php_runtime::experimental::layout_stats::record_symbol_map_lookup();
        let value = self
            .inner
            .constant_lookup
            .candidates(name)
            .find_map(|index| {
                let entry = self.inner.unit.constant_table.get(index)?;
                (entry.name == name).then_some(entry.value)
            })?;
        self.inner.unit.constants.get(value.index())
    }

    /// Finds a class by normalized name.
    #[must_use]
    pub fn lookup_class(&self, name: &str) -> Option<&ClassEntry> {
        php_runtime::experimental::layout_stats::record_symbol_map_lookup();
        let normalized = normalized_class_name(name);
        let index = self
            .inner
            .class_lookup
            .candidates(normalized.as_ref())
            .find(|index| {
                normalize_class_name(&self.inner.unit.classes[*index].name) == normalized.as_ref()
            })?;
        self.inner.unit.classes.get(index)
    }

    /// Finds a class by normalized name, returning a shared handle to the
    /// (potentially large) `ClassEntry` via a cheap `Arc` refcount bump instead
    /// of a deep clone.
    #[must_use]
    pub fn lookup_class_handle(&self, name: &str) -> Option<CompiledClass> {
        php_runtime::experimental::layout_stats::record_symbol_map_lookup();
        let normalized = normalized_class_name(name);
        let index = self
            .inner
            .class_lookup
            .candidates(normalized.as_ref())
            .find(|index| {
                normalize_class_name(&self.inner.unit.classes[*index].name) == normalized.as_ref()
            })?;
        Some(CompiledClass::in_unit(self.clone(), index))
    }

    /// Finds any class entry in the underlying IR unit, including conditional declarations.
    #[must_use]
    pub fn lookup_unit_class(&self, name: &str) -> Option<&ClassEntry> {
        php_runtime::experimental::layout_stats::record_symbol_map_lookup();
        let normalized = normalized_class_name(name);
        let index = self
            .inner
            .unit_class_lookup
            .candidates(normalized.as_ref())
            .find(|index| {
                normalize_class_name(&self.inner.unit.classes[*index].name) == normalized.as_ref()
            })?;
        self.inner.unit.classes.get(index)
    }

    /// Finds any class and returns a handle retaining its canonical unit owner.
    #[must_use]
    pub fn lookup_unit_class_handle(&self, name: &str) -> Option<CompiledClass> {
        let normalized = normalized_class_name(name);
        let index = self
            .inner
            .unit_class_lookup
            .candidates(normalized.as_ref())
            .find(|index| {
                normalize_class_name(&self.inner.unit.classes[*index].name) == normalized.as_ref()
            })?;
        Some(CompiledClass::in_unit(self.clone(), index))
    }

    /// Returns the VM lookup table.
    #[must_use]
    pub fn function_table(&self) -> &[php_ir::module::FunctionEntry] {
        &self.inner.unit.function_table
    }

    /// Returns the VM constant lookup table.
    #[must_use]
    pub fn constant_table(&self) -> &[php_ir::module::GlobalConstantEntry] {
        &self.inner.unit.constant_table
    }

    /// Returns the VM class lookup table.
    pub fn class_table(&self) -> impl Iterator<Item = &ClassEntry> {
        self.inner
            .class_table
            .iter()
            .map(|index| &self.inner.unit.classes[*index])
    }

    /// Returns the display line from the immutable compile-time source snapshot.
    #[must_use]
    pub fn source_display_line(&self, span: IrSpan, end: bool) -> Option<i64> {
        let file_index = span.file.index();
        self.inner.unit.files.get(file_index)?;
        let offset = if end { span.end } else { span.start } as usize;
        self.inner
            .sources
            .entries
            .get(file_index)?
            .as_ref()
            .map(|source| source.lines.line_col(BytePos::new(offset)).line as i64)
    }

    /// Extracts the IR only when this is the unique artifact handle.
    pub fn try_into_unique_unit(self) -> Result<IrUnit, Self> {
        Arc::try_unwrap(self.inner)
            .map(|inner| inner.unit)
            .map_err(|inner| Self { inner })
    }

    /// Intentionally performs a deep copy of the IR.
    #[must_use]
    pub fn deep_clone_unit(&self) -> IrUnit {
        self.inner.unit.clone()
    }
}

impl std::fmt::Debug for CompiledUnit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CompiledUnit")
            .field("unit", &self.inner.unit)
            .field("artifact_identity", &self.inner.artifact_identity)
            .field("retained_sources", &self.inner.sources)
            .finish_non_exhaustive()
    }
}

impl PartialEq for CompiledUnit {
    fn eq(&self, other: &Self) -> bool {
        self.inner.unit == other.inner.unit
            && self.inner.artifact_identity == other.inner.artifact_identity
    }
}

fn artifact_identity(
    unit: &IrUnit,
    sources: &CompiledSourceRepository,
    function_ir_fingerprints: Option<&[String]>,
) -> u64 {
    let mut hash = stable_hash(b"phrust.compiled-unit.v3");
    hash = hash_bytes(hash, &unit.id.raw().to_le_bytes());
    for (index, file) in unit.files.iter().enumerate() {
        hash = hash_field(hash, file.path.as_bytes());
        if let Some(Some(source)) = sources.entries.get(index) {
            hash = hash_bytes(hash, &[1]);
            hash = hash_field(hash, source.text.as_bytes());
        } else {
            hash = hash_bytes(hash, &[0]);
        }
    }
    for entry in &unit.function_table {
        hash = hash_field(hash, entry.name.as_bytes());
        hash = hash_bytes(hash, &entry.function.raw().to_le_bytes());
    }
    for entry in &unit.constant_table {
        hash = hash_field(hash, entry.name.as_bytes());
        hash = hash_bytes(hash, &entry.value.raw().to_le_bytes());
    }
    for class in &unit.classes {
        hash = hash_field(hash, class.name.as_bytes());
        hash = hash_bytes(hash, &class.id.raw().to_le_bytes());
    }
    if let Some(function_ir_fingerprints) = function_ir_fingerprints {
        hash = hash_bytes(hash, &[0]);
        for fingerprint in function_ir_fingerprints {
            hash = hash_field(hash, fingerprint.as_bytes());
        }
    } else {
        // Complete retained source text is the canonical identity. The
        // compiler/build identity separately versions deterministic lowering.
        hash = hash_bytes(hash, &[1]);
    }
    hash
}

fn validate_native_class_table(unit: &IrUnit) -> Result<(), String> {
    let find_class = |name: &str| {
        let normalized = normalize_class_name(name);
        unit.classes.iter().find(|class| class.name == normalized)
    };
    for class in unit
        .classes
        .iter()
        .filter(|class| !class.flags.is_conditional)
    {
        if let Some(parent_name) = class.parent.as_deref()
            && let Some(parent) = find_class(parent_name)
        {
            if parent.flags.is_final || parent.flags.is_enum {
                return Err(format!(
                    "Class {} cannot extend final class {}",
                    class.display_name, parent.display_name
                ));
            }
            for method in &class.methods {
                let mut ancestor = Some(parent);
                while let Some(current) = ancestor {
                    if current.methods.iter().any(|candidate| {
                        candidate.name.eq_ignore_ascii_case(&method.name)
                            && candidate.flags.is_final
                    }) {
                        return Err(format!(
                            "Cannot override final method {}::{}()",
                            current.display_name, method.name
                        ));
                    }
                    ancestor = current.parent.as_deref().and_then(&find_class);
                }
            }
        }

        if class.flags.is_abstract || class.flags.is_interface || class.flags.is_trait {
            continue;
        }
        let implements = |name: &str| {
            let mut current = Some(class);
            while let Some(candidate) = current {
                if let Some(method) = candidate
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case(name))
                {
                    return Some(method);
                }
                current = candidate.parent.as_deref().and_then(&find_class);
            }
            None
        };
        let mut required = Vec::new();
        let mut ancestor = class.parent.as_deref().and_then(&find_class);
        while let Some(current) = ancestor {
            required.extend(
                current
                    .methods
                    .iter()
                    .filter(|method| method.flags.is_abstract)
                    .map(|method| (current, method)),
            );
            ancestor = current.parent.as_deref().and_then(&find_class);
        }
        for interface_name in &class.interfaces {
            if let Some(interface) = find_class(interface_name) {
                required.extend(interface.methods.iter().map(|method| (interface, method)));
            }
        }
        for (owner, method) in required {
            let Some(implementation) = implements(&method.name) else {
                return Err(format!(
                    "Class {} contains an abstract method {}::{}()",
                    class.display_name, owner.display_name, method.name
                ));
            };
            if implementation.flags.is_abstract {
                return Err(format!(
                    "Class {} contains an abstract method {}::{}()",
                    class.display_name, owner.display_name, method.name
                ));
            }
            if owner.flags.is_interface
                && (implementation.flags.is_private || implementation.flags.is_protected)
            {
                return Err(format!(
                    "Access level to {}::{}() must be public",
                    class.display_name, method.name
                ));
            }
        }
    }
    Ok(())
}

fn stable_hash(bytes: &[u8]) -> u64 {
    hash_bytes(0xcbf2_9ce4_8422_2325, bytes)
}

fn native_callsite_target(
    target: &php_jit::region_ir::RegionCallTarget,
) -> (NativeCallSiteKind, Option<Arc<str>>, Option<FunctionId>) {
    use php_jit::region_ir::RegionCallTarget;
    match target {
        RegionCallTarget::Function { name, function } => (
            NativeCallSiteKind::Function,
            Some(Arc::from(name.as_str())),
            *function,
        ),
        RegionCallTarget::Method { method, .. } => (
            NativeCallSiteKind::Method,
            Some(Arc::from(method.as_str())),
            None,
        ),
        RegionCallTarget::StaticMethod { method, .. } => (
            NativeCallSiteKind::StaticMethod,
            Some(Arc::from(method.as_str())),
            None,
        ),
        RegionCallTarget::Closure { .. } => (NativeCallSiteKind::Closure, None, None),
        RegionCallTarget::Callable { .. } => (NativeCallSiteKind::Callable, None, None),
        RegionCallTarget::Pipe { .. } => (NativeCallSiteKind::Pipe, None, None),
        RegionCallTarget::Constructor { class_name, .. } => (
            NativeCallSiteKind::Constructor,
            Some(Arc::from(class_name.as_str())),
            None,
        ),
        RegionCallTarget::DynamicConstructor { .. } => {
            (NativeCallSiteKind::DynamicConstructor, None, None)
        }
        RegionCallTarget::Semantic { .. } => (NativeCallSiteKind::Semantic, None, None),
    }
}

fn prepared_builtin_arity_is_valid(
    name: &str,
    metadata: Option<&php_std::generated::arginfo::GeneratedFunctionMetadata>,
    argument_count: usize,
) -> bool {
    let Some(metadata) = metadata else {
        return true;
    };
    let required = metadata
        .params
        .iter()
        .filter(|parameter| {
            !parameter.optional && parameter.default_value.is_none() && !parameter.variadic
        })
        .count();
    let required = match name {
        "array_intersect_uassoc" | "array_intersect_ukey" | "array_uintersect" => 2,
        "array_uintersect_uassoc" => 3,
        _ => required,
    };
    let variadic = metadata
        .params
        .last()
        .is_some_and(|parameter| parameter.variadic);
    argument_count >= required && (variadic || argument_count <= metadata.params.len())
}

fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn hash_field(hash: u64, bytes: &[u8]) -> u64 {
    let hash = hash_bytes(hash, &(bytes.len() as u64).to_le_bytes());
    hash_bytes(hash, bytes)
}

impl From<IrUnit> for CompiledUnit {
    fn from(unit: IrUnit) -> Self {
        Self::new(unit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use php_ir::ConstId;
    use php_ir::ids::{ClassId, FileId, UnitId};
    use php_ir::module::{ClassFlags, FileEntry, FunctionEntry, GlobalConstantEntry};

    fn class_entry(id: u32, name: &str, is_conditional: bool) -> ClassEntry {
        ClassEntry {
            id: ClassId::new(id),
            name: name.to_owned(),
            display_name: name.to_owned(),
            parent: None,
            parent_display_name: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor: None,
            flags: ClassFlags {
                is_conditional,
                ..ClassFlags::default()
            },
            span: IrSpan::default(),
        }
    }

    #[test]
    fn class_graph_validation_is_prepared_once_per_compiled_unit() {
        let compiled = CompiledUnit::new(IrUnit::new(UnitId::new(77)));

        assert_eq!(compiled.prepared_unit_stats().class_validation_runs, 0);
        assert_eq!(
            compiled.prepared_class_validation(),
            &PreparedClassValidation::Valid
        );
        assert_eq!(compiled.prepared_unit_stats().class_validation_runs, 1);
        assert_eq!(
            compiled.prepared_class_validation(),
            &PreparedClassValidation::Valid
        );
        assert_eq!(compiled.prepared_unit_stats().class_validation_runs, 1);
    }

    #[test]
    fn deployment_symbol_image_is_prepared_once_and_preserves_first_function() {
        let mut unit = IrUnit::new(UnitId::new(78));
        unit.function_table.push(FunctionEntry {
            name: "App\\Boot".to_owned(),
            function: FunctionId::new(1),
        });
        unit.function_table.push(FunctionEntry {
            name: "app\\boot".to_owned(),
            function: FunctionId::new(2),
        });
        let compiled = CompiledUnit::new(unit);

        let first = compiled.prepared_deployment_image();
        let functions = Arc::clone(&first.function_exports);
        let classes = Arc::clone(&first.exported_classes);
        let second = compiled.prepared_deployment_image();

        assert!(std::ptr::eq(first, second));
        assert!(Arc::ptr_eq(&functions, &second.function_exports));
        assert!(Arc::ptr_eq(&classes, &second.exported_classes));
        assert_eq!(
            second.function_exports.get("app\\boot"),
            Some(&FunctionId::new(1))
        );
    }

    #[test]
    fn continuation_instruction_index_is_shared_per_compiled_unit() {
        let compiled = CompiledUnit::new(IrUnit::new(UnitId::new(0)));

        let first = compiled.prepared_continuation_instructions();
        let second = compiled.prepared_continuation_instructions();
        let first_ir_fingerprint = compiled.prepared_ir_fingerprint();
        let second_ir_fingerprint = compiled.prepared_ir_fingerprint();
        let first_dependency_identity = compiled.prepared_dependency_identity();
        let second_dependency_identity = compiled.prepared_dependency_identity();

        assert!(Arc::ptr_eq(&first, &second));
        assert!(std::ptr::eq(first_ir_fingerprint, second_ir_fingerprint));
        assert_eq!(
            first_ir_fingerprint,
            format!(
                "php-compiled-artifact-v1-{:016x}",
                compiled.artifact_identity()
            )
        );
        assert!(std::ptr::eq(
            first_dependency_identity,
            second_dependency_identity
        ));
        assert_eq!(compiled.prepared_unit_stats().continuation_index_runs, 1);
        assert_eq!(compiled.prepared_unit_stats().ir_fingerprint_runs, 1);
        assert_eq!(compiled.prepared_unit_stats().dependency_identity_runs, 1);
    }

    #[test]
    fn source_display_lines_survive_source_replacement_and_deletion() {
        let root = std::env::temp_dir().join(format!(
            "phrust-compiled-unit-lines-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("temp line-cache root should be created");
        let first_path = root.join("fixture.php");
        let second_path = root.join("dependency.php");
        std::fs::write(&first_path, "<?php\nline2\nline3\n")
            .expect("fixture source should be written");
        std::fs::write(&second_path, "one\ntwo\nthree\n")
            .expect("dependency source should be written");

        let mut unit = IrUnit::new(UnitId::new(0));
        unit.files.push(FileEntry {
            id: FileId::new(0),
            path: first_path.to_string_lossy().into_owned(),
        });
        unit.files.push(FileEntry {
            id: FileId::new(1),
            path: second_path.to_string_lossy().into_owned(),
        });
        let compiled = CompiledUnit::new(unit);

        std::fs::write(&first_path, "replaced without original line structure")
            .expect("fixture source should be replaceable");
        std::fs::remove_file(&second_path).expect("dependency source should be removable");

        assert_eq!(
            compiled.source_display_line(IrSpan::new(FileId::new(0), 0, 0), false),
            Some(1)
        );
        assert_eq!(
            compiled.source_display_line(IrSpan::new(FileId::new(0), 5, 5), false),
            Some(1)
        );
        assert_eq!(
            compiled.source_display_line(IrSpan::new(FileId::new(0), 6, 6), false),
            Some(2)
        );

        assert_eq!(
            compiled.source_display_line(IrSpan::new(FileId::new(0), 12, 12), false),
            Some(3)
        );
        assert_eq!(
            compiled.source_display_line(IrSpan::new(FileId::new(1), 8, 8), false),
            Some(3)
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn symbol_lookups_use_maps_and_preserve_first_duplicate() {
        php_runtime::experimental::layout_stats::reset_layout_stats();

        let mut unit = IrUnit::new(UnitId::new(0));
        unit.constants.push(IrConstant::Int(10));
        unit.constants.push(IrConstant::Int(20));
        unit.function_table.push(FunctionEntry {
            name: "app\\boot".to_owned(),
            function: FunctionId::new(1),
        });
        unit.function_table.push(FunctionEntry {
            name: "app\\boot".to_owned(),
            function: FunctionId::new(2),
        });
        unit.constant_table.push(GlobalConstantEntry {
            name: "APP_CONST".to_owned(),
            value: ConstId::new(0),
            span: IrSpan::default(),
        });
        unit.constant_table.push(GlobalConstantEntry {
            name: "APP_CONST".to_owned(),
            value: ConstId::new(1),
            span: IrSpan::default(),
        });
        unit.classes.push(class_entry(0, "App\\Thing", false));
        unit.classes.push(class_entry(1, "APP\\THING", false));

        let compiled = CompiledUnit::new(unit);
        assert_eq!(
            compiled.lookup_function("app\\boot"),
            Some(FunctionId::new(1))
        );
        assert_eq!(
            compiled.lookup_constant("APP_CONST"),
            Some(&IrConstant::Int(10))
        );
        assert_eq!(
            compiled.lookup_class("\\app\\thing").map(|class| class.id),
            Some(ClassId::new(0))
        );
        assert!(compiled.lookup_function("missing").is_none());
        assert!(compiled.lookup_constant("MISSING").is_none());
        assert!(compiled.lookup_class("Missing").is_none());

        let stats = php_runtime::experimental::layout_stats::take_layout_stats();
        assert_eq!(stats.symbol_map_lookups, 6, "{stats:?}");
        assert_eq!(stats.symbol_linear_fallbacks, 0, "{stats:?}");
    }

    #[test]
    fn unit_class_lookup_keeps_conditional_entries_separate() {
        let mut unit = IrUnit::new(UnitId::new(0));
        unit.classes.push(class_entry(0, "AlwaysVisible", true));
        unit.classes.push(class_entry(1, "Declared", false));

        let compiled = CompiledUnit::new(unit);
        assert!(compiled.lookup_class("AlwaysVisible").is_none());
        assert_eq!(
            compiled
                .lookup_unit_class("alwaysvisible")
                .map(|class| class.id),
            Some(ClassId::new(0))
        );
        assert_eq!(
            compiled.lookup_class("\\declared").map(|class| class.id),
            Some(ClassId::new(1))
        );
    }

    #[test]
    fn class_handles_are_canonical_within_one_unit_and_not_across_replacements() {
        let mut unit = IrUnit::new(UnitId::new(0));
        unit.classes.push(class_entry(7, "App\\Canonical", false));

        let compiled = CompiledUnit::new(unit.clone());
        let cloned_handle = compiled.clone();
        let first = compiled
            .lookup_class_handle("app\\canonical")
            .expect("canonical class should resolve");
        let second = cloned_handle
            .lookup_class_handle("\\APP\\CANONICAL")
            .expect("equivalent class spelling should resolve");

        assert_eq!(compiled.cache_identity(), cloned_handle.cache_identity());
        assert!(compiled.ptr_eq(&cloned_handle));
        assert!(first.ptr_eq(&second));
        assert_eq!(first.id, ClassId::new(7));

        let replacement = CompiledUnit::new(unit);
        let replacement_class = replacement
            .lookup_class_handle("App\\Canonical")
            .expect("replacement class should resolve");
        assert_ne!(compiled.cache_identity(), replacement.cache_identity());
        assert!(!compiled.ptr_eq(&replacement));
        assert!(!first.ptr_eq(&replacement_class));
        assert_eq!(first.id, replacement_class.id);
    }

    #[test]
    fn unique_extraction_never_hides_a_deep_clone() {
        let unit = IrUnit::new(UnitId::new(9));
        let compiled = CompiledUnit::new(unit.clone());
        let shared = compiled.clone();

        let compiled = compiled
            .try_into_unique_unit()
            .expect_err("shared artifact must not be extracted");
        drop(shared);
        assert_eq!(compiled.try_into_unique_unit(), Ok(unit));
    }

    #[test]
    fn artifact_identity_includes_retained_source_contents() {
        let mut unit = IrUnit::new(UnitId::new(4));
        unit.files.push(FileEntry {
            id: FileId::new(0),
            path: "memory.php".to_owned(),
        });
        let first = CompiledUnit::try_with_sources(
            unit.clone(),
            [(FileId::new(0), Arc::<str>::from("<?php echo 1;"))],
        )
        .expect("source ID should be valid");
        let same = CompiledUnit::try_with_sources(
            unit.clone(),
            [(FileId::new(0), Arc::<str>::from("<?php echo 1;"))],
        )
        .expect("source ID should be valid");
        let changed = CompiledUnit::try_with_sources(
            unit,
            [(FileId::new(0), Arc::<str>::from("<?php echo 2;"))],
        )
        .expect("source ID should be valid");

        assert_eq!(first.artifact_identity(), same.artifact_identity());
        assert_ne!(first.cache_identity(), same.cache_identity());
        assert_ne!(first.artifact_identity(), changed.artifact_identity());
        assert_eq!(
            first.layout_stats(),
            CompiledUnitLayoutStats {
                source_files: 1,
                retained_source_files: 1,
                retained_source_bytes: 13,
                canonical_classes: 0,
                duplicated_classes: 0,
                indexed_symbols: 0,
                duplicated_symbol_name_bytes: 0,
            }
        );
        assert_eq!(first.metadata_json(), same.metadata_json());
        assert!(first.metadata_json().contains("phrust.compiled-unit.v1"));
    }

    #[test]
    fn artifact_identity_includes_function_ir_without_retained_source() {
        fn returning_unit(value: i64) -> IrUnit {
            let mut builder = php_ir::IrBuilder::new(UnitId::new(77));
            let file = builder.add_file("synthetic.php");
            let span = IrSpan::new(file, 0, 8);
            let constant = builder.intern_constant(IrConstant::Int(value));
            let function = builder.start_function("main", php_ir::FunctionFlags::default(), span);
            let block = builder.append_block(function);
            let result = builder.alloc_register(function);
            builder.emit_load_const(function, block, result, constant, span);
            builder.terminate_return(
                function,
                block,
                Some(php_ir::Operand::Register(result)),
                span,
            );
            builder.set_entry(function);
            builder.finish()
        }

        let first = CompiledUnit::new(returning_unit(1));
        let same = CompiledUnit::new(returning_unit(1));
        let changed = CompiledUnit::new(returning_unit(2));

        assert_eq!(first.artifact_identity(), same.artifact_identity());
        assert_ne!(first.artifact_identity(), changed.artifact_identity());
        assert_eq!(first.prepared_unit_stats().function_ir_fingerprint_runs, 1);
    }

    #[test]
    fn retained_source_defers_function_fingerprints_until_requested() {
        let mut builder = php_ir::IrBuilder::new(UnitId::new(78));
        let file = builder.add_file("retained.php");
        let span = IrSpan::new(file, 0, 20);
        let constant = builder.intern_constant(IrConstant::Int(1));
        for name in ["entry", "dormant"] {
            let function = builder.start_function(name, php_ir::FunctionFlags::default(), span);
            let block = builder.append_block(function);
            let result = builder.alloc_register(function);
            builder.emit_load_const(function, block, result, constant, span);
            builder.terminate_return(
                function,
                block,
                Some(php_ir::Operand::Register(result)),
                span,
            );
        }
        builder.set_entry(FunctionId::new(0));
        let compiled = CompiledUnit::try_with_sources(
            builder.finish(),
            [(
                FileId::new(0),
                Arc::<str>::from("<?php function dormant() {}"),
            )],
        )
        .expect("retained source should match the unit file");

        assert_eq!(
            compiled.prepared_unit_stats().function_ir_fingerprint_runs,
            0
        );
        let first = compiled
            .prepared_function_ir_fingerprint(FunctionId::new(0))
            .expect("entry fingerprint should exist")
            .to_owned();
        assert_eq!(
            compiled.prepared_unit_stats().function_ir_fingerprint_runs,
            1
        );
        assert_eq!(
            compiled.prepared_function_ir_fingerprint(FunctionId::new(0)),
            Some(first.as_str())
        );
        assert_eq!(
            compiled.prepared_unit_stats().function_ir_fingerprint_runs,
            1
        );
        assert!(
            compiled
                .prepared_function_ir_fingerprint(FunctionId::new(1))
                .is_some()
        );
        assert_eq!(
            compiled.prepared_unit_stats().function_ir_fingerprint_runs,
            2
        );
    }

    #[test]
    fn explicit_source_repository_rejects_unknown_and_duplicate_ids() {
        let mut unit = IrUnit::new(UnitId::new(5));
        unit.files.push(FileEntry {
            id: FileId::new(0),
            path: "entry.php".to_owned(),
        });

        assert_eq!(
            CompiledUnit::try_with_sources(
                unit.clone(),
                [(FileId::new(1), Arc::<str>::from("unknown"))],
            )
            .expect_err("unknown file ID must fail"),
            CompiledUnitBuildError::UnknownSourceFile(FileId::new(1))
        );
        assert_eq!(
            CompiledUnit::try_with_sources(
                unit,
                [
                    (FileId::new(0), Arc::<str>::from("first")),
                    (FileId::new(0), Arc::<str>::from("second")),
                ],
            )
            .expect_err("duplicate file ID must fail"),
            CompiledUnitBuildError::DuplicateSourceFile(FileId::new(0))
        );
    }
}
