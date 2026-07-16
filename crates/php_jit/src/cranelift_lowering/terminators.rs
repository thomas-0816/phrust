use super::*;

#[allow(clippy::too_many_arguments)]
fn lower_region_condition(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    locals: &BTreeMap<LocalId, Variable>,
    registers: &BTreeMap<RegId, Variable>,
    native_operations: NativeOperationFunctions,
    result_out: ir::Value,
    condition: RegionOperand,
    constants: &[IrConstant],
    value_flow: &ExecutableValueFlow,
) -> Result<ir::Value, CraneliftLoweringError> {
    let value = lower_region_operand(builder, locals, registers, condition)?;
    let fact = value_flow.operand_fact(constants, condition);
    match fact.class {
        SsaValueClass::Int if fact.certainty != crate::region_ir::SsaCertainty::Unknown => {
            return Ok(builder.ins().icmp_imm(IntCC::NotEqual, value, 0));
        }
        SsaValueClass::Null if fact.certainty != crate::region_ir::SsaCertainty::Unknown => {
            return Ok(builder.ins().icmp(IntCC::NotEqual, value, value));
        }
        SsaValueClass::Bool if fact.certainty != crate::region_ir::SsaCertainty::Unknown => {
            return Ok(builder.ins().icmp_imm(
                IntCC::Equal,
                value,
                crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
            ));
        }
        _ => {}
    }
    if let Some(helper) = native_operations.truthy {
        let slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
        let out = builder
            .ins()
            .stack_addr(module.target_config().pointer_type(), slot, 0);
        let context = builder.ins().iconst(types::I64, 0);
        let call = call_native_helper(module, builder, helper, &[context, value, out]);
        require_native_operation_ok(builder, builder.inst_results(call)[0], result_out)?;
        let truthy = builder.ins().stack_load(types::I64, slot, 0);
        Ok(builder.ins().icmp_imm(IntCC::NotEqual, truthy, 0))
    } else if builder.func.dfg.value_type(value) == types::I64 {
        Ok(builder.ins().icmp_imm(IntCC::NotEqual, value, 0))
    } else {
        Ok(value)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_region_terminator(
    builder: &mut FunctionBuilder<'_>,
    blocks: &[ir::Block],
    locals: &BTreeMap<LocalId, Variable>,
    registers: &BTreeMap<RegId, Variable>,
    result_out: ir::Value,
    pending_status: Variable,
    pending_value: Variable,
    module: &mut JITModule,
    native_operations: NativeOperationFunctions,
    function: FunctionId,
    return_check_required: bool,
    terminator: &RegionTerminator,
    constants: &[IrConstant],
    value_flow: &ExecutableValueFlow,
) -> Result<(), CraneliftLoweringError> {
    match terminator {
        RegionTerminator::Jump { target } => {
            builder.ins().jump(cranelift_block(blocks, *target)?, &[]);
        }
        RegionTerminator::JumpIfFalse {
            condition,
            target,
            fallthrough,
        } => {
            let condition = lower_region_condition(
                module,
                builder,
                locals,
                registers,
                native_operations,
                result_out,
                *condition,
                constants,
                value_flow,
            )?;
            let false_block = cranelift_block(blocks, *target)?;
            let true_block = cranelift_block(blocks, *fallthrough)?;
            builder
                .ins()
                .brif(condition, true_block, &[], false_block, &[]);
        }
        RegionTerminator::JumpIfTrue {
            condition,
            target,
            fallthrough,
        } => {
            let condition = lower_region_condition(
                module,
                builder,
                locals,
                registers,
                native_operations,
                result_out,
                *condition,
                constants,
                value_flow,
            )?;
            let true_block = cranelift_block(blocks, *target)?;
            let false_block = cranelift_block(blocks, *fallthrough)?;
            builder
                .ins()
                .brif(condition, true_block, &[], false_block, &[]);
        }
        RegionTerminator::JumpIf {
            condition,
            if_true,
            if_false,
        } => {
            let condition = lower_region_condition(
                module,
                builder,
                locals,
                registers,
                native_operations,
                result_out,
                *condition,
                constants,
                value_flow,
            )?;
            builder.ins().brif(
                condition,
                cranelift_block(blocks, *if_true)?,
                &[],
                cranelift_block(blocks, *if_false)?,
                &[],
            );
        }
        RegionTerminator::Return { value, finally } => {
            let value = lower_region_operand(builder, locals, registers, *value)?;
            let value = if return_check_required {
                let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
                lower_native_value_operation(
                    module,
                    builder,
                    native_operations.return_check,
                    0,
                    &[value, function_value],
                    result_out,
                )?
            } else {
                value
            };
            let status = builder
                .ins()
                .iconst(types::I32, i64::from(crate::JitCallStatus::RETURN.0));
            lower_region_frame_exit(
                builder,
                blocks,
                locals,
                result_out,
                pending_status,
                pending_value,
                value,
                status,
                *finally,
                module,
                native_operations,
                value_flow,
                function,
            )?;
        }
        RegionTerminator::ReturnReference { local, finally } => {
            let value = use_local_variable(builder, locals, *local)?;
            let status = builder.ins().iconst(
                types::I32,
                i64::from(crate::JitCallStatus::RETURN_REFERENCE.0),
            );
            lower_region_frame_exit(
                builder,
                blocks,
                locals,
                result_out,
                pending_status,
                pending_value,
                value,
                status,
                *finally,
                module,
                native_operations,
                value_flow,
                function,
            )?;
        }
        RegionTerminator::Exit { value, finally } => {
            let value = value
                .map(|value| lower_region_operand(builder, locals, registers, value))
                .transpose()?
                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
            let status = builder
                .ins()
                .iconst(types::I32, i64::from(crate::JitCallStatus::EXIT.0));
            lower_region_frame_exit(
                builder,
                blocks,
                locals,
                result_out,
                pending_status,
                pending_value,
                value,
                status,
                *finally,
                module,
                native_operations,
                value_flow,
                function,
            )?;
        }
        RegionTerminator::MissingLowering => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_MISSING_TERMINATOR_LOWERING",
                "terminator has no native lowering",
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn lower_region_frame_exit(
    builder: &mut FunctionBuilder<'_>,
    blocks: &[ir::Block],
    locals: &BTreeMap<LocalId, Variable>,
    result_out: ir::Value,
    pending_status: Variable,
    pending_value: Variable,
    value: ir::Value,
    status: ir::Value,
    finally: Option<BlockId>,
    module: &mut JITModule,
    native_operations: NativeOperationFunctions,
    value_flow: &ExecutableValueFlow,
    function: FunctionId,
) -> Result<(), CraneliftLoweringError> {
    if let Some(finally) = finally {
        builder.def_var(pending_status, status);
        builder.def_var(pending_value, value);
        builder.ins().jump(cranelift_block(blocks, finally)?, &[]);
    } else {
        lower_owned_frame_locals(
            module,
            builder,
            locals,
            native_operations,
            value_flow,
            function,
            result_out,
        )?;
        builder
            .ins()
            .store(MemFlagsData::new(), value, result_out, 0);
        builder.ins().return_(&[status]);
    }
    Ok(())
}

pub(super) fn lower_owned_frame_locals(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    locals: &BTreeMap<LocalId, Variable>,
    native_operations: NativeOperationFunctions,
    value_flow: &ExecutableValueFlow,
    function: FunctionId,
    result_out: ir::Value,
) -> Result<(), CraneliftLoweringError> {
    for (local, variable) in locals {
        let fact = value_flow.local_fact(*local);
        if value_flow.releases_local_at_frame_exit(*local)
            && fact.has_runtime_lifecycle()
            && fact.ownership == SsaOwnership::Owned
        {
            let value = builder.use_var(*variable);
            let _ = lower_native_value_operation(
                module,
                builder,
                native_operations.value_lifecycle,
                native_frame_cleanup_operation(function),
                &[value],
                result_out,
            )?;
        }
    }
    Ok(())
}
