//! Function-scoped native compile planning.
//!
//! A production compile group contains exactly one PHP function.  The plan is
//! built before Cranelift lowering so compile breadth and structural cost are
//! explicit and testable instead of being inferred from a module afterwards.

use crate::region_ir::{
    RegionBlock, RegionGraph, RegionTerminator, baseline_instruction_lowering, build_executable_ssa,
};
use php_ir::instruction::TerminatorKind;
use php_ir::{BlockId, FunctionId, InstrId, LocalId};
use std::collections::{BTreeMap, BTreeSet};

pub const BASELINE_FRAGMENT_MAX_PHP_BLOCKS: usize = 64;
// These ceilings are intentionally below the backend's final CLIF admission
// limits. Planning must leave enough headroom for helper continuations,
// resume loaders, and frontend SSA edge splitting. The finished CLIF function
// is checked again before `define_function` can enter regalloc2.
pub const BASELINE_FRAGMENT_MAX_IR_INSTRUCTIONS: usize = 400;
pub const BASELINE_SINGLE_BLOCK_MAX_IR_INSTRUCTIONS: usize = 1_500;
pub const BASELINE_FRAGMENT_MAX_ESTIMATED_CLIF_BLOCKS: usize = 600;
pub const BASELINE_FRAGMENT_MAX_ESTIMATED_LIVE_SET: usize = 768;
pub const BASELINE_FRAGMENT_MAX_SAFEPOINT_LIVE_SUM: usize = 8_192;
pub const OPTIMIZING_REGION_MAX_PHP_BLOCKS: usize = 256;
pub const OPTIMIZING_REGION_MAX_IR_INSTRUCTIONS: usize = 1_500;
pub const OPTIMIZING_REGION_MAX_VIRTUAL_VALUES: usize = 768;
const MAX_REGION_BLOCK_INSTRUCTIONS_BEFORE_SPLIT: usize = 256;

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
            let chunks = if region_block_requires_split(block) {
                block
                    .instructions
                    .len()
                    .max(1)
                    .div_ceil(MAX_REGION_BLOCK_INSTRUCTIONS_BEFORE_SPLIT)
            } else {
                1
            };
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
        let ranges = if block.instructions.is_empty() {
            vec![(0, 0)]
        } else if !region_block_requires_split(block) {
            vec![(0, block.instructions.len())]
        } else {
            (0..block.instructions.len())
                .step_by(MAX_REGION_BLOCK_INSTRUCTIONS_BEFORE_SPLIT)
                .map(|start| {
                    (
                        start,
                        start
                            .saturating_add(MAX_REGION_BLOCK_INSTRUCTIONS_BEFORE_SPLIT)
                            .min(block.instructions.len()),
                    )
                })
                .collect::<Vec<_>>()
        };
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
    let safepoints = block
        .instructions
        .iter()
        .filter(|instruction| {
            baseline_instruction_lowering(&instruction.source_kind).requires_safepoint
        })
        .count();
    let transitions = block
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(
                instruction.kind,
                crate::region_ir::RegionInstructionKind::Binary { .. }
            )
        })
        .count();
    let suspensions = block
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(
                instruction.kind,
                crate::region_ir::RegionInstructionKind::NativeSuspend(_)
            )
        })
        .count();
    1_usize
        .saturating_add(safepoints)
        .saturating_add(transitions.saturating_mul(3))
        .saturating_add(suspensions.saturating_mul(3))
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
            && self.estimated_clif_blocks <= BASELINE_FRAGMENT_MAX_ESTIMATED_CLIF_BLOCKS
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
    /// Returns whether whole-region SSA is structurally bounded.
    #[must_use]
    pub fn permits_whole_region_optimization(&self) -> bool {
        self.fragments.len() == 1
            && self.php_cfg_blocks <= OPTIMIZING_REGION_MAX_PHP_BLOCKS
            && self.ir_instructions <= OPTIMIZING_REGION_MAX_IR_INSTRUCTIONS
            && self.virtual_values <= OPTIMIZING_REGION_MAX_VIRTUAL_VALUES
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
        let eligible_locals = (0..region.local_count)
            .map(LocalId::new)
            .collect::<BTreeSet<_>>();
        let phi_count = build_executable_ssa(region, &eligible_locals).phi_count();
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
        let mut fragments = Vec::<NativeFragmentPlan>::new();
        let mut current_blocks = Vec::new();
        let mut current_instructions = 0_usize;
        let mut current_clif_blocks = 1_usize;
        let mut current_maximum_live_set = 0_usize;
        let mut current_safepoint_live_sum = 0_usize;
        for block in &region.blocks {
            let block_instructions = block.instructions.len();
            let block_safepoints = block
                .instructions
                .iter()
                .filter(|instruction| {
                    baseline_instruction_lowering(&instruction.source_kind).requires_safepoint
                })
                .count();
            let block_transitions = block
                .instructions
                .iter()
                .filter(|instruction| {
                    matches!(
                        instruction.kind,
                        crate::region_ir::RegionInstructionKind::Binary { .. }
                    )
                })
                .count();
            let block_suspensions = block
                .instructions
                .iter()
                .filter(|instruction| {
                    matches!(
                        instruction.kind,
                        crate::region_ir::RegionInstructionKind::NativeSuspend(_)
                    )
                })
                .count();
            let block_clif_blocks = 1_usize
                .saturating_add(block_safepoints)
                .saturating_add(block_transitions.saturating_mul(3))
                .saturating_add(block_suspensions.saturating_mul(3));
            let block_maximum_live_set = block
                .instructions
                .iter()
                .map(|instruction| {
                    instruction
                        .live_locals
                        .len()
                        .saturating_add(instruction.register_uses().len())
                })
                .max()
                .unwrap_or(block.entry_live_locals.len());
            let block_safepoint_live_sum = block
                .instructions
                .iter()
                .filter(|instruction| {
                    baseline_instruction_lowering(&instruction.source_kind).requires_safepoint
                })
                .map(|instruction| instruction.live_locals.len())
                .sum::<usize>();
            let exceeds_budget = !current_blocks.is_empty()
                && (current_blocks.len().saturating_add(1) > BASELINE_FRAGMENT_MAX_PHP_BLOCKS
                    || current_instructions.saturating_add(block_instructions)
                        > BASELINE_FRAGMENT_MAX_IR_INSTRUCTIONS
                    || current_clif_blocks.saturating_add(block_clif_blocks)
                        > BASELINE_FRAGMENT_MAX_ESTIMATED_CLIF_BLOCKS
                    || current_maximum_live_set.max(block_maximum_live_set)
                        > BASELINE_FRAGMENT_MAX_ESTIMATED_LIVE_SET
                    || current_safepoint_live_sum.saturating_add(block_safepoint_live_sum)
                        > BASELINE_FRAGMENT_MAX_SAFEPOINT_LIVE_SUM);
            if exceeds_budget {
                fragments.push(NativeFragmentPlan {
                    id: u32::try_from(fragments.len()).unwrap_or(u32::MAX),
                    blocks: std::mem::take(&mut current_blocks),
                    ir_instructions: current_instructions,
                    estimated_clif_blocks: current_clif_blocks,
                    maximum_estimated_live_set: current_maximum_live_set,
                    safepoint_live_set_sum: current_safepoint_live_sum,
                });
                current_instructions = 0;
                current_clif_blocks = 1;
                current_maximum_live_set = 0;
                current_safepoint_live_sum = 0;
            }
            current_blocks.push(block.id);
            current_instructions = current_instructions.saturating_add(block_instructions);
            current_clif_blocks = current_clif_blocks.saturating_add(block_clif_blocks);
            current_maximum_live_set = current_maximum_live_set.max(block_maximum_live_set);
            current_safepoint_live_sum =
                current_safepoint_live_sum.saturating_add(block_safepoint_live_sum);
        }
        if !current_blocks.is_empty() {
            fragments.push(NativeFragmentPlan {
                id: u32::try_from(fragments.len()).unwrap_or(u32::MAX),
                blocks: current_blocks,
                ir_instructions: current_instructions,
                estimated_clif_blocks: current_clif_blocks,
                maximum_estimated_live_set: current_maximum_live_set,
                safepoint_live_set_sum: current_safepoint_live_sum,
            });
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region_ir::{BaselineRegionBuilder, CompileMetadata, NativeCompilerTier};
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
        assert_eq!(region.blocks.len(), 7);
        assert!(
            region
                .blocks
                .iter()
                .all(|block| block.instructions.len() <= 256)
        );
        let plan = NativeCompilePlan::for_region(&region);
        assert!(
            plan.fragments
                .iter()
                .all(NativeFragmentPlan::is_within_budget)
        );
    }
}
