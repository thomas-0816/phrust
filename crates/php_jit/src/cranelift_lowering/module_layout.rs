//! Function-scoped native compile planning.
//!
//! A production compile group contains exactly one PHP function.  The plan is
//! built before Cranelift lowering so compile breadth and structural cost are
//! explicit and testable instead of being inferred from a module afterwards.

use crate::region_ir::{RegionBlock, RegionGraph, RegionTerminator, baseline_instruction_lowering};
use php_ir::instruction::TerminatorKind;
use php_ir::{BlockId, FunctionId, InstrId};
use std::collections::{BTreeMap, BTreeSet};

pub const BASELINE_FRAGMENT_MAX_PHP_BLOCKS: usize = 64;
/// Persistent schema for deterministic native fragment boundaries and frame
/// traffic. Increment whenever planning can change emitted fragment code.
pub const NATIVE_FRAGMENT_PLAN_SCHEMA_VERSION: u32 = 7;
// These ceilings are intentionally below the backend's final CLIF admission
// limits. Planning must leave enough headroom for helper continuations,
// resume loaders, and frontend SSA edge splitting. The finished CLIF function
// is checked again before `define_function` can enter regalloc2.
pub const BASELINE_FRAGMENT_MAX_IR_INSTRUCTIONS: usize = 400;
pub const BASELINE_SINGLE_BLOCK_MAX_IR_INSTRUCTIONS: usize = 512;
pub const BASELINE_FRAGMENT_MAX_ESTIMATED_CLIF_BLOCKS: usize = 450;
pub const BASELINE_SINGLE_BLOCK_MAX_ESTIMATED_CLIF_BLOCKS: usize = 537;
pub const BASELINE_FRAGMENT_MAX_ESTIMATED_LIVE_SET: usize = 384;
pub const BASELINE_FRAGMENT_MAX_SAFEPOINT_LIVE_SUM: usize = 4_096;
pub const OPTIMIZING_REGION_MAX_PHP_BLOCKS: usize = 256;
pub const OPTIMIZING_REGION_MAX_IR_INSTRUCTIONS: usize = 1_500;
pub const OPTIMIZING_REGION_MAX_VIRTUAL_VALUES: usize = 768;
const MAX_REGION_BLOCK_INSTRUCTIONS_BEFORE_SPLIT: usize = 128;

fn estimated_instruction_clif_blocks(instruction: &crate::region_ir::RegionInstruction) -> usize {
    let manifest = baseline_instruction_lowering(&instruction.source_kind);
    match &instruction.kind {
        // The generic call trampoline allocates argument and call frames,
        // branches on the call result, and releases both frames on the normal
        // and side-exit paths. Argument ownership/reference handling can add
        // another continuation per operand. Counting a call as one generic
        // safepoint underestimates real WordPress registration files by an
        // order of magnitude.
        crate::region_ir::RegionInstructionKind::NativeCall(call) => 12_usize
            .saturating_add(call.operands.len())
            .min(BASELINE_FRAGMENT_MAX_ESTIMATED_CLIF_BLOCKS),
        // Locals and value copies may carry reference, ownership, and
        // lifecycle guards even when the source instruction is not itself a
        // safepoint. Account for those continuation blocks up front so exact
        // preflight refines exceptional shapes instead of routinely
        // rediscovering the baseline lowering contract.
        crate::region_ir::RegionInstructionKind::Move { .. } => {
            3_usize.saturating_add(usize::from(manifest.requires_safepoint))
        }
        crate::region_ir::RegionInstructionKind::LoadLocal { .. } => {
            8_usize.saturating_add(usize::from(manifest.requires_safepoint))
        }
        crate::region_ir::RegionInstructionKind::StoreLocal { .. }
        | crate::region_ir::RegionInstructionKind::AssignLocalResult { .. } => {
            7_usize.saturating_add(usize::from(manifest.requires_safepoint))
        }
        crate::region_ir::RegionInstructionKind::Binary { .. } => {
            3_usize.saturating_add(usize::from(manifest.requires_safepoint))
        }
        crate::region_ir::RegionInstructionKind::Compare { .. } => {
            3_usize.saturating_add(usize::from(manifest.requires_safepoint))
        }
        crate::region_ir::RegionInstructionKind::NativeSuspend(_) => {
            3_usize.saturating_add(usize::from(manifest.requires_safepoint))
        }
        _ => usize::from(manifest.requires_safepoint),
    }
}

fn region_block_instruction_ranges(block: &RegionBlock) -> Vec<(usize, usize)> {
    if block.instructions.is_empty() {
        return vec![(0, 0)];
    }
    if !region_block_requires_split(block) {
        return vec![(0, block.instructions.len())];
    }

    let mut ranges = Vec::new();
    let mut start = 0_usize;
    let mut estimated_blocks = 1_usize;
    for (index, instruction) in block.instructions.iter().enumerate() {
        let instruction_cost = estimated_instruction_clif_blocks(instruction);
        let instruction_count = index.saturating_sub(start);
        if index > start
            && (instruction_count >= MAX_REGION_BLOCK_INSTRUCTIONS_BEFORE_SPLIT
                || estimated_blocks.saturating_add(instruction_cost)
                    > BASELINE_FRAGMENT_MAX_ESTIMATED_CLIF_BLOCKS)
        {
            ranges.push((start, index));
            start = index;
            estimated_blocks = 1;
        }
        estimated_blocks = estimated_blocks.saturating_add(instruction_cost);
    }
    ranges.push((start, block.instructions.len()));
    ranges
}

