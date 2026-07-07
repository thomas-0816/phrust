//! Minimal x86-64 (AMD64) machine-code encoder for the native tier's
//! copy-and-patch codegen — the System V ABI sibling of [`crate::aarch64`].
//!
//! Like the aarch64 encoder, this is pure, safe code: it only builds a
//! little-endian byte buffer of encoded instructions and resolves branch fixups
//! (here `rel32` displacements). Making those bytes executable and calling them
//! is the caller's concern, through [`crate::code_memory::CodeMemory`]. The
//! instruction set mirrors the aarch64 surface — enough to emit the guarded
//! scalar-int / scalar-float leaf and counted-loop stencils the copy-and-patch
//! tier needs — so a follow-up can target x86-64 with the same driver shape.
//!
//! This module intentionally builds and is tested on any host (including
//! arm64): it is a byte-buffer encoder with no execution, and every encoder is
//! checked against known-good machine code in the unit tests.
//!
//! # Calling convention
//!
//! Emitted stencils follow the System V AMD64 ABI, matching the aarch64 tier's
//! `extern "C" fn(slot_base: *mut JitCValue) -> i32` shape:
//!
//! - the slot-base pointer arrives in [`RDI`] (first integer argument);
//! - the `i32` status is returned in `eax` (the low half of [`RAX`]);
//! - [`RAX`], [`RCX`], [`RDX`], [`RSI`], and [`R8`]–[`R11`] are caller-saved
//!   scratch, so a leaf stencil may clobber them freely.

/// x86-64 general-purpose register index (`0..=15`). The low three bits go in
/// ModRM/SIB fields; the fourth bit is carried by a REX prefix bit.
pub type Reg = u8;

/// `rax` — also holds the `i32`/`i64` return value (`eax`/`rax`), the `idiv`
/// dividend, and its quotient.
pub const RAX: Reg = 0;
/// `rcx` — scratch; its low byte `cl` supplies the variable shift count.
pub const RCX: Reg = 1;
/// `rdx` — scratch; `cqo` sign-extends `rax` into it and `idiv` leaves the
/// remainder there.
pub const RDX: Reg = 2;
/// `rsi` — second integer argument / scratch.
pub const RSI: Reg = 6;
/// `rdi` — first integer argument; the slot-base pointer for emitted stencils.
pub const RDI: Reg = 7;
/// `r8` — scratch.
pub const R8: Reg = 8;
/// `r9` — scratch.
pub const R9: Reg = 9;
/// `r10` — scratch.
pub const R10: Reg = 10;
/// `r11` — scratch.
pub const R11: Reg = 11;

/// SSE register index (`0..=15`). Addresses `xmm0..xmm15` in SSE-form
/// instructions; encoded in the same ModRM/REX fields as a GPR index.
pub type Xmm = u8;

/// `xmm0` — double-precision scratch / first result register.
pub const XMM0: Xmm = 0;
/// `xmm1` — double-precision scratch.
pub const XMM1: Xmm = 1;
/// `xmm2` — double-precision scratch (e.g. a zeroed divisor comparand).
pub const XMM2: Xmm = 2;

/// Condition codes for conditional branches and `setcc`. Mirrors
/// [`crate::aarch64::Cond`]; each maps to an x86 condition (`tttn`) nibble used
/// by both `Jcc` (`0F 80+tttn`) and `SETcc` (`0F 90+tttn`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cond {
    /// Signed overflow set (`OF == 1`) — `O`; used for guarded arithmetic side
    /// exits after `add`/`sub`/`imul`.
    Overflow,
    /// Equal (`ZF == 1`) — `E`.
    Equal,
    /// Not equal (`ZF == 0`) — `NE`.
    NotEqual,
    /// Signed less than (`SF != OF`) — `L`.
    LessThan,
    /// Signed less than or equal (`ZF == 1 || SF != OF`) — `LE`.
    LessEqual,
    /// Signed greater than (`ZF == 0 && SF == OF`) — `G`.
    GreaterThan,
    /// Signed greater than or equal (`SF == OF`) — `GE`.
    GreaterEqual,
    /// Unsigned above (`CF == 0 && ZF == 0`) — `A`; used for the shift-range
    /// guard (a negative amount reads as a huge unsigned value) and, after
    /// `ucomisd`, for NaN-correct ordered float `>` (unordered clears the
    /// answer because `ucomisd` sets `CF`/`ZF` on a NaN operand).
    UnsignedHigher,
}

