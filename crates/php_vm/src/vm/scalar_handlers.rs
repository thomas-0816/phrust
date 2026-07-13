use super::dispatch_contract::DenseBinaryRequest;
use super::prelude::*;

pub(super) fn int_int_specialization_for_binary_op(
    op: BinaryOp,
) -> Option<QuickeningSpecialization> {
    match op {
        BinaryOp::Add => Some(QuickeningSpecialization::AddIntInt),
        BinaryOp::Sub => Some(QuickeningSpecialization::SubIntInt),
        BinaryOp::Mul => Some(QuickeningSpecialization::MulIntInt),
        BinaryOp::Div
        | BinaryOp::Mod
        | BinaryOp::Concat
        | BinaryOp::Pow
        | BinaryOp::BitAnd
        | BinaryOp::BitOr
        | BinaryOp::BitXor
        | BinaryOp::ShiftLeft
        | BinaryOp::ShiftRight => None,
    }
}

fn dense_binary_op(opcode: DenseOpcode) -> Option<BinaryOp> {
    match opcode {
        DenseOpcode::BinaryAdd => Some(BinaryOp::Add),
        DenseOpcode::BinarySub => Some(BinaryOp::Sub),
        DenseOpcode::BinaryMul => Some(BinaryOp::Mul),
        DenseOpcode::BinaryDiv => Some(BinaryOp::Div),
        DenseOpcode::BinaryMod => Some(BinaryOp::Mod),
        DenseOpcode::BinaryConcat => Some(BinaryOp::Concat),
        DenseOpcode::BinaryPow => Some(BinaryOp::Pow),
        DenseOpcode::BinaryBitAnd => Some(BinaryOp::BitAnd),
        DenseOpcode::BinaryBitOr => Some(BinaryOp::BitOr),
        DenseOpcode::BinaryBitXor => Some(BinaryOp::BitXor),
        DenseOpcode::BinaryShiftLeft => Some(BinaryOp::ShiftLeft),
        DenseOpcode::BinaryShiftRight => Some(BinaryOp::ShiftRight),
        _ => None,
    }
}

pub(super) fn checked_int_binary(op: BinaryOp, lhs: i64, rhs: i64) -> Option<i64> {
    match op {
        BinaryOp::Add => lhs.checked_add(rhs),
        BinaryOp::Sub => lhs.checked_sub(rhs),
        BinaryOp::Mul => lhs.checked_mul(rhs),
        BinaryOp::Div
        | BinaryOp::Mod
        | BinaryOp::Concat
        | BinaryOp::Pow
        | BinaryOp::BitAnd
        | BinaryOp::BitOr
        | BinaryOp::BitXor
        | BinaryOp::ShiftLeft
        | BinaryOp::ShiftRight => None,
    }
}

fn dense_compare_op(opcode: DenseOpcode) -> Option<CompareOp> {
    match opcode {
        DenseOpcode::CompareEqual => Some(CompareOp::Equal),
        DenseOpcode::CompareNotEqual => Some(CompareOp::NotEqual),
        DenseOpcode::CompareIdentical => Some(CompareOp::Identical),
        DenseOpcode::CompareNotIdentical => Some(CompareOp::NotIdentical),
        DenseOpcode::CompareLess => Some(CompareOp::Less),
        DenseOpcode::CompareLessEqual => Some(CompareOp::LessEqual),
        DenseOpcode::CompareGreater => Some(CompareOp::Greater),
        DenseOpcode::CompareGreaterEqual => Some(CompareOp::GreaterEqual),
        DenseOpcode::CompareSpaceship => Some(CompareOp::Spaceship),
        _ => None,
    }
}

fn dense_unary_op(opcode: DenseOpcode) -> Option<UnaryOp> {
    match opcode {
        DenseOpcode::UnaryPlus => Some(UnaryOp::Plus),
        DenseOpcode::UnaryMinus => Some(UnaryOp::Minus),
        DenseOpcode::UnaryNot => Some(UnaryOp::Not),
        DenseOpcode::UnaryBitNot => Some(UnaryOp::BitNot),
        _ => None,
    }
}

