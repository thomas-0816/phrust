//! Function-scoped native compile planning.
//!
//! A production compile group contains exactly one PHP function.  The plan is
//! built before Cranelift lowering so compile breadth and structural cost are
//! explicit and testable instead of being inferred from a module afterwards.

use crate::region_ir::{RegionBlock, RegionGraph, RegionTerminator, generic_instruction_lowering};
use php_ir::{BlockId, FunctionId};
use std::collections::BTreeSet;

pub const NATIVE_FRAGMENT_HARD_MAX_PHP_BLOCKS: usize = 64;
/// Persistent schema for deterministic native fragment boundaries and frame
/// traffic. Increment whenever planning can change emitted fragment code.
pub const NATIVE_FRAGMENT_PLAN_SCHEMA_VERSION: u32 = 11;
// These ceilings are intentionally below the backend's final CLIF admission
// limits. Planning must leave enough headroom for helper continuations,
// resume loaders, and frontend SSA edge splitting. The finished CLIF function
// is checked again before `define_function` can enter regalloc2.
pub const NATIVE_FRAGMENT_HARD_MAX_IR_INSTRUCTIONS: usize = 400;
pub const NATIVE_SINGLE_BLOCK_HARD_MAX_IR_INSTRUCTIONS: usize = 512;
pub const NATIVE_FRAGMENT_HARD_MAX_ESTIMATED_CLIF_BLOCKS: usize = 450;
pub const NATIVE_SINGLE_BLOCK_HARD_MAX_ESTIMATED_CLIF_BLOCKS: usize = 537;
pub const NATIVE_FRAGMENT_HARD_MAX_ESTIMATED_LIVE_SET: usize = 384;
pub const NATIVE_FRAGMENT_HARD_MAX_SAFEPOINT_LIVE_SUM: usize = 4_096;
pub const OPTIMIZING_REGION_MAX_PHP_BLOCKS: usize = 256;
pub const OPTIMIZING_REGION_MAX_IR_INSTRUCTIONS: usize = 1_500;
pub const OPTIMIZING_REGION_MAX_VIRTUAL_VALUES: usize = 768;
// A baseline frame exit validates/releases every materialized lifecycle owner.
// Each owner can introduce a kind test plus guarded validate/commit branches,
// so counting only the PHP terminator made cleanup-heavy return blocks appear
// nearly free until exact CLIF preflight. This is deliberately conservative:
// it creates a native-fragment cut before the final value-producing
// instruction, not an additional runtime helper or operation fallback.
const ESTIMATED_FRAME_EXIT_CLIF_BLOCKS_PER_LOCAL: usize = 8;

fn estimated_instruction_clif_blocks(instruction: &crate::region_ir::RegionInstruction) -> usize {
    let manifest = generic_instruction_lowering(&instruction.source_kind);
    let operation_cost = match &instruction.kind {
        // The generic call trampoline allocates argument and call frames,
        // branches on the call result, and releases both frames on the normal
        // and side-exit paths. Argument ownership/reference handling can add
        // another continuation per operand. Counting a call as one generic
        // safepoint underestimates real WordPress registration files by an
        // order of magnitude.
        crate::region_ir::RegionInstructionKind::NativeCall(call) => 12_usize
            .saturating_add(call.operands.len())
            .min(NATIVE_FRAGMENT_HARD_MAX_ESTIMATED_CLIF_BLOCKS),
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
        crate::region_ir::RegionInstructionKind::Unary { .. } => {
            4_usize.saturating_add(usize::from(manifest.requires_safepoint))
        }
        crate::region_ir::RegionInstructionKind::NewArray { .. }
        | crate::region_ir::RegionInstructionKind::Discard { .. }
        | crate::region_ir::RegionInstructionKind::IssetLocal { .. }
        | crate::region_ir::RegionInstructionKind::EmptyLocal { .. }
        | crate::region_ir::RegionInstructionKind::UnsetLocal { .. } => {
            6_usize.saturating_add(usize::from(manifest.requires_safepoint))
        }
        crate::region_ir::RegionInstructionKind::ArrayInsert { .. }
        | crate::region_ir::RegionInstructionKind::AppendDim { .. }
        | crate::region_ir::RegionInstructionKind::IssetDim { .. }
        | crate::region_ir::RegionInstructionKind::EmptyDim { .. }
        | crate::region_ir::RegionInstructionKind::FetchDim { .. }
        | crate::region_ir::RegionInstructionKind::FetchProperty { .. } => {
            10_usize.saturating_add(usize::from(manifest.requires_safepoint))
        }
        crate::region_ir::RegionInstructionKind::ForeachInit { .. }
        | crate::region_ir::RegionInstructionKind::ForeachNext { .. }
        | crate::region_ir::RegionInstructionKind::ForeachCleanup { .. } => {
            8_usize.saturating_add(usize::from(manifest.requires_safepoint))
        }
        crate::region_ir::RegionInstructionKind::NativeSuspend(_) => {
            3_usize.saturating_add(usize::from(manifest.requires_safepoint))
        }
        _ => usize::from(manifest.requires_safepoint),
    };
    let continuation_state_cost = if manifest.requires_safepoint {
        instruction.live_locals.len().saturating_add(
            instruction
                .transition_live_registers
                .as_ref()
                .map_or(0, Vec::len),
        )
    } else {
        0
    };
    operation_cost.saturating_add(continuation_state_cost)
}

