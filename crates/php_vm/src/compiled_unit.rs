//! VM-facing wrapper around verified IR units.

use php_ir::constants::IrConstant;
use php_ir::ids::FunctionId;
use php_ir::module::{ClassEntry, normalize_class_name};
use php_ir::source_map::IrSpan;
use php_ir::{ConstId, IrUnit};
use std::sync::{Arc, Mutex};

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
    unit: IrUnit,
    function_table: Vec<CompiledFunctionEntry>,
    constant_table: Vec<CompiledConstantEntry>,
    class_table: Vec<ClassEntry>,
    source_line_cache: Mutex<Vec<Option<SourceLineIndex>>>,
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
        let constant_table = unit
            .constant_table
            .iter()
            .map(|entry| CompiledConstantEntry {
                name: entry.name.clone(),
                value: entry.value,
            })
            .collect();
        let class_table = unit
            .classes
            .iter()
            .filter(|entry| !entry.flags.is_conditional)
            .cloned()
            .collect();
        Self {
            inner: Arc::new(CompiledUnitInner {
                unit,
                function_table,
                constant_table,
                class_table,
                source_line_cache: Mutex::new(vec![None; file_count]),
            }),
        }
    }

    /// Returns the underlying IR unit.
    #[must_use]
    pub fn unit(&self) -> &IrUnit {
        &self.inner.unit
    }

    /// Finds a user function by normalized name.
    #[must_use]
    pub fn lookup_function(&self, name: &str) -> Option<FunctionId> {
        self.inner
            .function_table
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.function)
    }

    /// Finds a user constant by canonical name.
    #[must_use]
    pub fn lookup_constant(&self, name: &str) -> Option<&IrConstant> {
        let value = self
            .inner
            .constant_table
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.value)?;
        self.inner.unit.constants.get(value.index())
    }

    /// Finds a class by normalized name.
    #[must_use]
    pub fn lookup_class(&self, name: &str) -> Option<&ClassEntry> {
        let normalized = normalize_class_name(name);
        self.inner
            .class_table
            .iter()
            .find(|entry| normalize_class_name(&entry.name) == normalized)
    }

    /// Finds any class entry in the underlying IR unit, including conditional declarations.
    #[must_use]
    pub fn lookup_unit_class(&self, name: &str) -> Option<&ClassEntry> {
        let normalized = normalize_class_name(name);
        self.inner
            .unit
            .classes
            .iter()
            .find(|entry| normalize_class_name(&entry.name) == normalized)
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
    pub fn class_table(&self) -> &[ClassEntry] {
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
    use php_ir::ids::{FileId, UnitId};
    use php_ir::module::FileEntry;

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
}