pub(super) fn execute_arithmetic(
    op: BinaryOp,
    lhs: NumericValue,
    rhs: NumericValue,
) -> Result<Value, String> {
    match op {
        BinaryOp::Add if !lhs.is_float() && !rhs.is_float() => match (lhs, rhs) {
            (NumericValue::Int(lhs), NumericValue::Int(rhs)) => Ok(lhs
                .checked_add(rhs)
                .map(Value::Int)
                .unwrap_or_else(|| Value::float(lhs as f64 + rhs as f64))),
            _ => unreachable!("guarded by integer check"),
        },
        BinaryOp::Sub if !lhs.is_float() && !rhs.is_float() => match (lhs, rhs) {
            (NumericValue::Int(lhs), NumericValue::Int(rhs)) => Ok(lhs
                .checked_sub(rhs)
                .map(Value::Int)
                .unwrap_or_else(|| Value::float(lhs as f64 - rhs as f64))),
            _ => unreachable!("guarded by integer check"),
        },
        BinaryOp::Mul if !lhs.is_float() && !rhs.is_float() => match (lhs, rhs) {
            (NumericValue::Int(lhs), NumericValue::Int(rhs)) => Ok(lhs
                .checked_mul(rhs)
                .map(Value::Int)
                .unwrap_or_else(|| Value::float(lhs as f64 * rhs as f64))),
            _ => unreachable!("guarded by integer check"),
        },
        BinaryOp::Div => {
            if rhs.as_f64() == 0.0 {
                return Err("division by zero".to_owned());
            }
            if let (NumericValue::Int(lhs), NumericValue::Int(rhs)) = (lhs, rhs)
                && lhs % rhs == 0
            {
                return Ok(Value::Int(lhs / rhs));
            }
            Ok(Value::float(lhs.as_f64() / rhs.as_f64()))
        }
        BinaryOp::Mod => match (lhs, rhs) {
            (NumericValue::Int(_), NumericValue::Int(0)) => Err("modulo by zero".to_owned()),
            (NumericValue::Int(lhs), NumericValue::Int(rhs)) => Ok(Value::Int(lhs % rhs)),
            _ => {
                let rhs = rhs.as_f64() as i64;
                if rhs == 0 {
                    return Err("modulo by zero".to_owned());
                }
                Ok(Value::Int((lhs.as_f64() as i64) % rhs))
            }
        },
        BinaryOp::Add => Ok(Value::float(lhs.as_f64() + rhs.as_f64())),
        BinaryOp::Sub => Ok(Value::float(lhs.as_f64() - rhs.as_f64())),
        BinaryOp::Mul => Ok(Value::float(lhs.as_f64() * rhs.as_f64())),
        BinaryOp::Concat
        | BinaryOp::Pow
        | BinaryOp::BitAnd
        | BinaryOp::BitOr
        | BinaryOp::BitXor
        | BinaryOp::ShiftLeft
        | BinaryOp::ShiftRight => unreachable!("handled outside arithmetic"),
    }
}

pub(super) fn execute_power(lhs: NumericValue, rhs: NumericValue) -> Result<Value, String> {
    if let (NumericValue::Int(lhs), NumericValue::Int(rhs)) = (lhs, rhs)
        && rhs >= 0
        && let Ok(exponent) = u32::try_from(rhs)
        && let Some(value) = lhs.checked_pow(exponent)
    {
        return Ok(Value::Int(value));
    }
    Ok(Value::float(lhs.as_f64().powf(rhs.as_f64())))
}

