//! Minimal aarch64 (ARM64) machine-code encoder for the native tier's
//! copy-and-patch codegen.
//!
//! This is pure, safe code: it only builds a little-endian byte buffer of
//! encoded instructions. Making those bytes executable and calling them is the
//! caller's concern, through [`crate::code_memory::CodeMemory`]. The instruction
//! set is intentionally tiny — enough to emit simple integer leaf sequences —
//! and grows as the copy-and-patch tier covers more dense opcodes. Every encoder
//! is checked against known-good machine code in the unit tests.

/// aarch64 general-purpose register index. `x0..x30` are `0..=30`; `31` denotes
/// the zero register `xzr` (or `sp`, depending on instruction).
pub type Reg = u8;

/// First argument / return-value register.
pub const X0: Reg = 0;
/// Second argument register.
pub const X1: Reg = 1;
/// Zero register.
pub const XZR: Reg = 31;
/// Link register (return address), used by `ret`.
pub const LR: Reg = 30;

/// Accumulates little-endian aarch64 machine code.
#[derive(Clone, Debug, Default)]
pub struct Aarch64Assembler {
    code: Vec<u8>,
}

impl Aarch64Assembler {
    /// Creates an empty assembler.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn emit(&mut self, instruction: u32) {
        self.code.extend_from_slice(&instruction.to_le_bytes());
    }

    /// `movz Xd, #imm16` — move a 16-bit immediate, zeroing the rest of `Xd`.
    pub fn movz(&mut self, rd: Reg, imm16: u16) {
        self.emit(0xD280_0000 | (u32::from(imm16) << 5) | u32::from(rd));
    }

    /// `add Xd, Xn, Xm` (64-bit register add).
    pub fn add(&mut self, rd: Reg, rn: Reg, rm: Reg) {
        self.emit(0x8B00_0000 | (u32::from(rm) << 16) | (u32::from(rn) << 5) | u32::from(rd));
    }

    /// `sub Xd, Xn, Xm` (64-bit register subtract).
    pub fn sub(&mut self, rd: Reg, rn: Reg, rm: Reg) {
        self.emit(0xCB00_0000 | (u32::from(rm) << 16) | (u32::from(rn) << 5) | u32::from(rd));
    }

    /// `mul Xd, Xn, Xm` (64-bit multiply; `madd` with the zero-register addend).
    pub fn mul(&mut self, rd: Reg, rn: Reg, rm: Reg) {
        self.emit(
            0x9B00_0000
                | (u32::from(rm) << 16)
                | (u32::from(XZR) << 10)
                | (u32::from(rn) << 5)
                | u32::from(rd),
        );
    }

    /// `mov Xd, Xm` (register move, encoded as `orr Xd, xzr, Xm`).
    pub fn mov(&mut self, rd: Reg, rm: Reg) {
        self.emit(0xAA00_0000 | (u32::from(rm) << 16) | (u32::from(XZR) << 5) | u32::from(rd));
    }

    /// `ret` — return to the address in the link register (`x30`).
    pub fn ret(&mut self) {
        self.emit(0xD65F_0000 | (u32::from(LR) << 5));
    }

    /// Borrows the accumulated machine code.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.code
    }

    /// Consumes the assembler, returning the accumulated machine code.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        self.code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_known_instructions() {
        let mut asm = Aarch64Assembler::new();
        asm.movz(X0, 42); // movz x0, #42   -> 0xD2800540
        asm.add(X0, X0, X1); // add x0, x0, x1 -> 0x8B010000
        asm.sub(X0, X0, X1); // sub x0, x0, x1 -> 0xCB010000
        asm.mul(X0, X0, X1); // mul x0, x0, x1 -> 0x9B017C00
        asm.mov(X1, X0); // mov x1, x0    -> 0xAA0003E1
        asm.ret(); // ret            -> 0xD65F03C0
        assert_eq!(
            asm.finish(),
            vec![
                0x40, 0x05, 0x80, 0xD2, // movz x0, #42
                0x00, 0x00, 0x01, 0x8B, // add x0, x0, x1
                0x00, 0x00, 0x01, 0xCB, // sub x0, x0, x1
                0x00, 0x7C, 0x01, 0x9B, // mul x0, x0, x1
                0xE1, 0x03, 0x00, 0xAA, // mov x1, x0
                0xC0, 0x03, 0x5F, 0xD6, // ret
            ]
        );
    }
}
