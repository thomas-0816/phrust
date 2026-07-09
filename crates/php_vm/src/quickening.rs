//! Request-local quickening side table for performance adaptive execution.

use std::collections::BTreeMap;

use php_ir::ids::{BlockId, FunctionId, InstrId, UnitId};

use crate::fallback::{DEQUICKEN_AFTER_GUARD_MISSES, FallbackProtocolStats};

const SPECIALIZE_AFTER_EXECUTIONS: u64 = 8;

/// Quickening runtime mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum QuickeningMode {
    /// Do not create or update quickening state.
    #[default]
    Off,
    /// Maintain request-local quickening metadata without changing semantics.
    On,
}

impl QuickeningMode {
    #[must_use]
    pub const fn enabled(self) -> bool {
        matches!(self, Self::On)
    }
}

/// Adaptive state for one instruction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuickeningState {
    Uninitialized,
    Observing,
    Specialized,
    Dequickened,
    Blacklisted,
}

/// Concrete quickening specialization installed for one instruction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuickeningSpecialization {
    AddIntInt,
    SubIntInt,
    MulIntInt,
    ConcatStringString,
    PackedArrayIntKey,
    BoolBranchCondition,
}

/// Result of observing one instruction dispatch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct QuickeningObservation {
    pub specialization: Option<QuickeningSpecialization>,
    pub attempt: bool,
    pub specialized: bool,
    pub guard_hit: bool,
    pub guard_miss: bool,
    pub guard_failure: bool,
    pub fallback_call: bool,
    pub dequickened: bool,
    pub megamorphic: bool,
    pub disabled: bool,
    /// The site's specialization came from persistent feedback, so guard
    /// hits/dequickens here attribute to the seed, not runtime warm-up.
    pub seeded: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum QuickeningKey {
    Ir {
        function: u32,
        block: u32,
        instruction: u32,
    },
    Dense {
        unit: u32,
        function: u32,
        instruction: u32,
    },
}

/// Stable coordinates of one quickening site, exportable across runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum QuickeningSiteKey {
    Ir {
        function: u32,
        block: u32,
        instruction: u32,
    },
    Dense {
        unit: u32,
        function: u32,
        instruction: u32,
    },
}

/// Metadata snapshot of one adaptive site for persistent feedback.
///
/// Snapshots are advisory: a seeded specialization still runs behind the
/// regular runtime guards, so stale feedback can only cause guard misses and
/// dequickening, never a semantic change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuickeningSiteSnapshot {
    pub site: QuickeningSiteKey,
    pub state: QuickeningState,
    pub specialization: Option<QuickeningSpecialization>,
    pub guard_failures: u64,
}

impl From<QuickeningKey> for QuickeningSiteKey {
    fn from(key: QuickeningKey) -> Self {
        match key {
            QuickeningKey::Ir {
                function,
                block,
                instruction,
            } => Self::Ir {
                function,
                block,
                instruction,
            },
            QuickeningKey::Dense {
                unit,
                function,
                instruction,
            } => Self::Dense {
                unit,
                function,
                instruction,
            },
        }
    }
}