pub(super) fn execute_bitwise(op: BinaryOp, lhs: &Value, rhs: &Value) -> Result<Value, String> {
    match op {
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor
            if matches!((lhs, rhs), (Value::String(_), Value::String(_))) =>
        {
            let (Value::String(lhs), Value::String(rhs)) = (lhs, rhs) else {
                unreachable!("guarded by string match");
            };
            Ok(Value::String(PhpString::from_bytes(bitwise_string_bytes(
                op,
                lhs.as_bytes(),
                rhs.as_bytes(),
            ))))
        }
        BinaryOp::BitAnd => Ok(Value::Int(to_int(lhs)? & to_int(rhs)?)),
        BinaryOp::BitOr => Ok(Value::Int(to_int(lhs)? | to_int(rhs)?)),
        BinaryOp::BitXor => Ok(Value::Int(to_int(lhs)? ^ to_int(rhs)?)),
        BinaryOp::ShiftLeft => {
            let shift = to_int(rhs)?;
            if shift < 0 {
                return Err("bit shift by negative number".to_owned());
            }
            Ok(Value::Int(to_int(lhs)?.wrapping_shl(shift as u32)))
        }
        BinaryOp::ShiftRight => {
            let shift = to_int(rhs)?;
            if shift < 0 {
                return Err("bit shift by negative number".to_owned());
            }
            Ok(Value::Int(to_int(lhs)?.wrapping_shr(shift as u32)))
        }
        BinaryOp::Add
        | BinaryOp::Sub
        | BinaryOp::Mul
        | BinaryOp::Div
        | BinaryOp::Mod
        | BinaryOp::Concat
        | BinaryOp::Pow => unreachable!("handled outside bitwise"),
    }
}

fn bitwise_string_bytes(op: BinaryOp, lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
    match op {
        BinaryOp::BitAnd => lhs
            .iter()
            .zip(rhs.iter())
            .map(|(left, right)| left & right)
            .collect(),
        BinaryOp::BitXor => lhs
            .iter()
            .zip(rhs.iter())
            .map(|(left, right)| left ^ right)
            .collect(),
        BinaryOp::BitOr => {
            let (longer, shorter, lhs_is_longer) = if lhs.len() >= rhs.len() {
                (lhs, rhs, true)
            } else {
                (rhs, lhs, false)
            };
            let mut bytes = Vec::with_capacity(longer.len());
            for index in 0..longer.len() {
                let byte = match (longer.get(index), shorter.get(index)) {
                    (Some(long), Some(short)) if lhs_is_longer => long | short,
                    (Some(long), Some(short)) => short | long,
                    (Some(long), None) => *long,
                    _ => unreachable!("bounded by longer length"),
                };
                bytes.push(byte);
            }
            bytes
        }
        BinaryOp::Add
        | BinaryOp::Sub
        | BinaryOp::Mul
        | BinaryOp::Div
        | BinaryOp::Mod
        | BinaryOp::Concat
        | BinaryOp::Pow
        | BinaryOp::ShiftLeft
        | BinaryOp::ShiftRight => unreachable!("not a string bitwise op"),
    }
}

/// Deprecation text for a float (or float-string) used in an int-only
/// context when the conversion loses precision; integral in-range floats
/// convert silently.
pub(super) fn implicit_int_deprecation_message(value: &Value) -> Option<String> {
    match effective_value(value) {
        Value::Float(float_value) => {
            let raw = float_value.to_f64();
            let lossless = php_runtime::api::float_fits_int(raw) && raw.trunc() == raw;
            (!lossless).then(|| {
                let rendered = to_string(&Value::float(raw))
                    .map(|text| text.to_string_lossy())
                    .unwrap_or_else(|_| raw.to_string());
                format!("Implicit conversion from float {rendered} to int loses precision")
            })
        }
        Value::String(text) => {
            let Ok(NumericValue::Float(raw)) = to_number(&Value::String(text.clone())) else {
                return None;
            };
            let lossless = php_runtime::api::float_fits_int(raw) && raw.trunc() == raw;
            (!lossless).then(|| {
                format!(
                    "Implicit conversion from float-string \"{}\" to int loses precision",
                    text.to_string_lossy()
                )
            })
        }
        _ => None,
    }
}

