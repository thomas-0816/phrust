use super::super::*;
use super::analyses::defined_registers;

/// Conservative constant folding for operations with no observable diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConstantFoldingPass;

impl OptimizerPass for ConstantFoldingPass {
    fn name(&self) -> &'static str {
        "constant_folding_safe_subset"
    }

    fn phase(&self) -> PassPhase {
        PassPhase::PreVerify
    }

    fn run(
        &self,
        transaction: &mut PassTransaction<'_>,
        _context: &PassContext,
    ) -> Result<PassReport, PassError> {
        let mut constants = transaction.unit().constants.clone();
        let mut stats = ConstantFoldingStats::default();

        for function_index in 0..transaction.unit().functions.len() {
            if !function_may_fold(&transaction.unit().functions[function_index]) {
                continue;
            }
            let mut touched_blocks = Vec::new();
            {
                let function = transaction.function_mut(function_index);
                for (block_index, block) in function.blocks.iter_mut().enumerate() {
                    let before_folded = stats.total_folded();
                    let mut known_constants = BTreeMap::<RegId, ConstId>::new();
                    for instruction in &mut block.instructions {
                        match &instruction.kind {
                            InstructionKind::LoadConst { dst, constant } => {
                                known_constants.insert(*dst, *constant);
                                continue;
                            }
                            InstructionKind::Binary { dst, op, lhs, rhs } => {
                                let dst = *dst;
                                known_constants.remove(&dst);
                                let lhs = resolve_constant(*lhs, &known_constants);
                                let rhs = resolve_constant(*rhs, &known_constants);
                                if let (Some(lhs), Some(rhs)) = (lhs, rhs) {
                                    match fold_binary(*op, lhs, rhs, &mut constants) {
                                        Some((constant, kind)) => {
                                            instruction.kind =
                                                InstructionKind::LoadConst { dst, constant };
                                            known_constants.insert(dst, constant);
                                            stats.record(kind);
                                        }
                                        None => stats.skipped_unsafe += 1,
                                    }
                                } else {
                                    stats.skipped_non_literal += 1;
                                }
                                continue;
                            }
                            InstructionKind::Compare { dst, op, lhs, rhs } => {
                                let dst = *dst;
                                known_constants.remove(&dst);
                                let lhs = resolve_constant(*lhs, &known_constants);
                                let rhs = resolve_constant(*rhs, &known_constants);
                                if let (Some(lhs), Some(rhs)) = (lhs, rhs) {
                                    match fold_compare(*op, lhs, rhs, &mut constants) {
                                        Some((constant, kind)) => {
                                            instruction.kind =
                                                InstructionKind::LoadConst { dst, constant };
                                            known_constants.insert(dst, constant);
                                            stats.record(kind);
                                        }
                                        None => stats.skipped_unsafe += 1,
                                    }
                                } else {
                                    stats.skipped_non_literal += 1;
                                }
                                continue;
                            }
                            InstructionKind::Unary { dst, op, src } => {
                                let dst = *dst;
                                known_constants.remove(&dst);
                                if let Some(src) = resolve_constant(*src, &known_constants) {
                                    match fold_unary(*op, src, &mut constants) {
                                        Some((constant, kind)) => {
                                            instruction.kind =
                                                InstructionKind::LoadConst { dst, constant };
                                            known_constants.insert(dst, constant);
                                            stats.record(kind);
                                        }
                                        None => stats.skipped_unsafe += 1,
                                    }
                                } else {
                                    stats.skipped_non_literal += 1;
                                }
                                continue;
                            }
                            _ => {}
                        }

                        for register in defined_registers(&instruction.kind) {
                            known_constants.remove(&register);
                        }
                    }
                    if stats.total_folded() > before_folded {
                        touched_blocks.push(block_index);
                    }
                }
            }
            for block_index in touched_blocks {
                transaction.touch_block(function_index, block_index);
            }
        }

        let total_folded = stats.total_folded();
        if total_folded > 0 {
            *transaction.constants_mut() = constants;
        }
        Ok(PassReport {
            name: self.name(),
            phase: self.phase(),
            enabled: true,
            changed: total_folded > 0,
            source_spans_preserved: true,
            rolled_back: false,
            scope: PassScopeReport::default(),
            stats: stats.into_report_stats(),
        })
    }
}

