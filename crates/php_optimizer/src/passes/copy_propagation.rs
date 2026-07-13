use super::super::*;
use super::analyses::defined_registers;

/// Block-local register copy propagation that never crosses local/reference state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CopyPropagationPass;

impl OptimizerPass for CopyPropagationPass {
    fn name(&self) -> &'static str {
        "copy_propagation_register_subset"
    }

    fn phase(&self) -> PassPhase {
        PassPhase::PreVerify
    }

    fn run(
        &self,
        transaction: &mut PassTransaction<'_>,
        _context: &PassContext,
    ) -> Result<PassReport, PassError> {
        let mut stats = CopyPropagationStats::default();

        for function_index in 0..transaction.unit().functions.len() {
            if !function_may_propagate_copies(&transaction.unit().functions[function_index]) {
                continue;
            }
            let mut touched_blocks = Vec::new();
            {
                let function = transaction.function_mut(function_index);
                for (block_index, block) in function.blocks.iter_mut().enumerate() {
                    let before_rewritten = stats.operands_rewritten;
                    let mut aliases = BTreeMap::<RegId, RegId>::new();
                    for instruction in &mut block.instructions {
                        let before_instruction = instruction.kind.clone();
                        rewrite_instruction_register_operands(&mut instruction.kind, &aliases);
                        if instruction.kind != before_instruction {
                            stats.operands_rewritten += 1;
                        }

                        for register in defined_registers(&instruction.kind) {
                            invalidate_aliases_touching(&mut aliases, register);
                        }

                        if let InstructionKind::Move {
                            dst,
                            src: Operand::Register(src),
                        } = instruction.kind
                        {
                            stats.moves_considered += 1;
                            if dst == src {
                                stats.skipped_self_move += 1;
                            } else {
                                aliases.insert(dst, resolve_register_alias(src, &aliases));
                                stats.aliases_recorded += 1;
                            }
                        }
                    }
                    if let Some(terminator) = &mut block.terminator {
                        let before_terminator = terminator.kind.clone();
                        rewrite_terminator_register_operands(&mut terminator.kind, &aliases);
                        if terminator.kind != before_terminator {
                            stats.operands_rewritten += 1;
                        }
                    }
                    if stats.operands_rewritten > before_rewritten {
                        touched_blocks.push(block_index);
                    }
                }
            }
            for block_index in touched_blocks {
                transaction.touch_block(function_index, block_index);
            }
        }

        Ok(PassReport {
            name: self.name(),
            phase: self.phase(),
            enabled: true,
            changed: stats.operands_rewritten > 0,
            source_spans_preserved: true,
            rolled_back: false,
            scope: PassScopeReport::default(),
            stats: stats.into_report_stats(),
        })
    }
}

fn function_may_propagate_copies(function: &IrFunction) -> bool {
    function.blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            matches!(
                instruction.kind,
                InstructionKind::Move {
                    src: Operand::Register(_),
                    ..
                }
            )
        })
    })
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct CopyPropagationStats {
    moves_considered: u64,
    aliases_recorded: u64,
    operands_rewritten: u64,
    skipped_self_move: u64,
}

impl CopyPropagationStats {
    fn into_report_stats(self) -> BTreeMap<&'static str, u64> {
        BTreeMap::from([
            ("aliases_recorded", self.aliases_recorded),
            ("moves_considered", self.moves_considered),
            ("operands_rewritten", self.operands_rewritten),
            ("skipped_self_move", self.skipped_self_move),
            ("transformations_attempted", self.moves_considered),
            ("transformations_applied", self.operands_rewritten),
            ("transformations_skipped", self.skipped_self_move),
        ])
    }
}

fn resolve_register_alias(register: RegId, aliases: &BTreeMap<RegId, RegId>) -> RegId {
    let mut current = register;
    for _ in 0..aliases.len() {
        let Some(next) = aliases.get(&current).copied() else {
            break;
        };
        if next == current {
            break;
        }
        current = next;
    }
    current
}

