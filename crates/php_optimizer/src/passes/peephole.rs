use super::super::*;

/// Peephole simplification for trivially side-effect-free IR patterns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PeepholeSimplify;

impl OptimizerPass for PeepholeSimplify {
    fn name(&self) -> &'static str {
        "peephole_simplify"
    }

    fn phase(&self) -> PassPhase {
        PassPhase::PreVerify
    }

    fn run(
        &self,
        transaction: &mut PassTransaction<'_>,
        _context: &PassContext,
    ) -> Result<PassReport, PassError> {
        let mut stats = PeepholeStats::default();

        while let Some(peephole) = find_peephole(transaction.unit()) {
            apply_peephole(transaction, peephole);
            stats.record(peephole);
        }

        let total = stats.total_transformations();
        Ok(PassReport {
            name: self.name(),
            phase: self.phase(),
            enabled: true,
            changed: total > 0,
            source_spans_preserved: true,
            rolled_back: false,
            scope: PassScopeReport::default(),
            stats: stats.into_report_stats(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Peephole {
    RemoveNop {
        function: usize,
        block: usize,
        instruction: usize,
    },
    RemoveSelfMove {
        function: usize,
        block: usize,
        instruction: usize,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct PeepholeStats {
    noops_removed: u64,
    self_moves_removed: u64,
}

impl PeepholeStats {
    fn record(&mut self, peephole: Peephole) {
        match peephole {
            Peephole::RemoveNop { .. } => self.noops_removed += 1,
            Peephole::RemoveSelfMove { .. } => self.self_moves_removed += 1,
        }
    }

    fn total_transformations(&self) -> u64 {
        self.noops_removed + self.self_moves_removed
    }

    fn into_report_stats(self) -> BTreeMap<&'static str, u64> {
        let total = self.total_transformations();
        BTreeMap::from([
            ("noops_removed", self.noops_removed),
            ("self_moves_removed", self.self_moves_removed),
            ("total_transformations", total),
            ("transformations_attempted", total),
            ("transformations_applied", total),
            ("transformations_skipped", 0),
            ("skipped_no_match", 0),
        ])
    }
}

fn find_peephole(unit: &IrUnit) -> Option<Peephole> {
    for (function_index, function) in unit.functions.iter().enumerate() {
        for (block_index, block) in function.blocks.iter().enumerate() {
            for (instruction_index, instruction) in block.instructions.iter().enumerate() {
                match instruction.kind {
                    InstructionKind::Nop => {
                        return Some(Peephole::RemoveNop {
                            function: function_index,
                            block: block_index,
                            instruction: instruction_index,
                        });
                    }
                    InstructionKind::Move {
                        dst,
                        src: Operand::Register(src),
                    } if dst == src => {
                        return Some(Peephole::RemoveSelfMove {
                            function: function_index,
                            block: block_index,
                            instruction: instruction_index,
                        });
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

fn apply_peephole(transaction: &mut PassTransaction<'_>, peephole: Peephole) {
    let (function, block, instruction) = match peephole {
        Peephole::RemoveNop {
            function,
            block,
            instruction,
        }
        | Peephole::RemoveSelfMove {
            function,
            block,
            instruction,
        } => (function, block, instruction),
    };
    transaction.touch_block(function, block);
    let block = &mut transaction.function_mut(function).blocks[block];
    block.instructions.remove(instruction);
    renumber_instructions(block);
}

fn renumber_instructions(block: &mut php_ir::BasicBlock) {
    for (index, instruction) in block.instructions.iter_mut().enumerate() {
        instruction.id = InstrId::new(index as u32);
    }
}
