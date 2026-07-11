use super::*;

pub(super) fn try_execute_dense_pcre_ascii_offset_block_fast_path(
    compiled: &CompiledUnit,
    dense: &DenseBytecodeUnit,
    instructions: &[DenseInstruction],
    stack: &mut CallStack,
    state: &mut ExecutionState,
) -> Result<Option<(u32, bool)>, String> {
    let mut active = [None; 8];
    let mut active_len = 0_usize;
    for instruction in instructions {
        if instruction.opcode == DenseOpcode::Nop {
            continue;
        }
        if active_len == active.len() {
            return Ok(None);
        }
        active[active_len] = Some(instruction);
        active_len += 1;
    }
    if !(5..=8).contains(&active_len) {
        return Ok(None);
    }
    let Some(first_active) = active[0] else {
        return Ok(None);
    };
    let Some((pattern_reg, pattern_const)) = dense_load_const_register(first_active) else {
        return Ok(None);
    };
    if !dense_constant_string_bytes_eq(compiled, pattern_const, br"/\G\w/u") {
        return Ok(None);
    }

    let mut cursor = 1;
    let Some(subject_active) = active[cursor] else {
        return Ok(None);
    };
    let Some((subject_reg, subject_local, fused_flags)) =
        dense_load_local_register_with_optional_const(subject_active)
    else {
        return Ok(None);
    };
    cursor += 1;

    let flags_reg = if let Some((flags_reg, flags_const)) = fused_flags {
        if !dense_constant_exact_int(compiled, flags_const, 0) {
            return Ok(None);
        }
        flags_reg
    } else {
        let Some((flags_reg, flags_const)) = active[cursor].and_then(dense_load_const_register)
        else {
            return Ok(None);
        };
        if !dense_constant_exact_int(compiled, flags_const, 0) {
            return Ok(None);
        }
        cursor += 1;
        flags_reg
    };

    let Some((offset_reg, offset_local, None)) =
        active[cursor].and_then(dense_load_local_register_with_optional_const)
    else {
        return Ok(None);
    };
    cursor += 1;

    let Some((call_dst, name, args)) = active[cursor].and_then(dense_call_function_operands) else {
        return Ok(None);
    };
    cursor += 1;

    while let Some(instruction) = active.get(cursor).and_then(|instruction| *instruction) {
        if instruction.opcode != DenseOpcode::Discard {
            break;
        }
        cursor += 1;
    }

    let Some((condition, if_true, if_false)) = active[cursor].and_then(dense_jump_if_operands)
    else {
        return Ok(None);
    };
    if condition.kind != DenseOperandKind::Register || condition.index != call_dst {
        return Ok(None);
    }

    let Some(name) = dense.names.get(name as usize) else {
        return Ok(None);
    };
    if !name.eq_ignore_ascii_case("preg_match")
        || name.contains('\\')
        || args.len() != 5
        || args.iter().any(|arg| arg.name.is_some())
    {
        return Ok(None);
    }

    if !dense_operand_is_register(args[0].value, pattern_reg)
        || !dense_operand_is_register(args[1].value, subject_reg)
        || !dense_operand_is_register(args[3].value, flags_reg)
        || !dense_operand_is_register(args[4].value, offset_reg)
    {
        return Ok(None);
    }
    let Some(call_subject_local) = dense_plain_by_ref_local(&args[1]) else {
        return Ok(None);
    };
    if call_subject_local != subject_local {
        return Ok(None);
    }
    let Some(matches_local) = dense_plain_by_ref_local(&args[2]) else {
        return Ok(None);
    };
    let Some(call_offset_local) = dense_plain_by_ref_local(&args[4]) else {
        return Ok(None);
    };
    if call_offset_local != offset_local {
        return Ok(None);
    }

    let Some(offset) = dense_local_exact_int(stack, LocalId::new(offset_local)) else {
        return Ok(None);
    };
    if offset < 0 {
        return Ok(None);
    }
    let start = offset as usize;
    let Some(match_result) =
        with_dense_local_string(stack, LocalId::new(subject_local), |subject| {
            let subject_bytes = subject.as_bytes();
            if start > subject_bytes.len() {
                return Ok(None);
            }
            if !state
                .builtins
                .pcre_state_mut()
                .cache_mut()
                .validate_utf8_ascii_subject_at_offset(subject, start)
                .map_err(|error| error.message().to_owned())?
            {
                return Ok(None);
            }
            Ok(Some(match subject_bytes.get(start).copied() {
                Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_' => {
                    DensePcreAsciiOffsetBlockMatch::Matched(byte)
                }
                _ => DensePcreAsciiOffsetBlockMatch::NoMatch,
            }))
        })?
    else {
        return Ok(None);
    };

    let matches = stack
        .current_mut()
        .ok_or_else(|| "no active frame".to_owned())?
        .locals
        .ensure_reference_cell(LocalId::new(matches_local))?;
    state.builtins.pcre_state_mut().last_error_mut().clear();
    let truthy = match match_result {
        DensePcreAsciiOffsetBlockMatch::Matched(byte) => {
            builtin_intrinsics::set_preg_match_single_byte_match(&matches, &[byte]);
            true
        }
        DensePcreAsciiOffsetBlockMatch::NoMatch => {
            builtin_intrinsics::set_preg_match_empty_matches(&matches);
            false
        }
    };
    let next_block = if truthy { if_true } else { if_false };
    Ok(Some((next_block, truthy)))
}

