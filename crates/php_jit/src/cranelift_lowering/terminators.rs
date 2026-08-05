use super::*;

fn lower_optimizing_terminator_reference_local(
    builder: &mut FunctionBuilder<'_>,
    local: ir::Value,
    deopt_out: ir::Value,
    proof: NativeReferencePayloadProof,
) -> ir::Value {
    lower_optimizing_admitted_reference_scalar(builder, local, deopt_out, proof)
}

fn lower_terminator_storage_value(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    operand: RegionOperand,
    constants: &[IrConstant],
    deopt_out: ir::Value,
) -> (ir::Value, Option<ir::Value>) {
    if let RegionOperand::Constant(index) = operand
        && constant_requires_native_storage(constants, index)
    {
        let (value, borrowed) = lower_native_literal_value(builder, value, index, deopt_out);
        return (value, Some(borrowed));
    }
    (value, None)
}

fn lower_total_optimizing_frame_cleanup(
    builder: &mut FunctionBuilder<'_>,
    cleanup: &[(LocalId, ir::Value)],
    value_release_commit: ir::FuncRef,
    runtime: ir::Value,
    deopt_out: ir::Value,
) {
    for (_, value) in cleanup {
        let _ = builder
            .ins()
            .call(value_release_commit, &[runtime, deopt_out, *value]);
    }
}

