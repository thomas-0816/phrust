//! Value-preserving copy-and-patch leaf recognition.

use std::collections::{HashMap, HashSet};

use php_ir::instruction::IrCallArgValueKind;

use super::*;

/// Lower the common one-argument identity wrapper (`return $value`) by copying
/// the complete 24-byte ABI value. Heap handles remain borrowed for the
/// synchronous call and the VM clones them while unmarshaling the result.
pub(super) fn compile_value_passthrough_leaf(
    function: &IrFunction,
) -> Option<CompiledScalarRegion> {
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
    if !matches!(param.type_.as_ref(), None | Some(IrReturnType::Mixed))
        || !matches!(
            function.return_type.as_ref(),
            None | Some(IrReturnType::Mixed)
        )
    {
        return None;
    }
    let [block] = function.blocks.as_slice() else {
        return None;
    };
    let kinds = meaningful_kinds(block);
    let [InstructionKind::LoadLocal { dst, local }] = kinds.as_slice() else {
        return None;
    };
    if *local != param.local {
        return None;
    }
    let TerminatorKind::Return {
        value: Some(Operand::Register(returned)),
        by_ref_local: None,
    } = &block.terminator.as_ref()?.kind
    else {
        return None;
    };
    if returned != dst {
        return None;
    }
    let result_slot = function.local_count.checked_add(function.register_count)?;
    let buffer_slots = result_slot.checked_add(1)?;
    if result_slot > MAX_SLOT || param.local.raw() > MAX_SLOT {
        return None;
    }
    let code = emit_scalar_int_ops(&[ScalarIntOp::Copy {
        dst: result_slot,
        src: param.local.raw(),
    }])
    .ok()?;
    Some(CompiledScalarRegion {
        code,
        result_slot,
        buffer_slots,
        tail_call: None,
    })
}

/// Lower `return callee($a, ...)` wrappers whose arguments are direct parameter
/// loads. The native prefix transports complete ABI values; the VM still
/// resolves/calls the callee and applies the wrapper's return coercion.
pub(super) fn compile_value_tailcall_leaf(
    function: &IrFunction,
    permits: NativeCallPermits,
) -> Option<CompiledScalarRegion> {
    if !permits.allow_userland_tailcall {
        return None;
    }
    let flags = function.flags;
    if flags.is_top_level || flags.is_closure || flags.is_method || flags.is_generator {
        return None;
    }
    if function.returns_by_ref || !function.captures.is_empty() {
        return None;
    }
    if function.params.iter().any(|param| {
        param.by_ref
            || param.variadic
            || param.default.is_some()
            || !matches!(param.type_.as_ref(), None | Some(IrReturnType::Mixed))
    }) || !matches!(
        function.return_type.as_ref(),
        None | Some(IrReturnType::Mixed)
    ) {
        return None;
    }
    let parameter_locals = function
        .params
        .iter()
        .map(|param| param.local)
        .collect::<HashSet<_>>();
    let [block] = function.blocks.as_slice() else {
        return None;
    };
    let kinds = meaningful_kinds(block);
    let (call, loads) = kinds.split_last()?;
    let InstructionKind::CallFunction {
        dst: call_dst,
        name,
        args,
    } = call
    else {
        return None;
    };
    let TerminatorKind::Return {
        value: Some(Operand::Register(returned)),
        by_ref_local: None,
    } = &block.terminator.as_ref()?.kind
    else {
        return None;
    };
    if returned != call_dst {
        return None;
    }
    let mut loaded = HashMap::new();
    for kind in loads {
        let InstructionKind::LoadLocal { dst, local } = kind else {
            return None;
        };
        if !parameter_locals.contains(local) {
            return None;
        }
        loaded.insert(*dst, *local);
    }
    let mut sources = Vec::with_capacity(args.len());
    for arg in args {
        if arg.name.is_some()
            || arg.unpack
            || arg.value_kind == IrCallArgValueKind::ByRefLocationPlaceholder
        {
            return None;
        }
        let local = match arg.value {
            Operand::Register(reg) => *loaded.get(&reg)?,
            Operand::Local(local) if parameter_locals.contains(&local) => local,
            _ => return None,
        };
        sources.push(TailArgSource::ValueCopy { src: local.raw() });
    }
    let first_arg_slot = function.local_count.checked_add(function.register_count)?;
    let buffer_slots = first_arg_slot.checked_add(u32::try_from(args.len()).ok()?)?;
    let (code, arg_slots) = emit_tailcall_region(&[], &sources, &[], first_arg_slot).ok()?;
    Some(CompiledScalarRegion {
        code,
        result_slot: 0,
        buffer_slots,
        tail_call: Some(TailCallPlan {
            callee_name: name.clone(),
            arg_slots,
        }),
    })
}
