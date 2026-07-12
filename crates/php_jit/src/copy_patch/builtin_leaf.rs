//! Single-argument builtin copy-and-patch leaf recognition.

use super::*;

struct SingleArgBuiltinLeaf<'a> {
    param: &'a php_ir::IrParam,
    call_name: String,
    result_slot: u32,
    buffer_slots: u32,
}

fn match_single_arg_builtin_leaf(function: &IrFunction) -> Option<SingleArgBuiltinLeaf<'_>> {
    let flags = function.flags;
    if flags.is_top_level || flags.is_closure || flags.is_method || flags.is_generator {
        return None;
    }
    if function.returns_by_ref || !function.captures.is_empty() {
        return None;
    }
    let [param] = function.params.as_slice() else {
        return None;
    };
    if param.by_ref || param.variadic || param.default.is_some() {
        return None;
    }

    let [block] = function.blocks.as_slice() else {
        return None;
    };
    let kinds = meaningful_kinds(block);
    let [
        InstructionKind::LoadLocal {
            dst: load_reg,
            local: load_local,
        },
        InstructionKind::CallFunction {
            dst: call_dst,
            name,
            args,
        },
    ] = kinds.as_slice()
    else {
        return None;
    };
    if *load_local != param.local {
        return None;
    }
    let [arg] = args.as_slice() else {
        return None;
    };
    if arg.name.is_some() || arg.unpack {
        return None;
    }
    let Operand::Register(arg_reg) = arg.value else {
        return None;
    };
    if arg_reg != *load_reg {
        return None;
    }
    let TerminatorKind::Return {
        value: Some(Operand::Register(ret_reg)),
        by_ref_local: None,
    } = &block.terminator.as_ref()?.kind
    else {
        return None;
    };
    if ret_reg != call_dst {
        return None;
    }

    let result_slot = function.local_count.checked_add(function.register_count)?;
    let buffer_slots = result_slot.checked_add(1)?;
    if result_slot > MAX_SLOT {
        return None;
    }
    Some(SingleArgBuiltinLeaf {
        param,
        call_name: name.clone(),
        result_slot,
        buffer_slots,
    })
}

pub(super) fn compile_scalar_int_count_leaf(
    function: &IrFunction,
    permits: NativeCallPermits,
    helpers: CopyPatchRuntimeHelpers,
) -> Option<CompiledScalarRegion> {
    if !permits.builtin_count || helpers.array_len == 0 {
        return None;
    }
    if !matches!(function.return_type, None | Some(IrReturnType::Int)) {
        return None;
    }
    let leaf = match_single_arg_builtin_leaf(function)?;
    if leaf.call_name != "count" || !matches!(leaf.param.type_, None | Some(IrReturnType::Array)) {
        return None;
    }
    let code = emit_scalar_int_ops(&[ScalarIntOp::CallCountI64 {
        dst: leaf.result_slot,
        arg: leaf.param.local.raw(),
        array_len_helper: helpers.array_len,
    }])
    .ok()?;
    Some(CompiledScalarRegion {
        code,
        result_slot: leaf.result_slot,
        buffer_slots: leaf.buffer_slots,
        tail_call: None,
    })
}

pub(super) fn compile_scalar_int_strlen_leaf(
    function: &IrFunction,
    permits: NativeCallPermits,
    helpers: CopyPatchRuntimeHelpers,
) -> Option<CompiledScalarRegion> {
    if !permits.builtin_strlen || helpers.strlen == 0 {
        return None;
    }
    if !matches!(function.return_type, None | Some(IrReturnType::Int)) {
        return None;
    }
    let leaf = match_single_arg_builtin_leaf(function)?;
    if leaf.call_name != "strlen" || !matches!(leaf.param.type_, None | Some(IrReturnType::String))
    {
        return None;
    }
    let code = emit_scalar_int_ops(&[ScalarIntOp::CallStrlenI64 {
        dst: leaf.result_slot,
        arg: leaf.param.local.raw(),
        strlen_helper: helpers.strlen,
    }])
    .ok()?;
    Some(CompiledScalarRegion {
        code,
        result_slot: leaf.result_slot,
        buffer_slots: leaf.buffer_slots,
        tail_call: None,
    })
}

fn is_type_predicate_tag(name: &str, permits: NativeCallPermits) -> Option<u16> {
    match name {
        "is_int" if permits.builtin_is_int => Some(INT_TAG),
        "is_string" if permits.builtin_is_string => Some(STRING_TAG),
        "is_array" if permits.builtin_is_array => Some(ARRAY_TAG),
        "is_float" if permits.builtin_is_float => Some(FLOAT_TAG),
        "is_bool" if permits.builtin_is_bool => Some(BOOL_TAG),
        "is_null" if permits.builtin_is_null => Some(NULL_TAG),
        "is_object" if permits.builtin_is_object => Some(OBJECT_TAG),
        _ => None,
    }
}

pub(super) fn compile_scalar_int_is_type_leaf(
    function: &IrFunction,
    permits: NativeCallPermits,
) -> Option<CompiledScalarRegion> {
    if !matches!(function.return_type, None | Some(IrReturnType::Bool)) {
        return None;
    }
    let leaf = match_single_arg_builtin_leaf(function)?;
    let expected_tag = is_type_predicate_tag(&leaf.call_name, permits)?;
    let code = emit_scalar_int_ops(&[ScalarIntOp::IsType {
        dst: leaf.result_slot,
        arg: leaf.param.local.raw(),
        expected_tag,
    }])
    .ok()?;
    Some(CompiledScalarRegion {
        code,
        result_slot: leaf.result_slot,
        buffer_slots: leaf.buffer_slots,
        tail_call: None,
    })
}