impl From<QuickeningSiteKey> for QuickeningKey {
    fn from(key: QuickeningSiteKey) -> Self {
        match key {
            QuickeningSiteKey::Ir {
                function,
                block,
                instruction,
            } => Self::Ir {
                function,
                block,
                instruction,
            },
            QuickeningSiteKey::Dense {
                unit,
                function,
                instruction,
            } => Self::Dense {
                unit,
                function,
                instruction,
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct QuickeningEntry {
    state: QuickeningState,
    executions: u64,
    specialization: Option<QuickeningSpecialization>,
    stats: FallbackProtocolStats,
    /// Mirrors map-entry presence from the previous BTreeMap storage: dense
    /// slots live in pre-grown vectors, and only slots a writer has handed
    /// out (observe or candidate observation) read as present.
    present: bool,
    /// This site's state was installed from persistent feedback rather than
    /// learned at runtime; kept for seeded-vs-dequickened attribution.
    seeded: bool,
}

impl Default for QuickeningEntry {
    fn default() -> Self {
        Self {
            state: QuickeningState::Uninitialized,
            executions: 0,
            specialization: None,
            stats: FallbackProtocolStats::default(),
            present: false,
            seeded: false,
        }
    }
}

impl QuickeningEntry {
    /// Returns true once a writer has handed out this site, matching the
    /// previous BTreeMap storage where any write-path access inserted the
    /// entry (including candidate observations on cold sites).
    fn is_touched(&self) -> bool {
        self.present
    }

    fn observe(&mut self) -> QuickeningObservation {
        self.executions = self.executions.saturating_add(1);
        match self.state {
            QuickeningState::Uninitialized => {
                self.state = QuickeningState::Observing;
                QuickeningObservation {
                    attempt: true,
                    ..QuickeningObservation::default()
                }
            }
            QuickeningState::Observing => {
                if self.executions >= SPECIALIZE_AFTER_EXECUTIONS {
                    self.state = QuickeningState::Specialized;
                    QuickeningObservation {
                        attempt: true,
                        specialized: true,
                        ..QuickeningObservation::default()
                    }
                } else {
                    QuickeningObservation {
                        attempt: true,
                        ..QuickeningObservation::default()
                    }
                }
            }
            QuickeningState::Specialized | QuickeningState::Dequickened => QuickeningObservation {
                attempt: true,
                ..QuickeningObservation::default()
            },
            QuickeningState::Blacklisted => QuickeningObservation::default(),
        }
    }

    fn record_specialized_guard(&mut self, hit: bool) -> QuickeningObservation {
        let specialization = self.specialization;
        let seeded = self.seeded;
        if hit {
            let event = self.stats.record_guard_hit();
            return QuickeningObservation {
                specialization,
                guard_hit: event.guard_hit,
                seeded,
                ..QuickeningObservation::default()
            };
        }

        let fallback = self.stats.record_guard_fallback();
        let mut dequickened = false;
        let mut megamorphic = false;
        if self.state == QuickeningState::Specialized
            && self.stats.guard_failures >= DEQUICKEN_AFTER_GUARD_MISSES
        {
            let event = self.stats.record_dequicken();
            self.state = QuickeningState::Dequickened;
            self.specialization = None;
            dequickened = event.dequickened;
            megamorphic = event.megamorphic;
        }

        QuickeningObservation {
            specialization,
            guard_miss: fallback.guard_miss,
            guard_failure: fallback.guard_failure,
            fallback_call: fallback.fallback_call,
            dequickened,
            megamorphic,
            seeded,
            ..QuickeningObservation::default()
        }
    }

    fn observe_candidate(
        &mut self,
        specialization: QuickeningSpecialization,
    ) -> QuickeningObservation {
        if self.state == QuickeningState::Specialized && self.specialization.is_none() {
            self.specialization = Some(specialization);
            return QuickeningObservation {
                specialization: Some(specialization),
                specialized: true,
                ..QuickeningObservation::default()
            };
        }
        QuickeningObservation::default()
    }
}

/// Flat quickening entries for one dense-lowered function.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct DenseFunctionQuickening {
    unit: u32,
    function: u32,
    entries: Vec<QuickeningEntry>,
}

/// Per-request quickening metadata table.
///
/// Rich-IR sites keep the ordered map; dense sites are O(1) flat vectors
/// indexed by instruction index, with a last-function cache so consecutive
/// instructions in the same function skip the function lookup entirely and
/// an index map so cross-function switches stay O(1).
#[derive(Clone, Debug, Default)]
pub struct QuickeningTable {
    entries: BTreeMap<QuickeningKey, QuickeningEntry>,
    dense_functions: Vec<DenseFunctionQuickening>,
    dense_index: std::collections::HashMap<(u32, u32), usize>,
    dense_last: std::cell::Cell<usize>,
}

/// Equality covers quickening state only; the interior-mutable lookup cache
/// and the derived index are excluded so read-only queries cannot make two
/// logically identical tables compare unequal.
impl PartialEq for QuickeningTable {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries && self.dense_functions == other.dense_functions
    }
}

impl Eq for QuickeningTable {}

impl QuickeningTable {
    /// Observes one instruction dispatch and updates metadata only.
    pub fn observe(
        &mut self,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) -> QuickeningObservation {
        self.observe_key(ir_quickening_key(function, block, instruction))
    }

    /// Observes one dense bytecode instruction dispatch and updates metadata.
    pub fn observe_dense(
        &mut self,
        unit: UnitId,
        function: FunctionId,
        instruction: u32,
    ) -> QuickeningObservation {
        self.observe_key(dense_quickening_key(unit, function, instruction))
    }

    fn observe_key(&mut self, key: QuickeningKey) -> QuickeningObservation {
        self.entry_mut(key).observe()
    }

    fn entry_mut(&mut self, key: QuickeningKey) -> &mut QuickeningEntry {
        let entry = match key {
            QuickeningKey::Ir { .. } => self.entries.entry(key).or_default(),
            QuickeningKey::Dense {
                unit,
                function,
                instruction,
            } => self.dense_entry_mut(unit, function, instruction),
        };
        entry.present = true;
        entry
    }

    fn entry_if_touched(&self, key: QuickeningKey) -> Option<&QuickeningEntry> {
        match key {
            QuickeningKey::Ir { .. } => self.entries.get(&key),
            QuickeningKey::Dense {
                unit,
                function,
                instruction,
            } => self.dense_entry(unit, function, instruction),
        }
    }

    fn entry_if_touched_mut(&mut self, key: QuickeningKey) -> Option<&mut QuickeningEntry> {
        match key {
            QuickeningKey::Ir { .. } => self.entries.get_mut(&key),
            QuickeningKey::Dense {
                unit,
                function,
                instruction,
            } => {
                let index = self.dense_function_index(unit, function)?;
                let entry = self.dense_functions[index]
                    .entries
                    .get_mut(instruction as usize)?;
                entry.is_touched().then_some(entry)
            }
        }
    }

    fn dense_function_index(&self, unit: u32, function: u32) -> Option<usize> {
        let last = self.dense_last.get();
        if let Some(slot) = self.dense_functions.get(last)
            && slot.unit == unit
            && slot.function == function
        {
            return Some(last);
        }
        let index = *self.dense_index.get(&(unit, function))?;
        self.dense_last.set(index);
        Some(index)
    }

    fn dense_entry(&self, unit: u32, function: u32, instruction: u32) -> Option<&QuickeningEntry> {
        let index = self.dense_function_index(unit, function)?;
        let entry = self.dense_functions[index]
            .entries
            .get(instruction as usize)?;
        entry.is_touched().then_some(entry)
    }

    fn dense_entry_mut(
        &mut self,
        unit: u32,
        function: u32,
        instruction: u32,
    ) -> &mut QuickeningEntry {
        let index = match self.dense_function_index(unit, function) {
            Some(index) => index,
            None => {
                self.dense_functions.push(DenseFunctionQuickening {
                    unit,
                    function,
                    entries: Vec::new(),
                });
                let index = self.dense_functions.len() - 1;
                self.dense_index.insert((unit, function), index);
                self.dense_last.set(index);
                index
            }
        };
        let entries = &mut self.dense_functions[index].entries;
        let slot = instruction as usize;
        if entries.len() <= slot {
            entries.resize_with(slot + 1, QuickeningEntry::default);
        }
        &mut entries[slot]
    }

    /// Returns the specialization currently installed for one instruction.
    #[must_use]
    pub fn specialization(
        &self,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) -> Option<QuickeningSpecialization> {
        self.specialization_key(ir_quickening_key(function, block, instruction))
    }

    /// Returns the specialization currently installed for one dense bytecode instruction.
    #[must_use]
    pub fn dense_specialization(
        &self,
        unit: UnitId,
        function: FunctionId,
        instruction: u32,
    ) -> Option<QuickeningSpecialization> {
        self.specialization_key(dense_quickening_key(unit, function, instruction))
    }

    fn specialization_key(&self, key: QuickeningKey) -> Option<QuickeningSpecialization> {
        self.entry_if_touched(key)
            .and_then(|entry| entry.specialization)
    }

    /// Returns the adaptive state for one instruction.
    #[must_use]
    pub fn state(
        &self,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) -> Option<QuickeningState> {
        self.state_key(ir_quickening_key(function, block, instruction))
    }

    /// Returns the adaptive state for one dense bytecode instruction.
    #[must_use]
    pub fn dense_state(
        &self,
        unit: UnitId,
        function: FunctionId,
        instruction: u32,
    ) -> Option<QuickeningState> {
        self.state_key(dense_quickening_key(unit, function, instruction))
    }

    fn state_key(&self, key: QuickeningKey) -> Option<QuickeningState> {
        self.entry_if_touched(key).map(|entry| entry.state)
    }

    /// Applies the shared guard/fallback protocol for an installed
    /// specialization.
    pub fn record_specialized_guard(
        &mut self,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        hit: bool,
    ) -> QuickeningObservation {
        self.record_specialized_guard_key(ir_quickening_key(function, block, instruction), hit)
    }

    /// Applies the guard/fallback protocol for one dense bytecode specialization.
    pub fn record_dense_specialized_guard(
        &mut self,
        unit: UnitId,
        function: FunctionId,
        instruction: u32,
        hit: bool,
    ) -> QuickeningObservation {
        self.record_specialized_guard_key(dense_quickening_key(unit, function, instruction), hit)
    }

    fn record_specialized_guard_key(
        &mut self,
        key: QuickeningKey,
        hit: bool,
    ) -> QuickeningObservation {
        match self.entry_if_touched_mut(key) {
            Some(entry) => entry.record_specialized_guard(hit),
            None => QuickeningObservation::default(),
        }
    }

    /// Installs the ADD_INT_INT specialization after the generic instruction is hot.
    pub fn observe_add_int_int_candidate(
        &mut self,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) -> QuickeningObservation {
        self.observe_candidate(
            ir_quickening_key(function, block, instruction),
            QuickeningSpecialization::AddIntInt,
        )
    }

    /// Installs the SUB_INT_INT specialization after the generic instruction is hot.
    pub fn observe_sub_int_int_candidate(
        &mut self,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) -> QuickeningObservation {
        self.observe_candidate(
            ir_quickening_key(function, block, instruction),
            QuickeningSpecialization::SubIntInt,
        )
    }

    /// Installs the MUL_INT_INT specialization after the generic instruction is hot.
    pub fn observe_mul_int_int_candidate(
        &mut self,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) -> QuickeningObservation {
        self.observe_candidate(
            ir_quickening_key(function, block, instruction),
            QuickeningSpecialization::MulIntInt,
        )
    }

    /// Installs an int/int dense arithmetic specialization after warmup.
    pub fn observe_dense_int_int_candidate(
        &mut self,
        unit: UnitId,
        function: FunctionId,
        instruction: u32,
        specialization: QuickeningSpecialization,
    ) -> QuickeningObservation {
        self.observe_candidate(
            dense_quickening_key(unit, function, instruction),
            specialization,
        )
    }

    /// Installs the CONCAT_STRING_STRING specialization after the instruction is hot.
    pub fn observe_concat_string_string_candidate(
        &mut self,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) -> QuickeningObservation {
        self.observe_candidate(
            ir_quickening_key(function, block, instruction),
            QuickeningSpecialization::ConcatStringString,
        )
    }

    /// Installs the dense CONCAT_STRING_STRING specialization after warmup.
    pub fn observe_dense_concat_string_string_candidate(
        &mut self,
        unit: UnitId,
        function: FunctionId,
        instruction: u32,
    ) -> QuickeningObservation {
        self.observe_candidate(
            dense_quickening_key(unit, function, instruction),
            QuickeningSpecialization::ConcatStringString,
        )
    }

    /// Installs the PACKED_ARRAY_INT_KEY specialization after the instruction is hot.
    pub fn observe_packed_array_int_key_candidate(
        &mut self,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
    ) -> QuickeningObservation {
        self.observe_candidate(
            ir_quickening_key(function, block, instruction),
            QuickeningSpecialization::PackedArrayIntKey,
        )
    }

    /// Installs the dense BOOL_BRANCH_CONDITION specialization after warmup.
    pub fn observe_dense_bool_branch_candidate(
        &mut self,
        unit: UnitId,
        function: FunctionId,
        instruction: u32,
    ) -> QuickeningObservation {
        self.observe_candidate(
            dense_quickening_key(unit, function, instruction),
            QuickeningSpecialization::BoolBranchCondition,
        )
    }

    fn observe_candidate(
        &mut self,
        key: QuickeningKey,
        specialization: QuickeningSpecialization,
    ) -> QuickeningObservation {
        self.entry_mut(key).observe_candidate(specialization)
    }

    /// Exports the sites worth persisting across runs: installed
    /// specializations and blacklisted sites.
    #[must_use]
    pub fn export_persistent_sites(&self) -> Vec<QuickeningSiteSnapshot> {
        let mut sites = Vec::new();
        for (key, entry) in &self.entries {
            if let Some(snapshot) = persistent_site_snapshot(*key, entry) {
                sites.push(snapshot);
            }
        }
        for slot in &self.dense_functions {
            for (instruction, entry) in slot.entries.iter().enumerate() {
                let key = QuickeningKey::Dense {
                    unit: slot.unit,
                    function: slot.function,
                    instruction: instruction as u32,
                };
                if let Some(snapshot) = persistent_site_snapshot(key, entry) {
                    sites.push(snapshot);
                }
            }
        }
        sites
    }

    /// Seeds adaptive state from a prior run's exported sites and returns how
    /// many sites were actually installed (specialized or blacklisted).
    ///
    /// Seeded specializations enter the table already installed but keep the
    /// full guard/fallback protocol, so a wrong seed self-corrects through
    /// dequickening. Blacklisted seeds skip doomed specialization attempts.
    /// Already-touched sites are left as-is, so the returned count reflects the
    /// seeds that took effect (useful for reporting how much warm-up state a
    /// persistent-feedback consumer actually restored).
    pub fn seed_persistent_sites(&mut self, sites: &[QuickeningSiteSnapshot]) -> usize {
        let mut seeded = 0usize;
        for snapshot in sites {
            match snapshot.state {
                QuickeningState::Specialized => {
                    let Some(specialization) = snapshot.specialization else {
                        continue;
                    };
                    let entry = self.entry_mut(snapshot.site.into());
                    if entry.state != QuickeningState::Uninitialized {
                        continue;
                    }
                    entry.state = QuickeningState::Specialized;
                    entry.specialization = Some(specialization);
                    entry.executions = SPECIALIZE_AFTER_EXECUTIONS;
                    entry.seeded = true;
                    seeded += 1;
                }
                QuickeningState::Blacklisted => {
                    let entry = self.entry_mut(snapshot.site.into());
                    if entry.state != QuickeningState::Uninitialized {
                        continue;
                    }
                    entry.state = QuickeningState::Blacklisted;
                    entry.seeded = true;
                    seeded += 1;
                }
                QuickeningState::Uninitialized
                | QuickeningState::Observing
                | QuickeningState::Dequickened => {}
            }
        }
        seeded
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
            + self
                .dense_functions
                .iter()
                .map(|slot| {
                    slot.entries
                        .iter()
                        .filter(|entry| entry.is_touched())
                        .count()
                })
                .sum::<usize>()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn persistent_site_snapshot(
    key: QuickeningKey,
    entry: &QuickeningEntry,
) -> Option<QuickeningSiteSnapshot> {
    if !entry.is_touched() {
        return None;
    }
    let exportable = match entry.state {
        QuickeningState::Specialized => entry.specialization.is_some(),
        QuickeningState::Blacklisted => true,
        QuickeningState::Uninitialized
        | QuickeningState::Observing
        | QuickeningState::Dequickened => false,
    };
    exportable.then(|| QuickeningSiteSnapshot {
        site: key.into(),
        state: entry.state,
        specialization: entry.specialization,
        guard_failures: entry.stats.guard_failures,
    })
}

fn ir_quickening_key(function: FunctionId, block: BlockId, instruction: InstrId) -> QuickeningKey {
    QuickeningKey::Ir {
        function: function.raw(),
        block: block.raw(),
        instruction: instruction.raw(),
    }
}

fn dense_quickening_key(unit: UnitId, function: FunctionId, instruction: u32) -> QuickeningKey {
    QuickeningKey::Dense {
        unit: unit.raw(),
        function: function.raw(),
        instruction,
    }
}

#[cfg(test)]
mod tests {
    use super::{QuickeningSpecialization, QuickeningState, QuickeningTable};
    use php_ir::ids::{BlockId, FunctionId, InstrId, UnitId};

    #[test]
    fn quickening_table_warms_then_marks_metadata_specialized() {
        let mut table = QuickeningTable::default();
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);

        for _ in 0..7 {
            let observation = table.observe(function, block, instruction);
            assert!(observation.attempt);
            assert!(!observation.specialized);
        }

        let last = table.observe(function, block, instruction);
        assert!(last.attempt);
        assert!(last.specialized);
        assert_eq!(table.len(), 1);
        let entry = table.entries.values().next().expect("entry");
        assert_eq!(entry.state, QuickeningState::Specialized);
    }

    #[test]
    fn quickening_table_tracks_distinct_instructions() {
        let mut table = QuickeningTable::default();

        table.observe(FunctionId::new(0), BlockId::new(0), InstrId::new(0));
        table.observe(FunctionId::new(0), BlockId::new(0), InstrId::new(1));

        assert_eq!(table.len(), 2);
    }

    #[test]
    fn quickening_table_installs_add_int_int_after_warmup() {
        let mut table = QuickeningTable::default();
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);

        for _ in 0..8 {
            table.observe(function, block, instruction);
        }

        let observation = table.observe_add_int_int_candidate(function, block, instruction);

        assert!(observation.specialized);
        assert_eq!(
            table.specialization(function, block, instruction),
            Some(QuickeningSpecialization::AddIntInt)
        );
    }

    #[test]
    fn quickening_table_installs_dense_arithmetic_after_warmup() {
        let mut table = QuickeningTable::default();
        let unit = UnitId::new(7);
        let function = FunctionId::new(2);
        let instruction = 11;

        for _ in 0..8 {
            table.observe_dense(unit, function, instruction);
        }

        let observation = table.observe_dense_int_int_candidate(
            unit,
            function,
            instruction,
            QuickeningSpecialization::MulIntInt,
        );

        assert!(observation.specialized);
        assert_eq!(
            table.dense_specialization(unit, function, instruction),
            Some(QuickeningSpecialization::MulIntInt)
        );
        assert_eq!(
            table.dense_state(unit, function, instruction),
            Some(QuickeningState::Specialized)
        );
    }

    #[test]
    fn quickening_table_installs_concat_string_string_after_warmup() {
        let mut table = QuickeningTable::default();
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);

        for _ in 0..8 {
            table.observe(function, block, instruction);
        }

        let observation =
            table.observe_concat_string_string_candidate(function, block, instruction);

        assert!(observation.specialized);
        assert_eq!(
            table.specialization(function, block, instruction),
            Some(QuickeningSpecialization::ConcatStringString)
        );
    }

    #[test]
    fn quickening_table_installs_packed_array_int_key_after_warmup() {
        let mut table = QuickeningTable::default();
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);

        for _ in 0..8 {
            table.observe(function, block, instruction);
        }

        let observation =
            table.observe_packed_array_int_key_candidate(function, block, instruction);

        assert!(observation.specialized);
        assert_eq!(
            table.specialization(function, block, instruction),
            Some(QuickeningSpecialization::PackedArrayIntKey)
        );
    }

