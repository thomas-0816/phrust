//! CFG, dominance, and local-phi plan for executable Region IR.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use php_ir::{BlockId, LocalId};

use crate::region_ir::{RegionGraph, RegionInstructionKind};

/// Executable SSA structure consumed by optimizing lowering. Cranelift creates
/// the physical block parameters from this logical local-phi plan when its
/// `Variable`s are sealed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExecutableSsaGraph {
    predecessors: Vec<Vec<BlockId>>,
    reachable: BTreeSet<BlockId>,
    dominators: Vec<BTreeSet<BlockId>>,
    immediate_dominators: Vec<Option<BlockId>>,
    dominance_frontiers: Vec<BTreeSet<BlockId>>,
    local_phis: BTreeMap<BlockId, BTreeSet<LocalId>>,
}

impl ExecutableSsaGraph {
    #[must_use]
    pub fn predecessors(&self, block: BlockId) -> &[BlockId] {
        self.predecessors
            .get(block.index())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    #[must_use]
    pub fn dominates(&self, dominator: BlockId, block: BlockId) -> bool {
        self.dominators
            .get(block.index())
            .is_some_and(|dominators| dominators.contains(&dominator))
    }

    #[must_use]
    pub fn immediate_dominator(&self, block: BlockId) -> Option<BlockId> {
        self.immediate_dominators
            .get(block.index())
            .copied()
            .flatten()
    }

    #[must_use]
    pub fn dominance_frontier(&self, block: BlockId) -> Option<&BTreeSet<BlockId>> {
        self.dominance_frontiers.get(block.index())
    }

    #[must_use]
    pub fn local_phis(&self, block: BlockId) -> Option<&BTreeSet<LocalId>> {
        self.local_phis.get(&block)
    }

    #[must_use]
    pub fn phi_count(&self) -> usize {
        self.local_phis.values().map(BTreeSet::len).sum()
    }

    /// Checks the graph invariants relied on by Cranelift variable sealing.
    pub fn verify(&self, region: &RegionGraph) -> Result<(), String> {
        for block in &region.blocks {
            if !self.reachable.contains(&block.id) {
                continue;
            }
            if block.id.index() != 0 {
                let Some(idom) = self.immediate_dominator(block.id) else {
                    return Err(format!("reachable block {} has no idom", block.id.raw()));
                };
                if !self.dominates(idom, block.id) {
                    return Err(format!(
                        "idom {} does not dominate block {}",
                        idom.raw(),
                        block.id.raw()
                    ));
                }
            }
            if let Some(locals) = self.local_phis(block.id) {
                if self.predecessors(block.id).len() < 2 {
                    return Err(format!(
                        "block {} has local phis without multiple predecessors",
                        block.id.raw()
                    ));
                }
                if let Some(local) = locals
                    .iter()
                    .find(|local| local.index() >= region.local_count as usize)
                {
                    return Err(format!("phi local {} is out of range", local.raw()));
                }
            }
        }
        Ok(())
    }
}

/// Computes executable CFG, dominators, dominance frontiers, and iterated
/// dominance-frontier phi placement for eligible locals.
#[must_use]
pub fn build_executable_ssa(
    region: &RegionGraph,
    eligible_locals: &BTreeSet<LocalId>,
) -> ExecutableSsaGraph {
    let block_count = region.blocks.len();
    if block_count == 0 {
        return ExecutableSsaGraph::default();
    }
    let mut predecessors = vec![Vec::new(); block_count];
    for block in &region.blocks {
        for target in block.terminator.targets() {
            if let Some(incoming) = predecessors.get_mut(target.index()) {
                incoming.push(block.id);
            }
        }
    }
    for incoming in &mut predecessors {
        incoming.sort();
        incoming.dedup();
    }

    let entry = region.blocks[0].id;
    let reachable = reachable_blocks(region, entry);
    let all_reachable = reachable.clone();
    let mut dominators = vec![BTreeSet::new(); block_count];
    for block in &region.blocks {
        if !reachable.contains(&block.id) {
            continue;
        }
        dominators[block.id.index()] = if block.id == entry {
            BTreeSet::from([entry])
        } else {
            all_reachable.clone()
        };
    }
    loop {
        let mut changed = false;
        for block in region.blocks.iter().skip(1) {
            if !reachable.contains(&block.id) {
                continue;
            }
            let mut incoming = predecessors[block.id.index()]
                .iter()
                .filter(|predecessor| reachable.contains(predecessor));
            let mut next = incoming.next().map_or_else(BTreeSet::new, |predecessor| {
                dominators[predecessor.index()].clone()
            });
            for predecessor in incoming {
                next.retain(|candidate| dominators[predecessor.index()].contains(candidate));
            }
            next.insert(block.id);
            if next != dominators[block.id.index()] {
                dominators[block.id.index()] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut immediate_dominators = vec![None; block_count];
    for block in region.blocks.iter().skip(1) {
        if !reachable.contains(&block.id) {
            continue;
        }
        let strict = dominators[block.id.index()]
            .iter()
            .copied()
            .filter(|candidate| *candidate != block.id)
            .collect::<Vec<_>>();
        immediate_dominators[block.id.index()] = strict.iter().copied().find(|candidate| {
            strict
                .iter()
                .all(|other| other == candidate || dominators[candidate.index()].contains(other))
        });
    }

    let mut dominance_frontiers = vec![BTreeSet::new(); block_count];
    for block in &region.blocks {
        if !reachable.contains(&block.id) || predecessors[block.id.index()].len() < 2 {
            continue;
        }
        let stop = immediate_dominators[block.id.index()];
        for predecessor in &predecessors[block.id.index()] {
            let mut runner = Some(*predecessor);
            while runner != stop {
                let Some(current) = runner else { break };
                dominance_frontiers[current.index()].insert(block.id);
                runner = immediate_dominators[current.index()];
            }
        }
    }

    let mut definition_blocks = BTreeMap::<LocalId, BTreeSet<BlockId>>::new();
    for parameter in &region.params {
        if eligible_locals.contains(&parameter.local) {
            definition_blocks
                .entry(parameter.local)
                .or_default()
                .insert(entry);
        }
    }
    for block in &region.blocks {
        for instruction in &block.instructions {
            if let Some(local) = local_definition(&instruction.kind)
                && eligible_locals.contains(&local)
            {
                definition_blocks.entry(local).or_default().insert(block.id);
            }
        }
    }
    let mut local_phis = BTreeMap::<BlockId, BTreeSet<LocalId>>::new();
    for (local, definitions) in definition_blocks {
        let mut work = definitions.iter().copied().collect::<VecDeque<_>>();
        let mut visited = definitions;
        while let Some(definition) = work.pop_front() {
            for frontier in &dominance_frontiers[definition.index()] {
                if local_phis.entry(*frontier).or_default().insert(local)
                    && visited.insert(*frontier)
                {
                    work.push_back(*frontier);
                }
            }
        }
    }

    ExecutableSsaGraph {
        predecessors,
        reachable,
        dominators,
        immediate_dominators,
        dominance_frontiers,
        local_phis,
    }
}

fn reachable_blocks(region: &RegionGraph, entry: BlockId) -> BTreeSet<BlockId> {
    let mut reachable = BTreeSet::new();
    let mut work = vec![entry];
    while let Some(block) = work.pop() {
        if !reachable.insert(block) {
            continue;
        }
        if let Some(block) = region.blocks.get(block.index()) {
            work.extend(block.terminator.targets());
        }
    }
    reachable
}

const fn local_definition(kind: &RegionInstructionKind) -> Option<LocalId> {
    match kind {
        RegionInstructionKind::StoreLocal { local, .. }
        | RegionInstructionKind::AssignLocalResult { local, .. }
        | RegionInstructionKind::UnsetLocal { local } => Some(*local),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use php_ir::{FunctionFlags, InstructionKind, IrBuilder, IrConstant, IrSpan, Operand, UnitId};

    use super::*;

    #[test]
    fn places_local_phi_at_diamond_join_and_verifies_dominance() {
        let mut builder = IrBuilder::new(UnitId::new(4_205));
        let file = builder.add_file("ssa-diamond.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("diamond", FunctionFlags::default(), span);
        let local = builder.intern_local(function, "value");
        let entry = builder.append_block(function);
        let left = builder.append_block(function);
        let right = builder.append_block(function);
        let join = builder.append_block(function);
        let condition = builder.intern_constant(IrConstant::Bool(true));
        let one = builder.intern_constant(IrConstant::Int(1));
        let two = builder.intern_constant(IrConstant::Int(2));
        builder.terminate_jump_if(
            function,
            entry,
            Operand::Constant(condition),
            left,
            right,
            span,
        );
        builder.emit(
            function,
            left,
            InstructionKind::StoreLocal {
                local,
                src: Operand::Constant(one),
            },
            span,
        );
        builder.terminate_jump(function, left, join, span);
        builder.emit(
            function,
            right,
            InstructionKind::StoreLocal {
                local,
                src: Operand::Constant(two),
            },
            span,
        );
        builder.terminate_jump(function, right, join, span);
        let loaded = builder.alloc_register(function);
        builder.emit(
            function,
            join,
            InstructionKind::LoadLocal { dst: loaded, local },
            span,
        );
        builder.terminate_return(function, join, Some(Operand::Register(loaded)), span);
        let unit = builder.finish();
        let region = crate::region_ir::build_baseline_region(&unit, function).expect("region");

        let ssa = build_executable_ssa(&region, &BTreeSet::from([local]));

        assert_eq!(ssa.local_phis(join), Some(&BTreeSet::from([local])));
        assert_eq!(ssa.immediate_dominator(join), Some(entry));
        assert_eq!(ssa.verify(&region), Ok(()));
    }
}
