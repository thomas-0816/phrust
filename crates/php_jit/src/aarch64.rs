//! Minimal aarch64 (ARM64) machine-code encoder for the native tier's
//! copy-and-patch codegen.
//!
//! This is pure, safe code: it only builds a little-endian byte buffer of
//! encoded instructions and resolves forward branch fixups. Making those bytes
//! executable and calling them is the caller's concern, through
//! [`crate::code_memory::CodeMemory`]. The instruction set is intentionally tiny
//! — enough to emit simple integer leaf sequences with a guard/side-exit branch,
//! the shape a copy-and-patch `guarded_int_arithmetic` stencil needs — and grows
//! as the tier covers more dense opcodes. Every encoder is checked against
//! known-good machine code in the unit tests.

/// aarch64 general-purpose register index. `x0..x30` are `0..=30`; `31` denotes
/// the zero register `xzr` (or `sp`, depending on instruction).
pub type Reg = u8;

/// First argument / return-value register.
pub const X0: Reg = 0;
/// Second argument register.
pub const X1: Reg = 1;
/// Third argument register.
pub const X2: Reg = 2;
/// Scratch register.
pub const X3: Reg = 3;
/// Zero register.
pub const XZR: Reg = 31;
/// Link register (return address), used by `ret`.
pub const LR: Reg = 30;

/// Condition codes for conditional branches.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cond {
    /// Signed overflow set (`V == 1`) — used for guarded arithmetic side exits.
    Overflow,
    /// Equal (`Z == 1`).
    Equal,
    /// Not equal (`Z == 0`).
    NotEqual,
}

impl Cond {
    const fn encoding(self) -> u32 {
        match self {
            Self::Overflow => 0b0110, // VS
            Self::Equal => 0b0000,    // EQ
            Self::NotEqual => 0b0001, // NE
        }
    }
}

/// A branch target within the emitted code. Create with
/// [`Aarch64Assembler::new_label`], place with [`Aarch64Assembler::bind`].
#[derive(Clone, Copy, Debug)]
pub struct Label(usize);

/// Accumulates little-endian aarch64 machine code and resolves forward branches.
#[derive(Clone, Debug, Default)]
pub struct Aarch64Assembler {
    code: Vec<u8>,
    /// Bound byte offset of each label (`None` until bound).
    labels: Vec<Option<usize>>,
    /// `(branch instruction byte offset, label id)` for conditional branches.
    fixups: Vec<(usize, usize)>,
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

    /// Allocates a fresh, unbound branch label.
    pub fn new_label(&mut self) -> Label {
        let id = self.labels.len();
        self.labels.push(None);
        Label(id)
    }

    /// Binds `label` to the current emission point.
    pub fn bind(&mut self, label: Label) {
        self.labels[label.0] = Some(self.code.len());
    }

    /// `movz Xd, #imm16` — move a 16-bit immediate, zeroing the rest of `Xd`.
    pub fn movz(&mut self, rd: Reg, imm16: u16) {
        self.emit(0xD280_0000 | (u32::from(imm16) << 5) | u32::from(rd));
    }

    /// `add Xd, Xn, Xm` (64-bit register add).
    pub fn add(&mut self, rd: Reg, rn: Reg, rm: Reg) {
        self.emit(0x8B00_0000 | (u32::from(rm) << 16) | (u32::from(rn) << 5) | u32::from(rd));
    }

    /// `adds Xd, Xn, Xm` — add and set the condition flags (for overflow guards).
    pub fn adds(&mut self, rd: Reg, rn: Reg, rm: Reg) {
        self.emit(0xAB00_0000 | (u32::from(rm) << 16) | (u32::from(rn) << 5) | u32::from(rd));
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

    /// `str Xt, [Xn]` — store the 64-bit register `Xt` to `[Xn]` (zero offset).
    pub fn str_reg(&mut self, rt: Reg, rn: Reg) {
        self.emit(0xF900_0000 | (u32::from(rn) << 5) | u32::from(rt));
    }

    /// `b.<cond> label` — conditional branch to a (forward or bound) label. The
    /// 19-bit displacement is filled in by [`Aarch64Assembler::finish`].
    pub fn b_cond(&mut self, cond: Cond, label: Label) {
        self.fixups.push((self.code.len(), label.0));
        self.emit(0x5400_0000 | cond.encoding());
    }

    /// `ret` — return to the address in the link register (`x30`).
    pub fn ret(&mut self) {
        self.emit(0xD65F_0000 | (u32::from(LR) << 5));
    }

    /// Borrows the accumulated machine code (before fixups are resolved).
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.code
    }

    /// Resolves branch fixups and returns the finished machine code.
    ///
    /// # Panics
    /// Panics if a branched-to label was never bound.
    #[must_use]
    pub fn finish(mut self) -> Vec<u8> {
        for (pos, label_id) in &self.fixups {
            let target = self.labels[*label_id].expect("branch label must be bound");
            // Branch displacement is PC-relative to the branch, in instructions.
            let offset_insns = (target as isize - *pos as isize) / 4;
            let imm19 = (offset_insns as u32) & 0x0007_FFFF;
            let base = u32::from_le_bytes(self.code[*pos..*pos + 4].try_into().unwrap());
            let patched = base | (imm19 << 5);
            self.code[*pos..*pos + 4].copy_from_slice(&patched.to_le_bytes());
        }
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
        asm.adds(X3, X0, X1); // adds x3, x0, x1 -> 0xAB010003
        asm.sub(X0, X0, X1); // sub x0, x0, x1 -> 0xCB010000
        asm.mul(X0, X0, X1); // mul x0, x0, x1 -> 0x9B017C00
        asm.mov(X1, X0); // mov x1, x0    -> 0xAA0003E1
        asm.str_reg(X3, X2); // str x3, [x2]  -> 0xF9000043
        asm.ret(); // ret            -> 0xD65F03C0
        assert_eq!(
            asm.finish(),
            vec![
                0x40, 0x05, 0x80, 0xD2, // movz x0, #42
                0x00, 0x00, 0x01, 0x8B, // add x0, x0, x1
                0x03, 0x00, 0x01, 0xAB, // adds x3, x0, x1
                0x00, 0x00, 0x01, 0xCB, // sub x0, x0, x1
                0x00, 0x7C, 0x01, 0x9B, // mul x0, x0, x1
                0xE1, 0x03, 0x00, 0xAA, // mov x1, x0
                0x43, 0x00, 0x00, 0xF9, // str x3, [x2]
                0xC0, 0x03, 0x5F, 0xD6, // ret
            ]
        );
    }

    #[test]
    fn resolves_forward_conditional_branch() {
        // b.vs +2 insns ; ret ; <target> ret  — displacement should be 2.
        let mut asm = Aarch64Assembler::new();
        let target = asm.new_label();
        asm.b_cond(Cond::Overflow, target); // 0x54000006 | (2 << 5) = 0x54000046
        asm.ret();
        asm.bind(target);
        asm.ret();
        let code = asm.finish();
        assert_eq!(
            &code[0..4],
            &[0x46, 0x00, 0x00, 0x54],
            "b.vs displacement must resolve to +2 instructions"
        );
    }
}