    #[test]
    fn quickening_guard_fallback_dequickens_to_megamorphic() {
        let mut table = QuickeningTable::default();
        let function = FunctionId::new(0);
        let block = BlockId::new(0);
        let instruction = InstrId::new(0);

        for _ in 0..8 {
            table.observe(function, block, instruction);
        }
        table.observe_add_int_int_candidate(function, block, instruction);

        let first = table.record_specialized_guard(function, block, instruction, false);
        assert!(first.guard_miss);
        assert!(first.guard_failure);
        assert!(first.fallback_call);
        assert!(!first.dequickened);

        let second = table.record_specialized_guard(function, block, instruction, false);
        assert!(second.guard_miss);
        assert!(second.guard_failure);
        assert!(second.fallback_call);
        assert!(second.dequickened);
        assert!(second.megamorphic);
        assert!(
            !second.seeded,
            "a runtime-learned site never attributes to persistent feedback"
        );
        assert_eq!(
            table.state(function, block, instruction),
            Some(QuickeningState::Dequickened)
        );
        assert_eq!(table.specialization(function, block, instruction), None);
    }

    #[test]
    fn dense_quickening_guard_fallback_dequickens_site() {
        let mut table = QuickeningTable::default();
        let unit = UnitId::new(3);
        let function = FunctionId::new(0);
        let instruction = 4;

        for _ in 0..8 {
            table.observe_dense(unit, function, instruction);
        }
        table.observe_dense_bool_branch_candidate(unit, function, instruction);

        let first = table.record_dense_specialized_guard(unit, function, instruction, false);
        assert!(first.guard_failure);
        assert!(!first.dequickened);

        let second = table.record_dense_specialized_guard(unit, function, instruction, false);
        assert!(second.guard_failure);
        assert!(second.dequickened);
        assert_eq!(
            table.dense_state(unit, function, instruction),
            Some(QuickeningState::Dequickened)
        );
        assert_eq!(
            table.dense_specialization(unit, function, instruction),
            None
        );
    }

