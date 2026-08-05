//! Generated lowering for the fixed `call_user_func` builtin.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_call_user_func(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    register_variables: &NativeRegisterMap,
    locals: &NativeLocalMap,
    registers: &mut NativeRegisterMap,
    call: &RegionNativeCall,
    instruction: &RegionInstruction,
    constants: &[IrConstant],
    value_flow: &ExecutableValueFlow,
    runtime: ir::Value,
    result_out: ir::Value,
    deopt_out: ir::Value,
    function: FunctionId,
    optimizing_operations: NativeOptimizingOperations,
    transition: NativeOptimizingTransition<'_>,
) -> Result<(), CraneliftLoweringError> {
    let callback_operand = call.operands[0].expect("admitted call_user_func callback operand");
    let callback = lower_prepared_native_call_operand(
        builder,
        locals,
        registers,
        constants,
        callback_operand,
        transition,
    )?;
    let callback = lower_optimizing_reference_scalar(builder, callback, false, transition)?;
    let callable = emit_total_exact_owned_runtime_value!(
        module,
        builder,
        optimizing_operations.acquire_callable,
        &[callback],
        &[],
        transition,
        "call_user_func callback has no exact callable acquisition handler",
    )?;
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    let callable_slot = lower_optimizing_slot_address(builder, callable, transition.deopt_out);
    let callable_view = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        callable_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let callable_flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        callable_view,
        std::mem::offset_of!(crate::JitNativePreparedCallableView, flags) as i32,
    );
    let fixed = builder.ins().band_imm(
        callable_flags,
        i64::from(crate::JIT_NATIVE_PREPARED_CALLABLE_FIXED_BINDING),
    );
    let fixed = builder.ins().icmp_imm(IntCC::NotEqual, fixed, 0);
    let valid = builder.create_block();
    let invalid = builder.create_block();
    builder.ins().brif(fixed, valid, &[], invalid, &[]);

    builder.switch_to_block(invalid);
    let prefix = lower_optimizing_static_string(
        builder,
        b"call_user_func(): Argument #1 ($callback) must be a valid callback, function \"",
        transition,
    )?;
    let suffix = lower_optimizing_static_string(
        builder,
        b"\" not found or invalid function name",
        transition,
    )?;
    let partial = emit_total_exact_runtime_value!(
        module,
        builder,
        optimizing_operations.concat,
        &[prefix, callback],
        transition,
        "invalid callback diagnostic has no exact concatenation leaf",
    )?;
    let message = emit_total_exact_runtime_value!(
        module,
        builder,
        optimizing_operations.concat,
        &[partial, suffix],
        transition,
        "invalid callback diagnostic has no exact concatenation leaf",
    )?;
    let prepared = lower_optimizing_prepared_exception_pointer(
        builder,
        function,
        instruction.continuation_id,
        transition.deopt_out,
    );
    let code = builder.ins().iconst(types::I64, 0);
    let previous = builder
        .ins()
        .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
    let throwable = emit_total_exact_runtime_value!(
        module,
        builder,
        optimizing_operations.prepared_exception_new,
        &[prepared, message, code, previous],
        transition,
        "invalid callback TypeError has no exact allocator",
    )?;
    for owner in [callable, prefix, suffix, partial, message] {
        lower_optimizing_commit_owned_value(builder, owner, transition);
    }
    let detail = builder.ins().iconst(types::I32, 0);
    transition.emit_exact_throw(builder, detail, throwable)?;

    builder.switch_to_block(valid);
    let visible_arity = call.args.len().saturating_sub(1);
    let prepared_callable = lower_optimizing_prepare_dynamic_callable(
        builder,
        callable,
        OptimizingDynamicCallableArity::Fixed(visible_arity),
        call.caller_strict_types,
        false,
        false,
        false,
        &[callable],
        transition,
    )?;
    let mut arguments = Vec::with_capacity(visible_arity);
    for operand in call.operands.iter().skip(1).copied().flatten() {
        let argument = lower_prepared_native_call_operand(
            builder, locals, registers, constants, operand, transition,
        )?;
        arguments.push(lower_optimizing_reference_scalar(
            builder, argument, false, transition,
        )?);
    }
    let mut cleanup = lower_optimizing_prevalidate_consumed_dynamic_call_operands(
        builder,
        call,
        instruction,
        registers,
        value_flow,
        transition,
    )?;
    cleanup.push(callable);
    let (packed, packed_length) = lower_optimizing_pack_published_method_arguments(
        builder,
        prepared_callable,
        &arguments,
        transition,
    )?;
    let result = lower_optimizing_invoke_packed_dynamic(
        module,
        builder,
        prepared_callable.callback,
        packed,
        packed_length,
        &cleanup,
        None,
        runtime,
        result_out,
        deopt_out,
        instruction,
        transition,
    )?;
    for owner in cleanup {
        lower_optimizing_commit_owned_value(builder, owner, transition);
    }
    match call.result {
        RegionCallResult::Register(destination) => {
            define_region_register(builder, register_variables, registers, destination, result)?
        }
        RegionCallResult::Discard => {
            lower_optimizing_commit_owned_value(builder, result, transition);
        }
        RegionCallResult::ReferenceLocal(_) => unreachable!("admission rejected reference result"),
    }
    Ok(())
}
