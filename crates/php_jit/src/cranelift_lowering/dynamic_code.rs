use super::*;

fn lower_generic_dynamic_caller_frame(
    builder: &mut FunctionBuilder<'_>,
    locals: &NativeLocalMap,
    pointer_type: ir::Type,
) -> Result<ir::Value, CraneliftLoweringError> {
    if locals.is_empty() {
        return Ok(builder.ins().iconst(pointer_type, 0));
    }
    let frame_size = u32::try_from(locals.len().saturating_mul(8)).map_err(|_| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_DYNAMIC_CODE",
            "caller local frame exceeds stack-slot limits",
        )
    })?;
    let frame_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        frame_size,
        3,
    ));
    let frame_ptr = builder.ins().stack_addr(pointer_type, frame_slot, 0);
    for local in locals.keys() {
        let value = use_local_variable(builder, locals, *local)?;
        builder.ins().store(
            MemFlagsData::new(),
            value,
            frame_ptr,
            i32::try_from(local.index().saturating_mul(8)).unwrap_or(i32::MAX),
        );
    }
    Ok(frame_ptr)
}

fn reload_baseline_dynamic_caller_frame(
    builder: &mut FunctionBuilder<'_>,
    locals: &NativeLocalMap,
    caller_frame: ir::Value,
) -> Result<(), CraneliftLoweringError> {
    for local in locals.keys() {
        let value = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            caller_frame,
            i32::try_from(local.index().saturating_mul(8)).unwrap_or(i32::MAX),
        );
        define_local_variable(builder, locals, *local, value)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_native_dynamic_code(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    native_dynamic_code_helper: Option<NativeHelper>,
    value_release_commit: Option<ir::FuncRef>,
    locals: &NativeLocalMap,
    register_variables: &NativeRegisterMap,
    registers: &mut NativeRegisterMap,
    operation: &RegionNativeDynamicCode,
    instruction: &RegionInstruction,
    result_out: ir::Value,
    runtime: ir::Value,
    deopt_out: ir::Value,
    function: FunctionId,
    pointer_type: ir::Type,
) -> Result<(), CraneliftLoweringError> {
    let helper = native_dynamic_code_helper.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_DYNAMIC_CODE",
            "dynamic code site has no native compiler/invoker",
        )
    })?;
    let (kind, destination, declared_function, source, symbol_hash, flags) = match operation {
        RegionNativeDynamicCode::Include { dst, kind, path } => (
            match kind {
                php_ir::instruction::IncludeKind::Include => {
                    crate::JitNativeDynamicCodeKind::INCLUDE
                }
                php_ir::instruction::IncludeKind::IncludeOnce => {
                    crate::JitNativeDynamicCodeKind::INCLUDE_ONCE
                }
                php_ir::instruction::IncludeKind::Require => {
                    crate::JitNativeDynamicCodeKind::REQUIRE
                }
                php_ir::instruction::IncludeKind::RequireOnce => {
                    crate::JitNativeDynamicCodeKind::REQUIRE_ONCE
                }
            },
            Some(*dst),
            None,
            Some(*path),
            0,
            0,
        ),
        RegionNativeDynamicCode::Eval { dst, code } => (
            crate::JitNativeDynamicCodeKind::EVAL,
            Some(*dst),
            None,
            Some(*code),
            0,
            0,
        ),
        RegionNativeDynamicCode::DeclareFunction { name, function } => (
            crate::JitNativeDynamicCodeKind::DECLARE_FUNCTION,
            None,
            Some(*function),
            None,
            stable_call_symbol_hash(name),
            0,
        ),
        RegionNativeDynamicCode::DeclareClass { name } => (
            crate::JitNativeDynamicCodeKind::DECLARE_CLASS,
            None,
            None,
            None,
            stable_call_symbol_hash(name),
            0,
        ),
        RegionNativeDynamicCode::RegisterConstant { name, value } => (
            crate::JitNativeDynamicCodeKind::REGISTER_CONSTANT,
            None,
            None,
            Some(*value),
            stable_call_symbol_hash(name),
            0,
        ),
        RegionNativeDynamicCode::EmitDiagnostic => (
            crate::JitNativeDynamicCodeKind::EMIT_DIAGNOSTIC,
            None,
            None,
            None,
            0,
            0,
        ),
        RegionNativeDynamicCode::MakeClosure {
            dst,
            function,
            captures,
            ..
        } => (
            crate::JitNativeDynamicCodeKind::MAKE_CLOSURE,
            Some(*dst),
            Some(*function),
            None,
            0,
            u32::try_from(captures.len()).unwrap_or(u32::MAX),
        ),
    };
    let request_size = u32::try_from(std::mem::size_of::<crate::JitNativeDynamicCodeRequest>())
        .map_err(|_| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_DYNAMIC_CODE",
                "dynamic code request exceeds stack-slot limits",
            )
        })?;
    let request_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        request_size,
        3,
    ));
    let request_ptr = builder.ins().stack_addr(pointer_type, request_slot, 0);
    let zero = builder.ins().iconst(types::I64, 0);
    for offset in (0..request_size).step_by(8) {
        builder.ins().store(
            MemFlagsData::new(),
            zero,
            request_ptr,
            i32::try_from(offset).unwrap_or(i32::MAX),
        );
    }
    let store_i32 = |builder: &mut FunctionBuilder<'_>, offset: usize, value: u32| {
        let value = builder.ins().iconst(types::I32, i64::from(value));
        builder.ins().store(
            MemFlagsData::new(),
            value,
            request_ptr,
            i32::try_from(offset).unwrap_or(i32::MAX),
        );
    };
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeDynamicCodeRequest, abi_version),
        crate::JIT_RUNTIME_ABI_VERSION,
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeDynamicCodeRequest, struct_size),
        request_size,
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeDynamicCodeRequest, kind),
        kind.0,
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeDynamicCodeRequest, flags),
        flags,
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeDynamicCodeRequest, caller_function_id),
        function.raw(),
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeDynamicCodeRequest, continuation_id),
        instruction.continuation_id,
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeDynamicCodeRequest, result_slot),
        destination.map_or(u32::MAX, RegId::raw),
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeDynamicCodeRequest, declared_function_id),
        declared_function.map_or(u32::MAX, FunctionId::raw),
    );
    if let Some(source) = source {
        let source = lower_region_operand(builder, locals, registers, source)?;
        store_i32(
            builder,
            std::mem::offset_of!(crate::JitNativeDynamicCodeRequest, source)
                + std::mem::offset_of!(crate::JitAbiSlot, tag),
            3,
        );
        builder.ins().store(
            MemFlagsData::new(),
            source,
            request_ptr,
            (std::mem::offset_of!(crate::JitNativeDynamicCodeRequest, source)
                + std::mem::offset_of!(crate::JitAbiSlot, payload)) as i32,
        );
    }
    let symbol_hash = builder.ins().iconst(types::I64, symbol_hash as i64);
    builder.ins().store(
        MemFlagsData::new(),
        symbol_hash,
        request_ptr,
        std::mem::offset_of!(crate::JitNativeDynamicCodeRequest, symbol_hash) as i32,
    );

    let binding_capacity = locals.len().max(1);
    let binding_size = u32::try_from(
        binding_capacity.saturating_mul(std::mem::size_of::<crate::JitNativeDynamicBinding>()),
    )
    .map_err(|_| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_DYNAMIC_CODE",
            "dynamic binding plan exceeds stack-slot limits",
        )
    })?;
    let binding_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        binding_size,
        3,
    ));
    let binding_ptr = builder.ins().stack_addr(pointer_type, binding_slot, 0);
    let out_size = std::mem::size_of::<crate::JitNativeDynamicUnitResolution>() as u32;
    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        out_size,
        3,
    ));
    let out_ptr = builder.ins().stack_addr(pointer_type, out_slot, 0);
    for offset in (0..out_size).step_by(8) {
        builder.ins().store(
            MemFlagsData::new(),
            zero,
            out_ptr,
            i32::try_from(offset).unwrap_or(i32::MAX),
        );
    }
    builder.ins().store(
        MemFlagsData::new(),
        binding_ptr,
        out_ptr,
        std::mem::offset_of!(crate::JitNativeDynamicUnitResolution, include_binding_plan) as i32,
    );
    let capacity = builder.ins().iconst(
        types::I32,
        i64::try_from(binding_capacity).unwrap_or(i64::MAX),
    );
    builder.ins().store(
        MemFlagsData::new(),
        capacity,
        out_ptr,
        std::mem::offset_of!(
            crate::JitNativeDynamicUnitResolution,
            include_binding_capacity,
        ) as i32,
    );
    let caller_frame = lower_generic_dynamic_caller_frame(builder, locals, pointer_type)?;
    builder.ins().store(
        MemFlagsData::new(),
        caller_frame,
        request_ptr,
        std::mem::offset_of!(crate::JitNativeDynamicCodeRequest, caller_frame) as i32,
    );
    let vm_context = builder.ins().iconst(types::I64, 0);
    let helper_call =
        call_native_helper(module, builder, helper, &[vm_context, request_ptr, out_ptr]);
    let status = builder.inst_results(helper_call)[0];
    let resolved = builder.create_block();
    let helper_failed = builder.create_block();
    let is_success = builder.ins().icmp_imm(
        IntCC::Equal,
        status,
        i64::from(crate::JitCallStatus::RETURN.0),
    );
    builder
        .ins()
        .brif(is_success, resolved, &[], helper_failed, &[]);
    builder.switch_to_block(helper_failed);
    let control_value = builder.ins().stack_load(
        types::I64,
        out_slot,
        std::mem::offset_of!(crate::JitNativeDynamicUnitResolution, control_value) as i32,
    );
    builder
        .ins()
        .store(MemFlagsData::new(), control_value, result_out, 0);
    return_native_or_fragment_control(builder, status, result_out);
    builder.switch_to_block(resolved);

    let invoke = builder.create_block();
    let complete = builder.create_block();
    builder.append_block_param(complete, types::I64);
    let action = builder.ins().stack_load(
        types::I32,
        out_slot,
        std::mem::offset_of!(crate::JitNativeDynamicUnitResolution, action) as i32,
    );
    let should_invoke = builder.ins().icmp_imm(
        IntCC::Equal,
        action,
        i64::from(crate::JitNativeDynamicUnitAction::INVOKE.0),
    );
    let completed_value = builder.ins().stack_load(
        types::I64,
        out_slot,
        std::mem::offset_of!(crate::JitNativeDynamicUnitResolution, control_value) as i32,
    );
    builder.ins().brif(
        should_invoke,
        invoke,
        &[],
        complete,
        &[completed_value.into()],
    );

    builder.switch_to_block(invoke);
    let preferred_cell = builder.ins().stack_load(
        pointer_type,
        out_slot,
        std::mem::offset_of!(crate::JitNativeDynamicUnitResolution, preferred_entry_cell,) as i32,
    );
    let address = builder
        .ins()
        .atomic_load(pointer_type, MemFlagsData::new(), preferred_cell);
    let callee_view = builder.ins().stack_load(
        pointer_type,
        out_slot,
        std::mem::offset_of!(crate::JitNativeDynamicUnitResolution, runtime_view) as i32,
    );
    let deopt_view_pointer = builder.ins().iadd_imm(
        deopt_out,
        i64::try_from(std::mem::offset_of!(
            crate::JitDeoptState,
            runtime_view_pointer,
        ))
        .unwrap_or(i64::MAX),
    );
    let fast_view_pointer = builder.ins().iadd_imm(
        runtime,
        i64::try_from(std::mem::offset_of!(
            crate::JitNativeFastStateHeader,
            runtime_view_pointer,
        ))
        .unwrap_or(i64::MAX),
    );
    let previous_deopt_view =
        builder
            .ins()
            .load(pointer_type, MemFlagsData::new(), deopt_view_pointer, 0);
    let previous_fast_view =
        builder
            .ins()
            .load(pointer_type, MemFlagsData::new(), fast_view_pointer, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), callee_view, deopt_view_pointer, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), callee_view, fast_view_pointer, 0);
    let signature = builder.import_signature(native_php_entry_signature(module));
    let no_arguments = builder.ins().iconst(pointer_type, 0);
    let initial_resume = builder.ins().iconst(types::I32, -1);
    let no_resume_state = builder.ins().iconst(pointer_type, 0);
    let call = builder.ins().call_indirect(
        signature,
        address,
        &[
            runtime,
            no_arguments,
            deopt_out,
            initial_resume,
            no_resume_state,
        ],
    );
    let invoked_status = unpack_native_php_control(builder, call, result_out);
    let exit_deopt_view =
        builder
            .ins()
            .load(pointer_type, MemFlagsData::new(), deopt_view_pointer, 0);
    builder.ins().store(
        MemFlagsData::new(),
        previous_deopt_view,
        deopt_view_pointer,
        0,
    );
    builder.ins().store(
        MemFlagsData::new(),
        previous_fast_view,
        fast_view_pointer,
        0,
    );

    let binding_count = builder.ins().stack_load(
        types::I32,
        out_slot,
        std::mem::offset_of!(crate::JitNativeDynamicUnitResolution, include_binding_count,) as i32,
    );
    for index in 0..locals.len() {
        let inspect = builder.create_block();
        let done = builder.create_block();
        let present = builder.ins().icmp_imm(
            IntCC::UnsignedGreaterThan,
            binding_count,
            i64::try_from(index).unwrap_or(i64::MAX),
        );
        builder.ins().brif(present, inspect, &[], done, &[]);
        builder.switch_to_block(inspect);
        let record = builder.ins().iadd_imm(
            binding_ptr,
            i64::try_from(
                index.saturating_mul(std::mem::size_of::<crate::JitNativeDynamicBinding>()),
            )
            .unwrap_or(i64::MAX),
        );
        let caller_slot = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            record,
            std::mem::offset_of!(crate::JitNativeDynamicBinding, caller_slot) as i32,
        );
        let flags = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            record,
            std::mem::offset_of!(crate::JitNativeDynamicBinding, flags) as i32,
        );
        let has_caller = builder
            .ins()
            .icmp_imm(IntCC::NotEqual, caller_slot, i64::from(u32::MAX));
        let preserve = builder.ins().band_imm(
            flags,
            i64::from(crate::JitNativeDynamicBinding::PRESERVE_REFERENCE),
        );
        let replace = builder.ins().icmp_imm(IntCC::Equal, preserve, 0);
        let write = builder.ins().band(has_caller, replace);
        let writeback = builder.create_block();
        builder.ins().brif(write, writeback, &[], done, &[]);
        builder.switch_to_block(writeback);
        let reference = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            record,
            std::mem::offset_of!(crate::JitNativeDynamicBinding, reference) as i32,
        );
        let payload = lower_direct_reference_payload_unchecked(builder, reference, deopt_out);
        lower_optimizing_retain(builder, payload, deopt_out);
        let caller_slot = builder.ins().uextend(pointer_type, caller_slot);
        let caller_offset = builder.ins().ishl_imm(caller_slot, 3);
        let caller_address = builder.ins().iadd(caller_frame, caller_offset);
        let previous = builder
            .ins()
            .load(types::I64, MemFlagsData::new(), caller_address, 0);
        builder
            .ins()
            .store(MemFlagsData::new(), payload, caller_address, 0);
        lower_direct_release_with_commit(
            builder,
            previous,
            runtime,
            deopt_out,
            value_release_commit.ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_DYNAMIC_RELEASE_COMMIT",
                    "dynamic unit resolver has no generated release commit",
                )
            })?,
        )?;
        builder.ins().jump(done, &[]);
        builder.switch_to_block(done);
    }
    if !locals.is_empty() {
        reload_baseline_dynamic_caller_frame(builder, locals, caller_frame)?;
    }
    let invoke_ok = builder.create_block();
    let invoke_control = builder.create_block();
    let returned = builder.ins().icmp_imm(
        IntCC::Equal,
        invoked_status,
        i64::from(crate::JitCallStatus::RETURN.0),
    );
    builder
        .ins()
        .brif(returned, invoke_ok, &[], invoke_control, &[]);
    builder.switch_to_block(invoke_control);
    // The request fast state has already returned to the caller view, but the
    // precise side-exit coordinates and view were published by the included
    // unit or one of its transitive callees. Preserve the actual exit view in
    // the deopt record so the cold boundary never pairs callee coordinates
    // with caller metadata.
    builder
        .ins()
        .store(MemFlagsData::new(), exit_deopt_view, deopt_view_pointer, 0);
    return_native_or_fragment_control(builder, invoked_status, result_out);
    builder.switch_to_block(invoke_ok);
    let value = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), result_out, 0);
    let flags = builder.ins().stack_load(
        types::I32,
        out_slot,
        std::mem::offset_of!(crate::JitNativeDynamicUnitResolution, flags) as i32,
    );
    let implicit = builder.ins().band_imm(
        flags,
        i64::from(crate::JitNativeDynamicUnitResolution::IMPLICIT_INCLUDE_RETURN),
    );
    let implicit = builder.ins().icmp_imm(IntCC::NotEqual, implicit, 0);
    let is_null = builder
        .ins()
        .icmp_imm(IntCC::Equal, value, crate::jit_encode_constant(u32::MAX));
    let replace_implicit = builder.ins().band(implicit, is_null);
    let one = builder.ins().iconst(types::I64, 1);
    let value = builder.ins().select(replace_implicit, one, value);
    builder.ins().jump(complete, &[value.into()]);

    builder.switch_to_block(complete);
    let final_value = builder.block_params(complete)[0];
    let completed_status = builder.ins().stack_load(
        types::I32,
        out_slot,
        std::mem::offset_of!(crate::JitNativeDynamicUnitResolution, control_status) as i32,
    );
    let return_status = builder
        .ins()
        .iconst(types::I32, i64::from(crate::JitCallStatus::RETURN.0));
    let final_status = builder
        .ins()
        .select(should_invoke, return_status, completed_status);
    let final_ok = builder.create_block();
    let final_control = builder.create_block();
    let returned = builder.ins().icmp_imm(
        IntCC::Equal,
        final_status,
        i64::from(crate::JitCallStatus::RETURN.0),
    );
    builder
        .ins()
        .brif(returned, final_ok, &[], final_control, &[]);
    builder.switch_to_block(final_control);
    builder
        .ins()
        .store(MemFlagsData::new(), final_value, result_out, 0);
    return_native_or_fragment_control(builder, final_status, result_out);
    builder.switch_to_block(final_ok);
    if let Some(destination) = destination {
        define_region_register(
            builder,
            register_variables,
            registers,
            destination,
            final_value,
        )?;
    }
    Ok(())
}