    #[test]
    fn export_persistent_sites_covers_installed_and_blacklisted_sites_only() {
        let mut table = QuickeningTable::default();
        let unit = UnitId::new(0);
        let function = FunctionId::new(1);

        // Installed dense specialization: exported.
        for _ in 0..8 {
            table.observe_dense(unit, function, 2);
        }
        table.observe_dense_concat_string_string_candidate(unit, function, 2);
        // Still observing: not exported.
        table.observe_dense(unit, function, 5);
        // Specialized without an installed specialization: not exported.
        for _ in 0..8 {
            table.observe(FunctionId::new(0), BlockId::new(0), InstrId::new(0));
        }

        let sites = table.export_persistent_sites();
        assert_eq!(sites.len(), 1);
        assert_eq!(
            sites[0].site,
            super::QuickeningSiteKey::Dense {
                unit: 0,
                function: 1,
                instruction: 2,
            }
        );
        assert_eq!(sites[0].state, QuickeningState::Specialized);
        assert_eq!(
            sites[0].specialization,
            Some(QuickeningSpecialization::ConcatStringString)
        );
    }

    #[test]
    fn seed_persistent_sites_installs_specialization_with_guard_protocol() {
        let mut warm = QuickeningTable::default();
        let unit = UnitId::new(0);
        let function = FunctionId::new(0);
        for _ in 0..8 {
            warm.observe_dense(unit, function, 3);
        }
        warm.observe_dense_int_int_candidate(
            unit,
            function,
            3,
            QuickeningSpecialization::AddIntInt,
        );
        let exported = warm.export_persistent_sites();

        let mut cold = QuickeningTable::default();
        let seeded = cold.seed_persistent_sites(&exported);
        assert_eq!(seeded, 1, "one specialization was installed");
        assert_eq!(
            cold.dense_specialization(unit, function, 3),
            Some(QuickeningSpecialization::AddIntInt)
        );
        assert_eq!(
            cold.dense_state(unit, function, 3),
            Some(QuickeningState::Specialized)
        );

        // A wrong seed still dequickens through the normal guard protocol,
        // and both observations attribute to the persistent-feedback seed.
        let first = cold.record_dense_specialized_guard(unit, function, 3, false);
        assert!(first.seeded);
        let second = cold.record_dense_specialized_guard(unit, function, 3, false);
        assert!(second.dequickened);
        assert!(
            second.seeded,
            "the dequicken of a seeded site attributes to the seed"
        );
        assert_eq!(cold.dense_specialization(unit, function, 3), None);
    }

