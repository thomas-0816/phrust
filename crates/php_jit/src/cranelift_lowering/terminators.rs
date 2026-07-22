use super::*;

#[derive(Clone, Copy)]
struct NativeOptimizingTerminatorTransition<'a> {
    result_out: ir::Value,
    deopt_out: ir::Value,
    function: FunctionId,
    local_count: u32,
    continuation_id: u32,
    live_locals: &'a [LocalId],
    locals: &'a NativeLocalMap,
    registers: &'a NativeRegisterMap,
    live_registers: &'a [RegId],
    native_version: u32,
    value_release_validate: ir::FuncRef,
    value_release_commit: ir::FuncRef,
    emitted_transition: &'a Cell<bool>,
}

impl NativeOptimizingTerminatorTransition<'_> {
    fn emit(self, builder: &mut FunctionBuilder<'_>) -> Result<(), CraneliftLoweringError> {
        self.emitted_transition.set(true);
        publish_native_continuation_state(
            builder,
            self.deopt_out,
            self.function,
            self.local_count,
            self.continuation_id,
            self.live_locals,
            self.locals,
            self.native_version,
        )?;
        publish_native_register_state(
            builder,
            self.deopt_out,
            self.registers,
            self.live_registers,
        )?;
        let value = builder
            .ins()
            .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
        builder
            .ins()
            .store(MemFlagsData::new(), value, self.result_out, 0);
        let status = builder.ins().iconst(
            types::I32,
            i64::from(crate::JitCallStatus::RECOMPILE_REQUESTED.0),
        );
        builder.ins().return_(&[status]);
        let unreachable = builder.create_block();
        builder.switch_to_block(unreachable);
        builder.seal_block(unreachable);
        Ok(())
    }
}