fn invalidate_aliases_touching(aliases: &mut BTreeMap<RegId, RegId>, register: RegId) {
    aliases.retain(|alias, source| *alias != register && *source != register);
}

fn rewrite_operand_registers(operand: &mut Operand, aliases: &BTreeMap<RegId, RegId>) {
    if let Operand::Register(register) = operand {
        *register = resolve_register_alias(*register, aliases);
    }
}

fn rewrite_optional_operand_registers(
    operand: &mut Option<Operand>,
    aliases: &BTreeMap<RegId, RegId>,
) {
    if let Some(operand) = operand {
        rewrite_operand_registers(operand, aliases);
    }
}

fn rewrite_operands_registers(operands: &mut [Operand], aliases: &BTreeMap<RegId, RegId>) {
    for operand in operands {
        rewrite_operand_registers(operand, aliases);
    }
}

fn rewrite_call_args_registers(
    args: &mut [php_ir::instruction::IrCallArg],
    aliases: &BTreeMap<RegId, RegId>,
) {
    for arg in args {
        rewrite_operand_registers(&mut arg.value, aliases);
        if let Some(dim) = &mut arg.by_ref_dim {
            rewrite_operands_registers(&mut dim.dims, aliases);
        }
        if let Some(property) = &mut arg.by_ref_property {
            rewrite_operand_registers(&mut property.object, aliases);
        }
        if let Some(property_dim) = &mut arg.by_ref_property_dim {
            rewrite_operand_registers(&mut property_dim.object, aliases);
            rewrite_operands_registers(&mut property_dim.dims, aliases);
        }
    }
}

