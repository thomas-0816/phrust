//! Function-scoped native compile planning.
//!
//! A production compile group contains exactly one PHP function.  The plan is
//! built before Cranelift lowering so compile breadth and structural cost are
//! explicit and testable instead of being inferred from a module afterwards.

use crate::region_ir::{RegionGraph, baseline_instruction_lowering, build_executable_ssa};
use php_ir::{FunctionId, LocalId};
use std::collections::BTreeSet;

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
}

impl NativeCompilePlan {
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
        // Current baseline lowering still creates an instruction continuation
        // block and a terminator block in addition to the real CFG block.  P0.2
        // will reduce this estimate; recording it now prevents hidden growth.
        let estimated_clif_blocks = region
            .blocks
            .len()
            .saturating_mul(2)
            .saturating_add(instructions.len())
            .saturating_add(region.osr_entries().len())
            .saturating_add(region.exception_regions.len())
            .saturating_add(suspension_points)
            .saturating_add(4);

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
    }
}