impl Cond {
    /// The x86 `tttn` condition nibble. `Jcc` opcode is `0x80 | tttn`, `SETcc`
    /// opcode is `0x90 | tttn`.
    const fn tttn(self) -> u8 {
        match self {
            Self::Overflow => 0x0,       // O
            Self::Equal => 0x4,          // E / Z
            Self::NotEqual => 0x5,       // NE / NZ
            Self::LessThan => 0xC,       // L
            Self::LessEqual => 0xE,      // LE
            Self::GreaterThan => 0xF,    // G
            Self::GreaterEqual => 0xD,   // GE
            Self::UnsignedHigher => 0x7, // A
        }
    }
}

/// A branch target within the emitted code. Create with
/// [`X86Assembler::new_label`], place with [`X86Assembler::bind`].
#[derive(Clone, Copy, Debug)]
pub struct Label(usize);

/// Accumulates little-endian x86-64 machine code and resolves `rel32` branches.
#[derive(Clone, Debug, Default)]
pub struct X86Assembler {
    code: Vec<u8>,
    /// Bound byte offset of each label (`None` until bound).
    labels: Vec<Option<usize>>,
    /// `(rel32 field byte offset, label id)` for each pending branch. The field
    /// is patched with `target - (field_offset + 4)` in [`X86Assembler::finish`].
    fixups: Vec<(usize, usize)>,
}

impl X86Assembler {
    /// Creates an empty assembler.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn emit(&mut self, bytes: &[u8]) {
        self.code.extend_from_slice(bytes);
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

    // --- REX / ModRM helpers ---
    //
    // REX layout is `0100 WRXB`: W = 64-bit operand size, R extends the ModRM
    // `reg` field, X extends the SIB index (unused here), B extends the ModRM
    // `rm` field (or SIB base). REX, when present, follows any legacy prefix
    // (`66`/`F2`/`F3`) and precedes the opcode (including the `0F` escape).

    /// Emit a register-direct `r/m64, r64`-form op (`reg` operand in ModRM.reg,
    /// `rm` operand in ModRM.rm, `mod = 11`), with a `REX.W` prefix.
    fn emit_rr_w(&mut self, opcode: u8, reg: Reg, rm: Reg) {
        let rex = 0x48 | (u8::from(reg >= 8) << 2) | u8::from(rm >= 8);
        self.emit(&[rex, opcode, 0xC0 | ((reg & 7) << 3) | (rm & 7)]);
    }

    /// Emit a `[base + disp32]` memory operand for `reg_field`, choosing the
    /// SIB form only when the base's low three bits select `rsp`/`r12`.
    fn emit_mem(
        &mut self,
        legacy: Option<u8>,
        w: bool,
        opcode: &[u8],
        reg_field: Reg,
        base: Reg,
        disp: i32,
    ) {
        if let Some(prefix) = legacy {
            self.emit(&[prefix]);
        }
        let rex = (u8::from(w) << 3) | (u8::from(reg_field >= 8) << 2) | u8::from(base >= 8);
        if rex != 0 {
            self.emit(&[0x40 | rex]);
        }
        self.emit(opcode);
        // mod = 10 (disp32) so every base (including a zero displacement to
        // rbp/r13) uses an explicit 4-byte displacement.
        self.emit(&[0x80 | ((reg_field & 7) << 3) | (base & 7)]);
        if base & 7 == 4 {
            // rsp/r12 base: SIB with no index (index = 100, base = 100).
            self.emit(&[0x24]);
        }
        self.emit(&disp.to_le_bytes());
    }

    /// Emit a register-direct SSE op `legacy 0F <op> /r` (`dst` in ModRM.reg,
    /// `src` in ModRM.rm), inserting a REX only when an extended register needs
    /// one.
    fn emit_sse_rr(&mut self, legacy: u8, op: u8, dst: Reg, src: Reg) {
        self.emit(&[legacy]);
        let rex = (u8::from(dst >= 8) << 2) | u8::from(src >= 8);
        if rex != 0 {
            self.emit(&[0x40 | rex]);
        }
        self.emit(&[0x0F, op, 0xC0 | ((dst & 7) << 3) | (src & 7)]);
    }

    // --- Immediates and moves ---