fn function_may_fold(function: &IrFunction) -> bool {
    function.blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            matches!(
                instruction.kind,
                InstructionKind::Binary { .. }
                    | InstructionKind::Compare { .. }
                    | InstructionKind::Unary { .. }
            )
        })
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FoldKind {
    IntegerBinary,
    BoolNot,
    StringConcat,
    LiteralCompare,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ConstantFoldingStats {
    integer_binary_folded: u64,
    bool_not_folded: u64,
    string_concat_folded: u64,
    literal_compare_folded: u64,
    skipped_non_literal: u64,
    skipped_unsafe: u64,
}

impl ConstantFoldingStats {
    fn record(&mut self, kind: FoldKind) {
        match kind {
            FoldKind::IntegerBinary => self.integer_binary_folded += 1,
            FoldKind::BoolNot => self.bool_not_folded += 1,
            FoldKind::StringConcat => self.string_concat_folded += 1,
            FoldKind::LiteralCompare => self.literal_compare_folded += 1,
        }
    }

    fn total_folded(&self) -> u64 {
        self.integer_binary_folded
            + self.bool_not_folded
            + self.string_concat_folded
            + self.literal_compare_folded
    }

    fn into_report_stats(self) -> BTreeMap<&'static str, u64> {
        BTreeMap::from([
            ("bool_not_folded", self.bool_not_folded),
            ("integer_binary_folded", self.integer_binary_folded),
            ("literal_compare_folded", self.literal_compare_folded),
            ("skipped_non_literal", self.skipped_non_literal),
            ("skipped_unsafe", self.skipped_unsafe),
            ("string_concat_folded", self.string_concat_folded),
            (
                "transformations_attempted",
                self.total_folded() + self.skipped_unsafe + self.skipped_non_literal,
            ),
            ("transformations_applied", self.total_folded()),
            (
                "transformations_skipped",
                self.skipped_unsafe + self.skipped_non_literal,
            ),
            ("total_folded", self.total_folded()),
        ])
    }
}

fn resolve_constant(
    operand: Operand,
    known_constants: &BTreeMap<RegId, ConstId>,
) -> Option<ConstId> {
    match operand {
        Operand::Constant(constant) => Some(constant),
        Operand::Register(register) => known_constants.get(&register).copied(),
        Operand::Local(_) => None,
    }
}

fn fold_binary(
    op: BinaryOp,
    lhs: ConstId,
    rhs: ConstId,
    constants: &mut Vec<IrConstant>,
) -> Option<(ConstId, FoldKind)> {
    let folded = match (op, constants.get(lhs.index())?, constants.get(rhs.index())?) {
        (BinaryOp::Add, IrConstant::Int(lhs), IrConstant::Int(rhs)) => {
            IrConstant::Int(lhs.checked_add(*rhs)?)
        }
        (BinaryOp::Sub, IrConstant::Int(lhs), IrConstant::Int(rhs)) => {
            IrConstant::Int(lhs.checked_sub(*rhs)?)
        }
        (BinaryOp::Mul, IrConstant::Int(lhs), IrConstant::Int(rhs)) => {
            IrConstant::Int(lhs.checked_mul(*rhs)?)
        }
        (BinaryOp::Concat, IrConstant::String(lhs), IrConstant::String(rhs)) => {
            let mut value = String::with_capacity(lhs.len().checked_add(rhs.len())?);
            value.push_str(lhs);
            value.push_str(rhs);
            let constant = append_constant(constants, IrConstant::String(value))?;
            return Some((constant, FoldKind::StringConcat));
        }
        _ => return None,
    };

    let constant = append_constant(constants, folded)?;
    Some((constant, FoldKind::IntegerBinary))
}

