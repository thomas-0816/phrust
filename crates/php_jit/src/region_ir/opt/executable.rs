//! Semantics-preserving transformations over authoritative executable Region IR.

use std::collections::{BTreeMap, BTreeSet};

use super::super::{
    RegionBinaryOp, RegionCastOp, RegionCompareOpCode, RegionGraph, RegionInstructionKind,
    RegionOperand, RegionTerminator, RegionUnaryOp,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExecutableOptReport {
    pub constants_folded: u64,
    pub common_subexpressions: u64,
    pub branches_folded: u64,
    pub dead_instructions: u64,
    pub loop_invariants_hoisted: u64,
}

/// Apply SCCP-style scalar folding and block-local GVN to code that is later
/// consumed by Cranelift. Effects, diagnostics, throws, safepoints, and
/// lifecycle operations are never moved or removed here.
pub fn optimize_executable_region(region: &mut RegionGraph) -> ExecutableOptReport {
    let mut report = ExecutableOptReport::default();
    for block in &mut region.blocks {
        let mut constants = BTreeMap::new();
        let mut classes = BTreeMap::new();
        let mut expressions = BTreeMap::<String, php_ir::RegId>::new();
        for instruction in &mut block.instructions {
            let replacement = match &instruction.kind {
                RegionInstructionKind::Move { dst, src } => {
                    let value = resolve(*src, &constants);
                    if is_immediate(value) {
                        constants.insert(*dst, value);
                    }
                    None
                }
                RegionInstructionKind::Binary { dst, op, lhs, rhs } => {
                    let lhs = resolve(*lhs, &constants);
                    let rhs = resolve(*rhs, &constants);
                    fold_binary(*op, lhs, rhs).map(|value| (*dst, value))
                }
                RegionInstructionKind::Unary { dst, op, src } => {
                    let src = resolve(*src, &constants);
                    fold_unary(*op, src).map(|value| (*dst, value))
                }
                RegionInstructionKind::Compare { dst, op, lhs, rhs } => {
                    let lhs = resolve(*lhs, &constants);
                    let rhs = resolve(*rhs, &constants);
                    fold_compare(*op, lhs, rhs).map(|value| (*dst, value))
                }
                RegionInstructionKind::Cast { dst, op, src } => {
                    let src = resolve(*src, &constants);
                    fold_cast(*op, src).map(|value| (*dst, value))
                }
                _ => None,
            };
            if let Some((dst, value)) = replacement {
                instruction.kind = RegionInstructionKind::Move { dst, src: value };
                constants.insert(dst, value);
                if let Some(class) = operand_class(value, &classes) {
                    classes.insert(dst, class);
                }
                report.constants_folded = report.constants_folded.saturating_add(1);
                continue;
            }

            let expression = match &instruction.kind {
                RegionInstructionKind::Binary { dst, op, lhs, rhs } => Some((
                    *dst,
                    format!(
                        "binary:{op:?}:{:?}:{:?}",
                        resolve(*lhs, &constants),
                        resolve(*rhs, &constants)
                    ),
                )),
                RegionInstructionKind::Unary { dst, op, src } => Some((
                    *dst,
                    format!("unary:{op:?}:{:?}", resolve(*src, &constants)),
                )),
                RegionInstructionKind::Compare { dst, op, lhs, rhs } => Some((
                    *dst,
                    format!(
                        "compare:{op:?}:{:?}:{:?}",
                        resolve(*lhs, &constants),
                        resolve(*rhs, &constants)
                    ),
                )),
                RegionInstructionKind::Cast { dst, op, src } => {
                    Some((*dst, format!("cast:{op:?}:{:?}", resolve(*src, &constants))))
                }
                _ => None,
            }
            .filter(|_| instruction_is_pure_scalar(&instruction.kind, &classes));
            if let Some((dst, key)) = expression {
                if let Some(existing) = expressions.get(&key).copied() {
                    instruction.kind = RegionInstructionKind::Move {
                        dst,
                        src: RegionOperand::Register(existing),
                    };
                    report.common_subexpressions = report.common_subexpressions.saturating_add(1);
                } else {
                    expressions.insert(key, dst);
                }
            }
            if let Some((dst, class)) = instruction_result_class(&instruction.kind, &classes) {
                classes.insert(dst, class);
            }
        }

        let condition = match block.terminator {
            RegionTerminator::JumpIfFalse { condition, .. }
            | RegionTerminator::JumpIfTrue { condition, .. }
            | RegionTerminator::JumpIf { condition, .. } => resolve(condition, &constants),
            _ => continue,
        };
        if let Some(truthy) = immediate_truthy(condition) {
            let target = match block.terminator {
                RegionTerminator::JumpIfFalse {
                    target,
                    fallthrough,
                    ..
                } => {
                    if truthy {
                        fallthrough
                    } else {
                        target
                    }
                }
                RegionTerminator::JumpIfTrue {
                    target,
                    fallthrough,
                    ..
                } => {
                    if truthy {
                        target
                    } else {
                        fallthrough
                    }
                }
                RegionTerminator::JumpIf {
                    if_true, if_false, ..
                } => {
                    if truthy {
                        if_true
                    } else {
                        if_false
                    }
                }
                _ => unreachable!(),
            };
            block.terminator = RegionTerminator::Jump { target };
            report.branches_folded = report.branches_folded.saturating_add(1);
        }
    }
    hoist_loop_invariants(region, &mut report);
    eliminate_dead_moves(region, &mut report);
    report
}

fn hoist_loop_invariants(region: &mut RegionGraph, report: &mut ExecutableOptReport) {
    let ssa = super::super::build_executable_ssa(region, &BTreeSet::new());
    let mut backedges = Vec::new();
    for block in &region.blocks {
        for target in block.terminator.targets() {
            if ssa.dominates(target, block.id) {
                backedges.push((block.id, target));
            }
        }
    }
    for (source, header) in backedges {
        let loop_blocks = natural_loop(&ssa, source, header);
        // Generated code can resume directly at a call/control continuation
        // after the VM handles an exception.  A value hoisted above that
        // continuation would then be undefined on the resume edge, which is
        // not represented by the ordinary Region CFG.  Keep those loops
        // pinned until exceptional/OSR resume edges are explicit SSA inputs.
        if region.blocks.iter().any(|block| {
            loop_blocks.contains(&block.id)
                && block
                    .instructions
                    .iter()
                    .any(|instruction| instruction_is_motion_barrier(&instruction.kind))
        }) {
            continue;
        }
        let outside_predecessors = ssa
            .predecessors(header)
            .iter()
            .copied()
            .filter(|predecessor| !loop_blocks.contains(predecessor))
            .collect::<Vec<_>>();
        let [preheader] = outside_predecessors.as_slice() else {
            continue;
        };
        let classes = infer_register_classes(region);
        let definitions = register_definition_blocks(region);
        let mut invariant = definitions
            .iter()
            .filter_map(|(register, block)| (!loop_blocks.contains(block)).then_some(*register))
            .collect::<BTreeSet<_>>();
        let mut candidates = Vec::<(usize, usize, php_ir::RegId)>::new();
        loop {
            let mut changed = false;
            for block in &region.blocks {
                if !loop_blocks.contains(&block.id) {
                    continue;
                }
                for (instruction_index, instruction) in block.instructions.iter().enumerate() {
                    let Some(result) = scalar_result_register(&instruction.kind, &classes) else {
                        continue;
                    };
                    if invariant.contains(&result)
                        || !instruction
                            .register_uses()
                            .iter()
                            .all(|use_| invariant.contains(use_))
                    {
                        continue;
                    }
                    invariant.insert(result);
                    candidates.push((block.id.index(), instruction_index, result));
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        if candidates.is_empty() {
            continue;
        }
        candidates.sort_by_key(|(block, instruction, _)| (*block, *instruction));
        let candidate_registers = candidates
            .iter()
            .map(|(_, _, register)| *register)
            .collect::<BTreeSet<_>>();
        let mut hoisted = Vec::new();
        for block in &mut region.blocks {
            if !loop_blocks.contains(&block.id) {
                continue;
            }
            let mut kept = Vec::with_capacity(block.instructions.len());
            for instruction in std::mem::take(&mut block.instructions) {
                if instruction_result_register(&instruction.kind)
                    .is_some_and(|register| candidate_registers.contains(&register))
                {
                    hoisted.push(instruction);
                } else {
                    kept.push(instruction);
                }
            }
            block.instructions = kept;
        }
        if let Some(block) = region.blocks.get_mut(preheader.index()) {
            report.loop_invariants_hoisted = report
                .loop_invariants_hoisted
                .saturating_add(hoisted.len() as u64);
            block.instructions.extend(hoisted);
        }
    }
}

const fn instruction_is_motion_barrier(kind: &RegionInstructionKind) -> bool {
    matches!(
        kind,
        RegionInstructionKind::NativeCall(_)
            | RegionInstructionKind::NativeControl(_)
            | RegionInstructionKind::NativeSuspend(_)
            | RegionInstructionKind::NativeDynamicCode(_)
            | RegionInstructionKind::RuntimeFatal { .. }
            | RegionInstructionKind::CompileTimeFatal { .. }
    )
}

fn natural_loop(
    ssa: &super::super::ExecutableSsaGraph,
    source: php_ir::BlockId,
    header: php_ir::BlockId,
) -> BTreeSet<php_ir::BlockId> {
    let mut blocks = BTreeSet::from([header, source]);
    let mut work = if source == header {
        Vec::new()
    } else {
        vec![source]
    };
    while let Some(block) = work.pop() {
        for predecessor in ssa.predecessors(block) {
            if blocks.insert(*predecessor) && *predecessor != header {
                work.push(*predecessor);
            }
        }
    }
    blocks
}

fn infer_register_classes(
    region: &RegionGraph,
) -> BTreeMap<php_ir::RegId, super::super::SsaValueClass> {
    let mut classes = BTreeMap::new();
    for _ in 0..=region.register_count {
        let previous = classes.clone();
        for block in &region.blocks {
            for instruction in &block.instructions {
                if let Some((register, class)) =
                    instruction_result_class(&instruction.kind, &classes)
                {
                    classes.insert(register, class);
                }
            }
        }
        if classes == previous {
            break;
        }
    }
    classes
}

fn register_definition_blocks(region: &RegionGraph) -> BTreeMap<php_ir::RegId, php_ir::BlockId> {
    region
        .blocks
        .iter()
        .flat_map(|block| {
            block.instructions.iter().filter_map(move |instruction| {
                instruction_result_register(&instruction.kind).map(|register| (register, block.id))
            })
        })
        .collect()
}

fn scalar_result_register(
    kind: &RegionInstructionKind,
    classes: &BTreeMap<php_ir::RegId, super::super::SsaValueClass>,
) -> Option<php_ir::RegId> {
    match kind {
        RegionInstructionKind::Move { dst, src }
            if operand_class(*src, classes).is_some_and(|class| {
                matches!(
                    class,
                    super::super::SsaValueClass::Null
                        | super::super::SsaValueClass::Bool
                        | super::super::SsaValueClass::Int
                )
            }) =>
        {
            Some(*dst)
        }
        _ if instruction_is_licm_safe_scalar(kind, classes) => instruction_result_register(kind),
        _ => None,
    }
}

fn instruction_is_licm_safe_scalar(
    kind: &RegionInstructionKind,
    classes: &BTreeMap<php_ir::RegId, super::super::SsaValueClass>,
) -> bool {
    match kind {
        RegionInstructionKind::Binary { op, lhs, rhs, .. } => {
            matches!(
                op,
                RegionBinaryOp::BitAnd | RegionBinaryOp::BitOr | RegionBinaryOp::BitXor
            ) && operand_class(*lhs, classes) == Some(super::super::SsaValueClass::Int)
                && operand_class(*rhs, classes) == Some(super::super::SsaValueClass::Int)
        }
        RegionInstructionKind::Unary { op, .. } => {
            !matches!(op, RegionUnaryOp::Minus) && instruction_is_pure_scalar(kind, classes)
        }
        RegionInstructionKind::Compare { .. } | RegionInstructionKind::Cast { .. } => {
            instruction_is_pure_scalar(kind, classes)
        }
        _ => false,
    }
}

const fn instruction_result_register(kind: &RegionInstructionKind) -> Option<php_ir::RegId> {
    match kind {
        RegionInstructionKind::Move { dst, .. }
        | RegionInstructionKind::Binary { dst, .. }
        | RegionInstructionKind::Unary { dst, .. }
        | RegionInstructionKind::Compare { dst, .. }
        | RegionInstructionKind::Cast { dst, .. } => Some(*dst),
        _ => None,
    }
}

fn eliminate_dead_moves(region: &mut RegionGraph, report: &mut ExecutableOptReport) {
    loop {
        let classes = infer_register_classes(region);
        let used = region
            .blocks
            .iter()
            .flat_map(|block| {
                block
                    .instructions
                    .iter()
                    .flat_map(super::super::RegionInstruction::register_uses)
                    .chain(block.terminator.register_uses())
            })
            .collect::<BTreeSet<_>>();
        let mut removed = 0_u64;
        for block in &mut region.blocks {
            block.instructions.retain(|instruction| {
                let dead = instruction_result_register(&instruction.kind).is_some_and(|dst| {
                    !used.contains(&dst)
                        && (matches!(instruction.kind, RegionInstructionKind::Move { .. })
                            || instruction_is_dce_safe_scalar(&instruction.kind, &classes))
                });
                removed = removed.saturating_add(u64::from(dead));
                !dead
            });
        }
        report.dead_instructions = report.dead_instructions.saturating_add(removed);
        if removed == 0 {
            break;
        }
    }
}

fn instruction_is_dce_safe_scalar(
    kind: &RegionInstructionKind,
    classes: &BTreeMap<php_ir::RegId, super::super::SsaValueClass>,
) -> bool {
    match kind {
        RegionInstructionKind::Binary { op, lhs, rhs, .. } => {
            matches!(
                op,
                RegionBinaryOp::Add
                    | RegionBinaryOp::Sub
                    | RegionBinaryOp::Mul
                    | RegionBinaryOp::BitAnd
                    | RegionBinaryOp::BitOr
                    | RegionBinaryOp::BitXor
            ) && operand_class(*lhs, classes) == Some(super::super::SsaValueClass::Int)
                && operand_class(*rhs, classes) == Some(super::super::SsaValueClass::Int)
        }
        RegionInstructionKind::Unary { .. }
        | RegionInstructionKind::Compare { .. }
        | RegionInstructionKind::Cast { .. } => instruction_is_pure_scalar(kind, classes),
        _ => false,
    }
}

fn instruction_is_pure_scalar(
    kind: &RegionInstructionKind,
    classes: &BTreeMap<php_ir::RegId, super::super::SsaValueClass>,
) -> bool {
    match kind {
        RegionInstructionKind::Binary { lhs, rhs, .. } => {
            operand_class(*lhs, classes) == Some(super::super::SsaValueClass::Int)
                && operand_class(*rhs, classes) == Some(super::super::SsaValueClass::Int)
        }
        RegionInstructionKind::Unary { op, src, .. } => {
            let class = operand_class(*src, classes);
            matches!(
                (op, class),
                (RegionUnaryOp::Not, Some(super::super::SsaValueClass::Null))
                    | (RegionUnaryOp::Not, Some(super::super::SsaValueClass::Bool))
                    | (RegionUnaryOp::Not, Some(super::super::SsaValueClass::Int))
                    | (
                        RegionUnaryOp::Plus | RegionUnaryOp::Minus | RegionUnaryOp::BitNot,
                        Some(super::super::SsaValueClass::Int)
                    )
            )
        }
        RegionInstructionKind::Compare { lhs, rhs, .. } => {
            let lhs = operand_class(*lhs, classes);
            let rhs = operand_class(*rhs, classes);
            lhs.is_some()
                && rhs.is_some()
                && lhs.is_some_and(|class| {
                    matches!(
                        class,
                        super::super::SsaValueClass::Null
                            | super::super::SsaValueClass::Bool
                            | super::super::SsaValueClass::Int
                    )
                })
                && rhs.is_some_and(|class| {
                    matches!(
                        class,
                        super::super::SsaValueClass::Null
                            | super::super::SsaValueClass::Bool
                            | super::super::SsaValueClass::Int
                    )
                })
        }
        RegionInstructionKind::Cast { op, src, .. } => {
            matches!(
                (op, operand_class(*src, classes)),
                (
                    RegionCastOp::Bool | RegionCastOp::Int | RegionCastOp::Void,
                    Some(
                        super::super::SsaValueClass::Null
                            | super::super::SsaValueClass::Bool
                            | super::super::SsaValueClass::Int
                    )
                )
            )
        }
        _ => false,
    }
}

fn instruction_result_class(
    kind: &RegionInstructionKind,
    classes: &BTreeMap<php_ir::RegId, super::super::SsaValueClass>,
) -> Option<(php_ir::RegId, super::super::SsaValueClass)> {
    let (dst, class) = match kind {
        RegionInstructionKind::Move { dst, src } => (*dst, operand_class(*src, classes)?),
        RegionInstructionKind::Binary { dst, lhs, rhs, .. }
            if operand_class(*lhs, classes) == Some(super::super::SsaValueClass::Int)
                && operand_class(*rhs, classes) == Some(super::super::SsaValueClass::Int) =>
        {
            (*dst, super::super::SsaValueClass::Int)
        }
        RegionInstructionKind::Unary { dst, op, src } => match op {
            RegionUnaryOp::Not => (*dst, super::super::SsaValueClass::Bool),
            _ if operand_class(*src, classes) == Some(super::super::SsaValueClass::Int) => {
                (*dst, super::super::SsaValueClass::Int)
            }
            _ => return None,
        },
        RegionInstructionKind::Compare { dst, op, .. } => (
            *dst,
            if *op == RegionCompareOpCode::Spaceship {
                super::super::SsaValueClass::Int
            } else {
                super::super::SsaValueClass::Bool
            },
        ),
        RegionInstructionKind::Cast { dst, op, .. } => (
            *dst,
            match op {
                RegionCastOp::Bool => super::super::SsaValueClass::Bool,
                RegionCastOp::Int => super::super::SsaValueClass::Int,
                RegionCastOp::Void => super::super::SsaValueClass::Null,
                _ => return None,
            },
        ),
        _ => return None,
    };
    Some((dst, class))
}

fn operand_class(
    operand: RegionOperand,
    classes: &BTreeMap<php_ir::RegId, super::super::SsaValueClass>,
) -> Option<super::super::SsaValueClass> {
    match operand {
        RegionOperand::I64(_) => Some(super::super::SsaValueClass::Int),
        RegionOperand::Constant(value) if value == u32::MAX => {
            Some(super::super::SsaValueClass::Null)
        }
        RegionOperand::Constant(crate::JIT_VALUE_FALSE | crate::JIT_VALUE_TRUE) => {
            Some(super::super::SsaValueClass::Bool)
        }
        RegionOperand::Register(register) => classes.get(&register).copied(),
        RegionOperand::Local(_) | RegionOperand::Constant(_) => None,
    }
}

fn resolve(
    operand: RegionOperand,
    constants: &BTreeMap<php_ir::RegId, RegionOperand>,
) -> RegionOperand {
    match operand {
        RegionOperand::Register(register) => constants.get(&register).copied().unwrap_or(operand),
        _ => operand,
    }
}

const fn is_immediate(operand: RegionOperand) -> bool {
    matches!(operand, RegionOperand::I64(_) | RegionOperand::Constant(_))
}

fn fold_binary(
    op: RegionBinaryOp,
    lhs: RegionOperand,
    rhs: RegionOperand,
) -> Option<RegionOperand> {
    let (RegionOperand::I64(lhs), RegionOperand::I64(rhs)) = (lhs, rhs) else {
        return None;
    };
    let value = match op {
        RegionBinaryOp::Add => lhs.checked_add(rhs)?,
        RegionBinaryOp::Sub => lhs.checked_sub(rhs)?,
        RegionBinaryOp::Mul => lhs.checked_mul(rhs)?,
        RegionBinaryOp::BitAnd => lhs & rhs,
        RegionBinaryOp::BitOr => lhs | rhs,
        RegionBinaryOp::BitXor => lhs ^ rhs,
        RegionBinaryOp::ShiftLeft if (0..64).contains(&rhs) => lhs.wrapping_shl(rhs as u32),
        RegionBinaryOp::ShiftRight if (0..64).contains(&rhs) => lhs.wrapping_shr(rhs as u32),
        RegionBinaryOp::Div
        | RegionBinaryOp::Mod
        | RegionBinaryOp::Concat
        | RegionBinaryOp::Pow
        | RegionBinaryOp::ShiftLeft
        | RegionBinaryOp::ShiftRight => return None,
    };
    Some(RegionOperand::I64(value))
}

fn fold_unary(op: RegionUnaryOp, src: RegionOperand) -> Option<RegionOperand> {
    let RegionOperand::I64(src) = src else {
        return None;
    };
    match op {
        RegionUnaryOp::Plus => Some(RegionOperand::I64(src)),
        RegionUnaryOp::Minus => src.checked_neg().map(RegionOperand::I64),
        RegionUnaryOp::BitNot => Some(RegionOperand::I64(!src)),
        RegionUnaryOp::Not => Some(encoded_bool(src == 0)),
    }
}

fn fold_compare(
    op: RegionCompareOpCode,
    lhs: RegionOperand,
    rhs: RegionOperand,
) -> Option<RegionOperand> {
    let (RegionOperand::I64(lhs), RegionOperand::I64(rhs)) = (lhs, rhs) else {
        return None;
    };
    if op == RegionCompareOpCode::Spaceship {
        let ordering = match lhs.cmp(&rhs) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        };
        return Some(RegionOperand::I64(ordering));
    }
    let value = match op {
        RegionCompareOpCode::Equal | RegionCompareOpCode::Identical => lhs == rhs,
        RegionCompareOpCode::NotEqual | RegionCompareOpCode::NotIdentical => lhs != rhs,
        RegionCompareOpCode::Less => lhs < rhs,
        RegionCompareOpCode::LessEqual => lhs <= rhs,
        RegionCompareOpCode::Greater => lhs > rhs,
        RegionCompareOpCode::GreaterEqual => lhs >= rhs,
        RegionCompareOpCode::Spaceship => unreachable!(),
    };
    Some(encoded_bool(value))
}

fn fold_cast(op: RegionCastOp, src: RegionOperand) -> Option<RegionOperand> {
    match (op, src) {
        (RegionCastOp::Bool, RegionOperand::I64(value)) => Some(encoded_bool(value != 0)),
        (RegionCastOp::Int, RegionOperand::I64(value)) => Some(RegionOperand::I64(value)),
        (RegionCastOp::Void, _) => Some(RegionOperand::Constant(u32::MAX)),
        _ => None,
    }
}

const fn encoded_bool(value: bool) -> RegionOperand {
    RegionOperand::Constant(if value {
        crate::JIT_VALUE_TRUE
    } else {
        crate::JIT_VALUE_FALSE
    })
}

const fn immediate_truthy(value: RegionOperand) -> Option<bool> {
    match value {
        RegionOperand::I64(value) => Some(value != 0),
        RegionOperand::Constant(value) if value == u32::MAX || value == crate::JIT_VALUE_FALSE => {
            Some(false)
        }
        RegionOperand::Constant(value) if value == crate::JIT_VALUE_TRUE => Some(true),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use php_ir::{
        BinaryOp, FunctionFlags, InstructionKind, IrBuilder, IrConstant, IrSpan, Operand, UnitId,
    };

    use super::*;

    #[test]
    fn executable_optimizer_folds_scalar_chain_and_branch() {
        let mut builder = IrBuilder::new(UnitId::new(4_203));
        let file = builder.add_file("executable-opt.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("opt", FunctionFlags::default(), span);
        let entry = builder.append_block(function);
        let yes = builder.append_block(function);
        let no = builder.append_block(function);
        let one = builder.intern_constant(IrConstant::Int(1));
        let two = builder.intern_constant(IrConstant::Int(2));
        let sum = builder.alloc_register(function);
        builder.emit(
            function,
            entry,
            InstructionKind::Binary {
                dst: sum,
                op: BinaryOp::Add,
                lhs: Operand::Constant(one),
                rhs: Operand::Constant(two),
            },
            span,
        );
        builder.terminate_jump_if(function, entry, Operand::Register(sum), yes, no, span);
        builder.terminate_return(function, yes, Some(Operand::Register(sum)), span);
        builder.terminate_return(function, no, Some(Operand::Constant(one)), span);
        let unit = builder.finish();
        let mut region = crate::region_ir::build_baseline_region(&unit, function).expect("region");

        let report = optimize_executable_region(&mut region);

        assert_eq!(report.constants_folded, 1);
        assert_eq!(report.branches_folded, 1);
        assert!(matches!(
            region.blocks[0].instructions[0].kind,
            RegionInstructionKind::Move {
                src: RegionOperand::I64(3),
                ..
            }
        ));
        assert!(
            matches!(region.blocks[0].terminator, RegionTerminator::Jump { target } if target == yes)
        );
    }

    #[test]
    fn executable_licm_hoists_invariant_scalar_operation_to_preheader() {
        let mut builder = IrBuilder::new(UnitId::new(4_208));
        let file = builder.add_file("executable-licm.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("licm", FunctionFlags::default(), span);
        let entry = builder.append_block(function);
        let header = builder.append_block(function);
        let body = builder.append_block(function);
        let seven = builder.intern_constant(IrConstant::Int(7));
        let three = builder.intern_constant(IrConstant::Int(3));
        let seed = builder.alloc_register(function);
        builder.emit(
            function,
            entry,
            InstructionKind::Move {
                dst: seed,
                src: Operand::Constant(seven),
            },
            span,
        );
        builder.terminate_jump(function, entry, header, span);
        builder.terminate_jump(function, header, body, span);
        let masked = builder.alloc_register(function);
        builder.emit(
            function,
            body,
            InstructionKind::Binary {
                dst: masked,
                op: php_ir::BinaryOp::BitAnd,
                lhs: Operand::Register(seed),
                rhs: Operand::Constant(three),
            },
            span,
        );
        builder.emit(
            function,
            body,
            InstructionKind::Echo {
                src: Operand::Register(masked),
            },
            span,
        );
        builder.terminate_jump(function, body, header, span);
        let unit = builder.finish();
        let mut region = crate::region_ir::build_baseline_region(&unit, function).expect("region");

        let report = optimize_executable_region(&mut region);

        assert_eq!(report.loop_invariants_hoisted, 1);
        assert!(
            region.blocks[entry.index()]
                .instructions
                .iter()
                .any(|instruction| {
                    matches!(
                        instruction.kind,
                        RegionInstructionKind::Binary {
                            dst,
                            op: RegionBinaryOp::BitAnd,
                            ..
                        } if dst == masked
                    )
                })
        );
        assert!(!region.blocks[body.index()].instructions.iter().any(|instruction| {
            matches!(instruction.kind, RegionInstructionKind::Binary { dst, .. } if dst == masked)
        }));
    }

    #[test]
    fn executable_licm_pins_loops_with_exception_resume_points() {
        let mut builder = IrBuilder::new(UnitId::new(4_209));
        let file = builder.add_file("executable-licm-call.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("licm_call", FunctionFlags::default(), span);
        let entry = builder.append_block(function);
        let header = builder.append_block(function);
        let body = builder.append_block(function);
        let seven = builder.intern_constant(IrConstant::Int(7));
        let three = builder.intern_constant(IrConstant::Int(3));
        let seed = builder.alloc_register(function);
        builder.emit(
            function,
            entry,
            InstructionKind::Move {
                dst: seed,
                src: Operand::Constant(seven),
            },
            span,
        );
        builder.terminate_jump(function, entry, header, span);
        builder.terminate_jump(function, header, body, span);
        let call_result = builder.alloc_register(function);
        builder.emit(
            function,
            body,
            InstructionKind::CallFunction {
                dst: call_result,
                name: "may_throw".to_owned(),
                args: Vec::new(),
            },
            span,
        );
        builder.emit(
            function,
            body,
            InstructionKind::Discard {
                src: Operand::Register(call_result),
            },
            span,
        );
        let masked = builder.alloc_register(function);
        builder.emit(
            function,
            body,
            InstructionKind::Binary {
                dst: masked,
                op: BinaryOp::BitAnd,
                lhs: Operand::Register(seed),
                rhs: Operand::Constant(three),
            },
            span,
        );
        builder.emit(
            function,
            body,
            InstructionKind::Echo {
                src: Operand::Register(masked),
            },
            span,
        );
        builder.terminate_jump(function, body, header, span);
        let unit = builder.finish();
        let mut region = crate::region_ir::build_baseline_region(&unit, function).expect("region");

        let report = optimize_executable_region(&mut region);

        assert_eq!(report.loop_invariants_hoisted, 0);
        assert!(region.blocks[body.index()].instructions.iter().any(|instruction| {
            matches!(instruction.kind, RegionInstructionKind::Binary { dst, .. } if dst == masked)
        }));
    }
}
