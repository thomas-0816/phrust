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
/// Scratch register.
pub const X4: Reg = 4;
/// Scratch register.
pub const X5: Reg = 5;
/// Scratch register.
pub const X6: Reg = 6;
/// Scratch register commonly used to hold a called address.
pub const X9: Reg = 9;
/// Frame pointer.
pub const FP: Reg = 29;
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
    /// Signed less than (`LT`).
    LessThan,
    /// Signed less than or equal (`LE`).
    LessEqual,
    /// Signed greater than (`GT`).
    GreaterThan,
    /// Signed greater than or equal (`GE`).
    GreaterEqual,
}

impl Cond {
    const fn encoding(self) -> u32 {
        match self {
            Self::Overflow => 0b0110,     // VS
            Self::Equal => 0b0000,        // EQ
            Self::NotEqual => 0b0001,     // NE
            Self::LessThan => 0b1011,     // LT
            Self::LessEqual => 0b1101,    // LE
            Self::GreaterThan => 0b1100,  // GT
            Self::GreaterEqual => 0b1010, // GE
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

    /// `movk Xd, #imm16, LSL #(16*hw)` — insert a 16-bit immediate into the
    /// `hw`-th 16-bit lane of `Xd`, keeping the other lanes.
    pub fn movk(&mut self, rd: Reg, imm16: u16, hw: u8) {
        self.emit(0xF280_0000 | (u32::from(hw) << 21) | (u32::from(imm16) << 5) | u32::from(rd));
    }

    /// Materializes a full 64-bit immediate (e.g. a runtime-resolved helper
    /// address) into `Xd` with a `movz`+`movk`×3 sequence.
    pub fn mov_imm64(&mut self, rd: Reg, value: u64) {
        self.movz(rd, (value & 0xFFFF) as u16);
        self.movk(rd, ((value >> 16) & 0xFFFF) as u16, 1);
        self.movk(rd, ((value >> 32) & 0xFFFF) as u16, 2);
        self.movk(rd, ((value >> 48) & 0xFFFF) as u16, 3);
    }

    /// `blr Xn` — branch with link to the address in `Xn` (call a helper).
    pub fn blr(&mut self, rn: Reg) {
        self.emit(0xD63F_0000 | (u32::from(rn) << 5));
    }

    /// `stp x29, x30, [sp, #-16]!` — non-leaf prologue: save frame pointer and
    /// link register before a `blr` clobbers `x30`.
    pub fn push_fp_lr(&mut self) {
        self.emit(0xA9BF_7BFD);
    }

    /// `ldp x29, x30, [sp], #16` — non-leaf epilogue: restore frame pointer and
    /// link register before `ret`.
    pub fn pop_fp_lr(&mut self) {
        self.emit(0xA8C1_7BFD);
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

    /// `subs Xd, Xn, Xm` — subtract and set the condition flags (for overflow
    /// guards on PHP integer subtraction, checked via `b.vs`).
    pub fn subs(&mut self, rd: Reg, rn: Reg, rm: Reg) {
        self.emit(0xEB00_0000 | (u32::from(rm) << 16) | (u32::from(rn) << 5) | u32::from(rd));
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

    /// `smulh Xd, Xn, Xm` — signed multiply returning the high 64 bits of the
    /// 128-bit product. Paired with [`Aarch64Assembler::mul`] and
    /// [`Aarch64Assembler::cmp_shifted_asr63`] to detect signed 64-bit overflow.
    pub fn smulh(&mut self, rd: Reg, rn: Reg, rm: Reg) {
        self.emit(0x9B40_7C00 | (u32::from(rm) << 16) | (u32::from(rn) << 5) | u32::from(rd));
    }

    /// `mov Xd, Xm` (register move, encoded as `orr Xd, xzr, Xm`).
    pub fn mov(&mut self, rd: Reg, rm: Reg) {
        self.emit(0xAA00_0000 | (u32::from(rm) << 16) | (u32::from(XZR) << 5) | u32::from(rd));
    }

    /// `str Xt, [Xn]` — store the 64-bit register `Xt` to `[Xn]` (zero offset).
    pub fn str_reg(&mut self, rt: Reg, rn: Reg) {
        self.str_x(rt, rn, 0);
    }

    /// `ldr Wt, [Xn, #byte_offset]` — load 32 bits (e.g. a `repr(u32)` tag).
    /// `byte_offset` must be a multiple of 4.
    pub fn ldr_w(&mut self, rt: Reg, rn: Reg, byte_offset: u32) {
        let imm12 = byte_offset / 4;
        self.emit(0xB940_0000 | (imm12 << 10) | (u32::from(rn) << 5) | u32::from(rt));
    }

    /// `ldr Xt, [Xn, #byte_offset]` — load 64 bits (e.g. a value payload).
    /// `byte_offset` must be a multiple of 8.
    pub fn ldr_x(&mut self, rt: Reg, rn: Reg, byte_offset: u32) {
        let imm12 = byte_offset / 8;
        self.emit(0xF940_0000 | (imm12 << 10) | (u32::from(rn) << 5) | u32::from(rt));
    }

    /// `str Wt, [Xn, #byte_offset]` — store 32 bits. Multiple of 4.
    pub fn str_w(&mut self, rt: Reg, rn: Reg, byte_offset: u32) {
        let imm12 = byte_offset / 4;
        self.emit(0xB900_0000 | (imm12 << 10) | (u32::from(rn) << 5) | u32::from(rt));
    }

    /// `str Xt, [Xn, #byte_offset]` — store 64 bits. Multiple of 8.
    pub fn str_x(&mut self, rt: Reg, rn: Reg, byte_offset: u32) {
        let imm12 = byte_offset / 8;
        self.emit(0xF900_0000 | (imm12 << 10) | (u32::from(rn) << 5) | u32::from(rt));
    }

    /// `cmp Wn, #imm12` — compare a 32-bit register to an immediate, setting
    /// flags (`subs wzr, Wn, #imm`). Used to guard a value tag.
    pub fn cmp_imm_w(&mut self, rn: Reg, imm12: u16) {
        self.emit(0x7100_0000 | (u32::from(imm12) << 10) | (u32::from(rn) << 5) | u32::from(XZR));
    }

    /// `cmp Xn, Xm, asr #63` — compare `Xn` against the arithmetic-shift-right
    /// (sign extension) of `Xm`, setting flags (`subs xzr, Xn, Xm, asr #63`).
    /// After `smulh`/`mul`, `Xn` = product high bits and `Xm` = product low
    /// bits; equal means the signed product fits in 64 bits, so a `b.ne` side
    /// exit fires exactly on multiplication overflow.
    pub fn cmp_shifted_asr63(&mut self, rn: Reg, rm: Reg) {
        self.emit(
            0xEB80_0000
                | (u32::from(rm) << 16)
                | (63 << 10)
                | (u32::from(rn) << 5)
                | u32::from(XZR),
        );
    }

    /// `cmp Xn, Xm` — compare two 64-bit registers, setting flags
    /// (`subs xzr, Xn, Xm`). Followed by a signed `b.<cond>` for loop
    /// conditions and integer comparisons.
    pub fn cmp_reg(&mut self, rn: Reg, rm: Reg) {
        self.subs(XZR, rn, rm);
    }

    /// `cset Xd, <cond>` — set `Xd` to 1 if `cond` holds after a `cmp`, else 0
    /// (encoded as `csinc Xd, xzr, xzr, invert(cond)`). Materializes a PHP bool
    /// from an integer comparison. Inverting a condition is `encoding ^ 1`.
    pub fn cset(&mut self, rd: Reg, cond: Cond) {
        let inverted = cond.encoding() ^ 1;
        self.emit(
            0x9A80_0400
                | (u32::from(XZR) << 16)
                | (inverted << 12)
                | (u32::from(XZR) << 5)
                | u32::from(rd),
        );
    }

    /// `b label` — unconditional branch to a (forward or bound) label.
    pub fn b(&mut self, label: Label) {
        self.fixups.push((self.code.len(), label.0));
        self.emit(0x1400_0000);
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
            // Branch displacement is PC-relative to the branch, in instructions
            // (may be negative for a backward branch to a bound label).
            let offset_insns = (target as isize - *pos as isize) / 4;
            let base = u32::from_le_bytes(self.code[*pos..*pos + 4].try_into().unwrap());
            let patched = if base & 0xFC00_0000 == 0x1400_0000 {
                // Unconditional `b`: 26-bit signed imm at bits 0..=25.
                base | ((offset_insns as u32) & 0x03FF_FFFF)
            } else {
                // Conditional `b.cond`: 19-bit signed imm at bits 5..=23.
                base | (((offset_insns as u32) & 0x0007_FFFF) << 5)
            };
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
    fn encodes_widened_arithmetic() {
        let mut asm = Aarch64Assembler::new();
        asm.subs(X3, X0, X1); // subs x3, x0, x1        -> 0xEB010003
        asm.smulh(X3, X4, X5); // smulh x3, x4, x5      -> 0x9B457C83
        asm.cmp_shifted_asr63(X3, X6); // cmp x3, x6, asr #63 -> 0xEB86FC7F
        assert_eq!(
            asm.finish(),
            vec![
                0x03, 0x00, 0x01, 0xEB, // subs x3, x0, x1
                0x83, 0x7C, 0x45, 0x9B, // smulh x3, x4, x5
                0x7F, 0xFC, 0x86, 0xEB, // cmp x3, x6, asr #63
            ]
        );
    }

    #[test]
    fn encodes_compare_and_forward_backward_branches() {
        // A loop skeleton: cmp ; b.ge end ; b header(back) ; end:
        let mut asm = Aarch64Assembler::new();
        asm.cmp_reg(X3, X4); // byte 0: subs xzr, x3, x4 -> 0xEB04007F
        let header = asm.new_label();
        let end = asm.new_label();
        asm.bind(header); // byte 4
        asm.b_cond(Cond::GreaterEqual, end); // byte 4: b.ge end (fwd +2)
        asm.b(header); // byte 8: b header (back -1)
        asm.bind(end); // byte 12
        let code = asm.finish();
        assert_eq!(&code[0..4], &[0x7F, 0x00, 0x04, 0xEB], "cmp x3, x4");
        assert_eq!(&code[4..8], &[0x4A, 0x00, 0x00, 0x54], "b.ge end (+2)");
        assert_eq!(&code[8..12], &[0xFF, 0xFF, 0xFF, 0x17], "b header (-1)");
    }

    #[test]
    fn encodes_memory_and_compare() {
        let mut asm = Aarch64Assembler::new();
        asm.ldr_w(X3, X0, 0); // ldr w3, [x0]      -> 0xB9400003
        asm.ldr_x(X0, X0, 8); // ldr x0, [x0, #8]  -> 0xF9400400
        asm.str_w(X3, X2, 0); // str w3, [x2]      -> 0xB9000043
        asm.str_x(X0, X2, 8); // str x0, [x2, #8]  -> 0xF9000440
        asm.cmp_imm_w(X3, 3); // cmp w3, #3        -> 0x71000C7F
        assert_eq!(
            asm.finish(),
            vec![
                0x03, 0x00, 0x40, 0xB9, // ldr w3, [x0]
                0x00, 0x04, 0x40, 0xF9, // ldr x0, [x0, #8]
                0x43, 0x00, 0x00, 0xB9, // str w3, [x2]
                0x40, 0x04, 0x00, 0xF9, // str x0, [x2, #8]
                0x7F, 0x0C, 0x00, 0x71, // cmp w3, #3
            ]
        );
    }

    #[test]
    fn encodes_calls_and_frame_ops() {
        let mut asm = Aarch64Assembler::new();
        asm.push_fp_lr(); // stp x29,x30,[sp,#-16]! -> 0xA9BF7BFD
        asm.movk(X9, 0x1234, 1); // movk x9,#0x1234,lsl 16 -> 0xF2A24689
        asm.blr(X9); // blr x9                -> 0xD63F0120
        asm.pop_fp_lr(); // ldp x29,x30,[sp],#16  -> 0xA8C17BFD
        assert_eq!(
            asm.finish(),
            vec![
                0xFD, 0x7B, 0xBF, 0xA9, // push fp/lr
                0x89, 0x46, 0xA2, 0xF2, // movk x9, #0x1234, lsl 16
                0x20, 0x01, 0x3F, 0xD6, // blr x9
                0xFD, 0x7B, 0xC1, 0xA8, // pop fp/lr
            ]
        );
    }

    #[test]
    fn mov_imm64_materializes_low_lane_first() {
        let mut asm = Aarch64Assembler::new();
        asm.mov_imm64(X9, 0xDEAD_BEEF_1234_5678);
        let code = asm.finish();
        assert_eq!(code.len(), 16, "movz + movk x3");
        // First instruction: movz x9, #0x5678 -> 0xD28ACF09.
        assert_eq!(&code[0..4], &0xD28A_CF09u32.to_le_bytes());
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