fn estimated_region_block_clif_blocks(block: &RegionBlock) -> usize {
    let estimate = 1_usize
        .saturating_add(
            block
                .instructions
                .iter()
                .map(estimated_instruction_clif_blocks)
                .sum::<usize>(),
        )
        .saturating_add(estimated_terminator_clif_blocks(block));
    if block.instructions.len() <= 1 {
        // No additional Region cut exists inside this shape. Keep the
        // conservative estimate at the singleton admission ceiling and let
        // exact CLIF preflight decide its real lifecycle-owner expansion.
        // Rejecting it here would make one overestimated block poison the
        // dynamic-programming parent chain and collapse all later blocks into
        // one invalid mega-fragment.
        // Fragment accounting adds its synthetic entry block after summing
        // member blocks, so reserve that final slot here.
        estimate.min(NATIVE_SINGLE_BLOCK_HARD_MAX_ESTIMATED_CLIF_BLOCKS.saturating_sub(1))
    } else {
        estimate
    }
}

fn estimated_terminator_clif_blocks(block: &RegionBlock) -> usize {
    match block.terminator {
        RegionTerminator::Jump { .. } => 1,
        RegionTerminator::JumpIfFalse { .. }
        | RegionTerminator::JumpIfTrue { .. }
        | RegionTerminator::JumpIf { .. } => 4,
        RegionTerminator::Return { .. }
        | RegionTerminator::ReturnReference { .. }
        | RegionTerminator::Exit { .. } => {
            let materialized_locals = block
                .terminator_live_locals
                .len()
                .max(block.terminator_state_locals.len());
            1_usize.saturating_add(
                materialized_locals.saturating_mul(ESTIMATED_FRAME_EXIT_CLIF_BLOCKS_PER_LOCAL),
            )
        }
    }
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
                generic_instruction_lowering(&instruction.source_kind).requires_safepoint
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
        self.blocks <= NATIVE_FRAGMENT_HARD_MAX_PHP_BLOCKS
            && (self.instructions <= NATIVE_FRAGMENT_HARD_MAX_IR_INSTRUCTIONS
                || (self.blocks == 1
                    && self.instructions <= NATIVE_SINGLE_BLOCK_HARD_MAX_IR_INSTRUCTIONS))
            && (self.clif_blocks.saturating_add(1)
                <= NATIVE_FRAGMENT_HARD_MAX_ESTIMATED_CLIF_BLOCKS
                || (self.blocks == 1
                    && self.clif_blocks.saturating_add(1)
                        <= NATIVE_SINGLE_BLOCK_HARD_MAX_ESTIMATED_CLIF_BLOCKS))
            && self.maximum_live_set <= NATIVE_FRAGMENT_HARD_MAX_ESTIMATED_LIVE_SET
            && self.safepoint_live_sum <= NATIVE_FRAGMENT_HARD_MAX_SAFEPOINT_LIVE_SUM
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
        self.blocks.len() <= NATIVE_FRAGMENT_HARD_MAX_PHP_BLOCKS
            && (self.ir_instructions <= NATIVE_FRAGMENT_HARD_MAX_IR_INSTRUCTIONS
                || (self.blocks.len() == 1
                    && self.ir_instructions <= NATIVE_SINGLE_BLOCK_HARD_MAX_IR_INSTRUCTIONS))
            && (self.estimated_clif_blocks <= NATIVE_FRAGMENT_HARD_MAX_ESTIMATED_CLIF_BLOCKS
                || (self.blocks.len() == 1
                    && self.estimated_clif_blocks
                        <= NATIVE_SINGLE_BLOCK_HARD_MAX_ESTIMATED_CLIF_BLOCKS))
            && self.maximum_estimated_live_set <= NATIVE_FRAGMENT_HARD_MAX_ESTIMATED_LIVE_SET
            && self.safepoint_live_set_sum <= NATIVE_FRAGMENT_HARD_MAX_SAFEPOINT_LIVE_SUM
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
    /// Builds the bounded fragment form even when the optimizing source
    /// heuristics initially admit one whole function. Exact CLIF preflight
    /// uses this when backend expansion disproves that optimistic estimate.
    #[must_use]
    pub(crate) fn for_bounded_fragments(region: &RegionGraph) -> Self {
        let mut plan = Self::for_region(region);
        plan.fragments = cost_aware_fragment_blocks(region)
            .into_iter()
            .enumerate()
            .map(|(id, blocks)| fragment_plan_for_blocks(region, id, blocks))
            .collect();
        plan
    }

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
                let left_boundary_cost = fragment_boundary_cost(
                    region,
                    &successors,
                    &live_in,
                    &boundary_metadata,
                    start,
                    *cut,
                );
                let right_boundary_cost = fragment_boundary_cost(
                    region,
                    &successors,
                    &live_in,
                    &boundary_metadata,
                    *cut,
                    end,
                );
                let left = fragment_plan_for_blocks(
                    region,
                    0,
                    region.blocks[start..*cut]
                        .iter()
                        .map(|block| block.id)
                        .collect(),
                );
                let right = fragment_plan_for_blocks(
                    region,
                    0,
                    region.blocks[*cut..end]
                        .iter()
                        .map(|block| block.id)
                        .collect(),
                );
                let clif_balance = left
                    .estimated_clif_blocks
                    .abs_diff(right.estimated_clif_blocks);
                let instruction_balance = left.ir_instructions.abs_diff(right.ir_instructions);
                // Exact recovery must make structural progress first. Giving
                // frame traffic priority can repeatedly peel off one cheap
                // block while leaving nearly the complete oversized CLIF
                // graph intact, forcing another full lowering round. Balance
                // the planner's backend-expansion estimate rather than raw
                // block count; call and exception blocks have very different
                // generated costs.
                (
                    clif_balance,
                    instruction_balance,
                    left_boundary_cost.saturating_add(right_boundary_cost),
                    *cut,
                )
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
        // This path is driven by exact finished-CLIF measurement. The source
        // estimator must not veto the measured natural partition or replace
        // it with arbitrary instruction chunks; the caller immediately
        // preflights every child against the real backend ceilings.
        Some(refined)
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
                generic_instruction_lowering(&instruction.source_kind).requires_safepoint
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
        // The function-scoped admission plan only needs a conservative phi-cost
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
        // A plan always begins as one complete PHP machine function. Only an
        // exact finished-CLIF hard-limit result may call
        // `for_bounded_fragments` or `refine_fragment_into` later.
        let fragments = vec![fragment_plan_for_blocks(
            region,
            0,
            region.blocks.iter().map(|block| block.id).collect(),
        )];

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
    use crate::region_ir::{CompileMetadata, GenericRegionBuilder, NativeCompilerTier};
    use php_ir::{FunctionFlags, IrBuilder, IrSpan, UnitId};

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
        let region = GenericRegionBuilder::build(
            &unit,
            FunctionId::new(1),
            &CompileMetadata {
                ir_fingerprint: "plan-test".to_owned(),
                tier: NativeCompilerTier::Generic,
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
}
