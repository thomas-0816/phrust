use super::dispatch_contract::DenseBinaryRequest;
use super::prelude::*;

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
                compiled,
                op,
                &lhs,
                &rhs,
                runtime_source_span(compiled, span),
                output,
                stack,
                state,
            )
            .map_err(|result| RichBinaryError::Route(Box::new(result)))?,
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
) -> Result<(), String> {
    let RichUnaryRequest {
        unit,
        frame_index,
        dst,
        op,
        src,
    } = request;
    let src = read_operand_at_frame(unit, stack, frame_index, src)?;
    let value = execute_unary(op, &src)?;
    stack
        .frame_mut(frame_index)
        .expect("frame was pushed")
        .registers
        .set(dst, value)?;
    Ok(())
}

impl Vm {
    pub(super) fn execute_dense_binary_op(
        &self,
        request: DenseBinaryRequest<'_>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
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
                    compiled,
                    op,
                    &lhs,
                    &rhs,
                    runtime_source_span(compiled, span),
                    output,
                    stack,
                    state,
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
    ) -> Result<(), String> {
        let src = self.read_dense_operand_ref(compiled, stack, src)?;
        let op = dense_unary_op(opcode).expect("dense unary opcode matched");
        let value = execute_unary(op, src.as_value())?;
        stack
            .current_mut()
            .expect("bytecode frame was pushed")
            .registers
            .set(RegId::new(dst), value)?;
        Ok(())
    }
}