fn lower_total_optimizing_scalar_slot(
    builder: &mut FunctionBuilder<'_>,
    payload: ir::Value,
    kind: u32,
    flags: u32,
    tag: u64,
    deopt_out: ir::Value,
) -> ir::Value {
    let slot_index = lower_reserve_direct_value_index(builder, deopt_out, None);
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let view = lower_active_runtime_view(builder, deopt_out);
    let slots = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_value_slots) as i32,
    );
    let wide_index = builder.ins().uextend(pointer_type, slot_index);
    let slot_offset = builder.ins().ishl_imm(wide_index, 5);
    let slot = builder.ins().iadd(slots, slot_offset);
    for (value, offset) in [
        (
            builder.ins().iconst(types::I32, 1),
            std::mem::offset_of!(crate::JitNativeValueSlot, refcount),
        ),
        (
            builder.ins().iconst(types::I32, i64::from(kind)),
            std::mem::offset_of!(crate::JitNativeValueSlot, kind),
        ),
        (
            builder.ins().iconst(types::I32, i64::from(flags)),
            std::mem::offset_of!(crate::JitNativeValueSlot, flags),
        ),
        (
            builder.ins().iconst(types::I32, 0),
            std::mem::offset_of!(crate::JitNativeValueSlot, reserved),
        ),
    ] {
        builder
            .ins()
            .store(MemFlagsData::new(), value, slot, offset as i32);
    }
    builder.ins().store(
        MemFlagsData::new(),
        payload,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().store(
        MemFlagsData::new(),
        zero,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let encoded_index = builder.ins().iadd_imm(
        slot_index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let encoded_index = builder.ins().uextend(types::I64, encoded_index);
    builder.ins().bor_imm(encoded_index, tag as i64)
}

fn lower_total_optimizing_return_plan(
    builder: &mut FunctionBuilder<'_>,
    original: ir::Value,
    plan: NativeOptimizingReturnPlan,
    deopt_out: ir::Value,
) -> ir::Value {
    match plan {
        NativeOptimizingReturnPlan::Immediate(value) => builder.ins().iconst(types::I64, value),
        NativeOptimizingReturnPlan::DirectInt(value) => {
            let payload = builder.ins().iconst(types::I64, value);
            lower_total_optimizing_scalar_slot(
                builder,
                payload,
                crate::JIT_NATIVE_VALUE_VIEW_DIRECT_INT,
                crate::JIT_NATIVE_DIRECT_INT_ABI_VERSION,
                crate::JIT_VALUE_RUNTIME_TAG,
                deopt_out,
            )
        }
        NativeOptimizingReturnPlan::DirectFloat(bits) => {
            let payload = builder.ins().iconst(types::I64, bits as i64);
            lower_total_optimizing_scalar_slot(
                builder,
                payload,
                crate::JIT_NATIVE_VALUE_VIEW_FLOAT,
                0,
                crate::JIT_VALUE_RUNTIME_FLOAT_TAG,
                deopt_out,
            )
        }
        NativeOptimizingReturnPlan::IntToFloat => {
            let integer = lower_optimizing_authoritative_integer(builder, original, deopt_out);
            let float = builder.ins().fcvt_from_sint(types::F64, integer);
            let payload = builder
                .ins()
                .bitcast(types::I64, MemFlagsData::new(), float);
            lower_total_optimizing_scalar_slot(
                builder,
                payload,
                crate::JIT_NATIVE_VALUE_VIEW_FLOAT,
                0,
                crate::JIT_VALUE_RUNTIME_FLOAT_TAG,
                deopt_out,
            )
        }
    }
}

fn lower_total_optimizing_reference_writeback(
    builder: &mut FunctionBuilder<'_>,
    reference: ir::Value,
    replacement: ir::Value,
    value_release_commit: ir::FuncRef,
    runtime: ir::Value,
    deopt_out: ir::Value,
) {
    let slot = lower_optimizing_slot_address(builder, reference, deopt_out);
    let previous = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let _ = builder
        .ins()
        .call(value_release_commit, &[runtime, deopt_out, previous]);
    builder.ins().store(
        MemFlagsData::new(),
        replacement,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let published = builder.ins().iconst(
        types::I32,
        i64::from(crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED),
    );
    builder.ins().store(
        MemFlagsData::new(),
        published,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    lower_mark_native_roots_dirty(builder, deopt_out);
}

fn lower_optimizing_condition(
    builder: &mut FunctionBuilder<'_>,
    condition: RegionOperand,
    locals: &NativeLocalMap,
    registers: &NativeRegisterMap,
    constants: &[IrConstant],
    value_flow: &ExecutableValueFlow,
    deopt_out: ir::Value,
    reference_payload_proof: NativeReferencePayloadProof,
) -> Result<ir::Value, CraneliftLoweringError> {
    let value = lower_region_operand(builder, locals, registers, condition)?;
    let value = if let RegionOperand::Local(local) = condition
        && value_flow.local_storage(local).is_reference_slot()
    {
        lower_optimizing_terminator_reference_local(
            builder,
            value,
            deopt_out,
            reference_payload_proof,
        )
    } else {
        value
    };
    let fact = value_flow.operand_fact(constants, condition);
    if fact.certainty == crate::region_ir::SsaCertainty::Unknown {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_TRUTHINESS_CONTRACT",
            "optimizing branch entered without a publication-fixed value class",
        ));
    }
    if fact.class == SsaValueClass::Int {
        if let RegionOperand::Constant(index) = condition
            && let Some(IrConstant::Int(value)) = constants.get(index as usize)
        {
            return Ok(builder.ins().iconst(types::I8, i64::from(*value != 0)));
        }
        let integer = lower_optimizing_authoritative_integer(builder, value, deopt_out);
        return Ok(builder.ins().icmp_imm(IntCC::NotEqual, integer, 0));
    }
    if let Some(truthy) = scalar_truthy(builder, value, fact.class) {
        return Ok(truthy);
    }
    match fact.class {
        SsaValueClass::Float => {
            let slot = lower_optimizing_slot_address(builder, value, deopt_out);
            let bits = builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                slot,
                std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
            );
            let magnitude = builder.ins().band_imm(bits, i64::MAX);
            Ok(builder.ins().icmp_imm(IntCC::NotEqual, magnitude, 0))
        }
        SsaValueClass::StringHandle => {
            let (_, length, bytes) = lower_native_string_key_descriptor(builder, value, deopt_out);
            let non_empty = builder.ins().icmp_imm(IntCC::NotEqual, length, 0);
            let single = builder.ins().icmp_imm(IntCC::Equal, length, 1);
            let inspect_byte = builder.create_block();
            let merge = builder.create_block();
            builder.append_block_param(merge, types::I8);
            builder
                .ins()
                .brif(single, inspect_byte, &[], merge, &[non_empty.into()]);
            builder.switch_to_block(inspect_byte);
            let byte = builder.ins().load(types::I8, MemFlagsData::new(), bytes, 0);
            let not_zero = builder.ins().icmp_imm(IntCC::NotEqual, byte, b'0' as i64);
            builder.ins().jump(merge, &[not_zero.into()]);
            builder.switch_to_block(merge);
            Ok(builder.block_params(merge)[0])
        }
        SsaValueClass::ArrayHandle => {
            let slot = lower_optimizing_slot_address(builder, value, deopt_out);
            let kind = builder.ins().load(
                types::I32,
                MemFlagsData::new(),
                slot,
                std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
            );
            let direct_length = builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                slot,
                std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
            );
            let shared_length = builder.ins().load(
                types::I32,
                MemFlagsData::new(),
                slot,
                std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
            );
            let shared_length = builder.ins().uextend(types::I64, shared_length);
            let shared = builder.ins().icmp_imm(
                IntCC::Equal,
                kind,
                i64::from(crate::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY),
            );
            let borrowed = builder.ins().icmp_imm(
                IntCC::Equal,
                kind,
                i64::from(crate::JIT_NATIVE_VALUE_VIEW_BORROWED_REFERENCE_ARRAY),
            );
            let shared = builder.ins().bor(shared, borrowed);
            let length = builder.ins().select(shared, shared_length, direct_length);
            Ok(builder.ins().icmp_imm(IntCC::NotEqual, length, 0))
        }
        SsaValueClass::ObjectHandle
        | SsaValueClass::CallableHandle
        | SsaValueClass::ResourceHandle
        | SsaValueClass::GeneratorHandle
        | SsaValueClass::FiberHandle => Ok(builder.ins().iconst(types::I8, 1)),
        SsaValueClass::Uninitialized
        | SsaValueClass::ReferenceHandle
        | SsaValueClass::MixedHandle => Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_TRUTHINESS_CONTRACT",
            "optimizing branch entered with an unclassified truthiness shape",
        )),
        SsaValueClass::Null | SsaValueClass::Bool | SsaValueClass::Int => {
            unreachable!("direct scalar truthiness returned above")
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_region_condition(
    _module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    locals: &NativeLocalMap,
    registers: &NativeRegisterMap,
    native_operations: GenericNativeOperations,
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
    let _ = native_operations;
    Ok(lower_generic_unknown_condition(builder, value, deopt_out))
}

/// Resolve every published native value kind without crossing the runtime ABI.
/// Reference cells are dereferenced from the authoritative slot/view before
/// the generated kind dispatch computes PHP truthiness.
pub(super) fn lower_generic_unknown_condition(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    deopt_out: ir::Value,
) -> ir::Value {
    let value = lower_published_reference_payload(builder, value, deopt_out);
    lower_optimizing_authoritative_truthy(builder, value, deopt_out)
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
    native_operations: GenericNativeOperations,
    runtime: ir::Value,
    value_release_commit: ir::FuncRef,
    function: FunctionId,
    local_count: u32,
    continuation_id: u32,
    native_version: u32,
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
            let fact = lowering_operand_fact(value_flow, constants, operand);
            let (value, literal_borrowed) =
                lower_terminator_storage_value(builder, value, operand, constants, deopt_out);
            let value = if return_check_required && finally.is_none() {
                let checked = lower_native_return_check_with_frame_cleanup(
                    module,
                    builder,
                    native_operations,
                    runtime,
                    value_release_commit,
                    function,
                    local_count,
                    continuation_id,
                    native_version,
                    value,
                    matches!(operand, RegionOperand::Register(_))
                        && fact.ownership == SsaOwnership::Owned
                        && fact.has_runtime_lifecycle(),
                    locals,
                    value_flow,
                    result_out,
                    deopt_out,
                )?;
                let unchanged = builder.ins().icmp(IntCC::Equal, checked, value);
                let preserve = builder.create_block();
                let converted = builder.create_block();
                let done = builder.create_block();
                builder.ins().brif(unchanged, preserve, &[], converted, &[]);

                builder.switch_to_block(preserve);
                if fact.ownership == SsaOwnership::Borrowed {
                    lower_guarded_value_owner_change(
                        builder,
                        true,
                        checked,
                        deopt_out,
                        runtime,
                        value_release_commit,
                    )?;
                } else if let Some(literal_borrowed) = literal_borrowed {
                    lower_optimizing_retain_if(builder, checked, literal_borrowed, deopt_out);
                }
                builder.ins().jump(done, &[]);

                builder.switch_to_block(converted);
                builder.ins().jump(done, &[]);

                builder.switch_to_block(done);
                checked
            } else {
                if fact.ownership == SsaOwnership::Borrowed {
                    lower_guarded_value_owner_change(
                        builder,
                        true,
                        value,
                        deopt_out,
                        runtime,
                        value_release_commit,
                    )?
                } else if let Some(literal_borrowed) = literal_borrowed {
                    lower_optimizing_retain_if(builder, value, literal_borrowed, deopt_out);
                    value
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
                runtime,
                value_release_commit,
                value_flow,
                function,
            )?;
        }
        RegionTerminator::ReturnReference { local, finally } => {
            // The returned reference is a new ABI owner independently of the
            // callee frame. This is required even when value-flow marks the
            // parameter/local as owned: frame cleanup releases that owner
            // immediately below. Returning the same handle without retaining
            // it lets the caller install a recycled direct-reference slot and
            // breaks aliases such as `$b =& identity_ref($a)`.
            let local_value = use_local_variable(builder, locals, *local)?;
            let local_value = if return_check_required && finally.is_none() {
                lower_native_return_check_with_frame_cleanup(
                    module,
                    builder,
                    native_operations,
                    runtime,
                    value_release_commit,
                    function,
                    local_count,
                    continuation_id,
                    native_version,
                    local_value,
                    false,
                    locals,
                    value_flow,
                    result_out,
                    deopt_out,
                )?
            } else {
                local_value
            };
            let value = lower_guarded_value_owner_change(
                builder,
                true,
                local_value,
                deopt_out,
                runtime,
                value_release_commit,
            )?;
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
                runtime,
                value_release_commit,
                value_flow,
                function,
            )?;
        }
        RegionTerminator::Exit { value, finally } => {
            let value = if let Some(operand) = *value {
                let value = lower_region_operand(builder, locals, registers, operand)?;
                let fact = lowering_operand_fact(value_flow, constants, operand);
                let (value, literal_borrowed) =
                    lower_terminator_storage_value(builder, value, operand, constants, deopt_out);
                if fact.ownership == SsaOwnership::Borrowed {
                    lower_guarded_value_owner_change(
                        builder,
                        true,
                        value,
                        deopt_out,
                        runtime,
                        value_release_commit,
                    )?
                } else if let Some(literal_borrowed) = literal_borrowed {
                    lower_optimizing_retain_if(builder, value, literal_borrowed, deopt_out);
                    value
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
                runtime,
                value_release_commit,
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
    runtime: ir::Value,
    deopt_out: ir::Value,
    value_release_commit: ir::FuncRef,
    return_plan: Option<NativeOptimizingReturnPlan>,
    return_reference_prebound: bool,
    reference_payload_proof: NativeReferencePayloadProof,
    terminator: &RegionTerminator,
    constants: &[IrConstant],
    value_flow: &ExecutableValueFlow,
) -> Result<EmittedOptimizingInstruction, CraneliftLoweringError> {
    let direct_condition = |builder: &mut FunctionBuilder<'_>, condition: RegionOperand| {
        lower_optimizing_condition(
            builder,
            condition,
            locals,
            registers,
            constants,
            value_flow,
            deopt_out,
            reference_payload_proof,
        )
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
            let operand = *value;
            let fact = lowering_operand_fact(value_flow, constants, operand);
            let reference_local = match operand {
                RegionOperand::Local(local) => value_flow.local_storage(local).is_reference_slot(),
                _ => false,
            };
            let value = lower_region_operand(builder, locals, registers, operand)?;
            let value = if reference_local {
                lower_optimizing_terminator_reference_local(
                    builder,
                    value,
                    deopt_out,
                    reference_payload_proof,
                )
            } else {
                value
            };
            let (value, literal_borrowed) = if let Some(plan) = return_plan {
                (
                    lower_total_optimizing_return_plan(builder, value, plan, deopt_out),
                    None,
                )
            } else {
                lower_terminator_storage_value(builder, value, operand, constants, deopt_out)
            };
            if return_plan.is_none() {
                if reference_local || fact.ownership == SsaOwnership::Borrowed {
                    lower_optimizing_retain(builder, value, deopt_out);
                } else if let Some(literal_borrowed) = literal_borrowed {
                    lower_optimizing_retain_if(builder, value, literal_borrowed, deopt_out);
                }
            }
            let cleanup = frame_cleanup_locals
                .iter()
                .copied()
                .map(|local| Ok((local, use_local_variable(builder, locals, local)?)))
                .collect::<Result<Vec<_>, CraneliftLoweringError>>()?;
            lower_total_optimizing_frame_cleanup(
                builder,
                &cleanup,
                value_release_commit,
                runtime,
                deopt_out,
            );
            let status = builder
                .ins()
                .iconst(types::I32, i64::from(crate::JitCallStatus::RETURN.0));
            let (status, value) =
                select_generated_release_control(builder, deopt_out, status, value);
            builder
                .ins()
                .store(MemFlagsData::new(), value, result_out, 0);
            return_native_or_fragment_control(builder, status, result_out);
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
            let current = use_local_variable(builder, locals, *local)?;
            let value = if return_reference_prebound {
                if let Some(plan) = return_plan {
                    let payload = lower_optimizing_terminator_reference_local(
                        builder,
                        current,
                        deopt_out,
                        reference_payload_proof,
                    );
                    let replacement =
                        lower_total_optimizing_return_plan(builder, payload, plan, deopt_out);
                    lower_total_optimizing_reference_writeback(
                        builder,
                        current,
                        replacement,
                        value_release_commit,
                        runtime,
                        deopt_out,
                    );
                }
                current
            } else {
                let payload = if let Some(plan) = return_plan {
                    let replacement =
                        lower_total_optimizing_return_plan(builder, current, plan, deopt_out);
                    let _ = builder
                        .ins()
                        .call(value_release_commit, &[runtime, deopt_out, current]);
                    replacement
                } else {
                    current
                };
                let reference = lower_total_native_bind_reference(builder, payload, deopt_out);
                define_local_variable(builder, locals, *local, reference)?;
                reference
            };
            lower_optimizing_retain(builder, value, deopt_out);
            let cleanup = frame_cleanup_locals
                .iter()
                .copied()
                .map(|local| Ok((local, use_local_variable(builder, locals, local)?)))
                .collect::<Result<Vec<_>, CraneliftLoweringError>>()?;
            lower_total_optimizing_frame_cleanup(
                builder,
                &cleanup,
                value_release_commit,
                runtime,
                deopt_out,
            );
            let status = builder.ins().iconst(
                types::I32,
                i64::from(crate::JitCallStatus::RETURN_REFERENCE.0),
            );
            let (status, value) =
                select_generated_release_control(builder, deopt_out, status, value);
            builder
                .ins()
                .store(MemFlagsData::new(), value, result_out, 0);
            return_native_or_fragment_control(builder, status, result_out);
        }
        RegionTerminator::Exit {
            value,
            finally: None,
        } => {
            let operand = *value;
            let fact = operand.map(|value| lowering_operand_fact(value_flow, constants, value));
            let reference_local = operand.is_some_and(|value| {
                matches!(value, RegionOperand::Local(local) if value_flow.local_storage(local).is_reference_slot())
            });
            let value = operand
                .map(|value| lower_region_operand(builder, locals, registers, value))
                .transpose()?
                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
            let value = if reference_local {
                let value = lower_optimizing_terminator_reference_local(
                    builder,
                    value,
                    deopt_out,
                    reference_payload_proof,
                );
                lower_optimizing_retain(builder, value, deopt_out);
                value
            } else {
                value
            };
            let (value, literal_borrowed) = match (operand, fact) {
                (Some(operand), Some(_)) => {
                    lower_terminator_storage_value(builder, value, operand, constants, deopt_out)
                }
                _ => (value, None),
            };
            if !reference_local {
                if fact.is_some_and(|fact| fact.ownership == SsaOwnership::Borrowed) {
                    lower_optimizing_retain(builder, value, deopt_out);
                } else if let Some(literal_borrowed) = literal_borrowed {
                    lower_optimizing_retain_if(builder, value, literal_borrowed, deopt_out);
                }
            }
            let cleanup = frame_cleanup_locals
                .iter()
                .copied()
                .map(|local| Ok((local, use_local_variable(builder, locals, local)?)))
                .collect::<Result<Vec<_>, CraneliftLoweringError>>()?;
            lower_total_optimizing_frame_cleanup(
                builder,
                &cleanup,
                value_release_commit,
                runtime,
                deopt_out,
            );
            let status = builder
                .ins()
                .iconst(types::I32, i64::from(crate::JitCallStatus::EXIT.0));
            let (status, value) =
                select_generated_release_control(builder, deopt_out, status, value);
            builder
                .ins()
                .store(MemFlagsData::new(), value, result_out, 0);
            return_native_or_fragment_control(builder, status, result_out);
        }
        RegionTerminator::Return {
            finally: Some(_), ..
        }
        | RegionTerminator::ReturnReference {
            finally: Some(_), ..
        }
        | RegionTerminator::Exit {
            finally: Some(_), ..
        } => unreachable!("non-total finally terminators are rejected during publication"),
    }
    Ok(EmittedOptimizingInstruction {
        class: crate::JitProductionLoweringClass::DirectClif,
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
    runtime: ir::Value,
    value_release_commit: ir::FuncRef,
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
            runtime,
            value_release_commit,
            value_flow,
            function,
            deopt_out,
        )?;
        let (status, value) = select_generated_release_control(builder, deopt_out, status, value);
        builder
            .ins()
            .store(MemFlagsData::new(), value, result_out, 0);
        return_native_or_fragment_control(builder, status, result_out);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_owned_frame_locals(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    locals: &NativeLocalMap,
    runtime: ir::Value,
    value_release_commit: ir::FuncRef,
    value_flow: &ExecutableValueFlow,
    function: FunctionId,
    deopt_out: ir::Value,
) -> Result<(), CraneliftLoweringError> {
    let mut owned_values = Vec::new();
    for local in locals.keys() {
        let fact = value_flow.local_fact(*local);
        let bound_reference_owner = value_flow.local_storage(*local)
            == crate::region_ir::LocalStorageClass::MemoryReference;
        if value_flow.releases_local_at_frame_exit(*local)
            && (bound_reference_owner
                || (fact.has_runtime_lifecycle() && fact.ownership == SsaOwnership::Owned))
        {
            owned_values.push(use_local_variable(builder, locals, *local)?);
        }
    }
    let Some(first) = owned_values.first().copied() else {
        return Ok(());
    };
    if owned_values.len() == 1 {
        let _ = lower_guarded_value_owner_change(
            builder,
            false,
            first,
            deopt_out,
            runtime,
            value_release_commit,
        )?;
        return Ok(());
    }

    // Frame cleanup is semantically ordered but its owner count is not a
    // compile-time code-size dimension. Materialize the already-selected
    // encoded owners into a compact native stack vector and emit one direct
    // CLIF release loop. Previously the guarded release CFG was duplicated
    // once per local, making ordinary large functions structurally
    // uncompileable before regalloc. The loop retains the same direct shared
    // decrement and the same cold baseline final-release continuation.
    let owner_count = owned_values.len();
    let bytes = owner_count
        .checked_mul(std::mem::size_of::<u64>())
        .and_then(|bytes| u32::try_from(bytes).ok())
        .ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_FRAME_CLEANUP_SIZE",
                format!(
                    "function {} frame cleanup vector does not fit a native stack slot",
                    function.raw()
                ),
            )
        })?;
    let values =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, bytes, 3));
    for (index, value) in owned_values.into_iter().enumerate() {
        let offset = index
            .checked_mul(std::mem::size_of::<u64>())
            .and_then(|offset| i32::try_from(offset).ok())
            .ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_FRAME_CLEANUP_SIZE",
                    "frame cleanup vector offset does not fit i32",
                )
            })?;
        builder.ins().stack_store(value, values, offset);
    }

    let pointer_type = module.target_config().pointer_type();
    let scan = builder.create_block();
    let complete = builder.create_block();
    builder.append_block_param(scan, pointer_type);
    let base = builder.ins().stack_addr(pointer_type, values, 0);
    let zero = builder.ins().iconst(pointer_type, 0);
    builder.ins().jump(scan, &[zero.into()]);

    builder.switch_to_block(scan);
    let index = builder.block_params(scan)[0];
    let byte_offset = builder.ins().ishl_imm(index, 3);
    let address = builder.ins().iadd(base, byte_offset);
    let value = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), address, 0);
    let _ = lower_guarded_value_owner_change(
        builder,
        false,
        value,
        deopt_out,
        runtime,
        value_release_commit,
    )?;
    let next = builder.ins().iadd_imm(index, 1);
    let more = builder.ins().icmp_imm(
        IntCC::UnsignedLessThan,
        next,
        i64::try_from(owner_count).unwrap_or(i64::MAX),
    );
    builder
        .ins()
        .brif(more, scan, &[next.into()], complete, &[]);

    builder.switch_to_block(complete);
    Ok(())
}
