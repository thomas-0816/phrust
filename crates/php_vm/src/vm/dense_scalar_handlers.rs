use super::prelude::*;

impl Vm {
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