    /// `movabs reg, imm64` (`REX.W B8+rd io`) — materialize a full 64-bit
    /// immediate (e.g. a constant `float`'s IEEE-754 bits, or a helper address).
    pub fn mov_imm64(&mut self, reg: Reg, value: u64) {
        let rex = 0x48 | u8::from(reg >= 8);
        self.emit(&[rex, 0xB8 | (reg & 7)]);
        self.emit(&value.to_le_bytes());
    }

    /// `mov dst, src` (64-bit register move, `REX.W 89 /r`).
    pub fn mov_reg(&mut self, dst: Reg, src: Reg) {
        // 0x89 = MOV r/m64, r64: ModRM.reg is the source, ModRM.rm the dest.
        self.emit_rr_w(0x89, src, dst);
    }

    // --- 64-bit arithmetic (all set the flags) ---

    /// `add dst, src` (`REX.W 01 /r`). Sets `OF`, so a following
    /// [`Cond::Overflow`] branch guards signed overflow.
    pub fn add(&mut self, dst: Reg, src: Reg) {
        self.emit_rr_w(0x01, src, dst);
    }

    /// `sub dst, src` (`REX.W 29 /r`). Sets `OF` for an overflow guard.
    pub fn sub(&mut self, dst: Reg, src: Reg) {
        self.emit_rr_w(0x29, src, dst);
    }

    /// `imul dst, src` (`REX.W 0F AF /r`) — two-operand signed multiply. Sets
    /// `OF`/`CF` when the signed product does not fit in 64 bits, so a following
    /// [`Cond::Overflow`] branch is the multiplication overflow guard.
    pub fn imul(&mut self, dst: Reg, src: Reg) {
        // 0F AF = IMUL r64, r/m64: ModRM.reg is the dest, ModRM.rm the source.
        let rex = 0x48 | (u8::from(dst >= 8) << 2) | u8::from(src >= 8);
        self.emit(&[rex, 0x0F, 0xAF, 0xC0 | ((dst & 7) << 3) | (src & 7)]);
    }

    /// `and dst, src` (`REX.W 21 /r`).
    pub fn and_reg(&mut self, dst: Reg, src: Reg) {
        self.emit_rr_w(0x21, src, dst);
    }

    /// `or dst, src` (`REX.W 09 /r`).
    pub fn or_reg(&mut self, dst: Reg, src: Reg) {
        self.emit_rr_w(0x09, src, dst);
    }

    /// `xor dst, src` (`REX.W 31 /r`).
    pub fn xor_reg(&mut self, dst: Reg, src: Reg) {
        self.emit_rr_w(0x31, src, dst);
    }

    /// `cqo` (`REX.W 99`) — sign-extend `rax` into `rdx:rax` ahead of `idiv`.
    pub fn cqo(&mut self) {
        self.emit(&[0x48, 0x99]);
    }

    /// `idiv divisor` (`REX.W F7 /7`) — signed divide `rdx:rax` by `divisor`,
    /// leaving the quotient in `rax` and the remainder in `rdx`. PHP `%` uses
    /// the remainder; callers must guard a zero divisor (which faults) before
    /// this. Pair with [`X86Assembler::cqo`].
    pub fn idiv(&mut self, divisor: Reg) {
        let rex = 0x48 | u8::from(divisor >= 8);
        self.emit(&[rex, 0xF7, 0xC0 | (7 << 3) | (divisor & 7)]);
    }

    /// `shl reg, cl` (`REX.W D3 /4`) — logical shift left by the count in `cl`.
    /// x86 masks the count to `0..=63`; PHP's `0..=63` domain differs, so
    /// callers guard the amount and side-exit otherwise.
    pub fn shl(&mut self, reg: Reg) {
        let rex = 0x48 | u8::from(reg >= 8);
        self.emit(&[rex, 0xD3, 0xC0 | (4 << 3) | (reg & 7)]);
    }

    /// `sar reg, cl` (`REX.W D3 /7`) — arithmetic shift right by the count in
    /// `cl`. Same `0..=63` masking caveat as [`X86Assembler::shl`].
    pub fn sar(&mut self, reg: Reg) {
        let rex = 0x48 | u8::from(reg >= 8);
        self.emit(&[rex, 0xD3, 0xC0 | (7 << 3) | (reg & 7)]);
    }