/// Executes a unary operator; the second tuple slot carries a pending
/// implicit-int-conversion deprecation for the stateful caller to emit.
fn execute_unary(op: UnaryOp, src: &Value) -> Result<(Value, Option<String>), String> {
    match op {
        UnaryOp::Plus => match to_number(src)? {
            NumericValue::Int(value) => Ok((Value::Int(value), None)),
            NumericValue::Float(value) => Ok((Value::float(value), None)),
        },
        UnaryOp::Minus => match to_number(src)? {
            NumericValue::Int(value) => Ok((
                value
                    .checked_neg()
                    .map(Value::Int)
                    .unwrap_or_else(|| Value::float(-(value as f64))),
                None,
            )),
            NumericValue::Float(value) => Ok((Value::float(-value), None)),
        },
        UnaryOp::Not => Ok((Value::Bool(!to_bool(src)?), None)),
        UnaryOp::BitNot => match effective_value(src) {
            Value::Int(value) => Ok((Value::Int(!value), None)),
            Value::String(value) => {
                let bytes: Vec<u8> = value.as_bytes().iter().map(|byte| !byte).collect();
                Ok((Value::String(PhpString::from_bytes(bytes)), None))
            }
            Value::Float(value) => Ok((
                Value::Int(!php_runtime::api::php_float_to_int(value.to_f64())),
                implicit_int_deprecation_message(src),
            )),
            _ => Err("bitwise not is only implemented for int and string operands".to_owned()),
        },
    }
}

fn execute_compare(op: CompareOp, lhs: &Value, rhs: &Value) -> Result<Value, String> {
    let value = match op {
        CompareOp::Equal => Value::Bool(equal(lhs, rhs)?),
        CompareOp::NotEqual => Value::Bool(!equal(lhs, rhs)?),
        CompareOp::Identical => Value::Bool(identical(lhs, rhs)),
        CompareOp::NotIdentical => Value::Bool(!identical(lhs, rhs)),
        CompareOp::Less => Value::Bool(compare(lhs, rhs)?.is_lt()),
        CompareOp::LessEqual => Value::Bool(!compare(lhs, rhs)?.is_gt()),
        CompareOp::Greater => Value::Bool(compare(lhs, rhs)?.is_gt()),
        CompareOp::GreaterEqual => Value::Bool(!compare(lhs, rhs)?.is_lt()),
        CompareOp::Spaceship => {
            let result = match compare(lhs, rhs)? {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            };
            Value::Int(result)
        }
    };
    Ok(value)
}

pub(super) fn execute_rich_compare_op(
    request: RichCompareRequest<'_>,
    stack: &mut CallStack,
) -> Result<(), String> {
    let RichCompareRequest {
        unit,
        frame_index,
        dst,
        op,
        lhs,
        rhs,
    } = request;
    let lhs = read_operand_at_frame(unit, stack, frame_index, lhs)?;
    let rhs = read_operand_at_frame(unit, stack, frame_index, rhs)?;
    let value = execute_compare(op, &lhs, &rhs)?;
    stack
        .frame_mut(frame_index)
        .expect("frame was pushed")
        .registers
        .set(dst, value)?;
    Ok(())
}

pub(super) fn execute_rich_binary_op(
    vm: &Vm,
    request: RichBinaryRequest<'_>,
    output: &mut OutputBuffer,
    stack: &mut CallStack,
    state: &mut ExecutionState,
) -> Result<(), RichBinaryError> {
    let RichBinaryRequest {
        compiled,
        unit,
        frame_index,
        function_id,
        block_id,
        instruction_id,
        dst,
        op,
        lhs,
        rhs,
        span,
    } = request;
    let lhs = read_operand_at_frame(unit, stack, frame_index, lhs).map_err(|message| {
        RichBinaryError::Direct(Box::new(vm.runtime_error(output, compiled, stack, message)))
    })?;
    let rhs = read_operand_at_frame(unit, stack, frame_index, rhs).map_err(|message| {
        RichBinaryError::Direct(Box::new(vm.runtime_error(output, compiled, stack, message)))
    })?;
    let value = match op {
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => {
            vm.try_quickened_int_int_binary(function_id, block_id, instruction_id, op, &lhs, &rhs)
        }
        BinaryOp::Concat => {
            vm.try_quickened_concat_string_string(function_id, block_id, instruction_id, &lhs, &rhs)
        }
        _ => None,
    };
    let value = match value {
        Some(value) => value,
        None => vm
            .execute_binary(
                ExecutionCursor::new(compiled, output, stack, state),
                op,
                &lhs,
                &rhs,
                runtime_source_span(compiled, span),
            )
            .map_err(|result| RichBinaryError::Route(Box::new(*result)))?,
    };
    stack
        .frame_mut(frame_index)
        .expect("frame was pushed")
        .registers
        .set(dst, value)
        .map_err(|message| {
            RichBinaryError::Direct(Box::new(vm.runtime_error(output, compiled, stack, message)))
        })?;
    Ok(())
}

