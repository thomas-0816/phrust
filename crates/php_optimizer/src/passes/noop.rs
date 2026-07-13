use super::super::*;

/// No-op pass used until real optimizations land.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoopPass {
    name: &'static str,
    phase: PassPhase,
}

impl NoopPass {
    /// Creates a named no-op pass.
    #[must_use]
    pub const fn new(name: &'static str, phase: PassPhase) -> Self {
        Self { name, phase }
    }
}

impl OptimizerPass for NoopPass {
    fn name(&self) -> &'static str {
        self.name
    }

    fn phase(&self) -> PassPhase {
        self.phase
    }

    fn run(
        &self,
        transaction: &mut PassTransaction<'_>,
        _context: &PassContext,
    ) -> Result<PassReport, PassError> {
        let unit = transaction.unit();
        let mut stats = BTreeMap::new();
        stats.insert("functions", unit.functions.len() as u64);
        stats.insert(
            "blocks",
            unit.functions
                .iter()
                .map(|function| function.blocks.len() as u64)
                .sum(),
        );
        stats.insert(
            "instructions",
            unit.functions
                .iter()
                .flat_map(|function| &function.blocks)
                .map(|block| block.instructions.len() as u64)
                .sum(),
        );
        stats.insert("source_map_entries", unit.source_map.entries().len() as u64);
        stats.insert("transformations_attempted", 0);
        stats.insert("transformations_applied", 0);
        stats.insert("transformations_skipped", 0);

        Ok(PassReport {
            name: self.name,
            phase: self.phase,
            enabled: true,
            changed: false,
            source_spans_preserved: true,
            rolled_back: false,
            scope: PassScopeReport::default(),
            stats,
        })
    }
}
