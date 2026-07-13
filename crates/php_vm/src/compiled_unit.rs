//! VM-facing wrapper around verified IR units.

use php_ir::IrUnit;
use php_ir::constants::IrConstant;
use php_ir::ids::FunctionId;
use php_ir::module::{ClassEntry, normalize_class_name, normalized_class_name};
use php_ir::source_map::IrSpan;
use php_ir::verify::verify_unit;
use php_runtime::api::RuntimeDiagnostic;
use php_source::{BytePos, LineIndex};
use std::{
    collections::HashMap,
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
    class_validation: OnceLock<Result<(), Box<PreparedClassValidationError>>>,
    ir_verification_runs: AtomicU64,
    class_validation_runs: AtomicU64,
    function_facts: Box<[OnceLock<PreparedFunctionFacts>]>,
}

impl PreparedUnit {
    fn new(function_count: usize) -> Self {
        Self {
            ir_verification_errors: OnceLock::new(),
            class_validation: OnceLock::new(),
            ir_verification_runs: AtomicU64::new(0),
            class_validation_runs: AtomicU64::new(0),
            function_facts: (0..function_count).map(|_| OnceLock::new()).collect(),
        }
    }
}

/// Immutable class-validation failure retained with a compiled artifact.
#[derive(Clone, Debug)]
pub(crate) struct PreparedClassValidationError {
    pub(crate) message: String,
    pub(crate) diagnostic: Option<RuntimeDiagnostic>,
}

/// Number of immutable preparation passes performed for a compiled unit.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PreparedUnitStats {
    /// IR verification passes.
    pub ir_verification_runs: u64,
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

/// Function-invariant execution facts shared by all requests for a unit.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct PreparedFunctionFacts {
    pub(crate) observes_argument_vector: bool,
    pub(crate) has_try_or_finally: bool,
    pub(crate) may_hold_destructor_sensitive_value: bool,
    pub(crate) has_inline_blocker: bool,
}

impl CompiledUnit {
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
        let function_count = unit.functions.len();
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
        let artifact_identity = artifact_identity(&unit, &sources);
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
                prepared: PreparedUnit::new(function_count),
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

    /// Returns cached static class validation, computing it at most once.
    pub(crate) fn prepared_class_validation(
        &self,
        prepare: impl FnOnce() -> Result<(), Box<PreparedClassValidationError>>,
    ) -> Result<(), Box<PreparedClassValidationError>> {
        self.inner
            .prepared
            .class_validation
            .get_or_init(|| {
                self.inner
                    .prepared
                    .class_validation_runs
                    .fetch_add(1, Ordering::Relaxed);
                prepare()
            })
            .clone()
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
            class_validation_runs: self
                .inner
                .prepared
                .class_validation_runs
                .load(Ordering::Relaxed),
        }
    }

    /// Returns immutable per-function facts, scanning a function at most once.
    pub(crate) fn prepared_function_facts(
        &self,
        function: FunctionId,
        prepare: impl FnOnce() -> PreparedFunctionFacts,
    ) -> PreparedFunctionFacts {
        let Some(facts) = self.inner.prepared.function_facts.get(function.index()) else {
            return prepare();
        };
        *facts.get_or_init(prepare)
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

    pub(crate) fn class_handle(&self, index: usize) -> Option<CompiledClass> {
        self.inner
            .unit
            .classes
            .get(index)
            .map(|_| CompiledClass::in_unit(self.clone(), index))
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

fn artifact_identity(unit: &IrUnit, sources: &CompiledSourceRepository) -> u64 {
    let mut hash = stable_hash(b"phrust.compiled-unit.v1");
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
    hash
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
