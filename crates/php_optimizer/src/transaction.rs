use super::*;

/// Copy-on-first-write optimizer transaction owned by the pass pipeline.
///
/// The transaction snapshots only mutated functions and unit tables. It never
/// clones or serializes the complete [`IrUnit`]. Dropping an uncommitted
/// transaction restores every touched scope.
pub struct PassTransaction<'unit> {
    unit: &'unit mut IrUnit,
    function_snapshots: BTreeMap<usize, IrFunction>,
    constants_snapshot: Option<Vec<IrConstant>>,
    classes_snapshot: Option<Vec<php_ir::ClassEntry>>,
    constant_table_snapshot: Option<Vec<php_ir::GlobalConstantEntry>>,
    touched_blocks: BTreeSet<(usize, usize)>,
    snapshot_bytes: u64,
    committed: bool,
}

impl<'unit> PassTransaction<'unit> {
    pub(crate) fn new(unit: &'unit mut IrUnit) -> Self {
        Self {
            unit,
            function_snapshots: BTreeMap::new(),
            constants_snapshot: None,
            classes_snapshot: None,
            constant_table_snapshot: None,
            touched_blocks: BTreeSet::new(),
            snapshot_bytes: 0,
            committed: false,
        }
    }

    /// Returns the current unit for read-only analysis.
    #[must_use]
    pub const fn unit(&self) -> &IrUnit {
        self.unit
    }

    /// Returns one function for mutation, snapshotting that function once.
    pub fn function_mut(&mut self, function: usize) -> &mut IrFunction {
        if !self.function_snapshots.contains_key(&function) {
            let snapshot = self.unit.functions[function].clone();
            self.snapshot_bytes += estimated_function_bytes(&snapshot);
            self.function_snapshots.insert(function, snapshot);
        }
        &mut self.unit.functions[function]
    }

    /// Records a block changed through [`Self::function_mut`].
    pub fn touch_block(&mut self, function: usize, block: usize) {
        self.touched_blocks.insert((function, block));
    }

    /// Returns the constant pool for mutation, snapshotting it once.
    pub fn constants_mut(&mut self) -> &mut Vec<IrConstant> {
        if self.constants_snapshot.is_none() {
            let snapshot = self.unit.constants.clone();
            self.snapshot_bytes += estimated_constants_bytes(&snapshot);
            self.constants_snapshot = Some(snapshot);
        }
        &mut self.unit.constants
    }

    pub(crate) fn classes_mut(&mut self) -> &mut Vec<php_ir::ClassEntry> {
        if self.classes_snapshot.is_none() {
            let snapshot = self.unit.classes.clone();
            self.snapshot_bytes += std::mem::size_of_val(snapshot.as_slice()) as u64;
            self.classes_snapshot = Some(snapshot);
        }
        &mut self.unit.classes
    }

    pub(crate) fn constant_table_mut(&mut self) -> &mut Vec<php_ir::GlobalConstantEntry> {
        if self.constant_table_snapshot.is_none() {
            let snapshot = self.unit.constant_table.clone();
            self.snapshot_bytes += std::mem::size_of_val(snapshot.as_slice()) as u64;
            self.constant_table_snapshot = Some(snapshot);
        }
        &mut self.unit.constant_table
    }

    pub(crate) fn changed(&self) -> bool {
        self.function_snapshots
            .iter()
            .any(|(index, snapshot)| self.unit.functions.get(*index) != Some(snapshot))
            || self
                .constants_snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot != &self.unit.constants)
            || self
                .classes_snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot != &self.unit.classes)
            || self
                .constant_table_snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot != &self.unit.constant_table)
    }

    pub(crate) fn add_instrumentation(&self, report: &mut PassReport) {
        report.scope = PassScopeReport {
            functions: self.function_snapshots.keys().copied().collect(),
            blocks: self.touched_blocks.iter().copied().collect(),
            constants: self.constants_snapshot.is_some(),
            metadata: [
                self.classes_snapshot.is_some().then_some("classes"),
                self.constant_table_snapshot
                    .is_some()
                    .then_some("constant_table"),
            ]
            .into_iter()
            .flatten()
            .collect(),
            source_mappings_may_change: false,
        };
        report
            .stats
            .insert("functions_touched", self.function_snapshots.len() as u64);
        report
            .stats
            .insert("blocks_touched", self.touched_blocks.len() as u64);
        report.stats.insert(
            "constant_pool_touched",
            u64::from(self.constants_snapshot.is_some()),
        );
        report.stats.insert(
            "metadata_tables_touched",
            u64::from(self.classes_snapshot.is_some())
                + u64::from(self.constant_table_snapshot.is_some()),
        );
        report.stats.insert("source_mappings_may_change", 0);
        report.stats.insert(
            "scope_snapshots",
            (self.function_snapshots.len()
                + usize::from(self.constants_snapshot.is_some())
                + usize::from(self.classes_snapshot.is_some())
                + usize::from(self.constant_table_snapshot.is_some())) as u64,
        );
        report.stats.insert("snapshot_bytes", self.snapshot_bytes);
    }

    pub(crate) fn rollback(&mut self) {
        for (index, function) in std::mem::take(&mut self.function_snapshots) {
            self.unit.functions[index] = function;
        }
        if let Some(constants) = self.constants_snapshot.take() {
            self.unit.constants = constants;
        }
        if let Some(classes) = self.classes_snapshot.take() {
            self.unit.classes = classes;
        }
        if let Some(constant_table) = self.constant_table_snapshot.take() {
            self.unit.constant_table = constant_table;
        }
        self.committed = true;
    }

    pub(crate) fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for PassTransaction<'_> {
    fn drop(&mut self) {
        if !self.committed {
            self.rollback();
        }
    }
}

fn estimated_function_bytes(function: &IrFunction) -> u64 {
    let mut bytes = std::mem::size_of::<IrFunction>()
        + function.name.len()
        + function.locals.iter().map(String::len).sum::<usize>();
    bytes += std::mem::size_of_val(function.params.as_slice());
    bytes += std::mem::size_of_val(function.blocks.as_slice());
    bytes += function
        .blocks
        .iter()
        .map(|block| std::mem::size_of_val(block.instructions.as_slice()))
        .sum::<usize>();
    bytes as u64
}

fn estimated_constants_bytes(constants: &[IrConstant]) -> u64 {
    (std::mem::size_of_val(constants)
        + constants
            .iter()
            .map(|constant| match constant {
                IrConstant::String(value) => value.len(),
                _ => 0,
            })
            .sum::<usize>()) as u64
}
