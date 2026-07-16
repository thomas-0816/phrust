use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_native_dynamic_code(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    native_dynamic_code_helper: Option<NativeHelper>,
    locals: &BTreeMap<LocalId, Variable>,
    register_variables: &BTreeMap<RegId, Variable>,
    registers: &mut BTreeMap<RegId, Variable>,
    operation: &RegionNativeDynamicCode,
    instruction: &RegionInstruction,
    result_out: ir::Value,
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
            capture_count,
        } => (
            crate::JitNativeDynamicCodeKind::MAKE_CLOSURE,
            Some(*dst),
            Some(*function),
            None,
            0,
            *capture_count,
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

    let out_size = std::mem::size_of::<crate::JitCallResult>() as u32;
    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        out_size,
        3,
    ));
    let out_ptr = builder.ins().stack_addr(pointer_type, out_slot, 0);
    let caller_frame = if locals.is_empty() {
        builder.ins().iconst(pointer_type, 0)
    } else {
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
        for (local, variable) in locals {
            let value = builder.use_var(*variable);
            builder.ins().store(
                MemFlagsData::new(),
                value,
                frame_ptr,
                i32::try_from(local.index().saturating_mul(8)).unwrap_or(i32::MAX),
            );
        }
        frame_ptr
    };
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
    let success = builder.create_block();
    let side_exit = builder.create_block();
    let is_success = builder.ins().icmp_imm(
        IntCC::Equal,
        status,
        i64::from(crate::JitCallStatus::RETURN.0),
    );
    builder.ins().brif(is_success, success, &[], side_exit, &[]);
    builder.switch_to_block(side_exit);
    let control_value = builder.ins().stack_load(
        types::I64,
        out_slot,
        (std::mem::offset_of!(crate::JitCallResult, value)
            + std::mem::offset_of!(crate::JitAbiSlot, payload)) as i32,
    );
    builder
        .ins()
        .store(MemFlagsData::new(), control_value, result_out, 0);
    builder.ins().return_(&[status]);
    builder.switch_to_block(success);
    if !locals.is_empty() {
        for (local, variable) in locals {
            let value = builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                caller_frame,
                i32::try_from(local.index().saturating_mul(8)).unwrap_or(i32::MAX),
            );
            builder.def_var(*variable, value);
        }
    }
    if let Some(destination) = destination {
        let value = builder.ins().stack_load(
            types::I64,
            out_slot,
            (std::mem::offset_of!(crate::JitCallResult, value)
                + std::mem::offset_of!(crate::JitAbiSlot, payload)) as i32,
        );
        define_region_register(builder, register_variables, registers, destination, value)?;
    }
    Ok(())
}