pub(super) fn dense_load_const_register(instruction: &DenseInstruction) -> Option<(u32, u32)> {
    if instruction.opcode != DenseOpcode::LoadConst {
        return None;
    }
    let DenseOperands::RegConst { dst, constant } = instruction.operands else {
        return None;
    };
    Some((dst, constant))
}

type DenseLoadLocalInfo = (u32, u32, Option<(u32, u32)>);

pub(super) fn dense_load_local_register_with_optional_const(
    instruction: &DenseInstruction,
) -> Option<DenseLoadLocalInfo> {
    match instruction.opcode {
        DenseOpcode::LoadLocal => {
            let DenseOperands::RegOperand { dst, src } = instruction.operands else {
                return None;
            };
            if src.kind != DenseOperandKind::Local {
                return None;
            }
            Some((dst, src.index, None))
        }
        DenseOpcode::LoadLocalLoadConst => {
            let DenseOperands::LoadLocalLoadConst {
                first_dst,
                local,
                second_dst,
                constant,
            } = instruction.operands
            else {
                return None;
            };
            Some((first_dst, local.index, Some((second_dst, constant))))
        }
        _ => None,
    }
}

pub(super) fn dense_call_function_operands(
    instruction: &DenseInstruction,
) -> Option<(u32, u32, &[DenseCallArg])> {
    if instruction.opcode != DenseOpcode::CallFunction {
        return None;
    }
    let DenseOperands::Call {
        dst,
        name,
        ref args,
    } = instruction.operands
    else {
        return None;
    };
    Some((dst, name, args))
}

pub(super) fn dense_jump_if_operands(
    instruction: &DenseInstruction,
) -> Option<(DenseOperand, u32, u32)> {
    if instruction.opcode != DenseOpcode::JumpIf {
        return None;
    }
    let DenseOperands::JumpIfElse {
        condition,
        if_true,
        if_false,
    } = instruction.operands
    else {
        return None;
    };
    Some((condition, if_true, if_false))
}

pub(super) fn dense_operand_is_register(operand: DenseOperand, register: u32) -> bool {
    operand.kind == DenseOperandKind::Register && operand.index == register
}

pub(super) fn dense_constant_string_bytes_eq(
    compiled: &CompiledUnit,
    constant: u32,
    expected: &[u8],
) -> bool {
    compiled
        .unit()
        .constants
        .get(constant as usize)
        .is_some_and(|constant| match constant {
            IrConstant::String(value) => value.as_bytes() == expected,
            IrConstant::StringBytes(value) => value.as_slice() == expected,
            _ => false,
        })
}

pub(super) fn dense_constant_exact_int(
    compiled: &CompiledUnit,
    constant: u32,
    expected: i64,
) -> bool {
    compiled
        .unit()
        .constants
        .get(constant as usize)
        .is_some_and(|constant| matches!(constant, IrConstant::Int(value) if *value == expected))
}

pub(super) fn with_dense_local_string<T>(
    stack: &CallStack,
    local: LocalId,
    f: impl FnOnce(&PhpString) -> Result<Option<T>, String>,
) -> Result<Option<T>, String> {
    let Some(slot) = stack
        .current()
        .and_then(|frame| frame.locals.get_slot(local))
    else {
        return Ok(None);
    };
    match slot {
        Slot::Value(Value::String(value)) => f(value),
        Slot::Reference(cell) => cell
            .try_with_value(|value| match value {
                Value::String(value) => f(value),
                _ => Ok(None),
            })
            .unwrap_or_else(|message| Err(message.to_string())),
        _ => Ok(None),
    }
}

pub(super) enum DensePcreAsciiOffsetBlockMatch {
    Matched(u8),
    NoMatch,
}

pub(super) fn dense_plain_by_ref_local(arg: &DenseCallArg) -> Option<u32> {
    if arg.by_ref_dim.is_none()
        && arg.by_ref_property.is_none()
        && arg.by_ref_property_dim.is_none()
    {
        arg.by_ref_local
    } else {
        None
    }
}

pub(super) fn dense_local_exact_int(stack: &CallStack, local: LocalId) -> Option<i64> {
    stack
        .current()
        .and_then(|frame| frame.locals.get_slot(local))
        .and_then(slot_exact_int)
}

pub(super) fn slot_exact_int(slot: &Slot) -> Option<i64> {
    match slot {
        Slot::Value(Value::Int(value)) => Some(*value),
        Slot::Reference(cell) => cell
            .try_with_value(|value| match value {
                Value::Int(value) => Some(*value),
                _ => None,
            })
            .unwrap_or(None),
        _ => None,
    }
}