pub(super) fn split_oversized_region_blocks(mut region: RegionGraph) -> RegionGraph {
    if region
        .blocks
        .iter()
        .all(|block| !region_block_requires_split(block))
    {
        return region;
    }

    let mut next_block = 0_u32;
    let chunk_ids = region
        .blocks
        .iter()
        .map(|block| {
            let chunks = region_block_instruction_ranges(block).len();
            (0..chunks)
                .map(|_| {
                    let id = BlockId::new(next_block);
                    next_block = next_block.saturating_add(1);
                    id
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let first_block = chunk_ids
        .iter()
        .enumerate()
        .map(|(index, chunks)| (BlockId::new(index as u32), chunks[0]))
        .collect::<BTreeMap<_, _>>();
    let mut next_continuation = region
        .blocks
        .iter()
        .flat_map(|block| {
            block
                .instructions
                .iter()
                .map(|instruction| instruction.continuation_id)
                .chain(std::iter::once(block.terminator_continuation_id))
        })
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let mut instruction_owner = BTreeMap::<(BlockId, InstrId), BlockId>::new();
    let mut blocks = Vec::with_capacity(next_block as usize);

    for (old_index, block) in region.blocks.iter().enumerate() {
        let old_id = BlockId::new(old_index as u32);
        let ranges = region_block_instruction_ranges(block);
        for (chunk_index, (start, end)) in ranges.into_iter().enumerate() {
            let id = chunk_ids[old_index][chunk_index];
            let instructions = block.instructions[start..end].to_vec();
            for instruction in &instructions {
                instruction_owner.insert((old_id, instruction.id), id);
            }
            let is_last = chunk_index + 1 == chunk_ids[old_index].len();
            let entry_live_locals = if chunk_index == 0 {
                block.entry_live_locals.clone()
            } else {
                instructions
                    .first()
                    .map(|instruction| instruction.live_locals.clone())
                    .unwrap_or_default()
            };
            let entry_state_locals = if chunk_index == 0 {
                block.entry_state_locals.clone()
            } else {
                entry_live_locals.clone()
            };
            let (source_terminator, terminator, terminator_span, continuation, live, state) =
                if is_last {
                    (
                        remap_source_terminator(&block.source_terminator, &first_block),
                        remap_region_terminator(&block.terminator, &first_block),
                        block.terminator_span,
                        block.terminator_continuation_id,
                        block.terminator_live_locals.clone(),
                        block.terminator_state_locals.clone(),
                    )
                } else {
                    let target = chunk_ids[old_index][chunk_index + 1];
                    let next_live = block.instructions[end].live_locals.clone();
                    let continuation = next_continuation;
                    next_continuation = next_continuation.saturating_add(1);
                    (
                        TerminatorKind::Jump { target },
                        RegionTerminator::Jump { target },
                        instructions
                            .last()
                            .map_or(block.terminator_span, |instruction| instruction.span),
                        continuation,
                        next_live.clone(),
                        next_live,
                    )
                };
            blocks.push(RegionBlock {
                id,
                source_block: old_id,
                entry_live_locals,
                entry_state_locals,
                instructions,
                terminator_span,
                terminator_continuation_id: continuation,
                terminator_live_locals: live,
                terminator_state_locals: state,
                source_terminator,
                terminator,
            });
        }
    }

    for exception in &mut region.exception_regions {
        exception.block = instruction_owner
            .get(&(exception.block, exception.instruction))
            .copied()
            .unwrap_or_else(|| first_block[&exception.block]);
        exception.protected_blocks = exception
            .protected_blocks
            .iter()
            .flat_map(|block| chunk_ids[block.index()].iter().copied())
            .collect();
        exception.catch = exception.catch.map(|block| first_block[&block]);
        exception.finally = exception.finally.map(|block| first_block[&block]);
        exception.after = first_block[&exception.after];
    }
    region.blocks = blocks;
    region
}

fn region_block_requires_split(block: &RegionBlock) -> bool {
    block.instructions.len() > BASELINE_SINGLE_BLOCK_MAX_IR_INSTRUCTIONS
        || estimated_region_block_clif_blocks(block) > BASELINE_FRAGMENT_MAX_ESTIMATED_CLIF_BLOCKS
}

fn estimated_region_block_clif_blocks(block: &RegionBlock) -> usize {
    1_usize.saturating_add(
        block
            .instructions
            .iter()
            .map(estimated_instruction_clif_blocks)
            .sum::<usize>(),
    )
}

fn planning_successors(region: &RegionGraph) -> Vec<BTreeSet<BlockId>> {
    let mut successors = region
        .blocks
        .iter()
        .map(|block| block.terminator.targets().into_iter().collect())
        .collect::<Vec<BTreeSet<_>>>();
    for exception in &region.exception_regions {
        for protected in &exception.protected_blocks {
            if let Some(edges) = successors.get_mut(protected.index()) {
                edges.extend(exception.catch);
                edges.extend(exception.finally);
            }
        }
    }
    successors
}

fn planning_register_live_in(
    region: &RegionGraph,
    successors: &[BTreeSet<BlockId>],
) -> Vec<BTreeSet<php_ir::RegId>> {
    let mut uses = vec![BTreeSet::new(); region.blocks.len()];
    let mut definitions = vec![BTreeSet::new(); region.blocks.len()];
    for block in &region.blocks {
        for instruction in &block.instructions {
            for register in instruction.register_uses() {
                if !definitions[block.id.index()].contains(&register) {
                    uses[block.id.index()].insert(register);
                }
            }
            definitions[block.id.index()].extend(instruction.register_definitions());
        }
        for register in block.terminator.register_uses() {
            if !definitions[block.id.index()].contains(&register) {
                uses[block.id.index()].insert(register);
            }
        }
    }

    let mut live_in = uses.clone();
    let mut live_out = vec![BTreeSet::new(); region.blocks.len()];
    loop {
        let mut changed = false;
        for block in region.blocks.iter().rev() {
            let next_out = successors[block.id.index()]
                .iter()
                .flat_map(|target| live_in[target.index()].iter().copied())
                .collect::<BTreeSet<_>>();
            let mut next_in = uses[block.id.index()].clone();
            next_in.extend(
                next_out
                    .iter()
                    .filter(|register| !definitions[block.id.index()].contains(register))
                    .copied(),
            );
            if next_out != live_out[block.id.index()] || next_in != live_in[block.id.index()] {
                live_out[block.id.index()] = next_out;
                live_in[block.id.index()] = next_in;
                changed = true;
            }
        }
        if !changed {
            return live_in;
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct FragmentPlanningCost {
    blocks: usize,
    instructions: usize,
    clif_blocks: usize,
    maximum_live_set: usize,
    safepoint_live_sum: usize,
}

fn block_planning_cost(block: &RegionBlock) -> FragmentPlanningCost {
    FragmentPlanningCost {
        blocks: 1,
        instructions: block.instructions.len(),
        clif_blocks: estimated_region_block_clif_blocks(block),
        maximum_live_set: block
            .instructions
            .iter()
            .map(|instruction| {
                instruction
                    .live_locals
                    .len()
                    .saturating_add(instruction.register_uses().len())
            })
            .max()
            .unwrap_or(block.entry_live_locals.len()),
        safepoint_live_sum: block
            .instructions
            .iter()
            .filter(|instruction| {
                baseline_instruction_lowering(&instruction.source_kind).requires_safepoint
            })
            .map(|instruction| instruction.live_locals.len())
            .sum(),
    }
}

impl FragmentPlanningCost {
    fn add(&mut self, other: Self) {
        self.blocks = self.blocks.saturating_add(other.blocks);
        self.instructions = self.instructions.saturating_add(other.instructions);
        self.clif_blocks = self.clif_blocks.saturating_add(other.clif_blocks);
        self.maximum_live_set = self.maximum_live_set.max(other.maximum_live_set);
        self.safepoint_live_sum = self
            .safepoint_live_sum
            .saturating_add(other.safepoint_live_sum);
    }

    fn is_within_budget(self) -> bool {
        self.blocks <= BASELINE_FRAGMENT_MAX_PHP_BLOCKS
            && (self.instructions <= BASELINE_FRAGMENT_MAX_IR_INSTRUCTIONS
                || (self.blocks == 1
                    && self.instructions <= BASELINE_SINGLE_BLOCK_MAX_IR_INSTRUCTIONS))
            && (self.clif_blocks.saturating_add(1) <= BASELINE_FRAGMENT_MAX_ESTIMATED_CLIF_BLOCKS
                || (self.blocks == 1
                    && self.clif_blocks.saturating_add(1)
                        <= BASELINE_SINGLE_BLOCK_MAX_ESTIMATED_CLIF_BLOCKS))
            && self.maximum_live_set <= BASELINE_FRAGMENT_MAX_ESTIMATED_LIVE_SET
            && self.safepoint_live_sum <= BASELINE_FRAGMENT_MAX_SAFEPOINT_LIVE_SUM
    }
}

fn fragment_boundary_cost(
    region: &RegionGraph,
    successors: &[BTreeSet<BlockId>],
    live_in: &[BTreeSet<php_ir::RegId>],
    boundary_metadata: &FragmentBoundaryMetadata,
    start: usize,
    end: usize,
) -> usize {
    const FRAGMENT_OVERHEAD: usize = 1_024;
    let mut traffic = 0_usize;
    for targets in &successors[start..end] {
        for target in targets {
            if target.index() < start || target.index() >= end {
                traffic = traffic.saturating_add(
                    live_in[target.index()]
                        .len()
                        .saturating_add(region.blocks[target.index()].entry_state_locals.len()),
                );
            }
        }
    }
    let mut cost = FRAGMENT_OVERHEAD.saturating_add(traffic.saturating_mul(32));
    if end < region.blocks.len() {
        if boundary_metadata.preferred[end] {
            cost = cost.saturating_sub(128);
        }
        if boundary_metadata.cuts_exception_region[end] {
            cost = cost.saturating_add(512);
        }
    }
    cost
}

/// Properties of a cut between adjacent Region blocks. These depend only on
/// the cut position, not on the candidate fragment start. Computing them in
/// `fragment_boundary_cost` made the dynamic-programming planner rescan the
/// complete CFG and every exception region for every candidate range.
struct FragmentBoundaryMetadata {
    preferred: Vec<bool>,
    cuts_exception_region: Vec<bool>,
}

impl FragmentBoundaryMetadata {
    fn new(region: &RegionGraph, successors: &[BTreeSet<BlockId>]) -> Self {
        let count = region.blocks.len();
        let mut preferred = vec![false; count.saturating_add(1)];
        let mut cuts_exception_region = vec![false; count.saturating_add(1)];
        for end in 1..count {
            let before = &region.blocks[end - 1];
            let after = &region.blocks[end];
            let call_boundary = before
                .instructions
                .last()
                .into_iter()
                .chain(after.instructions.first())
                .any(|instruction| {
                    matches!(
                        instruction.kind,
                        crate::region_ir::RegionInstructionKind::NativeCall(_)
                    )
                });
            let loop_header = successors[end..]
                .iter()
                .any(|targets| targets.contains(&after.id));
            preferred[end] = call_boundary || loop_header;
            cuts_exception_region[end] = region.exception_regions.iter().any(|exception| {
                exception.protected_blocks.contains(&before.id)
                    && exception.protected_blocks.contains(&after.id)
            });
        }
        Self {
            preferred,
            cuts_exception_region,
        }
    }
}

fn cost_aware_fragment_blocks(region: &RegionGraph) -> Vec<Vec<BlockId>> {
    let successors = planning_successors(region);
    let live_in = planning_register_live_in(region, &successors);
    let boundary_metadata = FragmentBoundaryMetadata::new(region, &successors);
    let block_costs = region
        .blocks
        .iter()
        .map(block_planning_cost)
        .collect::<Vec<_>>();
    let count = region.blocks.len();
    let mut best = vec![usize::MAX; count + 1];
    let mut parent = vec![0_usize; count + 1];
    best[0] = 0;
    for end in 1..=count {
        let mut range = FragmentPlanningCost::default();
        for start in (0..end).rev() {
            range.add(block_costs[start]);
            if !range.is_within_budget() {
                break;
            }
            let candidate = best[start].saturating_add(fragment_boundary_cost(
                region,
                &successors,
                &live_in,
                &boundary_metadata,
                start,
                end,
            ));
            if candidate < best[end] || (candidate == best[end] && start < parent[end]) {
                best[end] = candidate;
                parent[end] = start;
            }
        }
    }
    let mut groups = Vec::new();
    let mut end = count;
    while end != 0 {
        let start = parent[end];
        groups.push(
            region.blocks[start..end]
                .iter()
                .map(|block| block.id)
                .collect(),
        );
        end = start;
    }
    groups.reverse();
    groups
}

fn remap_source_terminator(
    terminator: &TerminatorKind,
    blocks: &BTreeMap<BlockId, BlockId>,
) -> TerminatorKind {
    match terminator {
        TerminatorKind::Jump { target } => TerminatorKind::Jump {
            target: blocks[target],
        },
        TerminatorKind::JumpIfFalse { condition, target } => TerminatorKind::JumpIfFalse {
            condition: *condition,
            target: blocks[target],
        },
        TerminatorKind::JumpIfTrue { condition, target } => TerminatorKind::JumpIfTrue {
            condition: *condition,
            target: blocks[target],
        },
        TerminatorKind::JumpIf {
            condition,
            if_true,
            if_false,
        } => TerminatorKind::JumpIf {
            condition: *condition,
            if_true: blocks[if_true],
            if_false: blocks[if_false],
        },
        TerminatorKind::Return {
            value,
            by_ref_local,
        } => TerminatorKind::Return {
            value: *value,
            by_ref_local: *by_ref_local,
        },
        TerminatorKind::Exit { value } => TerminatorKind::Exit { value: *value },
    }
}

fn remap_region_terminator(
    terminator: &RegionTerminator,
    blocks: &BTreeMap<BlockId, BlockId>,
) -> RegionTerminator {
    match terminator {
        RegionTerminator::Jump { target } => RegionTerminator::Jump {
            target: blocks[target],
        },
        RegionTerminator::JumpIfFalse {
            condition,
            target,
            fallthrough,
        } => RegionTerminator::JumpIfFalse {
            condition: *condition,
            target: blocks[target],
            fallthrough: blocks[fallthrough],
        },
        RegionTerminator::JumpIfTrue {
            condition,
            target,
            fallthrough,
        } => RegionTerminator::JumpIfTrue {
            condition: *condition,
            target: blocks[target],
            fallthrough: blocks[fallthrough],
        },
        RegionTerminator::JumpIf {
            condition,
            if_true,
            if_false,
        } => RegionTerminator::JumpIf {
            condition: *condition,
            if_true: blocks[if_true],
            if_false: blocks[if_false],
        },
        RegionTerminator::Return { value, finally } => RegionTerminator::Return {
            value: *value,
            finally: finally.map(|block| blocks[&block]),
        },
        RegionTerminator::ReturnReference { local, finally } => RegionTerminator::ReturnReference {
            local: *local,
            finally: finally.map(|block| blocks[&block]),
        },
        RegionTerminator::Exit { value, finally } => RegionTerminator::Exit {
            value: *value,
            finally: finally.map(|block| blocks[&block]),
        },
    }
}

/// One bounded internal native fragment of a single PHP function.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeFragmentPlan {
    pub id: u32,
    pub blocks: Vec<BlockId>,
    pub ir_instructions: usize,
    pub estimated_clif_blocks: usize,
    pub maximum_estimated_live_set: usize,
    pub safepoint_live_set_sum: usize,
}

impl NativeFragmentPlan {
    #[must_use]
    pub fn is_within_budget(&self) -> bool {
        self.blocks.len() <= BASELINE_FRAGMENT_MAX_PHP_BLOCKS
            && (self.ir_instructions <= BASELINE_FRAGMENT_MAX_IR_INSTRUCTIONS
                || (self.blocks.len() == 1
                    && self.ir_instructions <= BASELINE_SINGLE_BLOCK_MAX_IR_INSTRUCTIONS))
            && (self.estimated_clif_blocks <= BASELINE_FRAGMENT_MAX_ESTIMATED_CLIF_BLOCKS
                || (self.blocks.len() == 1
                    && self.estimated_clif_blocks
                        <= BASELINE_SINGLE_BLOCK_MAX_ESTIMATED_CLIF_BLOCKS))
            && self.maximum_estimated_live_set <= BASELINE_FRAGMENT_MAX_ESTIMATED_LIVE_SET
            && self.safepoint_live_set_sum <= BASELINE_FRAGMENT_MAX_SAFEPOINT_LIVE_SUM
    }
}

/// Pre-Cranelift structural estimate for one PHP function compile group.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeCompilePlan {
    /// The only PHP function body admitted to this compile group.
    pub function: FunctionId,
    pub ir_instructions: usize,
    pub php_cfg_blocks: usize,
    pub estimated_clif_blocks: usize,
    pub virtual_values: usize,
    pub maximum_estimated_live_set: usize,
    pub safepoint_count: usize,
    pub safepoint_live_set_sum: usize,
    pub phi_count: usize,
    pub exception_regions: usize,
    pub suspension_points: usize,
    pub call_sites: usize,
    pub estimated_helper_branches: usize,
    pub fragments: Vec<NativeFragmentPlan>,
}

impl NativeCompilePlan {
    /// Cost tokens used by the bounded compiler scheduler. The estimate mixes
    /// total translation work with the largest fragment's peak regalloc shape;
    /// it is deterministic and independent of host timing noise.
    #[must_use]
    pub(crate) fn admission_cost_tokens(&self) -> usize {
        let largest = self
            .fragments
            .iter()
            .map(|fragment| {
                fragment
                    .estimated_clif_blocks
                    .saturating_mul(32)
                    .saturating_add(fragment.ir_instructions.saturating_mul(8))
                    .saturating_add(fragment.maximum_estimated_live_set.saturating_mul(16))
            })
            .max()
            .unwrap_or(1);
        largest
            .saturating_add(self.ir_instructions.saturating_mul(4))
            .saturating_add(self.safepoint_live_set_sum / 2)
            .saturating_add(self.fragments.len().saturating_mul(128))
            .clamp(1, 100_000)
    }

    /// Returns whether whole-region SSA is structurally bounded.
    #[must_use]
    pub fn permits_whole_region_optimization(&self) -> bool {
        self.fragments.len() == 1
            && self.php_cfg_blocks <= OPTIMIZING_REGION_MAX_PHP_BLOCKS
            && self.ir_instructions <= OPTIMIZING_REGION_MAX_IR_INSTRUCTIONS
            && self.virtual_values <= OPTIMIZING_REGION_MAX_VIRTUAL_VALUES
    }

    /// Deterministically refines one fragment after exact pre-regalloc
    /// Refines one fragment into at least `pieces` bounded contiguous groups.
    /// Exact CLIF preflight uses this after observing how far a planner
    /// estimate missed the backend shape. Splitting to the measured ratio in
    /// one pass avoids rebuilding the same oversized CLIF graph once per
    /// bisection level; the caller still preflights every resulting fragment
    /// and rejects any remaining unsplittable offender before regalloc.
    #[must_use]
    pub(crate) fn refine_fragment_into(
        &self,
        region: &RegionGraph,
        fragment_id: u32,
        pieces: usize,
    ) -> Option<Self> {
        let position = self
            .fragments
            .iter()
            .position(|fragment| fragment.id == fragment_id)?;
        let blocks = &self.fragments[position].blocks;
        if blocks.len() < 2 {
            return None;
        }
        let pieces = pieces.clamp(2, blocks.len());
        let successors = planning_successors(region);
        let live_in = planning_register_live_in(region, &successors);
        let boundary_metadata = FragmentBoundaryMetadata::new(region, &successors);
        let mut replacement = vec![blocks.clone()];
        while replacement.len() < pieces {
            let split_position = replacement
                .iter()
                .enumerate()
                .filter(|(_, group)| group.len() > 1)
                .max_by_key(|(index, group)| {
                    let plan = fragment_plan_for_blocks(region, 0, (*group).clone());
                    (
                        plan.estimated_clif_blocks,
                        plan.ir_instructions,
                        group.len(),
                        usize::MAX.saturating_sub(*index),
                    )
                })?
                .0;
            let group = &replacement[split_position];
            let start = group.first()?.index();
            let end = group.last()?.index().checked_add(1)?;
            let cut = (start + 1..end).min_by_key(|cut| {
                let left_cost = fragment_boundary_cost(
                    region,
                    &successors,
                    &live_in,
                    &boundary_metadata,
                    start,
                    *cut,
                );
                let right_cost = fragment_boundary_cost(
                    region,
                    &successors,
                    &live_in,
                    &boundary_metadata,
                    *cut,
                    end,
                );
                let balance = cut.saturating_sub(start).abs_diff(end.saturating_sub(*cut));
                // Exact recovery must make structural progress first. Giving
                // frame traffic priority can repeatedly peel off one cheap
                // block while leaving nearly the complete oversized CLIF
                // graph intact, forcing another full lowering round.
                (balance, left_cost.saturating_add(right_cost), *cut)
            })?;
            let offset = cut.saturating_sub(start);
            let right = replacement[split_position].split_off(offset);
            replacement.insert(split_position + 1, right);
        }

        let mut groups = self
            .fragments
            .iter()
            .map(|fragment| fragment.blocks.clone())
            .collect::<Vec<_>>();
        groups.splice(position..=position, replacement);
        let mut refined = self.clone();
        refined.fragments = groups
            .into_iter()
            .enumerate()
            .map(|(id, blocks)| fragment_plan_for_blocks(region, id, blocks))
            .collect();
        refined
            .fragments
            .iter()
            .all(NativeFragmentPlan::is_within_budget)
            .then_some(refined)
    }

    /// Builds the mandatory plan for one already verified Region graph.
    #[must_use]
    pub fn for_region(region: &RegionGraph) -> Self {
        let instructions = region
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .collect::<Vec<_>>();
        let safepoints = instructions
            .iter()
            .copied()
            .filter(|instruction| {
                baseline_instruction_lowering(&instruction.source_kind).requires_safepoint
            })
            .collect::<Vec<_>>();
        let safepoint_live_set_sum = safepoints
            .iter()
            .map(|instruction| instruction.live_locals.len())
            .sum();
        let maximum_estimated_live_set = instructions
            .iter()
            .map(|instruction| {
                instruction
                    .live_locals
                    .len()
                    .saturating_add(instruction.register_uses().len())
            })
            .max()
            .unwrap_or(0)
            .max(region.params.len());
        // The baseline admission plan only needs a conservative phi-cost
        // estimate. Building full dominator/frontier SSA here made every cold
        // function pay optimizing-tier analysis before fragmentation. Every
        // materialized local at a multi-predecessor block is a safe upper
        // bound for the phi work that a later optimizing compile may perform.
        let mut predecessor_counts = vec![0_usize; region.blocks.len()];
        for block in &region.blocks {
            for target in block.terminator.targets() {
                if let Some(count) = predecessor_counts.get_mut(target.index()) {
                    *count = count.saturating_add(1);
                }
            }
        }
        let phi_count = region
            .blocks
            .iter()
            .filter(|block| predecessor_counts[block.id.index()] > 1)
            .map(|block| block.entry_state_locals.len())
            .sum();
        let suspension_points = instructions
            .iter()
            .filter(|instruction| {
                matches!(
                    instruction.kind,
                    crate::region_ir::RegionInstructionKind::NativeSuspend(_)
                )
            })
            .count();
        let call_sites = instructions
            .iter()
            .filter(|instruction| {
                matches!(
                    instruction.kind,
                    crate::region_ir::RegionInstructionKind::NativeCall(_)
                )
            })
            .count();
        let estimated_helper_branches = safepoints.len();
        let native_transition_points = instructions
            .iter()
            .filter(|instruction| {
                matches!(
                    instruction.kind,
                    crate::region_ir::RegionInstructionKind::Binary { .. }
                )
            })
            .count();
        let handler_resume_points = region
            .exception_regions
            .iter()
            .flat_map(|handler| [handler.catch, handler.finally])
            .flatten()
            .collect::<BTreeSet<_>>()
            .len();
        let osr_entries = region.osr_entries().len();
        let resume_dispatch_points = handler_resume_points
            .saturating_add(suspension_points)
            .saturating_add(native_transition_points)
            .saturating_add(osr_entries);
        // Ordinary instructions and terminators remain in their real PHP CFG
        // blocks. Extra blocks are reserved for fallible-helper continuations,
        // actual native resume entries, and the native entry dispatcher.
        let estimated_clif_blocks = region
            .blocks
            .len()
            .saturating_add(1)
            .saturating_add(native_transition_points)
            .saturating_add(suspension_points)
            .saturating_add(resume_dispatch_points.saturating_mul(2))
            .saturating_add(estimated_helper_branches.saturating_mul(2))
            .saturating_add(4);
        let whole_region_optimizing = region.compile_metadata.tier
            == crate::region_ir::NativeCompilerTier::Optimizing
            && region.blocks.len() <= OPTIMIZING_REGION_MAX_PHP_BLOCKS
            && instructions.len() <= OPTIMIZING_REGION_MAX_IR_INSTRUCTIONS
            && region.register_count as usize <= OPTIMIZING_REGION_MAX_VIRTUAL_VALUES;
        let fragments = if whole_region_optimizing {
            vec![fragment_plan_for_blocks(
                region,
                0,
                region.blocks.iter().map(|block| block.id).collect(),
            )]
        } else {
            cost_aware_fragment_blocks(region)
                .into_iter()
                .enumerate()
                .map(|(id, blocks)| fragment_plan_for_blocks(region, id, blocks))
                .collect()
        };

        Self {
            function: region.function,
            ir_instructions: instructions.len(),
            php_cfg_blocks: region.blocks.len(),
            estimated_clif_blocks,
            virtual_values: region.register_count as usize,
            maximum_estimated_live_set,
            safepoint_count: safepoints.len(),
            safepoint_live_set_sum,
            phi_count,
            exception_regions: region.exception_regions.len(),
            suspension_points,
            call_sites,
            estimated_helper_branches,
            fragments,
        }
    }
}

fn fragment_plan_for_blocks(
    region: &RegionGraph,
    id: usize,
    blocks: Vec<BlockId>,
) -> NativeFragmentPlan {
    let mut cost = FragmentPlanningCost::default();
    for block in &blocks {
        cost.add(block_planning_cost(&region.blocks[block.index()]));
    }
    NativeFragmentPlan {
        id: u32::try_from(id).unwrap_or(u32::MAX),
        blocks,
        ir_instructions: cost.instructions,
        estimated_clif_blocks: cost.clif_blocks.saturating_add(1),
        maximum_estimated_live_set: cost.maximum_live_set,
        safepoint_live_set_sum: cost.safepoint_live_sum,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region_ir::{BaselineRegionBuilder, CompileMetadata, NativeCompilerTier};
    use php_ir::{FunctionFlags, InstructionKind, IrBuilder, IrSpan, UnitId};

    #[test]
    fn compile_plan_contains_exactly_the_requested_function() {
        let mut builder = IrBuilder::new(UnitId::new(1));
        let file = builder.add_file("layout.php");
        let span = IrSpan::new(file, 0, 1);
        for name in ["first", "second", "third"] {
            let function = builder.start_function(name, FunctionFlags::default(), span);
            let block = builder.append_block(function);
            builder.terminate_return(function, block, None, span);
        }
        let unit = builder.finish();
        let region = BaselineRegionBuilder::build(
            &unit,
            FunctionId::new(1),
            &CompileMetadata {
                ir_fingerprint: "plan-test".to_owned(),
                tier: NativeCompilerTier::Baseline,
                helper_abi_hash: 0,
                target_cpu: "test".to_owned(),
                semantic_config_hash: 0,
                dependency_identity: "test".to_owned(),
            },
        )
        .expect("region");
        let plan = NativeCompilePlan::for_region(&region);

        assert_eq!(plan.function, FunctionId::new(1));
        assert_eq!(plan.php_cfg_blocks, 1);
        assert_eq!(plan.ir_instructions, 0);
        assert_eq!(plan.fragments.len(), 1);
        assert_eq!(plan.fragments[0].blocks, vec![BlockId::new(0)]);
        assert!(plan.fragments[0].is_within_budget());
        assert!(plan.permits_whole_region_optimization());
    }

    #[test]
    fn oversized_straight_line_block_is_split_before_planning() {
        let mut builder = IrBuilder::new(UnitId::new(2));
        let file = builder.add_file("straight-line.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("straight_line", FunctionFlags::default(), span);
        let block = builder.append_block(function);
        let mut result = None;
        for value in 0..1_601 {
            let constant = builder.add_constant(php_ir::IrConstant::Int(value));
            let register = builder.alloc_register(function);
            builder.emit_load_const(function, block, register, constant, span);
            result = Some(register);
        }
        builder.terminate_return(function, block, result.map(php_ir::Operand::Register), span);
        let unit = builder.finish();
        let region = BaselineRegionBuilder::build(
            &unit,
            function,
            &CompileMetadata {
                ir_fingerprint: "split-test".to_owned(),
                tier: NativeCompilerTier::Baseline,
                helper_abi_hash: 0,
                target_cpu: "test".to_owned(),
                semantic_config_hash: 0,
                dependency_identity: "test".to_owned(),
            },
        )
        .unwrap();
        let region = split_oversized_region_blocks(region);
        region.verify().unwrap();
        assert_eq!(region.blocks.len(), 13);
        assert!(
            region
                .blocks
                .iter()
                .all(|block| block.instructions.len() <= 128)
        );
        let plan = NativeCompilePlan::for_region(&region);
        assert_eq!(plan, NativeCompilePlan::for_region(&region));
        assert!(
            plan.fragments
                .iter()
                .all(NativeFragmentPlan::is_within_budget)
        );
        assert!(plan.fragments.iter().all(|fragment| {
            fragment
                .blocks
                .windows(2)
                .all(|blocks| blocks[1].raw() == blocks[0].raw().saturating_add(1))
        }));
        let mut coarse = plan.clone();
        coarse.fragments = vec![fragment_plan_for_blocks(
            &region,
            0,
            vec![region.blocks[0].id, region.blocks[1].id],
        )];
        let refined = coarse
            .refine_fragment_into(&region, 0, 2)
            .expect("multi-block fragment can be refined");
        assert_eq!(
            refined,
            coarse
                .refine_fragment_into(&region, 0, 2)
                .expect("refinement is deterministic")
        );
        assert_eq!(refined.fragments.len(), 2);
        assert!(
            refined
                .fragments
                .iter()
                .all(NativeFragmentPlan::is_within_budget)
        );
    }

    #[test]
    fn optimizing_plan_uses_its_own_whole_region_budget() {
        let mut builder = IrBuilder::new(UnitId::new(4));
        let file = builder.add_file("optimizing-plan.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("optimizing_plan", FunctionFlags::default(), span);
        let block = builder.append_block(function);
        let mut result = None;
        for value in 0..480 {
            let constant = builder.add_constant(php_ir::IrConstant::Int(value));
            let register = builder.alloc_register(function);
            builder.emit_load_const(function, block, register, constant, span);
            result = Some(register);
        }
        builder.terminate_return(function, block, result.map(php_ir::Operand::Register), span);
        let unit = builder.finish();
        let mut region = BaselineRegionBuilder::build(
            &unit,
            function,
            &CompileMetadata {
                ir_fingerprint: "optimizing-plan-test".to_owned(),
                tier: NativeCompilerTier::Baseline,
                helper_abi_hash: 0,
                target_cpu: "test".to_owned(),
                semantic_config_hash: 0,
                dependency_identity: "test".to_owned(),
            },
        )
        .unwrap();
        region = split_oversized_region_blocks(region);

        let baseline = NativeCompilePlan::for_region(&region);
        assert!(baseline.fragments.len() > 1);

        region.compile_metadata.tier = NativeCompilerTier::Optimizing;
        let optimizing = NativeCompilePlan::for_region(&region);
        assert_eq!(optimizing.fragments.len(), 1);
        assert!(optimizing.permits_whole_region_optimization());
        assert_eq!(optimizing.fragments[0].blocks.len(), region.blocks.len());
    }

    #[test]
    fn call_dense_block_is_split_by_lowering_cost_before_cranelift() {
        let mut builder = IrBuilder::new(UnitId::new(3));
        let file = builder.add_file("call-dense.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("call_dense", FunctionFlags::default(), span);
        let block = builder.append_block(function);
        for _ in 0..100 {
            let result = builder.alloc_register(function);
            builder.emit(
                function,
                block,
                InstructionKind::CallFunction {
                    dst: result,
                    name: "runtime_call".to_owned(),
                    args: Vec::new(),
                },
                span,
            );
            builder.emit(
                function,
                block,
                InstructionKind::Discard {
                    src: php_ir::Operand::Register(result),
                },
                span,
            );
        }
        builder.terminate_return(function, block, None, span);
        let unit = builder.finish();
        let region = BaselineRegionBuilder::build(
            &unit,
            function,
            &CompileMetadata {
                ir_fingerprint: "call-cost-split-test".to_owned(),
                tier: NativeCompilerTier::Baseline,
                helper_abi_hash: 0,
                target_cpu: "test".to_owned(),
                semantic_config_hash: 0,
                dependency_identity: "test".to_owned(),
            },
        )
        .unwrap();
        let region = split_oversized_region_blocks(region);
        region.verify().unwrap();

        assert!(region.blocks.len() > 1);
        assert!(region.blocks.iter().all(|block| {
            estimated_region_block_clif_blocks(block) <= BASELINE_FRAGMENT_MAX_ESTIMATED_CLIF_BLOCKS
        }));
        assert!(
            NativeCompilePlan::for_region(&region)
                .fragments
                .iter()
                .all(NativeFragmentPlan::is_within_budget)
        );
    }
}