    #[test]
    fn seeded_guard_hits_attribute_to_the_persistent_seed() {
        let mut warm = QuickeningTable::default();
        let unit = UnitId::new(0);
        let function = FunctionId::new(0);
        for _ in 0..8 {
            warm.observe_dense(unit, function, 3);
        }
        warm.observe_dense_int_int_candidate(
            unit,
            function,
            3,
            QuickeningSpecialization::AddIntInt,
        );
        let exported = warm.export_persistent_sites();

        let mut cold = QuickeningTable::default();
        assert_eq!(cold.seed_persistent_sites(&exported), 1);
        let hit = cold.record_dense_specialized_guard(unit, function, 3, true);
        assert!(hit.guard_hit);
        assert!(hit.seeded);

        // The same guard hit on a runtime-learned table is not attributed.
        let learned_hit = warm.record_dense_specialized_guard(unit, function, 3, true);
        assert!(learned_hit.guard_hit);
        assert!(!learned_hit.seeded);
    }

    #[test]
    fn seed_persistent_sites_respects_blacklists_and_touched_entries() {
        let mut warm = QuickeningTable::default();
        let function = FunctionId::new(2);
        let block = BlockId::new(0);
        let instruction = InstrId::new(1);
        for _ in 0..8 {
            warm.observe(function, block, instruction);
        }
        warm.observe_add_int_int_candidate(function, block, instruction);
        let mut exported = warm.export_persistent_sites();
        exported.push(super::QuickeningSiteSnapshot {
            site: super::QuickeningSiteKey::Dense {
                unit: 0,
                function: 0,
                instruction: 9,
            },
            state: QuickeningState::Blacklisted,
            specialization: None,
            guard_failures: 4,
        });

        let mut cold = QuickeningTable::default();
        // A site the current run already touched is not overwritten.
        cold.observe(function, block, instruction);
        let seeded = cold.seed_persistent_sites(&exported);
        assert_eq!(
            seeded, 1,
            "only the fresh blacklist installs; the already-touched site is skipped"
        );
        assert_eq!(
            cold.state(function, block, instruction),
            Some(QuickeningState::Observing)
        );
        assert_eq!(
            cold.dense_state(UnitId::new(0), FunctionId::new(0), 9),
            Some(QuickeningState::Blacklisted)
        );
        // Blacklisted seeds keep the site disabled.
        let observation = cold.observe_dense(UnitId::new(0), FunctionId::new(0), 9);
        assert!(!observation.attempt);
    }
}
