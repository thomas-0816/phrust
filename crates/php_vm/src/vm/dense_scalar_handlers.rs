use super::prelude::*;

impl Vm {
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