    // --- Compares and branches ---

    /// `cmp a, b` (`REX.W 39 /r`) — compute `a - b`, setting flags for a
    /// following signed `Jcc`/`setcc`.
    pub fn cmp_reg(&mut self, a: Reg, b: Reg) {
        // 0x39 = CMP r/m64, r64: ModRM.rm is the first operand, reg the second.
        self.emit_rr_w(0x39, b, a);
    }

    /// `cmp reg, imm32` (`REX.W 81 /7 id`) — compare against a sign-extended
    /// 32-bit immediate (a value tag, `0` divisor, or the `63` shift bound).
    pub fn cmp_imm32(&mut self, reg: Reg, imm: i32) {
        let rex = 0x48 | u8::from(reg >= 8);
        self.emit(&[rex, 0x81, 0xC0 | (7 << 3) | (reg & 7)]);
        self.emit(&imm.to_le_bytes());
    }

    /// `setcc reg` then `movzx reg32, reg8` — materialize the boolean `0`/`1`
    /// for `cond` into `reg` after a `cmp`, zero-extending so the whole 64-bit
    /// register is a clean PHP bool. (`movzx r32` zeroes the upper 32 bits.)
    pub fn setcc(&mut self, reg: Reg, cond: Cond) {
        // SETcc r/m8 (0F 90+tttn). A REX is required to reach the low byte of
        // rsp/rbp/rsi/rdi (4..=7, else the encoding means ah/ch/dh/bh) or an
        // extended register (>= 8, via REX.B).
        if reg >= 8 {
            self.emit(&[0x41]);
        } else if reg >= 4 {
            self.emit(&[0x40]);
        }
        self.emit(&[0x0F, 0x90 | cond.tttn(), 0xC0 | (reg & 7)]);
        // MOVZX r32, r/m8 (0F B6 /r) with dst == src == reg.
        if reg >= 8 {
            self.emit(&[0x45]); // REX.R | REX.B
        } else if reg >= 4 {
            self.emit(&[0x40]);
        }
        self.emit(&[0x0F, 0xB6, 0xC0 | ((reg & 7) << 3) | (reg & 7)]);
    }

    fn emit_rel32(&mut self, label: Label) {
        let field = self.code.len();
        self.fixups.push((field, label.0));
        self.emit(&[0, 0, 0, 0]);
    }

    /// `jcc cond, label` (`0F 80+tttn rel32`) — conditional branch to a forward
    /// or already-bound label; the `rel32` is patched in [`X86Assembler::finish`].
    pub fn jcc(&mut self, cond: Cond, label: Label) {
        self.emit(&[0x0F, 0x80 | cond.tttn()]);
        self.emit_rel32(label);
    }

    /// `jmp label` (`E9 rel32`) — unconditional branch to a forward or bound
    /// label.
    pub fn jmp(&mut self, label: Label) {
        self.emit(&[0xE9]);
        self.emit_rel32(label);
    }

    /// `ret` (`C3`) — return to the caller.
    pub fn ret(&mut self) {
        self.emit(&[0xC3]);
    }

    // --- Loads / stores over `[base + disp32]` (the flat JitCValue slots) ---

    /// `mov reg32, [base + disp]` (`8B /r`) — 32-bit load (e.g. a `repr(u32)`
    /// value tag).
    pub fn ldr_w(&mut self, reg: Reg, base: Reg, disp: i32) {
        self.emit_mem(None, false, &[0x8B], reg, base, disp);
    }

    /// `mov reg64, [base + disp]` (`REX.W 8B /r`) — 64-bit load (a value
    /// payload).
    pub fn ldr_x(&mut self, reg: Reg, base: Reg, disp: i32) {
        self.emit_mem(None, true, &[0x8B], reg, base, disp);
    }

    /// `mov [base + disp], reg32` (`89 /r`) — 32-bit store.
    pub fn str_w(&mut self, reg: Reg, base: Reg, disp: i32) {
        self.emit_mem(None, false, &[0x89], reg, base, disp);
    }

    /// `mov [base + disp], reg64` (`REX.W 89 /r`) — 64-bit store.
    pub fn str_x(&mut self, reg: Reg, base: Reg, disp: i32) {
        self.emit_mem(None, true, &[0x89], reg, base, disp);
    }

