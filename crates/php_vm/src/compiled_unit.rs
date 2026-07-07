//! VM-facing wrapper around verified IR units.

use php_ir::constants::IrConstant;
use php_ir::ids::FunctionId;
use php_ir::module::{ClassEntry, normalize_class_name};
use php_ir::source_map::IrSpan;
use php_ir::{ConstId, IrUnit};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

static NEXT_COMPILED_UNIT_CACHE_ID: AtomicU64 = AtomicU64::new(1);

/// VM-facing function lookup entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledFunctionEntry {
    /// Normalized lookup name.
    pub name: String,
    /// Function ID.
    pub function: FunctionId,
}

/// VM-facing constant lookup entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledConstantEntry {
    /// Canonical runtime lookup name.
    pub name: String,
    /// Constant-pool value ID.
    pub value: ConstId,
}

/// Compiled unit handed to the interpreter.
#[derive(Clone)]
pub struct CompiledUnit {
    inner: Arc<CompiledUnitInner>,
}

struct CompiledUnitInner {
    cache_id: u64,
    unit: IrUnit,
    function_table: Vec<CompiledFunctionEntry>,
    constant_table: Vec<CompiledConstantEntry>,
    class_table: Vec<Arc<ClassEntry>>,
    function_lookup: HashMap<String, FunctionId>,
    constant_lookup: HashMap<String, ConstId>,
    class_lookup: HashMap<String, usize>,
    unit_class_lookup: HashMap<String, usize>,
    source_line_cache: Mutex<Vec<Option<SourceLineIndex>>>,
}

/// Dense executable artifact kind cached for one compiled unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum DenseExecutionArtifactMode {
    /// Mixed dense/rich plan used by automatic bytecode execution.
    Mixed,
    /// Strict fully dense plan used by bytecode-only execution.
    Strict,
}

/// Execution options that affect dense bytecode layout and validity.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DenseExecutionArtifactKey {
    /// Dense lowering mode.
    pub mode: DenseExecutionArtifactMode,
    /// Whether dense superinstructions have been selected.
    pub superinstructions: bool,
    /// Whether profiled dense layout has been applied.
    pub profiled_layout: bool,
    /// Profile entries used by profiled layout, kept in stable map order.
    pub layout_profile_entries: Vec<(String, u64)>,
    /// Whether dense jump threading has been applied.
    pub dense_jump_threading: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceLineIndex {
    newline_offsets: Vec<usize>,
    source_len: usize,
}

impl SourceLineIndex {
    fn new(source: &str) -> Self {
        Self {
            newline_offsets: source
                .as_bytes()
                .iter()
                .enumerate()
                .filter_map(|(offset, byte)| (*byte == b'\n').then_some(offset))
                .collect(),
            source_len: source.len(),
        }
    }

    fn line_for_offset(&self, offset: usize) -> i64 {
        let offset = offset.min(self.source_len);
        let zero_based_line = self
            .newline_offsets
            .partition_point(|newline_offset| *newline_offset < offset);
        (zero_based_line + 1) as i64
    }
}