pub(super) fn execute_rich_unary_op(
    request: RichUnaryRequest<'_>,
    stack: &mut CallStack,
) -> Result<Option<String>, String> {
    let RichUnaryRequest {
        unit,
        frame_index,
        dst,
        op,
        src,
    } = request;
    let src = read_operand_at_frame(unit, stack, frame_index, src)?;
    let (value, deprecation) = execute_unary(op, &src)?;
    stack
        .frame_mut(frame_index)
        .expect("frame was pushed")
        .registers
        .set(dst, value)?;
    Ok(deprecation)
}

impl Vm {
    pub(super) fn execute_dense_binary_op(
        &self,
        request: DenseBinaryRequest<'_>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), Box<VmResult>> {
        let DenseBinaryRequest {
            compiled,
            unit_id,
            function_id,
            instruction_index,
            opcode,
            dst,
            lhs,
            rhs,
            span,
        } = request;
        let lhs = self
            .read_dense_operand_ref(compiled, stack, lhs)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        let rhs = self
            .read_dense_operand_ref(compiled, stack, rhs)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        let op = dense_binary_op(opcode).expect("dense binary opcode matched");
        let value = match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => self
                .try_quickened_dense_int_int_binary(
                    unit_id,
                    function_id,
                    instruction_index,
                    op,
                    lhs.as_value(),
                    rhs.as_value(),
                ),
            BinaryOp::Concat => self.try_quickened_dense_concat_string_string(
                unit_id,
                function_id,
                instruction_index,
                lhs.as_value(),
                rhs.as_value(),
            ),
            _ => None,
        };
        let value = match value {
            Some(value) => value,
            None => {
                let lhs = lhs.into_owned();
                let rhs = rhs.into_owned();
                self.execute_binary(
                    ExecutionCursor::new(compiled, output, stack, state),
                    op,
                    &lhs,
                    &rhs,
                    runtime_source_span(compiled, span),
                )?
            }
        };
        stack
            .current_mut()
            .expect("bytecode frame was pushed")
            .registers
            .set(RegId::new(dst), value)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        Ok(())
    }

    pub(super) fn execute_dense_compare_op(
        &self,
        compiled: &CompiledUnit,
        stack: &mut CallStack,
        opcode: DenseOpcode,
        dst: u32,
        lhs: DenseOperand,
        rhs: DenseOperand,
    ) -> Result<(), String> {
        let lhs = self.read_dense_operand_ref(compiled, stack, lhs)?;
        let rhs = self.read_dense_operand_ref(compiled, stack, rhs)?;
        let op = dense_compare_op(opcode).expect("dense compare opcode matched");
        let value = execute_compare(op, lhs.as_value(), rhs.as_value())?;
        stack
            .current_mut()
            .expect("bytecode frame was pushed")
            .registers
            .set(RegId::new(dst), value)?;
        Ok(())
    }

    pub(super) fn execute_dense_unary_op(
        &self,
        compiled: &CompiledUnit,
        stack: &mut CallStack,
        opcode: DenseOpcode,
        dst: u32,
        src: DenseOperand,
    ) -> Result<Option<String>, String> {
        let src = self.read_dense_operand_ref(compiled, stack, src)?;
        let op = dense_unary_op(opcode).expect("dense unary opcode matched");
        let (value, deprecation) = execute_unary(op, src.as_value())?;
        stack
            .current_mut()
            .expect("bytecode frame was pushed")
            .registers
            .set(RegId::new(dst), value)?;
        Ok(deprecation)
    }
}