    // --- Double-precision SSE2 ---
    //
    // A PHP float is stored as its raw IEEE-754 bits in the slot payload, so a
    // float slot is loaded/stored with `movsd` at the same payload offset an
    // `i64` would use. To materialize a constant double, load its bits into a
    // GPR with `mov_imm64` and move them across with `movq_gpr_to_xmm`.

    /// `movsd xmm, [base + disp]` (`F2 0F 10 /r`) — load a 64-bit double.
    pub fn movsd_load(&mut self, xmm: Xmm, base: Reg, disp: i32) {
        self.emit_mem(Some(0xF2), false, &[0x0F, 0x10], xmm, base, disp);
    }

    /// `movsd [base + disp], xmm` (`F2 0F 11 /r`) — store a 64-bit double.
    pub fn movsd_store(&mut self, xmm: Xmm, base: Reg, disp: i32) {
        self.emit_mem(Some(0xF2), false, &[0x0F, 0x11], xmm, base, disp);
    }

    /// `addsd dst, src` (`F2 0F 58 /r`).
    pub fn addsd(&mut self, dst: Xmm, src: Xmm) {
        self.emit_sse_rr(0xF2, 0x58, dst, src);
    }

    /// `subsd dst, src` (`F2 0F 5C /r`).
    pub fn subsd(&mut self, dst: Xmm, src: Xmm) {
        self.emit_sse_rr(0xF2, 0x5C, dst, src);
    }

    /// `mulsd dst, src` (`F2 0F 59 /r`).
    pub fn mulsd(&mut self, dst: Xmm, src: Xmm) {
        self.emit_sse_rr(0xF2, 0x59, dst, src);
    }

    /// `divsd dst, src` (`F2 0F 5E /r`). PHP `/` raises on a zero divisor, so
    /// callers guard the divisor (see [`X86Assembler::ucomisd`] /
    /// [`X86Assembler::xorpd`]) and side-exit before dividing.
    pub fn divsd(&mut self, dst: Xmm, src: Xmm) {
        self.emit_sse_rr(0xF2, 0x5E, dst, src);
    }

    /// `ucomisd a, b` (`66 0F 2E /r`) — ordered double compare, setting
    /// `ZF`/`PF`/`CF`. On a NaN operand (unordered) all three are set, so a
    /// following [`Cond::Equal`] branch fires for both `a == b` and any NaN —
    /// exactly the conservative behavior a divisor guard wants (comparing the
    /// divisor against a zeroed register catches both `±0.0` and NaN). Ordered
    /// float comparisons use the unsigned conditions (e.g.
    /// [`Cond::UnsignedHigher`] for `>`), which return false on unordered.
    pub fn ucomisd(&mut self, a: Xmm, b: Xmm) {
        self.emit_sse_rr(0x66, 0x2E, a, b);
    }

    /// `xorpd dst, src` (`66 0F 57 /r`) — bitwise XOR; `xorpd x, x` zeroes `x`
    /// to build the `0.0` comparand for a divisor guard.
    pub fn xorpd(&mut self, dst: Xmm, src: Xmm) {
        self.emit_sse_rr(0x66, 0x57, dst, src);
    }

    /// `movq xmm, gpr` (`66 REX.W 0F 6E /r`) — copy the raw 64-bit pattern of a
    /// general register into a double register (materializing a constant double
    /// loaded via [`X86Assembler::mov_imm64`]).
    pub fn movq_gpr_to_xmm(&mut self, xmm: Xmm, gpr: Reg) {
        let rex = 0x48 | (u8::from(xmm >= 8) << 2) | u8::from(gpr >= 8);
        self.emit(&[0x66, rex, 0x0F, 0x6E, 0xC0 | ((xmm & 7) << 3) | (gpr & 7)]);
    }

    /// `movq gpr, xmm` (`66 REX.W 0F 7E /r`) — copy the raw 64-bit pattern of a
    /// double register back into a general register (to store a computed
    /// `float` result's bits).
    pub fn movq_xmm_to_gpr(&mut self, gpr: Reg, xmm: Xmm) {
        let rex = 0x48 | (u8::from(xmm >= 8) << 2) | u8::from(gpr >= 8);
        self.emit(&[0x66, rex, 0x0F, 0x7E, 0xC0 | ((xmm & 7) << 3) | (gpr & 7)]);
    }

