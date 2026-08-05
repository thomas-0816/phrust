//! VM-facing wrapper around verified IR units.

use php_ir::IrUnit;
use php_ir::constants::IrConstant;
use php_ir::ids::{ConstId, FunctionId};
use php_ir::module::{ClassEntry, ClassMethodEntry, normalize_class_name, normalized_class_name};
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

    /// Returns the source-spelled method name from the class owner's function
    /// table. Runtime-owned metadata falls back to its stable lookup name.
    #[must_use]
    pub fn method_display_name(&self, method: &ClassMethodEntry) -> String {
        match &self.storage {
            CompiledClassStorage::Unit { owner, .. } => owner
                .unit()
                .functions
                .get(method.function.index())
                .and_then(|function| function.name.rsplit_once("::"))
                .map_or_else(|| method.name.clone(), |(_, name)| name.to_owned()),
            CompiledClassStorage::Owned(_) => method.name.clone(),
        }
    }

    /// Resolves a class default's constant against the canonical owning unit.
    /// Runtime-owned classes have no IR constant table and therefore return
    /// `None`, retaining their single baseline continuation.
    #[must_use]
    pub fn constant(&self, id: ConstId) -> Option<&IrConstant> {
        match &self.storage {
            CompiledClassStorage::Unit { owner, .. } => owner.unit().constants.get(id.index()),
            CompiledClassStorage::Owned(_) => None,
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
    native_function_indexes: Box<[OnceLock<Arc<PreparedNativeFunctionIndexes>>]>,
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
    pub function_contract_views: Box<[php_jit::JitNativeFunctionContractView]>,
    _declared_function_contracts: Box<[PreparedNativeFunctionContracts]>,
    /// Dense baseline publication cells indexed by `FunctionId`. Generated
    /// code uses these only for an exact continuation after an optimizing
    /// callee side exit.
    pub generic_function_entries: Box<[std::sync::atomic::AtomicUsize]>,
    /// Dense ordinary-call cells indexed by `FunctionId`. Every published
    /// baseline initializes its cell and an optimizing publication atomically
    /// replaces that target, so generated calls never select a tier.
    pub preferred_function_entries: Box<[std::sync::atomic::AtomicUsize]>,
    /// Exact compiler metadata paired with the currently published preferred
    /// entry of each function. Catch/finally resume IDs are compiler artifact
    /// identities and must never be reconstructed from source IR after code
    /// generation.
    preferred_function_metadata:
        std::sync::RwLock<Box<[Option<Arc<php_jit::JitRegionStateMetadata>>]>>,
    /// Process-stable direct baseline-entry counters indexed by `FunctionId`.
    /// Baseline CLIF updates these counters and the request-completion
    /// coordinator consumes them to select optimizing candidates.
    pub generic_function_entry_counts: Box<[std::sync::atomic::AtomicU64]>,
}

#[derive(Debug)]
pub(crate) struct PreparedNativeParameterContract {
    pub function_name: Arc<str>,
    pub parameter_name: Arc<str>,
    pub position: usize,
    pub type_: php_ir::IrReturnType,
    pub by_ref: bool,
    pub span: IrSpan,
}

#[derive(Debug)]
pub(crate) struct PreparedNativeReturnContract {
    pub function_name: Arc<str>,
    pub type_: php_ir::IrReturnType,
    pub returns_by_ref: bool,
    pub span: IrSpan,
}

#[derive(Debug)]
struct PreparedNativeFunctionContracts {
    return_contract: Option<Box<PreparedNativeReturnContract>>,
    parameter_contracts: Box<[Option<Box<PreparedNativeParameterContract>>]>,
    parameter_contract_pointers: Box<[u64]>,
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
            native_function_indexes: (0..function_count).map(|_| OnceLock::new()).collect(),
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
    linked_function_count: usize,
}

/// A statically named call that may resolve to a function or exact method in
/// another unit.
///
/// Whether the target is currently visible and has by-reference parameters is
/// intentionally resolved at runtime. Only the source-IR scan and name
/// normalization are prepared here because those are immutable. Methods use
/// the collision-free `class::method` spelling already used by function
/// metadata; their link slot still points directly at the declaring unit's
/// published function entry.
#[derive(Debug)]
pub(crate) struct PreparedExternalFunctionCall {
    pub normalized_name: Box<str>,
    pub source_name: Box<str>,
    /// Dense immutable slot in the source unit's linked-function table.
    pub link_index: u32,
}

/// Immutable userland-call metadata shared by every invocation of a function.
#[derive(Debug)]
pub(crate) struct PreparedNativeFunctionMetadata {
    pub name: Arc<str>,
    pub params: Arc<[php_ir::IrParam]>,
    /// Immutable lexical-local names consumed by exact frame-projection
    /// leaves such as compact() and get_defined_vars().
    pub local_names: Arc<[String]>,
    pub span: IrSpan,
    pub trace_function: Arc<str>,
    pub trace_class: Option<Arc<str>>,
    pub trace_call_type: Option<&'static str>,
    pub trace_file: Option<Arc<str>>,
    pub trace_line: i64,
    pub capture_count: usize,
    pub implicit_closure_this: bool,
    pub instance_method: bool,
    pub native_binding_plan: php_jit::JitNativeFunctionBindingPlan,
    _native_parameter_bindings: Box<[php_jit::JitNativeParameterBinding]>,
}

#[derive(Debug)]
struct PreparedNativeFunctionIndexes {
    continuation_instructions: Arc<[Option<Arc<php_ir::Instruction>>]>,
    property_sites: Arc<[Option<PreparedNativePropertySite>]>,
    closure_sites: Arc<[Option<Arc<PreparedNativeClosureSite>>]>,
    global_sites: Arc<[Option<Arc<str>>]>,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedNativePropertySite {
    pub class_index: Option<u32>,
    pub property: Arc<str>,
    pub required_state: u32,
    pub dynamic_stdclass: bool,
    /// The assigned SSA value is already an exact member of the declaration's
    /// type and therefore needs neither coercion nor a runtime type lookup.
    pub direct_typed_assignment: bool,
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
    /// Visible positional arity admitted by the same native entry used for
    /// ordinary compiled closure calls. The hidden prefix is fixed by this
    /// site's receiver/capture descriptors.
    pub fixed_visible_arity: Option<u32>,
    pub first_parameter_by_reference: bool,
    pub returns_int: bool,
    pub returns_string: bool,
    pub returns_releasable_scalar: bool,
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
            let declared_function_contracts = unit
                .functions
                .iter()
                .map(|function| {
                    let is_generator = function.flags.is_generator
                        || function.blocks.iter().any(|block| {
                            block.instructions.iter().any(|instruction| {
                                matches!(
                                    instruction.kind,
                                    php_ir::instruction::InstructionKind::Yield { .. }
                                        | php_ir::instruction::InstructionKind::YieldFrom { .. }
                                )
                            })
                        });
                    let mut return_spans = function
                        .blocks
                        .iter()
                        .filter_map(|block| block.terminator.as_ref())
                        .filter_map(|terminator| {
                            matches!(
                                terminator.kind,
                                php_ir::instruction::TerminatorKind::Return { .. }
                            )
                            .then_some(terminator.span)
                        });
                    let first_return_span = return_spans.next();
                    let return_span = match (first_return_span, return_spans.next()) {
                        (Some(span), None) => span,
                        _ => function.span,
                    };
                    let return_contract = (!is_generator)
                        .then(|| function.return_type.as_ref())
                        .flatten()
                        .map(|type_| {
                            Box::new(PreparedNativeReturnContract {
                                function_name: Arc::from(function.name.as_str()),
                                type_: type_.clone(),
                                returns_by_ref: function.returns_by_ref,
                                span: return_span,
                            })
                        });
                    let parameter_contracts = function
                        .params
                        .iter()
                        .enumerate()
                        .map(|(position, parameter)| {
                            parameter.type_.as_ref().map(|type_| {
                                Box::new(PreparedNativeParameterContract {
                                    function_name: Arc::from(function.name.as_str()),
                                    parameter_name: Arc::from(parameter.name.as_str()),
                                    position,
                                    type_: type_.clone(),
                                    by_ref: parameter.by_ref,
                                    span: function.span,
                                })
                            })
                        })
                        .collect::<Box<[_]>>();
                    let parameter_contract_pointers = parameter_contracts
                        .iter()
                        .map(|contract| {
                            contract
                                .as_deref()
                                .map_or(0, |contract| std::ptr::from_ref(contract) as usize as u64)
                        })
                        .collect();
                    PreparedNativeFunctionContracts {
                        return_contract,
                        parameter_contracts,
                        parameter_contract_pointers,
                    }
                })
                .collect::<Box<[_]>>();
            let function_contract_views = declared_function_contracts
                .iter()
                .enumerate()
                .map(
                    |(index, contracts)| php_jit::JitNativeFunctionContractView {
                        return_contract: contracts
                            .return_contract
                            .as_deref()
                            .map_or(0, |contract| std::ptr::from_ref(contract) as usize as u64),
                        parameter_contracts: contracts.parameter_contract_pointers.as_ptr() as usize
                            as u64,
                        parameter_count: u32::try_from(contracts.parameter_contracts.len())
                            .unwrap_or(u32::MAX),
                        reserved: 0,
                        trace_metadata: self
                            .prepared_native_function_metadata_ptr(php_ir::FunctionId::new(
                                u32::try_from(index).unwrap_or(u32::MAX),
                            ))
                            .map_or(0, |metadata| metadata as usize as u64),
                    },
                )
                .collect();
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
                function_contract_views,
                _declared_function_contracts: declared_function_contracts,
                generic_function_entries: (0..unit.functions.len())
                    .map(|_| std::sync::atomic::AtomicUsize::new(0))
                    .collect(),
                preferred_function_entries: (0..unit.functions.len())
                    .map(|_| std::sync::atomic::AtomicUsize::new(0))
                    .collect(),
                preferred_function_metadata: std::sync::RwLock::new(
                    (0..unit.functions.len()).map(|_| None).collect(),
                ),
                generic_function_entry_counts: (0..unit.functions.len())
                    .map(|_| std::sync::atomic::AtomicU64::new(0))
                    .collect(),
            }
        })
    }

    pub(crate) fn publish_preferred_function_metadata(
        &self,
        function: FunctionId,
        handle: &php_jit::JitFunctionHandle,
    ) {
        let Some(metadata) = handle.region_state_metadata_arc() else {
            return;
        };
        let mut published = self
            .prepared_deployment_image()
            .preferred_function_metadata
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(slot) = published.get_mut(function.index()) {
            *slot = Some(metadata);
        }
    }

    pub(crate) fn preferred_function_metadata(
        &self,
        function: FunctionId,
    ) -> Option<Arc<php_jit::JitRegionStateMetadata>> {
        self.prepared_deployment_image()
            .preferred_function_metadata
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(function.index())
            .cloned()
            .flatten()
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

    fn prepared_native_function_indexes(
        &self,
        function: FunctionId,
    ) -> Option<&Arc<PreparedNativeFunctionIndexes>> {
        let slot = self
            .inner
            .prepared
            .native_function_indexes
            .get(function.index())?;
        Some(slot.get_or_init(|| {
            self.inner
                .prepared
                .continuation_index_runs
                .fetch_add(1, Ordering::Relaxed);
            let mut function_instructions = Vec::new();
            let mut function_property_sites = Vec::new();
            let mut function_closure_sites = Vec::new();
            let mut function_global_sites = Vec::new();
            let metadata = php_jit::region_ir::CompileMetadata::default();
            if let Ok(region) = php_jit::region_ir::GenericRegionBuilder::build(
                &self.inner.unit,
                function,
                &metadata,
            ) {
                let value_flow = php_jit::region_ir::analyze_executable_value_flow(
                    &region,
                    &self.inner.unit.constants,
                );
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
                                php_jit::region_ir::RegionInstructionKind::NativeCall(call) => {
                                    match &call.target {
                                        php_jit::region_ir::RegionCallTarget::Semantic {
                                            operation:
                                                php_jit::region_ir::RegionSemanticOp::PropertyIsset {
                                                    property: php_jit::region_ir::RegionPropertyName::FixedDynamic(property),
                                                    ..
                                                }
                                                | php_jit::region_ir::RegionSemanticOp::PropertyEmpty {
                                                    property: php_jit::region_ir::RegionPropertyName::FixedDynamic(property),
                                                    ..
                                                }
                                                | php_jit::region_ir::RegionSemanticOp::PropertyDimIsset {
                                                    property: php_jit::region_ir::RegionPropertyName::FixedDynamic(property),
                                                    ..
                                                }
                                                | php_jit::region_ir::RegionSemanticOp::PropertyDimEmpty {
                                                    property: php_jit::region_ir::RegionPropertyName::FixedDynamic(property),
                                                    ..
                                                },
                                        } => Some(PreparedNativePropertySite {
                                            class_index: None,
                                            property: Arc::from(property.as_str()),
                                            required_state: php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_PUBLISHED,
                                            dynamic_stdclass: true,
                                            direct_typed_assignment: false,
                                        }),
                                        php_jit::region_ir::RegionCallTarget::Semantic {
                                            operation:
                                                php_jit::region_ir::RegionSemanticOp::PropertyUnset {
                                                    property: php_jit::region_ir::RegionPropertyName::FixedDynamic(property),
                                                    ..
                                                }
                                                | php_jit::region_ir::RegionSemanticOp::PropertyDimAssign {
                                                    property: php_jit::region_ir::RegionPropertyName::FixedDynamic(property),
                                                    ..
                                                }
                                                | php_jit::region_ir::RegionSemanticOp::PropertyDimUnset {
                                                    property: php_jit::region_ir::RegionPropertyName::FixedDynamic(property),
                                                    ..
                                                },
                                        } => Some(PreparedNativePropertySite {
                                            class_index: None,
                                            property: Arc::from(property.as_str()),
                                            required_state: php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_WRITABLE,
                                            dynamic_stdclass: true,
                                            direct_typed_assignment: false,
                                        }),
                                        _ => None,
                                    }
                                }
                                php_jit::region_ir::RegionInstructionKind::FetchProperty {
                                    property,
                                    dynamic_stdclass: true,
                                    ..
                                } => Some(PreparedNativePropertySite {
                                    class_index: None,
                                    property: Arc::from(property.as_str()),
                                    required_state:
                                        php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_PUBLISHED,
                                    dynamic_stdclass: true,
                                    direct_typed_assignment: false,
                                }),
                                php_jit::region_ir::RegionInstructionKind::FetchProperty {
                                    property,
                                    prepared_class: Some(class_index),
                                    ..
                                } => Some(PreparedNativePropertySite {
                                    class_index: Some(*class_index),
                                    property: Arc::from(property.as_str()),
                                    required_state:
                                        php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_PUBLISHED,
                                    dynamic_stdclass: false,
                                    direct_typed_assignment: false,
                                }),
                                php_jit::region_ir::RegionInstructionKind::AssignProperty {
                                    property,
                                    dynamic_stdclass: true,
                                    ..
                                } => Some(PreparedNativePropertySite {
                                    class_index: None,
                                    property: Arc::from(property.as_str()),
                                    required_state:
                                        php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_WRITABLE,
                                    dynamic_stdclass: true,
                                    direct_typed_assignment: false,
                                }),
                                php_jit::region_ir::RegionInstructionKind::AssignProperty {
                                    property,
                                    value,
                                    prepared_class: Some(class_index),
                                    ..
                                } => {
                                    let direct_typed_assignment = self
                                        .inner
                                        .unit
                                        .classes
                                        .get(*class_index as usize)
                                        .and_then(|class| {
                                            class
                                                .properties
                                                .iter()
                                                .rev()
                                                .find(|entry| entry.name == *property)
                                        })
                                        .and_then(|entry| entry.type_.as_ref())
                                        .is_some_and(|type_| {
                                            php_jit::region_ir::generated_fact_satisfies_type(
                                                value_flow.operand_fact(
                                                    &self.inner.unit.constants,
                                                    *value,
                                                ),
                                                type_,
                                            )
                                        });
                                    Some(PreparedNativePropertySite {
                                        class_index: Some(*class_index),
                                        property: Arc::from(property.as_str()),
                                        required_state:
                                            php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_WRITABLE,
                                        dynamic_stdclass: false,
                                        direct_typed_assignment,
                                    })
                                }
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
                                    class_index: Some(*class_index),
                                    property: Arc::from(property.as_str()),
                                    required_state:
                                        php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_REFERENCEABLE,
                                    dynamic_stdclass: false,
                                    direct_typed_assignment: false,
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
                                    class_index: Some(*class_index),
                                    property: Arc::from(property.as_str()),
                                    required_state:
                                        php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_DIMENSION_WRITABLE,
                                    dynamic_stdclass: false,
                                    direct_typed_assignment: false,
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
                                    bound_this_local,
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
                                let fixed_callable_plan =
                                    crate::vm::native_fixed_callable_plan(
                                        self,
                                        *closure_function,
                                        false,
                                    )
                                    .filter(|plan| {
                                        usize::from(bound_this_local.is_some())
                                            .saturating_add(captures.len())
                                            .saturating_add(plan.visible_arity as usize)
                                            <= u8::MAX as usize
                                    });
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
                                        binds_this: bound_this_local.is_some(),
                                        fixed_visible_arity: fixed_callable_plan
                                            .map(|plan| plan.visible_arity),
                                        first_parameter_by_reference: fixed_callable_plan
                                            .is_some_and(|plan| {
                                                plan.first_parameter_by_reference
                                            }),
                                        returns_int: fixed_callable_plan
                                            .is_some_and(|plan| plan.returns_int),
                                        returns_string: fixed_callable_plan
                                            .is_some_and(|plan| plan.returns_string),
                                        returns_releasable_scalar: fixed_callable_plan
                                            .is_some_and(|plan| plan.returns_releasable_scalar),
                                    }));
                            }
                    }
                }
            }
            Arc::new(PreparedNativeFunctionIndexes {
                continuation_instructions: function_instructions.into(),
                property_sites: function_property_sites.into(),
                closure_sites: function_closure_sites.into(),
                global_sites: function_global_sites.into(),
            })
        }))
    }

    pub(crate) fn prepared_continuation_instructions(
        &self,
        function: FunctionId,
    ) -> Option<Arc<[Option<Arc<php_ir::Instruction>>]>> {
        self.prepared_native_function_indexes(function)
            .map(|indexes| Arc::clone(&indexes.continuation_instructions))
    }

    pub(crate) fn prepared_method_specializations(
        &self,
        _function: FunctionId,
    ) -> Vec<php_jit::JitMethodSpecialization> {
        // Call identities no longer vary by receiver-layout feedback. Stable
        // generated entry cells own method dispatch and publication instead.
        Vec::new()
    }

    pub(crate) fn prepared_native_property_sites(
        &self,
        function: FunctionId,
    ) -> Option<Arc<[Option<PreparedNativePropertySite>]>> {
        self.prepared_native_function_indexes(function)
            .map(|indexes| Arc::clone(&indexes.property_sites))
    }

    pub(crate) fn prepared_native_closure_sites(
        &self,
        function: FunctionId,
    ) -> Option<Arc<[Option<Arc<PreparedNativeClosureSite>>]>> {
        self.prepared_native_function_indexes(function)
            .map(|indexes| Arc::clone(&indexes.closure_sites))
    }

    pub(crate) fn prepared_native_global_sites(
        &self,
        function: FunctionId,
    ) -> Option<Arc<[Option<Arc<str>>]>> {
        self.prepared_native_function_indexes(function)
            .map(|indexes| Arc::clone(&indexes.global_sites))
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
            let local_classes = self
                .inner
                .unit
                .classes
                .iter()
                .map(|class| class.name.trim_start_matches('\\').to_ascii_lowercase())
                .collect::<std::collections::HashSet<_>>();
            let external_parent_dependency = |class_name: &str| {
                let mut current = class_name.trim_start_matches('\\').to_ascii_lowercase();
                let mut visited = std::collections::HashSet::new();
                loop {
                    if !visited.insert(current.clone()) {
                        return None;
                    }
                    let class = self.inner.unit.classes.iter().find(|class| {
                        class
                            .name
                            .trim_start_matches('\\')
                            .eq_ignore_ascii_case(&current)
                    })?;
                    let parent = class.parent.as_deref()?.trim_start_matches('\\');
                    let normalized_parent = parent.to_ascii_lowercase();
                    if !local_classes.contains(&normalized_parent) {
                        return Some(parent.to_owned());
                    }
                    current = normalized_parent;
                }
            };
            let mut whole_unit = BTreeMap::<String, String>::new();
            let by_function = self
                .inner
                .unit
                .functions
                .iter()
                .map(|function| {
                    let mut calls = BTreeMap::<String, String>::new();
                    let mut external_object_registers = BTreeMap::<php_ir::RegId, String>::new();
                    let mut external_object_locals = BTreeMap::<php_ir::LocalId, String>::new();
                    for instruction in function.blocks.iter().flat_map(|block| &block.instructions)
                    {
                        match &instruction.kind {
                            php_ir::InstructionKind::CallFunction { name, .. }
                            | php_ir::InstructionKind::BindReferenceFromCall { name, .. } => {
                                let normalized = name.to_ascii_lowercase();
                                if local_functions.contains(&normalized) {
                                    continue;
                                }
                                calls.insert(normalized.clone(), name.clone());
                                whole_unit.insert(normalized, name.clone());
                            }
                            php_ir::InstructionKind::NewObject {
                                dst, class_name, ..
                            } => {
                                let normalized_class =
                                    class_name.trim_start_matches('\\').to_ascii_lowercase();
                                if local_classes.contains(&normalized_class) {
                                    if let Some(parent) = external_parent_dependency(class_name) {
                                        // A local allocation inherits the
                                        // external parent's published layout.
                                        // Model that immutable class-plan
                                        // dependency beside ordinary
                                        // cross-unit constructor links so
                                        // declaration-time publication can
                                        // recompile the caller exactly once.
                                        let source_name = format!("{parent}::__construct");
                                        let normalized = source_name.to_ascii_lowercase();
                                        calls.insert(normalized.clone(), source_name.clone());
                                        whole_unit.insert(normalized, source_name);
                                    }
                                } else {
                                    external_object_registers.insert(
                                        *dst,
                                        class_name.trim_start_matches('\\').to_owned(),
                                    );
                                    let source_name = format!(
                                        "{}::__construct",
                                        class_name.trim_start_matches('\\')
                                    );
                                    let normalized = source_name.to_ascii_lowercase();
                                    calls.insert(normalized.clone(), source_name.clone());
                                    whole_unit.insert(normalized, source_name);
                                }
                            }
                            php_ir::InstructionKind::Move { dst, src }
                            | php_ir::InstructionKind::CloneObject { dst, object: src } => {
                                if let php_ir::Operand::Register(source) = src
                                    && let Some(class) =
                                        external_object_registers.get(source).cloned()
                                {
                                    external_object_registers.insert(*dst, class);
                                }
                            }
                            php_ir::InstructionKind::LoadLocal { dst, local }
                            | php_ir::InstructionKind::LoadLocalQuiet { dst, local } => {
                                if let Some(class) = external_object_locals.get(local).cloned() {
                                    external_object_registers.insert(*dst, class);
                                }
                            }
                            php_ir::InstructionKind::StoreLocal { local, src } => {
                                if let php_ir::Operand::Register(source) = src
                                    && let Some(class) =
                                        external_object_registers.get(source).cloned()
                                {
                                    external_object_locals.insert(*local, class);
                                } else {
                                    external_object_locals.remove(local);
                                }
                            }
                            php_ir::InstructionKind::CallMethod { object, method, .. }
                            | php_ir::InstructionKind::BindReferenceFromMethodCall {
                                object,
                                method,
                                ..
                            } => {
                                let class = match object {
                                    php_ir::Operand::Register(register) => {
                                        external_object_registers.get(register)
                                    }
                                    php_ir::Operand::Local(local) => {
                                        external_object_locals.get(local)
                                    }
                                    _ => None,
                                };
                                let Some(class) = class else {
                                    continue;
                                };
                                let source_name = format!("{class}::{method}");
                                let normalized = source_name.to_ascii_lowercase();
                                calls.insert(normalized.clone(), source_name.clone());
                                whole_unit.insert(normalized.clone(), source_name);
                            }
                            php_ir::InstructionKind::CallStaticMethod {
                                class_name,
                                method,
                                ..
                            } => {
                                let normalized_class =
                                    class_name.trim_start_matches('\\').to_ascii_lowercase();
                                if matches!(normalized_class.as_str(), "self" | "parent" | "static")
                                    || local_classes.contains(&normalized_class)
                                {
                                    continue;
                                }
                                let source_name =
                                    format!("{}::{method}", class_name.trim_start_matches('\\'));
                                let normalized = source_name.to_ascii_lowercase();
                                calls.insert(normalized.clone(), source_name.clone());
                                whole_unit.insert(normalized, source_name);
                            }
                            _ => {}
                        }
                    }
                    calls
                        .into_iter()
                        .map(
                            |(normalized_name, source_name)| PreparedExternalFunctionCall {
                                normalized_name: normalized_name.into_boxed_str(),
                                source_name: source_name.into_boxed_str(),
                                link_index: 0,
                            },
                        )
                        .collect::<Vec<_>>()
                        .into_boxed_slice()
                })
                .collect::<Vec<_>>();
            let mut whole_unit = whole_unit
                .into_iter()
                .map(
                    |(normalized_name, source_name)| PreparedExternalFunctionCall {
                        normalized_name: normalized_name.into_boxed_str(),
                        source_name: source_name.into_boxed_str(),
                        link_index: 0,
                    },
                )
                .collect::<Vec<_>>();
            let link_indexes = whole_unit
                .iter_mut()
                .enumerate()
                .filter_map(|(index, call)| {
                    let index = u32::try_from(index).ok()?;
                    call.link_index = index;
                    Some((call.normalized_name.to_string(), index))
                })
                .collect::<BTreeMap<_, _>>();
            let by_function = by_function
                .into_iter()
                .map(|mut calls| {
                    for call in &mut calls {
                        call.link_index = link_indexes
                            .get(call.normalized_name.as_ref())
                            .copied()
                            .expect("per-function external call must have a unit link slot");
                    }
                    calls
                })
                .collect::<Vec<_>>()
                .into_boxed_slice();
            let linked_function_count = whole_unit.len();
            PreparedExternalFunctionCalls {
                by_function,
                whole_unit: whole_unit.into_boxed_slice(),
                linked_function_count,
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

    pub(crate) fn prepared_linked_function_count(&self) -> usize {
        self.prepared_external_function_call_index()
            .linked_function_count
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
                        let native_parameter_bindings = function
                            .params
                            .iter()
                            .map(|parameter| {
                                let mut flags = 0;
                                if parameter.by_ref {
                                    flags |= php_jit::JIT_NATIVE_PARAMETER_BINDING_BY_REFERENCE;
                                }
                                if parameter.variadic {
                                    flags |= php_jit::JIT_NATIVE_PARAMETER_BINDING_VARIADIC;
                                }
                                if parameter.default.is_some() {
                                    flags |= php_jit::JIT_NATIVE_PARAMETER_BINDING_HAS_DEFAULT;
                                }
                                php_jit::JitNativeParameterBinding {
                                    name_bytes: parameter.name.as_ptr() as usize as u64,
                                    name_length: u32::try_from(parameter.name.len())
                                        .unwrap_or(u32::MAX),
                                    flags,
                                    default_constant_index: parameter
                                        .default
                                        .as_ref()
                                        .and_then(|default| {
                                            self.inner
                                                .unit
                                                .constants
                                                .iter()
                                                .position(|constant| constant == default)
                                        })
                                        .and_then(|index| u32::try_from(index).ok())
                                        .unwrap_or(php_jit::JIT_NATIVE_PARAMETER_DEFAULT_NONE),
                                    reserved: 0,
                                }
                            })
                            .collect::<Box<[_]>>();
                        let native_binding_plan = php_jit::JitNativeFunctionBindingPlan {
                            parameters: native_parameter_bindings.as_ptr() as usize as u64,
                            parameter_count: u32::try_from(native_parameter_bindings.len())
                                .unwrap_or(u32::MAX),
                            required_count: u32::try_from(
                                function
                                    .params
                                    .iter()
                                    .take_while(|parameter| {
                                        parameter.default.is_none() && !parameter.variadic
                                    })
                                    .count(),
                            )
                            .unwrap_or(u32::MAX),
                        };
                        Arc::new(PreparedNativeFunctionMetadata {
                            name: Arc::from(function.name.as_str()),
                            params: Arc::from(function.params.clone()),
                            local_names: Arc::from(function.locals.clone()),
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
                            native_binding_plan,
                            _native_parameter_bindings: native_parameter_bindings,
                        })
                    })
                    .collect::<Vec<_>>()
                    .into_boxed_slice()
            })
            .get(function.index())
            .map(Arc::as_ptr)
    }

    pub(crate) fn prepared_native_function_binding_plan_ptr(
        &self,
        function: FunctionId,
    ) -> Option<*const php_jit::JitNativeFunctionBindingPlan> {
        let metadata = self.prepared_native_function_metadata_ptr(function)?;
        // SAFETY: the pointer addresses an Arc-owned immutable metadata record
        // retained by this CompiledUnit's OnceLock for the unit lifetime.
        #[allow(unsafe_code)]
        Some(unsafe { std::ptr::addr_of!((*metadata).native_binding_plan) })
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
        let mut builder = php_ir::IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("continuation-index.php");
        let span = IrSpan::new(file, 0, 1);
        let entry = builder.start_function("main", php_ir::FunctionFlags::default(), span);
        let block = builder.append_block(entry);
        builder.terminate_return(entry, block, None, span);
        builder.set_entry(entry);
        let compiled = CompiledUnit::new(builder.finish());
        let entry = compiled.unit().entry;

        let first = compiled
            .prepared_continuation_instructions(entry)
            .expect("entry continuation index");
        let second = compiled
            .prepared_continuation_instructions(entry)
            .expect("entry continuation index");
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
    fn native_function_binding_plan_is_stable_and_preserves_parameter_order() {
        let mut builder = php_ir::IrBuilder::new(UnitId::new(79));
        let file = builder.add_file("native-binding-plan.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function(
            "native_binding_target",
            php_ir::FunctionFlags::default(),
            span,
        );
        let first = builder.intern_local(function, "first");
        let second = builder.intern_local(function, "second");
        builder.push_param(
            function,
            php_ir::IrParam {
                name: "first".to_owned(),
                local: first,
                required: true,
                default: None,
                type_: None,
                by_ref: true,
                variadic: false,
                attributes: Vec::new(),
            },
        );
        builder.push_param(
            function,
            php_ir::IrParam {
                name: "second".to_owned(),
                local: second,
                required: false,
                default: Some(IrConstant::Int(2)),
                type_: None,
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            },
        );
        let block = builder.append_block(function);
        builder.terminate_return(function, block, None, span);
        let compiled = CompiledUnit::new(builder.finish());

        let first_plan = compiled
            .prepared_native_function_binding_plan_ptr(function)
            .expect("native binding plan");
        let second_plan = compiled
            .prepared_native_function_binding_plan_ptr(function)
            .expect("stable native binding plan");
        assert_eq!(first_plan, second_plan);
        // SAFETY: both pointers address immutable CompiledUnit-owned metadata
        // retained for the unit lifetime.
        #[allow(unsafe_code)]
        let (plan, parameters) = unsafe {
            let plan = &*first_plan;
            let parameters = std::slice::from_raw_parts(
                plan.parameters as usize as *const php_jit::JitNativeParameterBinding,
                plan.parameter_count as usize,
            );
            (plan, parameters)
        };
        assert_eq!(plan.parameter_count, 2);
        assert_eq!(plan.required_count, 1);
        let parameter_name = |parameter: &php_jit::JitNativeParameterBinding| {
            // SAFETY: parameter names borrow immutable strings owned by the
            // same CompiledUnit metadata record.
            #[allow(unsafe_code)]
            unsafe {
                std::slice::from_raw_parts(
                    parameter.name_bytes as usize as *const u8,
                    parameter.name_length as usize,
                )
            }
        };
        assert_eq!(parameter_name(&parameters[0]), b"first");
        assert_eq!(parameter_name(&parameters[1]), b"second");
        assert_ne!(
            parameters[0].flags & php_jit::JIT_NATIVE_PARAMETER_BINDING_BY_REFERENCE,
            0
        );
        assert_ne!(
            parameters[1].flags & php_jit::JIT_NATIVE_PARAMETER_BINDING_HAS_DEFAULT,
            0
        );
        assert_eq!(
            compiled.unit().constants[parameters[1].default_constant_index as usize],
            IrConstant::Int(2)
        );
    }

    #[test]
    fn continuation_indexes_are_prepared_only_for_reached_functions() {
        let mut builder = php_ir::IrBuilder::new(UnitId::new(1));
        let file = builder.add_file("function-on-demand-index.php");
        let span = IrSpan::new(file, 0, 1);
        let entry = builder.start_function("main", php_ir::FunctionFlags::default(), span);
        let entry_block = builder.append_block(entry);
        builder.terminate_return(entry, entry_block, None, span);
        let dormant = builder.start_function("dormant", php_ir::FunctionFlags::default(), span);
        let dormant_block = builder.append_block(dormant);
        builder.terminate_return(dormant, dormant_block, None, span);
        builder.set_entry(entry);
        let compiled = CompiledUnit::new(builder.finish());

        let _entry = compiled
            .prepared_continuation_instructions(entry)
            .expect("entry continuation index");
        assert_eq!(compiled.prepared_unit_stats().continuation_index_runs, 1);

        let _dormant = compiled
            .prepared_continuation_instructions(dormant)
            .expect("dormant continuation index");
        assert_eq!(compiled.prepared_unit_stats().continuation_index_runs, 2);
    }

    #[test]
    fn local_allocation_indexes_its_external_parent_class_plan() {
        let mut builder = php_ir::IrBuilder::new(UnitId::new(2));
        let file = builder.add_file("external-parent-plan.php");
        let span = IrSpan::new(file, 0, 1);
        let entry = builder.start_function("main", php_ir::FunctionFlags::default(), span);
        let block = builder.append_block(entry);
        let object = builder.alloc_register(entry);
        builder.emit(
            entry,
            block,
            php_ir::InstructionKind::NewObject {
                dst: object,
                display_class_name: "LocalChild".to_owned(),
                class_name: "localchild".to_owned(),
                args: Vec::new(),
            },
            span,
        );
        builder.terminate_return(entry, block, Some(php_ir::Operand::Register(object)), span);
        let mut child = class_entry(0, "localchild", false);
        child.display_name = "LocalChild".to_owned();
        child.parent = Some("externalbase".to_owned());
        child.parent_display_name = Some("ExternalBase".to_owned());
        child.span = span;
        builder.push_class(child);
        builder.set_entry(entry);

        let compiled = CompiledUnit::new(builder.finish());
        let calls = compiled.prepared_external_function_calls(entry);
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].normalized_name.as_ref(),
            "externalbase::__construct"
        );
        assert_eq!(calls[0].source_name.as_ref(), "externalbase::__construct");
        assert_eq!(calls[0].link_index, 0);
        assert_eq!(compiled.prepared_unit_external_function_calls().len(), 1);
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