fn rewrite_instruction_register_operands(
    kind: &mut InstructionKind,
    aliases: &BTreeMap<RegId, RegId>,
) {
    match kind {
        InstructionKind::Move { src, .. }
        | InstructionKind::RegisterConstant { value: src, .. }
        | InstructionKind::StoreLocal { src, .. }
        | InstructionKind::InitStaticLocal { default: src, .. }
        | InstructionKind::Discard { src }
        | InstructionKind::Echo { src }
        | InstructionKind::YieldFrom { source: src, .. }
        | InstructionKind::Throw { value: src }
        | InstructionKind::Include { path: src, .. }
        | InstructionKind::Eval { code: src, .. }
        | InstructionKind::DynamicNewObject {
            class_name: src, ..
        }
        | InstructionKind::UnsetProperty { object: src, .. }
        | InstructionKind::AcquireCallable { value: src, .. }
        | InstructionKind::ForeachInit { source: src, .. } => {
            rewrite_operand_registers(src, aliases)
        }
        InstructionKind::UnsetPropertyDim { object, dims, .. } => {
            rewrite_operand_registers(object, aliases);
            rewrite_operands_registers(dims, aliases);
        }
        InstructionKind::UnsetDynamicProperty { object, property } => {
            rewrite_operand_registers(object, aliases);
            rewrite_operand_registers(property, aliases);
        }
        InstructionKind::Binary { lhs, rhs, .. }
        | InstructionKind::Compare { lhs, rhs, .. }
        | InstructionKind::DynamicInstanceOf {
            object: lhs,
            target: rhs,
            ..
        } => {
            rewrite_operand_registers(lhs, aliases);
            rewrite_operand_registers(rhs, aliases);
        }
        InstructionKind::InstanceOf { object, .. }
        | InstructionKind::Unary { src: object, .. }
        | InstructionKind::Cast { src: object, .. }
        | InstructionKind::CloneObject { object, .. }
        | InstructionKind::FetchProperty { object, .. }
        | InstructionKind::IssetProperty { object, .. }
        | InstructionKind::EmptyProperty { object, .. } => {
            rewrite_operand_registers(object, aliases);
        }
        InstructionKind::BindReferenceProperty { object, .. } => {
            rewrite_operand_registers(object, aliases);
        }
        InstructionKind::FetchDynamicProperty {
            object, property, ..
        }
        | InstructionKind::IssetDynamicProperty {
            object, property, ..
        }
        | InstructionKind::EmptyDynamicProperty {
            object, property, ..
        } => {
            rewrite_operand_registers(object, aliases);
            rewrite_operand_registers(property, aliases);
        }
        InstructionKind::IssetDynamicPropertyDim {
            object,
            property,
            dims,
            ..
        }
        | InstructionKind::EmptyDynamicPropertyDim {
            object,
            property,
            dims,
            ..
        } => {
            rewrite_operand_registers(object, aliases);
            rewrite_operand_registers(property, aliases);
            rewrite_operands_registers(dims, aliases);
        }
        InstructionKind::Yield { key, value, .. } => {
            rewrite_optional_operand_registers(key, aliases);
            rewrite_optional_operand_registers(value, aliases);
        }
        InstructionKind::MakeException { message, .. } => {
            rewrite_operand_registers(message, aliases);
        }
        InstructionKind::MakeClosure { captures, .. } => {
            for capture in captures {
                rewrite_operand_registers(&mut capture.src, aliases);
            }
        }
        InstructionKind::CallFunction { args, .. }
        | InstructionKind::CallStaticMethod { args, .. }
        | InstructionKind::NewObject { args, .. }
        | InstructionKind::BindReferenceFromCall { args, .. } => {
            rewrite_call_args_registers(args, aliases);
        }
        InstructionKind::BindReferenceFromMethodCall { object, args, .. } => {
            rewrite_operand_registers(object, aliases);
            rewrite_call_args_registers(args, aliases);
        }
        InstructionKind::BindReferenceFromProperty { object, .. } => {
            rewrite_operand_registers(object, aliases);
        }
        InstructionKind::BindReferenceFromPropertyDim { object, dims, .. } => {
            rewrite_operand_registers(object, aliases);
            rewrite_operands_registers(dims, aliases);
        }
        InstructionKind::CallMethod { object, args, .. } => {
            rewrite_operand_registers(object, aliases);
            rewrite_call_args_registers(args, aliases);
        }
        InstructionKind::CallClosure { callee, args, .. }
        | InstructionKind::CallCallable { callee, args, .. } => {
            rewrite_operand_registers(callee, aliases);
            rewrite_call_args_registers(args, aliases);
        }
        InstructionKind::Pipe {
            input, callable, ..
        } => {
            rewrite_operand_registers(input, aliases);
            rewrite_operand_registers(callable, aliases);
        }
        InstructionKind::CloneWith {
            object,
            replacements,
            ..
        }
        | InstructionKind::AssignProperty {
            object,
            value: replacements,
            ..
        } => {
            rewrite_operand_registers(object, aliases);
            rewrite_operand_registers(replacements, aliases);
        }
        InstructionKind::AssignPropertyDim {
            object,
            dims,
            value,
            ..
        } => {
            rewrite_operand_registers(object, aliases);
            rewrite_operands_registers(dims, aliases);
            rewrite_operand_registers(value, aliases);
        }
        InstructionKind::AssignDynamicProperty {
            object,
            property,
            value,
            ..
        } => {
            rewrite_operand_registers(object, aliases);
            rewrite_operand_registers(property, aliases);
            rewrite_operand_registers(value, aliases);
        }
        InstructionKind::IssetPropertyDim { object, dims, .. }
        | InstructionKind::EmptyPropertyDim { object, dims, .. }
        | InstructionKind::BindReferencePropertyDim { object, dims, .. } => {
            rewrite_operand_registers(object, aliases);
            rewrite_operands_registers(dims, aliases);
        }
        InstructionKind::BindReferenceDimFromProperty { object, dims, .. } => {
            rewrite_operand_registers(object, aliases);
            rewrite_operands_registers(dims, aliases);
        }
        InstructionKind::AssignDim { dims, value, .. }
        | InstructionKind::AppendDim { dims, value, .. } => {
            rewrite_operands_registers(dims, aliases);
            rewrite_operand_registers(value, aliases);
        }
        InstructionKind::ArrayInsert { key, value, .. } => {
            rewrite_optional_operand_registers(key, aliases);
            rewrite_operand_registers(value, aliases);
        }
        InstructionKind::ArraySpread { source, .. } => {
            rewrite_operand_registers(source, aliases);
        }
        InstructionKind::FetchDim { array, key, .. } => {
            rewrite_operand_registers(array, aliases);
            rewrite_operand_registers(key, aliases);
        }
        InstructionKind::IssetDim { dims, .. }
        | InstructionKind::EmptyDim { dims, .. }
        | InstructionKind::UnsetDim { dims, .. }
        | InstructionKind::IssetStaticPropertyDim { dims, .. }
        | InstructionKind::EmptyStaticPropertyDim { dims, .. }
        | InstructionKind::UnsetStaticPropertyDim { dims, .. }
        | InstructionKind::BindReferenceDim { dims, .. }
        | InstructionKind::BindReferenceFromDim { dims, .. }
        | InstructionKind::BindReferenceFromStaticPropertyDim { dims, .. } => {
            rewrite_operands_registers(dims, aliases);
        }
        InstructionKind::AssignStaticProperty { value, .. } => {
            rewrite_operand_registers(value, aliases);
        }
        InstructionKind::FetchDynamicStaticProperty { class_name, .. } => {
            rewrite_operand_registers(class_name, aliases);
        }
        InstructionKind::AssignDynamicStaticProperty {
            class_name, value, ..
        } => {
            rewrite_operand_registers(class_name, aliases);
            rewrite_operand_registers(value, aliases);
        }
        InstructionKind::FetchObjectClassName { object, .. } => {
            rewrite_operand_registers(object, aliases);
        }
        InstructionKind::ArrayGet { array, index, .. } => {
            rewrite_operand_registers(array, aliases);
            rewrite_operand_registers(index, aliases);
        }
        InstructionKind::Nop
        | InstructionKind::LoadConst { .. }
        | InstructionKind::FetchConst { .. }
        | InstructionKind::LoadLocal { .. }
        | InstructionKind::LoadLocalQuiet { .. }
        | InstructionKind::DeclareFunction { .. }
        | InstructionKind::DeclareClass { .. }
        | InstructionKind::BindReference { .. }
        | InstructionKind::BindGlobal { .. }
        | InstructionKind::BindReferenceStaticProperty { .. }
        | InstructionKind::EmitDiagnostic { .. }
        | InstructionKind::EnterTry { .. }
        | InstructionKind::LeaveTry
        | InstructionKind::EndFinally { .. }
        | InstructionKind::ResolveCallable { .. }
        | InstructionKind::FetchStaticProperty { .. }
        | InstructionKind::IssetStaticProperty { .. }
        | InstructionKind::EmptyStaticProperty { .. }
        | InstructionKind::FetchClassConstant { .. }
        | InstructionKind::NewArray { .. }
        | InstructionKind::IssetLocal { .. }
        | InstructionKind::EmptyLocal { .. }
        | InstructionKind::UnsetLocal { .. }
        | InstructionKind::ForeachNext { .. }
        | InstructionKind::ForeachCleanup { .. }
        | InstructionKind::ForeachInitRef { .. }
        | InstructionKind::ForeachNextRef { .. }
        | InstructionKind::Unsupported { .. }
        | InstructionKind::RuntimeError { .. } => {}
    }
}

fn rewrite_terminator_register_operands(
    kind: &mut TerminatorKind,
    aliases: &BTreeMap<RegId, RegId>,
) {
    match kind {
        TerminatorKind::Jump { .. } => {}
        TerminatorKind::JumpIfFalse { condition, .. }
        | TerminatorKind::JumpIfTrue { condition, .. }
        | TerminatorKind::JumpIf { condition, .. } => {
            rewrite_operand_registers(condition, aliases);
        }
        TerminatorKind::Return { value, .. } => {
            rewrite_optional_operand_registers(value, aliases);
        }
        TerminatorKind::Exit { value } => {
            rewrite_optional_operand_registers(value, aliases);
        }
    }
}
