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
        if dominator == block {
            return self.reachable.contains(&block);
        }
        let mut current = self.immediate_dominator(block);
        while let Some(block) = current {
            if block == dominator {
                return true;
            }
            current = self.immediate_dominator(block);
        }
        false
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
    let reverse_postorder = reverse_postorder(region, entry);
    let reachable = reverse_postorder.iter().copied().collect::<BTreeSet<_>>();
    let mut rpo_index = vec![usize::MAX; block_count];
    for (index, block) in reverse_postorder.iter().copied().enumerate() {
        rpo_index[block.index()] = index;
    }
    // Cooper-Harvey-Kennedy immediate dominators. The previous implementation
    // materialized one BTreeSet containing every dominator for every block and
    // repeatedly cloned/intersected those sets. Region fragmentation made
    // that O(B^3)-shaped implementation dominate real WordPress compilation.
    // Immediate dominators are the actual input required for dominance
    // frontiers and phi placement, so compute them directly and never build
    // the quadratic dominator matrix.
    let mut immediate_dominators = vec![None; block_count];
    immediate_dominators[entry.index()] = Some(entry);
    loop {
        let mut changed = false;
        for block in reverse_postorder.iter().copied().skip(1) {
            let mut incoming = predecessors[block.index()]
                .iter()
                .copied()
                .filter(|predecessor| immediate_dominators[predecessor.index()].is_some());
            let Some(mut next) = incoming.next() else {
                continue;
            };
            for predecessor in incoming {
                next = intersect_dominators(predecessor, next, &immediate_dominators, &rpo_index);
            }
            if immediate_dominators[block.index()] != Some(next) {
                immediate_dominators[block.index()] = Some(next);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    immediate_dominators[entry.index()] = None;

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
        immediate_dominators,
        dominance_frontiers,
        local_phis,
    }
}

fn intersect_dominators(
    mut left: BlockId,
    mut right: BlockId,
    immediate_dominators: &[Option<BlockId>],
    rpo_index: &[usize],
) -> BlockId {
    while left != right {
        while rpo_index[left.index()] > rpo_index[right.index()] {
            left = immediate_dominators[left.index()]
                .expect("processed predecessor has an immediate dominator");
        }
        while rpo_index[right.index()] > rpo_index[left.index()] {
            right = immediate_dominators[right.index()]
                .expect("processed predecessor has an immediate dominator");
        }
    }
    left
}

fn reverse_postorder(region: &RegionGraph, entry: BlockId) -> Vec<BlockId> {
    let mut visited = vec![false; region.blocks.len()];
    let mut postorder = Vec::new();
    let mut work = vec![(entry, false)];
    while let Some((block, expanded)) = work.pop() {
        if expanded {
            postorder.push(block);
            continue;
        }
        if visited[block.index()] {
            continue;
        }
        visited[block.index()] = true;
        work.push((block, true));
        if let Some(block) = region.blocks.get(block.index()) {
            let mut targets = block.terminator.targets();
            targets.reverse();
            for target in targets {
                if !visited[target.index()] {
                    work.push((target, false));
                }
            }
        }
    }
    postorder.reverse();
    postorder
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