fn fold_unary(
    op: UnaryOp,
    src: ConstId,
    constants: &mut Vec<IrConstant>,
) -> Option<(ConstId, FoldKind)> {
    match (op, constants.get(src.index())?) {
        (UnaryOp::Not, IrConstant::Bool(value)) => {
            let constant = append_constant(constants, IrConstant::Bool(!value))?;
            Some((constant, FoldKind::BoolNot))
        }
        _ => None,
    }
}

fn fold_compare(
    op: CompareOp,
    lhs: ConstId,
    rhs: ConstId,
    constants: &mut Vec<IrConstant>,
) -> Option<(ConstId, FoldKind)> {
    let lhs = constants.get(lhs.index())?;
    let rhs = constants.get(rhs.index())?;
    let folded = match op {
        CompareOp::Identical => IrConstant::Bool(strict_literal_identity(lhs, rhs)?),
        CompareOp::NotIdentical => IrConstant::Bool(!strict_literal_identity(lhs, rhs)?),
        CompareOp::Equal => IrConstant::Bool(same_type_literal_equality(lhs, rhs)?),
        CompareOp::NotEqual => IrConstant::Bool(!same_type_literal_equality(lhs, rhs)?),
        CompareOp::Less => IrConstant::Bool(int_pair(lhs, rhs).map(|(lhs, rhs)| lhs < rhs)?),
        CompareOp::LessEqual => IrConstant::Bool(int_pair(lhs, rhs).map(|(lhs, rhs)| lhs <= rhs)?),
        CompareOp::Greater => IrConstant::Bool(int_pair(lhs, rhs).map(|(lhs, rhs)| lhs > rhs)?),
        CompareOp::GreaterEqual => {
            IrConstant::Bool(int_pair(lhs, rhs).map(|(lhs, rhs)| lhs >= rhs)?)
        }
        CompareOp::Spaceship => {
            let (lhs, rhs) = int_pair(lhs, rhs)?;
            IrConstant::Int(match lhs.cmp(&rhs) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            })
        }
    };
    let constant = append_constant(constants, folded)?;
    Some((constant, FoldKind::LiteralCompare))
}

fn strict_literal_identity(lhs: &IrConstant, rhs: &IrConstant) -> Option<bool> {
    match (lhs, rhs) {
        (IrConstant::Null, IrConstant::Null) => Some(true),
        (IrConstant::Bool(lhs), IrConstant::Bool(rhs)) => Some(lhs == rhs),
        (IrConstant::Int(lhs), IrConstant::Int(rhs)) => Some(lhs == rhs),
        (IrConstant::String(lhs), IrConstant::String(rhs)) => Some(lhs == rhs),
        (IrConstant::StringBytes(lhs), IrConstant::StringBytes(rhs)) => Some(lhs == rhs),
        (lhs, rhs) if strict_identity_scalar(lhs) && strict_identity_scalar(rhs) => Some(false),
        _ => None,
    }
}

fn strict_identity_scalar(value: &IrConstant) -> bool {
    matches!(
        value,
        IrConstant::Null
            | IrConstant::Bool(_)
            | IrConstant::Int(_)
            | IrConstant::String(_)
            | IrConstant::StringBytes(_)
    )
}

fn same_type_literal_equality(lhs: &IrConstant, rhs: &IrConstant) -> Option<bool> {
    match (lhs, rhs) {
        (IrConstant::Null, IrConstant::Null) => Some(true),
        (IrConstant::Bool(lhs), IrConstant::Bool(rhs)) => Some(lhs == rhs),
        (IrConstant::Int(lhs), IrConstant::Int(rhs)) => Some(lhs == rhs),
        _ => None,
    }
}

fn int_pair(lhs: &IrConstant, rhs: &IrConstant) -> Option<(i64, i64)> {
    match (lhs, rhs) {
        (IrConstant::Int(lhs), IrConstant::Int(rhs)) => Some((*lhs, *rhs)),
        _ => None,
    }
}

fn append_constant(constants: &mut Vec<IrConstant>, constant: IrConstant) -> Option<ConstId> {
    let index = u32::try_from(constants.len()).ok()?;
    constants.push(constant);
    Some(ConstId::new(index))
}