impl CompiledUnit {
    /// Wraps an IR unit for execution.
    #[must_use]
    pub fn new(unit: IrUnit) -> Self {
        let file_count = unit.files.len();
        let function_table = unit
            .function_table
            .iter()
            .map(|entry| CompiledFunctionEntry {
                name: entry.name.clone(),
                function: entry.function,
            })
            .collect();
        let function_lookup = unit.function_table.iter().fold(
            HashMap::with_capacity(unit.function_table.len()),
            |mut lookup, entry| {
                lookup.entry(entry.name.clone()).or_insert(entry.function);
                lookup
            },
        );
        let constant_table = unit
            .constant_table
            .iter()
            .map(|entry| CompiledConstantEntry {
                name: entry.name.clone(),
                value: entry.value,
            })
            .collect();
        let constant_lookup = unit.constant_table.iter().fold(
            HashMap::with_capacity(unit.constant_table.len()),
            |mut lookup, entry| {
                lookup.entry(entry.name.clone()).or_insert(entry.value);
                lookup
            },
        );
        let class_table = unit
            .classes
            .iter()
            .filter(|entry| !entry.flags.is_conditional)
            .map(|entry| Arc::new(entry.clone()))
            .collect::<Vec<_>>();
        let class_lookup = class_table.iter().enumerate().fold(
            HashMap::with_capacity(class_table.len()),
            |mut lookup, (index, entry)| {
                lookup
                    .entry(normalize_class_name(&entry.name))
                    .or_insert(index);
                lookup
            },
        );
        let unit_class_lookup = unit.classes.iter().enumerate().fold(
            HashMap::with_capacity(unit.classes.len()),
            |mut lookup, (index, entry)| {
                lookup
                    .entry(normalize_class_name(&entry.name))
                    .or_insert(index);
                lookup
            },
        );
        Self {
            inner: Arc::new(CompiledUnitInner {
                cache_id: NEXT_COMPILED_UNIT_CACHE_ID.fetch_add(1, Ordering::Relaxed),
                unit,
                function_table,
                constant_table,
                class_table,
                function_lookup,
                constant_lookup,
                class_lookup,
                unit_class_lookup,
                source_line_cache: Mutex::new(vec![None; file_count]),
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

    /// Returns true when two handles point at the same compiled unit allocation.
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Finds a user function by normalized name.
    #[must_use]
    pub fn lookup_function(&self, name: &str) -> Option<FunctionId> {
        php_runtime::layout_stats::record_symbol_map_lookup();
        self.inner.function_lookup.get(name).copied()
    }

    /// Finds a user constant by canonical name.
    #[must_use]
    pub fn lookup_constant(&self, name: &str) -> Option<&IrConstant> {
        php_runtime::layout_stats::record_symbol_map_lookup();
        let value = self.inner.constant_lookup.get(name).copied()?;
        self.inner.unit.constants.get(value.index())
    }

    /// Finds a class by normalized name.
    #[must_use]
    pub fn lookup_class(&self, name: &str) -> Option<&ClassEntry> {
        php_runtime::layout_stats::record_symbol_map_lookup();
        let normalized = normalize_class_name(name);
        let index = self.inner.class_lookup.get(&normalized).copied()?;
        self.inner.class_table.get(index).map(Arc::as_ref)
    }

    /// Finds a class by normalized name, returning a shared handle to the
    /// (potentially large) `ClassEntry` via a cheap `Arc` refcount bump instead
    /// of a deep clone.
    #[must_use]
    pub fn lookup_class_arc(&self, name: &str) -> Option<Arc<ClassEntry>> {
        php_runtime::layout_stats::record_symbol_map_lookup();
        let normalized = normalize_class_name(name);
        let index = self.inner.class_lookup.get(&normalized).copied()?;
        self.inner.class_table.get(index).map(Arc::clone)
    }

    /// Finds any class entry in the underlying IR unit, including conditional declarations.
    #[must_use]
    pub fn lookup_unit_class(&self, name: &str) -> Option<&ClassEntry> {
        php_runtime::layout_stats::record_symbol_map_lookup();
        let normalized = normalize_class_name(name);
        let index = self.inner.unit_class_lookup.get(&normalized).copied()?;
        self.inner.unit.classes.get(index)
    }

    /// Returns the VM lookup table.
    #[must_use]
    pub fn function_table(&self) -> &[CompiledFunctionEntry] {
        &self.inner.function_table
    }

    /// Returns the VM constant lookup table.
    #[must_use]
    pub fn constant_table(&self) -> &[CompiledConstantEntry] {
        &self.inner.constant_table
    }

    /// Returns the VM class lookup table.
    #[must_use]
    pub fn class_table(&self) -> &[Arc<ClassEntry>] {
        &self.inner.class_table
    }

    /// Returns the display line for a source span using a lazy per-file line index.
    #[must_use]
    pub fn source_display_line(&self, span: IrSpan, end: bool) -> Option<i64> {
        let file_index = span.file.index();
        let file = self.inner.unit.files.get(file_index)?;
        let offset = if end { span.end } else { span.start } as usize;

        if let Ok(cache) = self.inner.source_line_cache.lock()
            && let Some(Some(index)) = cache.get(file_index)
        {
            return Some(index.line_for_offset(offset));
        }

        let source = std::fs::read_to_string(&file.path).ok()?;
        let index = SourceLineIndex::new(&source);
        let line = index.line_for_offset(offset);

        if let Ok(mut cache) = self.inner.source_line_cache.lock() {
            if cache.len() < self.inner.unit.files.len() {
                cache.resize_with(self.inner.unit.files.len(), || None);
            }
            if let Some(slot) = cache.get_mut(file_index) {
                *slot = Some(index);
            }
        }

        Some(line)
    }

    /// Consumes the wrapper and returns the IR unit.
    #[must_use]
    pub fn into_unit(self) -> IrUnit {
        Arc::try_unwrap(self.inner)
            .map(|inner| inner.unit)
            .unwrap_or_else(|inner| inner.unit.clone())
    }
}

impl std::fmt::Debug for CompiledUnit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CompiledUnit")
            .field("unit", &self.inner.unit)
            .field("function_table", &self.inner.function_table)
            .field("constant_table", &self.inner.constant_table)
            .field("class_table", &self.inner.class_table)
            .finish_non_exhaustive()
    }
}

impl PartialEq for CompiledUnit {
    fn eq(&self, other: &Self) -> bool {
        self.inner.unit == other.inner.unit
            && self.inner.function_table == other.inner.function_table
            && self.inner.constant_table == other.inner.constant_table
            && self.inner.class_table == other.inner.class_table
    }
}

impl From<IrUnit> for CompiledUnit {
    fn from(unit: IrUnit) -> Self {
        Self::new(unit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn source_display_line_uses_cached_byte_offset_index() {
        let root = std::env::temp_dir().join(format!(
            "phrust-compiled-unit-lines-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("temp line-cache root should be created");
        let path = root.join("fixture.php");
        std::fs::write(&path, "<?php\nline2\nline3\n").expect("fixture source should be written");

        let mut unit = IrUnit::new(UnitId::new(0));
        unit.files.push(FileEntry {
            id: FileId::new(0),
            path: path.to_string_lossy().into_owned(),
        });
        let compiled = CompiledUnit::new(unit);

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

        std::fs::remove_file(&path).expect("fixture source should be removable");

        assert_eq!(
            compiled.source_display_line(IrSpan::new(FileId::new(0), 12, 12), false),
            Some(3)
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn symbol_lookups_use_maps_and_preserve_first_duplicate() {
        php_runtime::layout_stats::reset_layout_stats();

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

        let stats = php_runtime::layout_stats::take_layout_stats();
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
}
