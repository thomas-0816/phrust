use super::super::*;
use super::analyses::defined_registers;

/// Conservative branch simplification backed by a minimal CFG view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BranchSimplify;

impl OptimizerPass for BranchSimplify {
    fn name(&self) -> &'static str {
        "branch_simplify"
    }

    fn phase(&self) -> PassPhase {
        PassPhase::PreVerify
    }

    fn run(
        &self,
        transaction: &mut PassTransaction<'_>,
        _context: &PassContext,
    ) -> Result<PassReport, PassError> {
        let mut stats = BranchSimplifyStats::default();

        while let Some(simplification) = find_branch_simplification(transaction.unit()) {
            apply_branch_simplification(transaction, simplification);
            stats.record(simplification);
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct CfgView {
    successors: Vec<Vec<BlockId>>,
    predecessors: Vec<Vec<BlockId>>,
    reachable: Vec<bool>,
}

impl CfgView {
    fn new(function: &IrFunction) -> Self {
        let successors: Vec<Vec<BlockId>> = (0..function.blocks.len())
            .map(|index| block_successors(function, index))
            .collect();
        let mut predecessors = vec![Vec::new(); function.blocks.len()];
        for (source, targets) in successors.iter().enumerate() {
            for target in targets {
                if target.index() < predecessors.len() {
                    predecessors[target.index()].push(BlockId::new(source as u32));
                }
            }
        }
        let mut reachable = vec![false; function.blocks.len()];
        let mut stack = if function.blocks.is_empty() {
            Vec::new()
        } else {
            vec![BlockId::new(0)]
        };
        while let Some(block) = stack.pop() {
            let index = block.index();
            if index >= reachable.len() || reachable[index] {
                continue;
            }
            reachable[index] = true;
            for successor in &successors[index] {
                stack.push(*successor);
            }
        }
        Self {
            successors,
            predecessors,
            reachable,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BranchSimplification {
    ConstantBranch {
        function: usize,
        block: usize,
        target: BlockId,
    },
    ForwardEmptyBlock {
        function: usize,
        block: usize,
        old_target: BlockId,
        new_target: BlockId,
    },
    RemoveUnreachableEmptyTail {
        function: usize,
        new_len: usize,
        removed: usize,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct BranchSimplifyStats {
    constant_branches: u64,
    empty_block_forwards: u64,
    unreachable_empty_tail_blocks_removed: u64,
}

impl BranchSimplifyStats {
    fn record(&mut self, simplification: BranchSimplification) {
        match simplification {
            BranchSimplification::ConstantBranch { .. } => self.constant_branches += 1,
            BranchSimplification::ForwardEmptyBlock { .. } => self.empty_block_forwards += 1,
            BranchSimplification::RemoveUnreachableEmptyTail { removed, .. } => {
                self.unreachable_empty_tail_blocks_removed += removed as u64;
            }
        }
    }

    fn total_transformations(&self) -> u64 {
        self.constant_branches
            + self.empty_block_forwards
            + self.unreachable_empty_tail_blocks_removed
    }

    fn into_report_stats(self) -> BTreeMap<&'static str, u64> {
        let total = self.total_transformations();
        BTreeMap::from([
            ("constant_branches", self.constant_branches),
            ("empty_block_forwards", self.empty_block_forwards),
            (
                "unreachable_empty_tail_blocks_removed",
                self.unreachable_empty_tail_blocks_removed,
            ),
            ("total_transformations", total),
            ("transformations_attempted", total),
            ("transformations_applied", total),
            ("transformations_skipped", 0),
            ("skipped_no_match", 0),
        ])
    }
}

fn find_branch_simplification(unit: &IrUnit) -> Option<BranchSimplification> {
    for (function_index, function) in unit.functions.iter().enumerate() {
        let cfg = CfgView::new(function);
        for (block_index, block) in function.blocks.iter().enumerate() {
            if block_has_exception_boundary(block) {
                continue;
            }
            if let Some(terminator) = &block.terminator
                && let Some(target) =
                    constant_branch_target(function, block_index, &terminator.kind, &unit.constants)
            {
                return Some(BranchSimplification::ConstantBranch {
                    function: function_index,
                    block: block_index,
                    target,
                });
            }
        }

        for (block_index, block) in function.blocks.iter().enumerate() {
            let Some(terminator) = &block.terminator else {
                continue;
            };
            for target in terminator_explicit_targets(&terminator.kind) {
                let target_index = target.index();
                if target_index >= function.blocks.len() {
                    continue;
                }
                let target_block = &function.blocks[target_index];
                if block_has_exception_boundary(block) || block_has_exception_boundary(target_block)
                {
                    continue;
                }
                if target_block.instructions.is_empty()
                    && let Some(target_terminator) = &target_block.terminator
                    && let TerminatorKind::Jump { target: new_target } = target_terminator.kind
                    && new_target != target
                {
                    return Some(BranchSimplification::ForwardEmptyBlock {
                        function: function_index,
                        block: block_index,
                        old_target: target,
                        new_target,
                    });
                }
            }
        }

        if let Some(simplification) =
            unreachable_empty_tail_simplification(function_index, function, &cfg)
        {
            return Some(simplification);
        }
    }
    None
}

fn apply_branch_simplification(
    transaction: &mut PassTransaction<'_>,
    simplification: BranchSimplification,
) {
    match simplification {
        BranchSimplification::ConstantBranch {
            function,
            block,
            target,
        } => {
            transaction.touch_block(function, block);
            let block = &mut transaction.function_mut(function).blocks[block];
            let span = block
                .terminator
                .as_ref()
                .map(|terminator| terminator.span)
                .expect("branch simplification requires a terminator");
            block.terminator = Some(php_ir::Terminator {
                span,
                kind: TerminatorKind::Jump { target },
            });
        }
        BranchSimplification::ForwardEmptyBlock {
            function,
            block,
            old_target,
            new_target,
        } => {
            transaction.touch_block(function, block);
            let terminator = transaction.function_mut(function).blocks[block]
                .terminator
                .as_mut()
                .expect("forwarding simplification requires a terminator");
            replace_terminator_target(&mut terminator.kind, old_target, new_target);
        }
        BranchSimplification::RemoveUnreachableEmptyTail {
            function, new_len, ..
        } => {
            transaction.touch_block(function, new_len);
            transaction.function_mut(function).blocks.truncate(new_len);
        }
    }
}

fn block_successors(function: &IrFunction, block_index: usize) -> Vec<BlockId> {
    let Some(terminator) = &function.blocks[block_index].terminator else {
        return Vec::new();
    };
    let next = || {
        let next_index = block_index + 1;
        (next_index < function.blocks.len()).then(|| BlockId::new(next_index as u32))
    };
    match terminator.kind {
        TerminatorKind::Jump { target } => vec![target],
        TerminatorKind::JumpIfFalse { target, .. } | TerminatorKind::JumpIfTrue { target, .. } => {
            let mut targets = vec![target];
            if let Some(next) = next() {
                targets.push(next);
            }
            targets
        }
        TerminatorKind::JumpIf {
            if_true, if_false, ..
        } => vec![if_true, if_false],
        TerminatorKind::Return { .. } | TerminatorKind::Exit { .. } => Vec::new(),
    }
}

fn terminator_explicit_targets(kind: &TerminatorKind) -> Vec<BlockId> {
    match kind {
        TerminatorKind::Jump { target }
        | TerminatorKind::JumpIfFalse { target, .. }
        | TerminatorKind::JumpIfTrue { target, .. } => vec![*target],
        TerminatorKind::JumpIf {
            if_true, if_false, ..
        } => vec![*if_true, *if_false],
        TerminatorKind::Return { .. } | TerminatorKind::Exit { .. } => Vec::new(),
    }
}

fn constant_branch_target(
    function: &IrFunction,
    block_index: usize,
    kind: &TerminatorKind,
    constants: &[IrConstant],
) -> Option<BlockId> {
    let bool_value = match kind {
        TerminatorKind::JumpIfFalse { condition, .. }
        | TerminatorKind::JumpIfTrue { condition, .. }
        | TerminatorKind::JumpIf { condition, .. } => {
            condition_bool_value(function, block_index, *condition, constants)?
        }
        TerminatorKind::Jump { .. }
        | TerminatorKind::Return { .. }
        | TerminatorKind::Exit { .. } => {
            return None;
        }
    };
    match kind {
        TerminatorKind::JumpIfFalse { target, .. } => {
            if bool_value {
                next_block(function, block_index)
            } else {
                Some(*target)
            }
        }
        TerminatorKind::JumpIfTrue { target, .. } => {
            if bool_value {
                Some(*target)
            } else {
                next_block(function, block_index)
            }
        }
        TerminatorKind::JumpIf {
            if_true, if_false, ..
        } => Some(if bool_value { *if_true } else { *if_false }),
        TerminatorKind::Jump { .. }
        | TerminatorKind::Return { .. }
        | TerminatorKind::Exit { .. } => None,
    }
}

fn condition_bool_value(
    function: &IrFunction,
    block_index: usize,
    condition: Operand,
    constants: &[IrConstant],
) -> Option<bool> {
    let constant = match condition {
        Operand::Constant(constant) => constant,
        Operand::Register(register) => {
            block_register_bool_constant(&function.blocks[block_index], register)?
        }
        Operand::Local(_) => return None,
    };
    match constants.get(constant.index())? {
        IrConstant::Bool(value) => Some(*value),
        _ => None,
    }
}

fn block_register_bool_constant(block: &php_ir::BasicBlock, register: RegId) -> Option<ConstId> {
    for instruction in block.instructions.iter().rev() {
        match instruction.kind {
            InstructionKind::LoadConst { dst, constant } if dst == register => {
                return Some(constant);
            }
            _ if defined_registers(&instruction.kind).contains(&register) => return None,
            _ => continue,
        }
    }
    None
}

fn next_block(function: &IrFunction, block_index: usize) -> Option<BlockId> {
    let next_index = block_index + 1;
    (next_index < function.blocks.len()).then(|| BlockId::new(next_index as u32))
}

fn replace_terminator_target(kind: &mut TerminatorKind, old_target: BlockId, new_target: BlockId) {
    match kind {
        TerminatorKind::Jump { target }
        | TerminatorKind::JumpIfFalse { target, .. }
        | TerminatorKind::JumpIfTrue { target, .. } => {
            if *target == old_target {
                *target = new_target;
            }
        }
        TerminatorKind::JumpIf {
            if_true, if_false, ..
        } => {
            if *if_true == old_target {
                *if_true = new_target;
            }
            if *if_false == old_target {
                *if_false = new_target;
            }
        }
        TerminatorKind::Return { .. } | TerminatorKind::Exit { .. } => {}
    }
}

fn unreachable_empty_tail_simplification(
    function_index: usize,
    function: &IrFunction,
    cfg: &CfgView,
) -> Option<BranchSimplification> {
    let last_reachable = cfg
        .reachable
        .iter()
        .rposition(|reachable| *reachable)
        .unwrap_or(0);
    let new_len = last_reachable + 1;
    if new_len >= function.blocks.len() {
        return None;
    }
    if kept_blocks_reference_removed_tail(function, new_len) {
        return None;
    }
    let tail = &function.blocks[new_len..];
    if tail.iter().all(|block| {
        block.instructions.is_empty()
            && block.terminator.is_some()
            && !block_has_exception_boundary(block)
    }) {
        return Some(BranchSimplification::RemoveUnreachableEmptyTail {
            function: function_index,
            new_len,
            removed: tail.len(),
        });
    }
    None
}

fn kept_blocks_reference_removed_tail(function: &IrFunction, new_len: usize) -> bool {
    function.blocks[..new_len]
        .iter()
        .flat_map(|block| &block.instructions)
        .flat_map(instruction_metadata_targets)
        .any(|target| target.index() >= new_len)
}

fn instruction_metadata_targets(instruction: &php_ir::Instruction) -> Vec<BlockId> {
    match &instruction.kind {
        InstructionKind::EnterTry {
            catch,
            finally,
            after,
            ..
        } => {
            let mut targets = vec![*after];
            if let Some(catch) = catch {
                targets.push(*catch);
            }
            if let Some(finally) = finally {
                targets.push(*finally);
            }
            targets
        }
        InstructionKind::EndFinally { after } => vec![*after],
        _ => Vec::new(),
    }
}

fn block_has_exception_boundary(block: &php_ir::BasicBlock) -> bool {
    block.instructions.iter().any(|instruction| {
        matches!(
            instruction.kind,
            InstructionKind::EnterTry { .. }
                | InstructionKind::LeaveTry
                | InstructionKind::EndFinally { .. }
                | InstructionKind::Throw { .. }
                | InstructionKind::MakeException { .. }
        )
    })
}