fn lower_optimizing_terminator_reference_local(
    builder: &mut FunctionBuilder<'_>,
    local: ir::Value,
    transition: NativeOptimizingTerminatorTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let inspect = builder.create_block();
    let direct = builder.create_block();
    let plain = builder.create_block();
    let rejected = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);

    let reference = lower_value_has_tag(builder, local, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
    builder.ins().brif(reference, inspect, &[], plain, &[]);

    builder.switch_to_block(plain);
    builder.ins().jump(merge, &[local.into()]);

    builder.switch_to_block(inspect);
    let slot = lower_optimizing_slot_address(builder, local, transition.deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let state = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    let direct_kind = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR),
    );
    let version = builder.ins().icmp_imm(
        IntCC::Equal,
        flags,
        i64::from(crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION),
    );
    let published = builder.ins().icmp_imm(
        IntCC::NotEqual,
        state,
        i64::from(crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY),
    );
    let admitted = builder.ins().band(direct_kind, version);
    let admitted = builder.ins().band(admitted, published);
    builder.ins().brif(admitted, direct, &[], rejected, &[]);

    builder.switch_to_block(direct);
    let value = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(rejected);
    transition.emit(builder)?;
    let placeholder = builder
        .ins()
        .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
    builder.ins().jump(merge, &[placeholder.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

fn lower_optimizing_frame_cleanup(
    builder: &mut FunctionBuilder<'_>,
    cleanup: &[(LocalId, ir::Value)],
    transition: NativeOptimizingTerminatorTransition<'_>,
) -> Result<(), CraneliftLoweringError> {
    if cleanup.is_empty() {
        return Ok(());
    }
    let mut safe = builder.ins().iconst(types::I8, 1);
    for (_, value) in cleanup {
        let validate = builder.ins().call(
            transition.value_release_validate,
            &[transition.deopt_out, *value],
        );
        let releasable = builder.inst_results(validate)[0];
        safe = builder.ins().band(safe, releasable);
    }
    let release = builder.create_block();
    let rejected = builder.create_block();
    builder.ins().brif(safe, release, &[], rejected, &[]);

    builder.switch_to_block(rejected);
    transition.emit(builder)?;
    builder.ins().jump(release, &[]);

    builder.switch_to_block(release);
    for (_, value) in cleanup {
        let _ = builder.ins().call(
            transition.value_release_commit,
            &[transition.deopt_out, *value],
        );
    }
    Ok(())
}

fn lower_optimizing_condition(
    builder: &mut FunctionBuilder<'_>,
    condition: RegionOperand,
    locals: &NativeLocalMap,
    registers: &NativeRegisterMap,
    constants: &[IrConstant],
    value_flow: &ExecutableValueFlow,
    transition: NativeOptimizingTerminatorTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let value = lower_region_operand(builder, locals, registers, condition)?;
    let value = if let RegionOperand::Local(local) = condition
        && value_flow.local_storage(local).is_reference_slot()
    {
        lower_optimizing_terminator_reference_local(builder, value, transition)?
    } else {
        value
    };
    let fact = value_flow.operand_fact(constants, condition);
    if let Some(truthy) = scalar_truthy(builder, value, fact.class)
        && fact.certainty != crate::region_ir::SsaCertainty::Unknown
    {
        return Ok(truthy);
    }

    let inspect_runtime = builder.create_block();
    let inspect_non_runtime = builder.create_block();
    let inspect_descriptor = builder.create_block();
    let rejected = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I8);

    let is_true = builder.ins().icmp_imm(
        IntCC::Equal,
        value,
        crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
    );
    let is_false = builder.ins().icmp_imm(
        IntCC::Equal,
        value,
        crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
    );
    let is_null = builder
        .ins()
        .icmp_imm(IntCC::Equal, value, crate::jit_encode_constant(u32::MAX));
    let is_uninitialized = builder.ins().icmp_imm(
        IntCC::Equal,
        value,
        crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
    );
    let false_lane = builder.ins().bor(is_false, is_null);
    let false_lane = builder.ins().bor(false_lane, is_uninitialized);
    let reserved = builder.ins().bor(is_true, false_lane);
    let runtime = lower_is_runtime_handle(builder, value);
    let constant = lower_value_has_namespace_tag(builder, value, crate::JIT_VALUE_CONSTANT_TAG);
    let not_reserved = builder.ins().icmp_imm(IntCC::Equal, reserved, 0);
    let opaque_constant = builder.ins().band(constant, not_reserved);
    let integer_truthy = builder.ins().icmp_imm(IntCC::NotEqual, value, 0);
    let direct_truthy = builder.ins().select(reserved, is_true, integer_truthy);
    builder
        .ins()
        .brif(runtime, inspect_runtime, &[], inspect_non_runtime, &[]);

    builder.switch_to_block(inspect_non_runtime);
    builder.ins().brif(
        opaque_constant,
        rejected,
        &[],
        merge,
        &[direct_truthy.into()],
    );

    builder.switch_to_block(inspect_runtime);
    let runtime_kind = builder
        .ins()
        .band_imm(value, crate::JIT_VALUE_RUNTIME_KIND_MASK as i64);
    let is_array = builder.ins().icmp_imm(
        IntCC::Equal,
        runtime_kind,
        crate::JIT_VALUE_RUNTIME_ARRAY_TAG as i64,
    );
    let is_string = builder.ins().icmp_imm(
        IntCC::Equal,
        runtime_kind,
        crate::JIT_VALUE_RUNTIME_STRING_TAG as i64,
    );
    let has_descriptor = builder.ins().bor(is_array, is_string);
    builder
        .ins()
        .brif(has_descriptor, inspect_descriptor, &[], rejected, &[]);

    builder.switch_to_block(inspect_descriptor);
    let descriptor = lower_optimizing_slot_address(builder, value, transition.deopt_out);
    let reserved_value = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let non_empty = builder.ins().icmp_imm(IntCC::NotEqual, length, 0);
    let zero_flag = builder.ins().band_imm(
        reserved_value,
        i64::from(crate::JIT_NATIVE_STRING_VALUE_ZERO),
    );
    let zero_string = builder.ins().icmp_imm(IntCC::NotEqual, zero_flag, 0);
    let not_zero_string = builder.ins().icmp_imm(IntCC::Equal, zero_string, 0);
    let string_truthy = builder.ins().band(non_empty, not_zero_string);
    let runtime_truthy = builder.ins().select(is_string, string_truthy, non_empty);
    builder.ins().jump(merge, &[runtime_truthy.into()]);

    builder.switch_to_block(rejected);
    transition.emit(builder)?;
    let false_value = builder.ins().iconst(types::I8, 0);
    builder.ins().jump(merge, &[false_value.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

#[allow(clippy::too_many_arguments)]
fn lower_region_condition(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    locals: &NativeLocalMap,
    registers: &NativeRegisterMap,
    native_operations: NativeOperationFunctions,
    deopt_out: ir::Value,
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
        lower_guarded_unknown_condition(module, builder, helper, value, deopt_out)
    } else if builder.func.dfg.value_type(value) == types::I64 {
        Ok(builder.ins().icmp_imm(IntCC::NotEqual, value, 0))
    } else {
        Ok(value)
    }
}

/// Resolve the stable null/bool/int lanes without crossing the runtime ABI.
/// Runtime handles and opaque constant-pool handles retain the typed helper
/// slow path.
pub(super) fn lower_guarded_unknown_condition(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: NativeHelper,
    value: ir::Value,
    deopt_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    if !helper.inline_runtime_view {
        let slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
        let out = builder
            .ins()
            .stack_addr(module.target_config().pointer_type(), slot, 0);
        let call = call_native_helper(module, builder, helper, &[value, out]);
        require_native_operation_ok(
            builder,
            builder.inst_results(call)[0],
            helper.terminal_exit()?,
        )?;
        let truthy = builder.ins().stack_load(types::I64, slot, 0);
        return Ok(builder.ins().icmp_imm(IntCC::NotEqual, truthy, 0));
    }
    let inspect_runtime = builder.create_block();
    let inspect_non_runtime = builder.create_block();
    let inspect_descriptor = builder.create_block();
    let slow = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I8);

    let is_true = builder.ins().icmp_imm(
        IntCC::Equal,
        value,
        crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
    );
    let is_false = builder.ins().icmp_imm(
        IntCC::Equal,
        value,
        crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
    );
    let is_null = builder
        .ins()
        .icmp_imm(IntCC::Equal, value, crate::jit_encode_constant(u32::MAX));
    let is_uninitialized = builder.ins().icmp_imm(
        IntCC::Equal,
        value,
        crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
    );
    let is_false_lane = builder.ins().bor(is_false, is_null);
    let is_false_lane = builder.ins().bor(is_false_lane, is_uninitialized);
    let is_reserved = builder.ins().bor(is_true, is_false_lane);
    let tag = builder
        .ins()
        .band_imm(value, crate::JIT_VALUE_TAG_MASK as i64);
    let is_runtime = builder
        .ins()
        .icmp_imm(IntCC::Equal, tag, crate::JIT_VALUE_RUNTIME_TAG as i64);
    let is_constant =
        builder
            .ins()
            .icmp_imm(IntCC::Equal, tag, crate::JIT_VALUE_CONSTANT_TAG as i64);
    let is_not_reserved = builder.ins().icmp_imm(IntCC::Equal, is_reserved, 0);
    let is_opaque_constant = builder.ins().band(is_constant, is_not_reserved);
    let integer_truthy = builder.ins().icmp_imm(IntCC::NotEqual, value, 0);
    let direct_truthy = builder.ins().select(is_reserved, is_true, integer_truthy);
    builder
        .ins()
        .brif(is_runtime, inspect_runtime, &[], inspect_non_runtime, &[]);

    builder.switch_to_block(inspect_non_runtime);
    builder.ins().brif(
        is_opaque_constant,
        slow,
        &[],
        merge,
        &[direct_truthy.into()],
    );

    builder.switch_to_block(inspect_runtime);
    let runtime_kind = builder
        .ins()
        .band_imm(value, crate::JIT_VALUE_RUNTIME_KIND_MASK as i64);
    let is_array = builder.ins().icmp_imm(
        IntCC::Equal,
        runtime_kind,
        crate::JIT_VALUE_RUNTIME_ARRAY_TAG as i64,
    );
    let is_string = builder.ins().icmp_imm(
        IntCC::Equal,
        runtime_kind,
        crate::JIT_VALUE_RUNTIME_STRING_TAG as i64,
    );
    let has_direct_descriptor = builder.ins().bor(is_array, is_string);
    builder
        .ins()
        .brif(has_direct_descriptor, inspect_descriptor, &[], slow, &[]);

    builder.switch_to_block(inspect_descriptor);
    let descriptor = lower_optimizing_slot_address(builder, value, deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let reserved = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let array_kind = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_ARRAY),
    );
    let array_version = builder.ins().icmp_imm(
        IntCC::Equal,
        flags,
        i64::from(crate::JIT_NATIVE_ARRAY_VIEW_ABI_VERSION),
    );
    let array_descriptor = builder.ins().band(array_kind, array_version);
    let direct_array_kind = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY),
    );
    let direct_array_version = builder.ins().icmp_imm(
        IntCC::Equal,
        flags,
        i64::from(crate::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION),
    );
    let direct_array_descriptor = builder.ins().band(direct_array_kind, direct_array_version);
    let array_descriptor = builder.ins().bor(array_descriptor, direct_array_descriptor);
    let array_ok = builder.ins().band(is_array, array_descriptor);
    let string_kind = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_STRING),
    );
    let string_version = builder.ins().icmp_imm(
        IntCC::Equal,
        flags,
        i64::from(crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION),
    );
    let string_descriptor = builder.ins().band(string_kind, string_version);
    let string_ok = builder.ins().band(is_string, string_descriptor);
    let descriptor_ok = builder.ins().bor(array_ok, string_ok);
    let non_empty = builder.ins().icmp_imm(IntCC::NotEqual, length, 0);
    let zero_flag = builder
        .ins()
        .band_imm(reserved, i64::from(crate::JIT_NATIVE_STRING_VALUE_ZERO));
    let is_zero_string = builder.ins().icmp_imm(IntCC::NotEqual, zero_flag, 0);
    let not_zero_string = builder.ins().icmp_imm(IntCC::Equal, is_zero_string, 0);
    let string_truthy = builder.ins().band(non_empty, not_zero_string);
    let runtime_truthy = builder.ins().select(is_string, string_truthy, non_empty);
    builder
        .ins()
        .brif(descriptor_ok, merge, &[runtime_truthy.into()], slow, &[]);

    builder.switch_to_block(slow);
    let slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    let out = builder
        .ins()
        .stack_addr(module.target_config().pointer_type(), slot, 0);
    let call = call_native_helper(module, builder, helper, &[value, out]);
    require_native_operation_ok(
        builder,
        builder.inst_results(call)[0],
        helper.terminal_exit()?,
    )?;
    let truthy = builder.ins().stack_load(types::I64, slot, 0);
    let truthy = builder.ins().icmp_imm(IntCC::NotEqual, truthy, 0);
    builder.ins().jump(merge, &[truthy.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_region_terminator(
    builder: &mut FunctionBuilder<'_>,
    blocks: &BTreeMap<BlockId, ir::Block>,
    locals: &NativeLocalMap,
    registers: &NativeRegisterMap,
    result_out: ir::Value,
    deopt_out: ir::Value,
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
                deopt_out,
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
                deopt_out,
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
                deopt_out,
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
            let operand = *value;
            let value = lower_region_operand(builder, locals, registers, operand)?;
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
                let fact = lowering_operand_fact(value_flow, constants, operand);
                if fact.ownership == SsaOwnership::Borrowed {
                    lower_guarded_value_release(
                        module,
                        builder,
                        native_operations.value_release,
                        native_dim_operation(0, function, 0),
                        value,
                        result_out,
                        deopt_out,
                    )?
                } else {
                    value
                }
            };
            let status = builder
                .ins()
                .iconst(types::I32, i64::from(crate::JitCallStatus::RETURN.0));
            lower_region_frame_exit(
                builder,
                blocks,
                locals,
                result_out,
                deopt_out,
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
            let mut value = use_local_variable(builder, locals, *local)?;
            if value_flow.local_fact(*local).ownership == SsaOwnership::Borrowed {
                value = lower_guarded_value_release(
                    module,
                    builder,
                    native_operations.value_release,
                    native_dim_operation(0, function, 0),
                    value,
                    result_out,
                    deopt_out,
                )?;
            }
            let status = builder.ins().iconst(
                types::I32,
                i64::from(crate::JitCallStatus::RETURN_REFERENCE.0),
            );
            lower_region_frame_exit(
                builder,
                blocks,
                locals,
                result_out,
                deopt_out,
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
            let value = if let Some(operand) = *value {
                let value = lower_region_operand(builder, locals, registers, operand)?;
                let fact = lowering_operand_fact(value_flow, constants, operand);
                if fact.ownership == SsaOwnership::Borrowed {
                    lower_guarded_value_release(
                        module,
                        builder,
                        native_operations.value_release,
                        native_dim_operation(0, function, 0),
                        value,
                        result_out,
                        deopt_out,
                    )?
                } else {
                    value
                }
            } else {
                builder.ins().iconst(types::I64, 0)
            };
            let status = builder
                .ins()
                .iconst(types::I32, i64::from(crate::JitCallStatus::EXIT.0));
            lower_region_frame_exit(
                builder,
                blocks,
                locals,
                result_out,
                deopt_out,
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
    }
    Ok(())
}

/// Terminators admitted to the optimizing tier have no runtime cleanup,
/// return coercion, or unknown truthiness.  Keeping this emitter separate from
/// `lower_region_terminator` prevents a future baseline helper branch from
/// silently becoming reachable by optimized code.
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_optimizing_region_terminator(
    builder: &mut FunctionBuilder<'_>,
    blocks: &BTreeMap<BlockId, ir::Block>,
    locals: &NativeLocalMap,
    registers: &NativeRegisterMap,
    result_out: ir::Value,
    deopt_out: ir::Value,
    function: FunctionId,
    local_count: u32,
    continuation_id: u32,
    live_locals: &[LocalId],
    live_registers: &[RegId],
    native_version: u32,
    value_release_validate: ir::FuncRef,
    value_release_commit: ir::FuncRef,
    return_type: Option<&php_ir::IrReturnType>,
    terminator: &RegionTerminator,
    constants: &[IrConstant],
    value_flow: &ExecutableValueFlow,
) -> Result<EmittedOptimizingInstruction, CraneliftLoweringError> {
    let emitted_transition = Cell::new(false);
    let operation_local_transition = Cell::new(false);
    let transition = NativeOptimizingTerminatorTransition {
        result_out,
        deopt_out,
        function,
        local_count,
        continuation_id,
        live_locals,
        locals,
        registers,
        live_registers,
        native_version,
        value_release_validate,
        value_release_commit,
        emitted_transition: &emitted_transition,
    };
    let direct_condition = |builder: &mut FunctionBuilder<'_>, condition: RegionOperand| {
        let condition = lower_optimizing_condition(
            builder, condition, locals, registers, constants, value_flow, transition,
        )?;
        if emitted_transition.get() {
            operation_local_transition.set(true);
        }
        Ok(condition)
    };
    let frame_cleanup_locals = locals
        .keys()
        .copied()
        .filter(|local| {
            let fact = value_flow.local_fact(*local);
            let bound_reference_owner = value_flow.local_storage(*local)
                == crate::region_ir::LocalStorageClass::MemoryReference;
            value_flow.releases_local_at_frame_exit(*local)
                && (bound_reference_owner
                    || (fact.has_runtime_lifecycle() && fact.ownership == SsaOwnership::Owned))
        })
        .collect::<Vec<_>>();
    match terminator {
        RegionTerminator::Jump { target } => {
            builder.ins().jump(cranelift_block(blocks, *target)?, &[]);
        }
        RegionTerminator::JumpIfFalse {
            condition,
            target,
            fallthrough,
        } => {
            let condition = direct_condition(builder, *condition)?;
            builder.ins().brif(
                condition,
                cranelift_block(blocks, *fallthrough)?,
                &[],
                cranelift_block(blocks, *target)?,
                &[],
            );
        }
        RegionTerminator::JumpIfTrue {
            condition,
            target,
            fallthrough,
        } => {
            let condition = direct_condition(builder, *condition)?;
            builder.ins().brif(
                condition,
                cranelift_block(blocks, *target)?,
                &[],
                cranelift_block(blocks, *fallthrough)?,
                &[],
            );
        }
        RegionTerminator::JumpIf {
            condition,
            if_true,
            if_false,
        } => {
            let condition = direct_condition(builder, *condition)?;
            builder.ins().brif(
                condition,
                cranelift_block(blocks, *if_true)?,
                &[],
                cranelift_block(blocks, *if_false)?,
                &[],
            );
        }
        RegionTerminator::Return {
            value,
            finally: None,
        } => {
            let return_check_required = return_type.is_some_and(|return_type| {
                !optimizing_fact_satisfies_type(
                    lowering_operand_fact(value_flow, constants, *value),
                    return_type,
                )
            });
            let fact = lowering_operand_fact(value_flow, constants, *value);
            let reference_local = match *value {
                RegionOperand::Local(local) => value_flow.local_storage(local).is_reference_slot(),
                _ => false,
            };
            let value = lower_region_operand(builder, locals, registers, *value)?;
            let value = if reference_local {
                lower_optimizing_terminator_reference_local(builder, value, transition)?
            } else {
                value
            };
            if return_check_required {
                let Some(matches_return_type) = return_type
                    .and_then(|type_| lower_optimizing_type_guard(builder, value, type_))
                else {
                    transition.emit(builder)?;
                    return Ok(EmittedOptimizingInstruction {
                        class: crate::JitProductionLoweringClass::BaselineFragmentTransition,
                        operation_local_transition: false,
                    });
                };
                let admitted = builder.create_block();
                let rejected = builder.create_block();
                builder
                    .ins()
                    .brif(matches_return_type, admitted, &[], rejected, &[]);
                builder.switch_to_block(rejected);
                transition.emit(builder)?;
                builder.ins().jump(admitted, &[]);
                builder.switch_to_block(admitted);
            }
            if reference_local || fact.ownership == SsaOwnership::Borrowed {
                lower_optimizing_retain(builder, value, deopt_out);
            }
            let cleanup = frame_cleanup_locals
                .iter()
                .copied()
                .map(|local| Ok((local, use_local_variable(builder, locals, local)?)))
                .collect::<Result<Vec<_>, CraneliftLoweringError>>()?;
            lower_optimizing_frame_cleanup(builder, &cleanup, transition)?;
            builder
                .ins()
                .store(MemFlagsData::new(), value, result_out, 0);
            let status = builder
                .ins()
                .iconst(types::I32, i64::from(crate::JitCallStatus::RETURN.0));
            builder.ins().return_(&[status]);
        }
        RegionTerminator::ReturnReference {
            local,
            finally: None,
        } => {
            // A reference result is an independently owned ABI value. The
            // local may only borrow a caller argument, or frame cleanup may
            // release its owner below, so retain before leaving the frame in
            // both cases. Direct compiled callers consume this owner when
            // they install the returned alias.
            let value = use_local_variable(builder, locals, *local)?;
            lower_optimizing_retain(builder, value, deopt_out);
            let cleanup = frame_cleanup_locals
                .iter()
                .copied()
                .map(|local| Ok((local, use_local_variable(builder, locals, local)?)))
                .collect::<Result<Vec<_>, CraneliftLoweringError>>()?;
            lower_optimizing_frame_cleanup(builder, &cleanup, transition)?;
            builder
                .ins()
                .store(MemFlagsData::new(), value, result_out, 0);
            let status = builder.ins().iconst(
                types::I32,
                i64::from(crate::JitCallStatus::RETURN_REFERENCE.0),
            );
            builder.ins().return_(&[status]);
        }
        RegionTerminator::Exit {
            value,
            finally: None,
        } => {
            let reference_local = value.is_some_and(|value| {
                matches!(value, RegionOperand::Local(local) if value_flow.local_storage(local).is_reference_slot())
            });
            let value = value
                .map(|value| lower_region_operand(builder, locals, registers, value))
                .transpose()?
                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
            let value = if reference_local {
                let value =
                    lower_optimizing_terminator_reference_local(builder, value, transition)?;
                lower_optimizing_retain(builder, value, deopt_out);
                value
            } else {
                value
            };
            let cleanup = frame_cleanup_locals
                .iter()
                .copied()
                .map(|local| Ok((local, use_local_variable(builder, locals, local)?)))
                .collect::<Result<Vec<_>, CraneliftLoweringError>>()?;
            lower_optimizing_frame_cleanup(builder, &cleanup, transition)?;
            builder
                .ins()
                .store(MemFlagsData::new(), value, result_out, 0);
            let status = builder
                .ins()
                .iconst(types::I32, i64::from(crate::JitCallStatus::EXIT.0));
            builder.ins().return_(&[status]);
        }
        RegionTerminator::Return {
            finally: Some(_), ..
        }
        | RegionTerminator::ReturnReference {
            finally: Some(_), ..
        }
        | RegionTerminator::Exit {
            finally: Some(_), ..
        } => {
            transition.emit(builder)?;
        }
    }
    Ok(EmittedOptimizingInstruction {
        class: if emitted_transition.get() {
            crate::JitProductionLoweringClass::BaselineFragmentTransition
        } else {
            crate::JitProductionLoweringClass::DirectClif
        },
        operation_local_transition: operation_local_transition.get(),
    })
}

#[allow(clippy::too_many_arguments)]
fn lower_region_frame_exit(
    builder: &mut FunctionBuilder<'_>,
    blocks: &BTreeMap<BlockId, ir::Block>,
    locals: &NativeLocalMap,
    result_out: ir::Value,
    deopt_out: ir::Value,
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
            deopt_out,
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
    locals: &NativeLocalMap,
    native_operations: NativeOperationFunctions,
    value_flow: &ExecutableValueFlow,
    function: FunctionId,
    result_out: ir::Value,
    deopt_out: ir::Value,
) -> Result<(), CraneliftLoweringError> {
    for local in locals.keys() {
        let fact = value_flow.local_fact(*local);
        let bound_reference_owner = value_flow.local_storage(*local)
            == crate::region_ir::LocalStorageClass::MemoryReference;
        if value_flow.releases_local_at_frame_exit(*local)
            && (bound_reference_owner
                || (fact.has_runtime_lifecycle() && fact.ownership == SsaOwnership::Owned))
        {
            let value = use_local_variable(builder, locals, *local)?;
            let _ = lower_guarded_value_release(
                module,
                builder,
                native_operations.value_release,
                native_frame_cleanup_operation(function),
                value,
                result_out,
                deopt_out,
            )?;
        }
    }
    Ok(())
}