    /// Borrows the accumulated machine code (before fixups are resolved).
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.code
    }

    /// Resolves branch fixups and returns the finished machine code.
    ///
    /// Each `rel32` is the signed distance from the end of the branch (the byte
    /// after its 4-byte displacement) to the target, which is what x86 near
    /// branches are relative to.
    ///
    /// # Panics
    /// Panics if a branched-to label was never bound.
    #[must_use]
    pub fn finish(mut self) -> Vec<u8> {
        for (field, label_id) in &self.fixups {
            let target = self.labels[*label_id].expect("branch label must be bound");
            let rel = target as isize - (*field as isize + 4);
            let rel32 = rel as i32;
            self.code[*field..*field + 4].copy_from_slice(&rel32.to_le_bytes());
        }
        self.code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_immediate_and_register_moves() {
        let mut asm = X86Assembler::new();
        asm.mov_imm64(RAX, 0xDEAD_BEEF_1234_5678); // movabs rax, imm
        asm.mov_imm64(R8, 1); // movabs r8, 1
        asm.mov_reg(RAX, RCX); // mov rax, rcx
        assert_eq!(
            asm.finish(),
            vec![
                0x48, 0xB8, 0x78, 0x56, 0x34, 0x12, 0xEF, 0xBE, 0xAD, 0xDE, // movabs rax, ...
                0x49, 0xB8, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // movabs r8, 1
                0x48, 0x89, 0xC8, // mov rax, rcx
            ]
        );
    }

    #[test]
    fn encodes_integer_arithmetic() {
        let mut asm = X86Assembler::new();
        asm.add(RAX, RCX); // add rax, rcx  -> 48 01 C8
        asm.sub(RDX, RSI); // sub rdx, rsi  -> 48 29 F2
        asm.imul(RAX, RCX); // imul rax, rcx -> 48 0F AF C1
        asm.and_reg(RAX, RCX); // and rax, rcx -> 48 21 C8
        asm.or_reg(RAX, RCX); // or rax, rcx  -> 48 09 C8
        asm.xor_reg(RAX, RCX); // xor rax, rcx -> 48 31 C8
        assert_eq!(
            asm.finish(),
            vec![
                0x48, 0x01, 0xC8, // add rax, rcx
                0x48, 0x29, 0xF2, // sub rdx, rsi
                0x48, 0x0F, 0xAF, 0xC1, // imul rax, rcx
                0x48, 0x21, 0xC8, // and rax, rcx
                0x48, 0x09, 0xC8, // or rax, rcx
                0x48, 0x31, 0xC8, // xor rax, rcx
            ]
        );
    }

    #[test]
    fn encodes_extended_register_arithmetic() {
        let mut asm = X86Assembler::new();
        asm.imul(R8, R9); // imul r8, r9   -> 4D 0F AF C1
        asm.add(R10, RAX); // add r10, rax  -> 49 01 C2
        asm.mov_reg(R11, RDI); // mov r11, rdi -> 49 89 FB
        assert_eq!(
            asm.finish(),
            vec![
                0x4D, 0x0F, 0xAF, 0xC1, // imul r8, r9
                0x49, 0x01, 0xC2, // add r10, rax
                0x49, 0x89, 0xFB, // mov r11, rdi
            ]
        );
    }

    #[test]
    fn encodes_division_and_shifts() {
        let mut asm = X86Assembler::new();
        asm.cqo(); // cqo         -> 48 99
        asm.idiv(RCX); // idiv rcx    -> 48 F7 F9
        asm.idiv(R10); // idiv r10    -> 49 F7 FA
        asm.shl(RAX); // shl rax, cl -> 48 D3 E0
        asm.sar(RAX); // sar rax, cl -> 48 D3 F8
        assert_eq!(
            asm.finish(),
            vec![
                0x48, 0x99, // cqo
                0x48, 0xF7, 0xF9, // idiv rcx
                0x49, 0xF7, 0xFA, // idiv r10
                0x48, 0xD3, 0xE0, // shl rax, cl
                0x48, 0xD3, 0xF8, // sar rax, cl
            ]
        );
    }

    #[test]
    fn encodes_compares() {
        let mut asm = X86Assembler::new();
        asm.cmp_reg(RAX, RCX); // cmp rax, rcx -> 48 39 C8
        asm.cmp_imm32(RCX, 63); // cmp rcx, 63  -> 48 81 F9 3F 00 00 00
        asm.cmp_imm32(RAX, 0); // cmp rax, 0   -> 48 81 F8 00 00 00 00
        assert_eq!(
            asm.finish(),
            vec![
                0x48, 0x39, 0xC8, // cmp rax, rcx
                0x48, 0x81, 0xF9, 0x3F, 0x00, 0x00, 0x00, // cmp rcx, 63
                0x48, 0x81, 0xF8, 0x00, 0x00, 0x00, 0x00, // cmp rax, 0
            ]
        );
    }

    #[test]
    fn encodes_setcc_with_zero_extension() {
        // sete al ; movzx eax, al  — low register needs no REX.
        let mut asm = X86Assembler::new();
        asm.setcc(RAX, Cond::Equal);
        assert_eq!(asm.finish(), vec![0x0F, 0x94, 0xC0, 0x0F, 0xB6, 0xC0]);

        // setl cl ; movzx ecx, cl.
        let mut asm = X86Assembler::new();
        asm.setcc(RCX, Cond::LessThan);
        assert_eq!(asm.finish(), vec![0x0F, 0x9C, 0xC1, 0x0F, 0xB6, 0xC9]);

        // rsi (6) needs a REX (0x40) to name sil in both instructions.
        let mut asm = X86Assembler::new();
        asm.setcc(RSI, Cond::Equal);
        assert_eq!(
            asm.finish(),
            vec![0x40, 0x0F, 0x94, 0xC6, 0x40, 0x0F, 0xB6, 0xF6]
        );

        // r8 (8): REX.B for setcc, REX.R|REX.B for movzx.
        let mut asm = X86Assembler::new();
        asm.setcc(R8, Cond::NotEqual);
        assert_eq!(
            asm.finish(),
            vec![0x41, 0x0F, 0x95, 0xC0, 0x45, 0x0F, 0xB6, 0xC0]
        );
    }

    #[test]
    fn encodes_loads_stores_and_ret() {
        let mut asm = X86Assembler::new();
        asm.ldr_w(RAX, RDI, 0); // mov eax, [rdi]      -> 8B 87 00 00 00 00
        asm.ldr_x(RAX, RDI, 8); // mov rax, [rdi+8]    -> 48 8B 87 08 00 00 00
        asm.str_w(RCX, RDI, 0); // mov [rdi], ecx      -> 89 8F 00 00 00 00
        asm.str_x(RCX, RDI, 16); // mov [rdi+16], rcx  -> 48 89 8F 10 00 00 00
        asm.ret(); // ret                              -> C3
        assert_eq!(
            asm.finish(),
            vec![
                0x8B, 0x87, 0x00, 0x00, 0x00, 0x00, // mov eax, [rdi]
                0x48, 0x8B, 0x87, 0x08, 0x00, 0x00, 0x00, // mov rax, [rdi+8]
                0x89, 0x8F, 0x00, 0x00, 0x00, 0x00, // mov [rdi], ecx
                0x48, 0x89, 0x8F, 0x10, 0x00, 0x00, 0x00, // mov [rdi+16], rcx
                0xC3, // ret
            ]
        );
    }

    #[test]
    fn encodes_extended_base_and_reg_memory() {
        let mut asm = X86Assembler::new();
        // mov [rdi+8], r8 — extended source register (REX.R).
        asm.str_x(R8, RDI, 8); // 4C 89 87 08 00 00 00
        // mov r9, [r10+4] — extended reg and base.
        asm.ldr_x(R9, R10, 4); // 4D 8B 8A 04 00 00 00
        assert_eq!(
            asm.finish(),
            vec![
                0x4C, 0x89, 0x87, 0x08, 0x00, 0x00, 0x00, // mov [rdi+8], r8
                0x4D, 0x8B, 0x8A, 0x04, 0x00, 0x00, 0x00, // mov r9, [r10+4]
            ]
        );
    }

    #[test]
    fn encodes_double_precision_sse() {
        let mut asm = X86Assembler::new();
        asm.movsd_load(XMM0, RDI, 8); // movsd xmm0, [rdi+8]  -> F2 0F 10 87 08 00 00 00
        asm.movsd_store(XMM1, RDI, 16); // movsd [rdi+16], xmm1 -> F2 0F 11 8F 10 00 00 00
        asm.addsd(XMM0, XMM1); // addsd xmm0, xmm1  -> F2 0F 58 C1
        asm.subsd(XMM0, XMM1); // subsd xmm0, xmm1  -> F2 0F 5C C1
        asm.mulsd(XMM0, XMM1); // mulsd xmm0, xmm1  -> F2 0F 59 C1
        asm.divsd(XMM0, XMM1); // divsd xmm0, xmm1  -> F2 0F 5E C1
        asm.ucomisd(XMM1, XMM2); // ucomisd xmm1, xmm2 -> 66 0F 2E CA
        asm.xorpd(XMM2, XMM2); // xorpd xmm2, xmm2  -> 66 0F 57 D2
        assert_eq!(
            asm.finish(),
            vec![
                0xF2, 0x0F, 0x10, 0x87, 0x08, 0x00, 0x00, 0x00, // movsd xmm0, [rdi+8]
                0xF2, 0x0F, 0x11, 0x8F, 0x10, 0x00, 0x00, 0x00, // movsd [rdi+16], xmm1
                0xF2, 0x0F, 0x58, 0xC1, // addsd xmm0, xmm1
                0xF2, 0x0F, 0x5C, 0xC1, // subsd xmm0, xmm1
                0xF2, 0x0F, 0x59, 0xC1, // mulsd xmm0, xmm1
                0xF2, 0x0F, 0x5E, 0xC1, // divsd xmm0, xmm1
                0x66, 0x0F, 0x2E, 0xCA, // ucomisd xmm1, xmm2
                0x66, 0x0F, 0x57, 0xD2, // xorpd xmm2, xmm2
            ]
        );
    }

    #[test]
    fn encodes_movq_between_gpr_and_xmm() {
        let mut asm = X86Assembler::new();
        asm.movq_gpr_to_xmm(XMM0, RAX); // movq xmm0, rax -> 66 48 0F 6E C0
        asm.movq_xmm_to_gpr(RAX, XMM0); // movq rax, xmm0 -> 66 48 0F 7E C0
        assert_eq!(
            asm.finish(),
            vec![
                0x66, 0x48, 0x0F, 0x6E, 0xC0, // movq xmm0, rax
                0x66, 0x48, 0x0F, 0x7E, 0xC0, // movq rax, xmm0
            ]
        );
    }

    #[test]
    fn resolves_forward_conditional_branch() {
        // jo <target> ; ret ; target: ret  — displacement should skip the ret.
        let mut asm = X86Assembler::new();
        let target = asm.new_label();
        asm.jcc(Cond::Overflow, target); // 0F 80 at 0, rel32 field at 2..6
        asm.ret(); // C3 at 6
        asm.bind(target); // offset 7
        asm.ret(); // C3 at 7
        let code = asm.finish();
        assert_eq!(&code[0..2], &[0x0F, 0x80], "jo opcode");
        // rel32 = target(7) - end_of_branch(6) = +1.
        assert_eq!(&code[2..6], &1i32.to_le_bytes(), "forward jo rel32 = +1");
        assert_eq!(&code[6..8], &[0xC3, 0xC3], "two rets follow");
    }

    #[test]
    fn resolves_forward_and_backward_branches() {
        // header: ret ; jne header (backward) ; jmp end (forward) ; end: ret.
        let mut asm = X86Assembler::new();
        let header = asm.new_label();
        let end = asm.new_label();
        asm.bind(header); // offset 0
        asm.ret(); // C3 at 0
        asm.jcc(Cond::NotEqual, header); // 0F 85 at 1, rel32 field at 3..7
        asm.jmp(end); // E9 at 7, rel32 field at 8..12
        asm.bind(end); // offset 12
        asm.ret(); // C3 at 12
        let code = asm.finish();

        assert_eq!(&code[1..3], &[0x0F, 0x85], "jne opcode");
        // Backward: target(0) - end_of_branch(7) = -7.
        assert_eq!(
            &code[3..7],
            &(-7i32).to_le_bytes(),
            "backward jne rel32 = -7"
        );

        assert_eq!(code[7], 0xE9, "jmp opcode");
        // Forward: target(12) - end_of_branch(12) = 0.
        assert_eq!(&code[8..12], &0i32.to_le_bytes(), "forward jmp rel32 = 0");
        assert_eq!(code[12], 0xC3, "end: ret");
    }
}
