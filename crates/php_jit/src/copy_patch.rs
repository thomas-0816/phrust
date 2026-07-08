//! Copy-and-patch stencil sequencing over the flat `JitCValue` slot buffer.
//!
//! This is the driver primitive the copy-and-patch tier uses to lower a dense
//! function: each opcode becomes a self-contained stencil that reads its
//! operands from the caller's flat slot buffer, computes, and writes its result
//! back to a slot — the classic template-JIT "value file in memory" model
//! described by the Frame-Local Slot ABI in
//! `docs/research/copy-and-patch-stencil-tier.md`. Chaining steps through the
//! slot buffer (rather than registers) keeps each stencil independent and needs
//! no register allocator; a later pass can promote hot slots to registers.
//!
//! The lowered scalar-int subset is integer const / add / sub / mul with type
//! and overflow guards. Every other shape (comparisons, arrays, calls, control
//! flow, non-int values) is rejected by the region compiler and left to the
//! interpreter.

use std::collections::HashMap;

use php_ir::instruction::{IrCallArgValueKind, TerminatorKind};
use php_ir::{
    BasicBlock, BinaryOp, CompareOp, ConstId, InstructionKind, IrConstant, IrFunction,
    IrReturnType, LocalId, Operand, RegId,
};

use crate::aarch64::{
    Aarch64Assembler, Cond, D0, D1, D2, Label, Reg, SP, X0, X1, X2, X3, X4, X5, X6, X9,
};
use crate::abi::JitCValueTag;
use crate::helpers::{JIT_HELPER_STATUS_OK, JIT_HELPER_STATUS_TAILCALL, phrust_jit_abs_i64};
use crate::region_ir::{
    NodeId, RegionBuilder, RegionCompareOp, RegionConst, RegionGraph, RegionId, RegionNode,
    RegionNodeKind, RegionValueType, VmSlotId,
};

const UNINIT_TAG: u16 = JitCValueTag::Uninitialized as u16;
const INT_TAG: u16 = JitCValueTag::Int as u16;
const BOOL_TAG: u16 = JitCValueTag::Bool as u16;
const FLOAT_TAG: u16 = JitCValueTag::FloatBits as u16;
const STRING_TAG: u16 = JitCValueTag::OpaqueString as u16;
const ARRAY_TAG: u16 = JitCValueTag::OpaqueArray as u16;
const OBJECT_TAG: u16 = JitCValueTag::OpaqueObject as u16;

/// A single guarded PHP integer-add step: `slot[dst] = slot[lhs] + slot[rhs]`.
///
/// Slot indices address the flat `[JitCValue]` buffer the VM marshals in/out
/// around the region call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GuardedIntAddStep {
    /// Destination slot index (result written here as an `Int`).
    pub dst: u32,
    /// Left operand slot index.
    pub lhs: u32,
    /// Right operand slot index.
    pub rhs: u32,
}

/// Reason a slot-add sequence cannot be emitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotSequenceError {
    /// A slot index whose tag/payload byte offset exceeds the scaled-immediate
    /// range (`imm12`), so it cannot be addressed with a single load/store.
    SlotIndexOutOfRange(u32),
}

/// `JitCValue` is `repr(C)` and 24 bytes: `tag` (u32) at 0, `payload` (u64) at
/// 8, `aux` (u64) at 16. Slot `i` lives at `i * 24`.
const STRIDE: u32 = 24;
const TAG_OFF: u32 = 0;
const PAYLOAD_OFF: u32 = 8;
const AUX_OFF: u32 = 16;

/// The `tag` word load (`ldr_w` at `slot * 24`, encoding `imm12 = slot * 6`) is
/// the binding scaled-immediate constraint: `slot * 6 <= 4095`. The payload
/// (`imm12 = slot * 3 + 1`) and aux (`imm12 = slot * 3 + 2`) double-words are
/// looser, so this bound covers all three accesses.
const MAX_SLOT: u32 = 4095 / 6;

const fn tag_off(slot: u32) -> u32 {
    slot * STRIDE + TAG_OFF
}

const fn payload_off(slot: u32) -> u32 {
    slot * STRIDE + PAYLOAD_OFF
}

const fn aux_off(slot: u32) -> u32 {
    slot * STRIDE + AUX_OFF
}

/// A binary PHP integer operation. Add/Sub/Mul carry a type + overflow guard;
/// Mod and the shifts carry an operand guard (divisor `!= 0`, shift amount in
/// `0..=63`); the bitwise ops carry only the type guard (they never overflow).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntBinOp {
    /// `lhs + rhs`, side exit on signed overflow.
    Add,
    /// `lhs - rhs`, side exit on signed overflow.
    Sub,
    /// `lhs * rhs`, side exit on signed overflow.
    Mul,
    /// `lhs % rhs`, side exit on a zero divisor (interpreter raises
    /// `DivisionByZeroError`). aarch64 wraps `INT_MIN % -1` to `0`, matching PHP.
    Mod,
    /// `lhs & rhs`.
    BitAnd,
    /// `lhs | rhs`.
    BitOr,
    /// `lhs ^ rhs`.
    BitXor,
    /// `lhs << rhs`, side exit when the shift amount is outside `0..=63`
    /// (negative reads as a large unsigned value; PHP would raise or return 0).
    Shl,
    /// `lhs >> rhs` (arithmetic), side exit when the shift amount is outside
    /// `0..=63`.
    Shr,
}

impl IntBinOp {
    /// Emit the operation on `X4` (lhs) / `X5` (rhs) into `X6`, taking `deopt`
    /// on overflow or an out-of-domain operand. `X3` is used as scratch.
    fn emit(self, asm: &mut Aarch64Assembler, deopt: Label) {
        match self {
            IntBinOp::Add => {
                asm.adds(X6, X4, X5);
                asm.b_cond(Cond::Overflow, deopt);
            }
            IntBinOp::Sub => {
                asm.subs(X6, X4, X5);
                asm.b_cond(Cond::Overflow, deopt);
            }
            IntBinOp::Mul => {
                // Overflow when the product high bits differ from the sign
                // extension of the low bits (see cmp_shifted_asr63).
                asm.mul(X6, X4, X5);
                asm.smulh(X3, X4, X5);
                asm.cmp_shifted_asr63(X3, X6);
                asm.b_cond(Cond::NotEqual, deopt);
            }
            IntBinOp::Mod => {
                // Side exit on a zero divisor; the interpreter raises the error.
                asm.cmp_imm_x(X5, 0);
                asm.b_cond(Cond::Equal, deopt);
                // remainder = lhs - (lhs / rhs) * rhs.
                asm.sdiv(X3, X4, X5);
                asm.msub(X6, X3, X5, X4);
            }
            IntBinOp::BitAnd => asm.and_reg(X6, X4, X5),
            IntBinOp::BitOr => asm.orr_reg(X6, X4, X5),
            IntBinOp::BitXor => asm.eor_reg(X6, X4, X5),
            IntBinOp::Shl => {
                // aarch64 masks the shift mod 64; PHP's 0..=63 domain differs, so
                // guard the amount (negative reads as a huge unsigned value).
                asm.cmp_imm_x(X5, 63);
                asm.b_cond(Cond::UnsignedHigher, deopt);
                asm.lslv(X6, X4, X5);
            }
            IntBinOp::Shr => {
                asm.cmp_imm_x(X5, 63);
                asm.b_cond(Cond::UnsignedHigher, deopt);
                asm.asrv(X6, X4, X5);
            }
        }
    }
}

/// A single scalar-int operation over the flat slot buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScalarIntOp {
    /// Materialize a statically-known `Int` into `slot[dst]` (no guard needed).
    Const { dst: u32, value: i64 },
    /// Guarded binary integer op: `slot[dst] = slot[lhs] <op> slot[rhs]`, with
    /// `Int` type guards on both operands and an overflow side exit.
    Binary {
        op: IntBinOp,
        dst: u32,
        lhs: u32,
        rhs: u32,
    },
    /// Guarded integer comparison writing a `Bool` (0/1) to `slot[dst]`, with
    /// `Int` type guards on both operands. `cond` is the aarch64 condition after
    /// `cmp lhs, rhs` (e.g. `LessThan` for `$lhs < $rhs`).
    Compare {
        cond: Cond,
        dst: u32,
        lhs: u32,
        rhs: u32,
    },
    /// Guarded binary op with a statically-known right operand:
    /// `slot[dst] = slot[lhs] <op> rhs` (the constant is materialized inline).
    BinaryConst {
        op: IntBinOp,
        dst: u32,
        lhs: u32,
        rhs: i64,
    },
    /// Copy a whole value (tag + payload + aux) `slot[dst] = slot[src]`. Used by
    /// the general CFG lowering to move values between the register and local
    /// slot ranges without a type guard (downstream ops guard as needed).
    Copy { dst: u32, src: u32 },
    /// Guarded native call to the pure builtin `abs()` on an int:
    /// `slot[dst] = abs(slot[arg])`. Guards `slot[arg]` is `Int`, then `blr`s
    /// the `phrust_jit_abs_i64` VM helper over the C ABI (fp/lr saved, a 16-byte
    /// scratch frame holding the out value and the saved slot base). A non-OK
    /// helper status — the `abs(PHP_INT_MIN)` overflow, where PHP returns a
    /// float — takes the side exit so the interpreter produces the float. This
    /// is only emitted when the VM bridge has confirmed the call resolves to the
    /// real builtin `abs` (see [`NativeCallPermits`]).
    CallAbsI64 { dst: u32, arg: u32 },
    /// Guarded native call to the builtin `count()` on a plain array:
    /// `slot[dst] = count(slot[arg])`. Guards `slot[arg]` is an `OpaqueArray`,
    /// then `blr`s the runtime `php_jit_array_len` ABI wrapper at
    /// `array_len_helper` over the C ABI (fp/lr saved, a 16-byte scratch frame
    /// holding the out length and the saved slot base). The array slot's payload
    /// is a borrowed `*const Value` the VM marshaled for the call's duration; the
    /// helper only reads its length. `php_jit_array_len` accepts only packed
    /// all-int arrays, so a hashed/associative/mixed array (or a non-array slot —
    /// a scalar, a `Countable` object, the `count()` TypeError case) returns a
    /// non-OK status and takes the side exit, so the interpreter reproduces the
    /// exact result/error. Emitted only when the VM bridge confirms the call
    /// resolves to the real builtin `count` (see [`NativeCallPermits`]) and
    /// provides the helper address (see [`CopyPatchRuntimeHelpers`]). `php_jit`
    /// cannot name the runtime symbol, so the address is carried in the op.
    CallCountI64 {
        dst: u32,
        arg: u32,
        array_len_helper: u64,
    },
    /// Guarded native call to the builtin `strlen()` on a string:
    /// `slot[dst] = strlen(slot[arg])`. Guards `slot[arg]` is an `OpaqueString`,
    /// then `blr`s the runtime string-length ABI wrapper at `strlen_helper` over
    /// the C ABI (identical frame discipline to [`Self::CallCountI64`]). The
    /// string slot's payload is a borrowed `*const Value` the VM marshaled for
    /// the call's duration; the helper only reads its byte length (PHP `strlen`
    /// is a byte count, not a multibyte length). A non-`String` slot — a scalar,
    /// an array, an object, or `null` (the interpreter would coerce or raise) —
    /// side-exits at the tag guard, and the helper reports a non-OK status for
    /// any value it cannot length (so an unexpected shape side-exits and the
    /// interpreter reproduces the exact result/error). Emitted only when the VM
    /// bridge confirms the call resolves to the real builtin `strlen` (see
    /// [`NativeCallPermits`]) and provides the helper address (see
    /// [`CopyPatchRuntimeHelpers`]).
    CallStrlenI64 {
        dst: u32,
        arg: u32,
        strlen_helper: u64,
    },
    /// Pure type predicate writing a `Bool` (0/1) to `slot[dst]`:
    /// `slot[dst] = (slot[arg].tag == expected_tag)`. Backs the canonical
    /// `is_int`/`is_string`/`is_array`/`is_float`/`is_bool` builtins — the answer
    /// is exactly the marshaled tag, so no helper call and no payload deref is
    /// needed (this is the safest stencil: it only reads the tag word).
    ///
    /// The one guard: an `Uninitialized`-marshaled argument is ambiguous (the
    /// bridge collapses `null`, objects, resources, references, and every other
    /// non-scalar/non-string/non-array value onto it), so the predicate cannot be
    /// decided from the tag — the stencil side-exits and the interpreter answers.
    /// Every *definite* tag (`Int`/`Bool`/`FloatBits`/`OpaqueString`/
    /// `OpaqueArray`) yields a correct true/false. `expected_tag` selects the
    /// predicate (e.g. `INT_TAG` for `is_int`). Emitted only when the VM bridge
    /// confirms the call resolves to the real builtin (see [`NativeCallPermits`]).
    IsType {
        dst: u32,
        arg: u32,
        expected_tag: u16,
    },
    /// Guarded native call reading a monomorphic declared *scalar* property:
    /// `slot[dst] = <object at slot[arg]>-><declared property>`. Guards
    /// `slot[arg]` is an `OpaqueObject`, then `blr`s the property-load ABI
    /// wrapper at `helper` over the C ABI (fp/lr saved; a 32-byte scratch frame
    /// holding the out `JitCValue` and the saved slot base). The object slot's
    /// payload is a borrowed `*const Value` the VM marshaled for the call's
    /// duration; `metadata_ptr` is a borrowed `*const JitPropertyLoadMetadata`
    /// the VM keeps alive for the whole life of the compiled leaf.
    ///
    /// The helper performs the *layout guard* (the object's runtime class must
    /// equal the expected receiver class the metadata records — else a
    /// polymorphic/subclass instance reaching the same site side-exits, never
    /// reading a wrong slot) and reads the declared property; it never invokes a
    /// hook/`__get` or re-enters the VM (those shapes are excluded at recognition
    /// time). Only when the property value is a scalar `Int`/`Bool`/`Float` does
    /// the helper marshal it into the out `JitCValue` and return `OK`; a
    /// non-scalar value (string/array/object/null), an uninitialized typed
    /// property, an absent property, a class mismatch, or a non-object slot
    /// returns a non-`OK` status and side-exits, so the interpreter produces the
    /// exact value/error. `php_jit` cannot name the runtime symbol or build the
    /// class metadata, so both addresses are carried in the op (the VM bridge
    /// recognizes the leaf and supplies them; see the bridge's
    /// `recognize_property_load_leaf`).
    CallPropertyLoadScalar {
        dst: u32,
        arg: u32,
        metadata_ptr: u64,
        helper: u64,
    },
}

/// Which builtin function calls the copy-and-patch compiler may lower to a
/// native helper call.
///
/// Function-name resolution (is `abs` the real math builtin, or a user-defined
/// or namespaced shadow?) is owned by the VM, which has the function registry;
/// `php_jit` has neither. So the VM bridge decides and passes explicit
/// permission here, and this compiler emits a guarded helper call *only* when
/// permitted. With the default (all-false) permits, every `CallFunction` is
/// rejected and the interpreter runs it, exactly as before this tier existed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeCallPermits {
    /// True when the callee name `abs` is confirmed to resolve to the real
    /// builtin `abs` (not a user-defined or namespaced function that shadows
    /// it). Set by the VM bridge after checking the function registry.
    pub builtin_abs: bool,
    /// True when the compiler may lower a native→userland *tail call* (a leaf
    /// whose result is exactly a `CallFunction`'s result) to the tail-call form:
    /// the region computes the arguments natively and returns
    /// [`JIT_HELPER_STATUS_TAILCALL`] so the
    /// VM bridge performs the call through its normal interpreter path. Unlike
    /// `builtin_abs`, this permits *lowering the shape*; the callee's identity
    /// (userland vs. builtin, arity, by-reference parameters) is validated by the
    /// VM at call time, which falls back to interpreting the whole leaf when the
    /// callee is out of scope. Defaults to false so the shape is never lowered
    /// without an opt-in.
    pub allow_userland_tailcall: bool,
    /// True when the callee name `count` is confirmed to resolve to the real
    /// builtin `count` (not a user-defined or namespaced function that shadows
    /// it). Set by the VM bridge after checking the function registry; mirrors
    /// `builtin_abs`. A namespaced shadow also carries its namespace in the
    /// lowered call name, so the `count` recognizer never matches it regardless.
    pub builtin_count: bool,
    /// True when the callee name `strlen` is confirmed to resolve to the real
    /// builtin `strlen` (not a user-defined or namespaced shadow). Set by the VM
    /// bridge after checking the function registry; mirrors `builtin_count`.
    pub builtin_strlen: bool,
    /// True when the callee name `is_int` is confirmed to resolve to the real
    /// builtin `is_int`. Each type predicate carries its own permit so that a
    /// user/namespaced shadow of one never enables the others.
    pub builtin_is_int: bool,
    /// True when the callee name `is_string` is confirmed to resolve to the real
    /// builtin `is_string`.
    pub builtin_is_string: bool,
    /// True when the callee name `is_array` is confirmed to resolve to the real
    /// builtin `is_array`.
    pub builtin_is_array: bool,
    /// True when the callee name `is_float` is confirmed to resolve to the real
    /// builtin `is_float`.
    pub builtin_is_float: bool,
    /// True when the callee name `is_bool` is confirmed to resolve to the real
    /// builtin `is_bool`.
    pub builtin_is_bool: bool,
}

/// Runtime-owned helper addresses the copy-and-patch tier `blr`s from emitted
/// code, mirroring Cranelift's `JitRuntimeHelperAddresses`.
///
/// `php_jit` cannot name runtime symbols — the array/string/object helpers live
/// in `php_runtime` and the VM, below which `php_jit` sits — so the VM bridge
/// resolves their addresses and passes them here, and the compiler embeds the
/// chosen address in the emitted stencil. An address of `0` means the helper is
/// unavailable, which disables the corresponding shape (the interpreter runs
/// it), so a build without the helper wired in behaves exactly as before.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CopyPatchRuntimeHelpers {
    /// Address of an `extern "C" fn(value_ptr: usize, out: *mut i64) -> i32`
    /// wrapping `php_jit_array_len`: it reads the borrowed `Value` at `value_ptr`
    /// and writes its packed-array length through `out`, returning a non-OK
    /// status for any non-packed-int array. `0` = unavailable → `count` is not
    /// lowered. Same ABI Cranelift uses for `packed_array_len`.
    pub array_len: u64,
    /// Address of an `extern "C" fn(value_ptr: usize, out: *mut i64) -> i32`
    /// wrapping the VM's string-length read: it reads the borrowed `Value` at
    /// `value_ptr` and writes its byte length through `out`, returning a non-OK
    /// status for any non-string value. `0` = unavailable → `strlen` is not
    /// lowered. Same ABI shape as `array_len`.
    pub strlen: u64,
}

/// A binary PHP float operation over IEEE-754 doubles. Add/Sub/Mul never fault
/// (they saturate to ±inf / NaN like PHP); Div carries a zero-divisor guard
/// because PHP `/` raises `DivisionByZeroError` on a zero divisor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloatBinOp {
    /// `lhs + rhs`.
    Add,
    /// `lhs - rhs`.
    Sub,
    /// `lhs * rhs`.
    Mul,
    /// `lhs / rhs`, side exit on a zero divisor (`+0.0` and `-0.0`).
    Div,
}

impl FloatBinOp {
    /// Emit the operation on `D0`/`D1` into `D2`, taking `deopt` on a zero
    /// divisor (Div only).
    fn emit(self, asm: &mut Aarch64Assembler, deopt: Label) {
        match self {
            FloatBinOp::Add => asm.fadd(D2, D0, D1),
            FloatBinOp::Sub => asm.fsub(D2, D0, D1),
            FloatBinOp::Mul => asm.fmul(D2, D0, D1),
            FloatBinOp::Div => {
                asm.fcmp_zero(D1);
                asm.b_cond(Cond::Equal, deopt);
                asm.fdiv(D2, D0, D1);
            }
        }
    }
}

/// A single scalar-float operation over the flat slot buffer. Mirrors
/// [`ScalarIntOp`] for `FloatBits`-tagged values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScalarFloatOp {
    /// Materialize a statically-known `float` (given as its IEEE-754 bits) into
    /// `slot[dst]` (no guard needed).
    Const { dst: u32, bits: u64 },
    /// Copy a whole value (tag + payload + aux) `slot[dst] = slot[src]`.
    Copy { dst: u32, src: u32 },
    /// Guarded binary float op `slot[dst] = slot[lhs] <op> slot[rhs]`, with
    /// `FloatBits` guards on both operands and a zero-divisor side exit for Div.
    Binary {
        op: FloatBinOp,
        dst: u32,
        lhs: u32,
        rhs: u32,
    },
}

fn check_float_op_slots(op: ScalarFloatOp) -> Result<(), SlotSequenceError> {
    match op {
        ScalarFloatOp::Const { dst, .. } => check_slot(dst),
        ScalarFloatOp::Copy { dst, src } => {
            check_slot(dst)?;
            check_slot(src)
        }
        ScalarFloatOp::Binary { dst, lhs, rhs, .. } => {
            check_slot(dst)?;
            check_slot(lhs)?;
            check_slot(rhs)
        }
    }
}

/// Emit a copy of the whole 24-byte value `slot[dst] = slot[src]` (tag +
/// payload + aux); the tag travels with the value so downstream ops still guard.
fn emit_value_copy(asm: &mut Aarch64Assembler, dst: u32, src: u32) {
    asm.ldr_w(X3, X0, tag_off(src));
    asm.str_w(X3, X0, tag_off(dst));
    asm.ldr_x(X4, X0, payload_off(src));
    asm.str_x(X4, X0, payload_off(dst));
    asm.ldr_x(X5, X0, aux_off(src));
    asm.str_x(X5, X0, aux_off(dst));
}

fn emit_float_op(asm: &mut Aarch64Assembler, deopt: Label, op: ScalarFloatOp) {
    match op {
        ScalarFloatOp::Const { dst, bits } => {
            asm.mov_imm64(X6, bits);
            asm.movz(X3, FLOAT_TAG);
            asm.str_w(X3, X0, tag_off(dst));
            asm.str_x(X6, X0, payload_off(dst));
        }
        ScalarFloatOp::Copy { dst, src } => emit_value_copy(asm, dst, src),
        ScalarFloatOp::Binary { op, dst, lhs, rhs } => {
            emit_float_guard(asm, deopt, lhs);
            emit_float_guard(asm, deopt, rhs);
            asm.ldr_d(D0, X0, payload_off(lhs));
            asm.ldr_d(D1, X0, payload_off(rhs));
            op.emit(asm, deopt);
            emit_store_float(asm, dst, D2);
        }
    }
}

fn check_slot(slot: u32) -> Result<(), SlotSequenceError> {
    if slot > MAX_SLOT {
        Err(SlotSequenceError::SlotIndexOutOfRange(slot))
    } else {
        Ok(())
    }
}

/// Guard that `slot`'s tag is `Int`, taking the side exit otherwise.
fn emit_int_guard(asm: &mut Aarch64Assembler, deopt: Label, slot: u32) {
    asm.ldr_w(X3, X0, tag_off(slot));
    asm.cmp_imm_w(X3, INT_TAG);
    asm.b_cond(Cond::NotEqual, deopt);
}

/// Guard that `slot`'s tag is `Bool`, taking the side exit otherwise. Used on a
/// branch condition so arbitrary-truthiness conditions (int, string, …) fall
/// back to the interpreter rather than being mis-tested here.
fn emit_bool_guard(asm: &mut Aarch64Assembler, deopt: Label, slot: u32) {
    asm.ldr_w(X3, X0, tag_off(slot));
    asm.cmp_imm_w(X3, BOOL_TAG);
    asm.b_cond(Cond::NotEqual, deopt);
}

/// Emit a guarded native call `slot[dst] = abs(slot[arg])` through the pure
/// `phrust_jit_abs_i64` VM helper.
///
/// The call stencil, in order:
/// 1. Guard `slot[arg]` is `Int` (while `X0` still holds the slot base), taking
///    the shared side exit otherwise.
/// 2. Load the argument payload into a scratch register.
/// 3. Enter a non-leaf frame: `push_fp_lr` (saves `x29`/`x30`, `sp -= 16`) then
///    `sub sp, sp, #16` reserving `[sp+0]` for the helper's `*out i64` and
///    `[sp+8]` for the saved slot base. `sp` stays 16-byte aligned per AAPCS64.
/// 4. Marshal the C-ABI arguments: `x0 = arg value`, `x1 = &out` (`sp+0`).
/// 5. Materialize the helper address into `x9` and `blr x9`.
/// 6. Check `w0` (the status): non-`OK` (the `abs(PHP_INT_MIN)` overflow) tears
///    the frame down and branches to the shared deopt so the interpreter runs.
/// 7. On `OK`: reload `*out` and the slot base, tear the frame down, and store
///    the `Int` result to `slot[dst]`.
///
/// Nothing is assumed to survive the `blr` except through the stack: both the
/// result and the slot base are reloaded from the reserved frame afterward.
fn emit_call_abs_i64(asm: &mut Aarch64Assembler, deopt: Label, dst: u32, arg: u32) {
    // 1–2: guard + read the argument while X0 is still the slot base.
    emit_int_guard(asm, deopt, arg);
    asm.ldr_x(X4, X0, payload_off(arg));
    // 3: non-leaf frame + 16-byte scratch ([sp+0]=out, [sp+8]=slot base).
    asm.push_fp_lr();
    asm.sub_imm(SP, SP, 16);
    asm.str_x(X0, SP, 8);
    // 4: C-ABI args — x0 = value, x1 = &out.
    asm.mov(X0, X4);
    asm.add_imm(X1, SP, 0);
    // 5: call phrust_jit_abs_i64(value, &out) -> status in w0.
    asm.mov_imm64(X9, phrust_jit_abs_i64 as *const () as usize as u64);
    asm.blr(X9);
    // 6: on a non-OK status, tear the frame down and take the shared side exit.
    let call_deopt = asm.new_label();
    let done = asm.new_label();
    asm.cmp_imm_w(X0, JIT_HELPER_STATUS_OK as u16);
    asm.b_cond(Cond::NotEqual, call_deopt);
    // 7: OK — reload the result and slot base, free the frame, store the Int.
    asm.ldr_x(X6, SP, 0);
    asm.ldr_x(X0, SP, 8);
    asm.add_imm(SP, SP, 16);
    asm.pop_fp_lr();
    emit_store_int(asm, dst, X6);
    asm.b(done);
    asm.bind(call_deopt);
    asm.add_imm(SP, SP, 16);
    asm.pop_fp_lr();
    asm.b(deopt);
    asm.bind(done);
}

/// Guard that `slot`'s tag is `OpaqueArray`, taking the side exit otherwise.
/// Only a genuine array slot reaches the length helper — a scalar, a `Countable`
/// object (marshaled as `Uninitialized`), or any other value side-exits here so
/// the interpreter reproduces `count()`'s exact semantics/errors.
fn emit_array_guard(asm: &mut Aarch64Assembler, deopt: Label, slot: u32) {
    asm.ldr_w(X3, X0, tag_off(slot));
    asm.cmp_imm_w(X3, ARRAY_TAG);
    asm.b_cond(Cond::NotEqual, deopt);
}

/// Emit a guarded native call `slot[dst] = count(slot[arg])` for the plain
/// packed-array case, through the runtime `php_jit_array_len` ABI wrapper at
/// `helper` (an `extern "C" fn(value_ptr: usize, out: *mut i64) -> i32`).
///
/// The call stencil mirrors [`emit_call_abs_i64`], differing only in the guard
/// (the arg must be an `OpaqueArray`) and in taking the helper address as a
/// parameter — `php_jit` cannot name the runtime symbol. In order:
/// 1. Guard `slot[arg]` is an `OpaqueArray` (while `X0` still holds the slot
///    base), taking the shared side exit otherwise.
/// 2. Load the array slot's `payload` — a borrowed `*const Value` the VM
///    marshaled for the call's duration — into a scratch register.
/// 3. Enter a non-leaf frame: `push_fp_lr` then a 16-byte scratch reserving
///    `[sp+0]` for the helper's `*out i64` and `[sp+8]` for the saved slot base.
/// 4. Marshal the C-ABI arguments: `x0 = value_ptr`, `x1 = &out` (`sp+0`).
/// 5. Materialize the helper address into `x9` and `blr x9`.
/// 6. Check `w0` (the status): a non-`OK` status — a hashed/associative/mixed
///    array the helper rejects — tears the frame down and takes the shared side
///    exit so the interpreter computes the length.
/// 7. On `OK`: reload `*out` (the length) and the slot base, tear the frame
///    down, and store the `Int` result to `slot[dst]`.
///
/// Nothing is assumed to survive the `blr` except through the stack; the helper
/// only reads the borrowed value and never mutates, frees, or re-enters the VM.
fn emit_call_count(asm: &mut Aarch64Assembler, deopt: Label, dst: u32, arg: u32, helper: u64) {
    // 1–2: guard the arg is an array + read its borrowed-pointer payload while
    // X0 is still the slot base.
    emit_array_guard(asm, deopt, arg);
    asm.ldr_x(X4, X0, payload_off(arg));
    // 3: non-leaf frame + 16-byte scratch ([sp+0]=out length, [sp+8]=slot base).
    asm.push_fp_lr();
    asm.sub_imm(SP, SP, 16);
    asm.str_x(X0, SP, 8);
    // 4: C-ABI args — x0 = value_ptr, x1 = &out.
    asm.mov(X0, X4);
    asm.add_imm(X1, SP, 0);
    // 5: call php_jit_array_len(value_ptr, &out) -> status in w0.
    asm.mov_imm64(X9, helper);
    asm.blr(X9);
    // 6: on a non-OK status, tear the frame down and take the shared side exit.
    let call_deopt = asm.new_label();
    let done = asm.new_label();
    asm.cmp_imm_w(X0, JIT_HELPER_STATUS_OK as u16);
    asm.b_cond(Cond::NotEqual, call_deopt);
    // 7: OK — reload the length and slot base, free the frame, store the Int.
    asm.ldr_x(X6, SP, 0);
    asm.ldr_x(X0, SP, 8);
    asm.add_imm(SP, SP, 16);
    asm.pop_fp_lr();
    emit_store_int(asm, dst, X6);
    asm.b(done);
    asm.bind(call_deopt);
    asm.add_imm(SP, SP, 16);
    asm.pop_fp_lr();
    asm.b(deopt);
    asm.bind(done);
}

/// Guard that `slot`'s tag is `OpaqueString`, taking the side exit otherwise.
/// Only a genuine string slot reaches the length helper — a scalar, an array,
/// an object, `null`, or any other value (all of which PHP's `strlen` would
/// coerce or reject) side-exits here so the interpreter reproduces the exact
/// semantics.
fn emit_string_guard(asm: &mut Aarch64Assembler, deopt: Label, slot: u32) {
    asm.ldr_w(X3, X0, tag_off(slot));
    asm.cmp_imm_w(X3, STRING_TAG);
    asm.b_cond(Cond::NotEqual, deopt);
}

/// Emit a guarded native call `slot[dst] = strlen(slot[arg])` through the
/// runtime string-length ABI wrapper at `helper` (an
/// `extern "C" fn(value_ptr: usize, out: *mut i64) -> i32`).
///
/// A near-clone of [`emit_call_count`], differing only in the guard (the arg
/// must be an `OpaqueString`): guard the arg tag, load its borrowed
/// `*const Value` payload, enter a non-leaf frame with a 16-byte scratch
/// (`[sp+0]` = out length, `[sp+8]` = saved slot base), marshal `x0 = value_ptr`
/// / `x1 = &out`, `blr` the helper, side-exit on a non-OK status, else reload
/// the byte length and store the `Int` result. The helper only reads the
/// borrowed value's byte length; it never mutates, frees, or re-enters the VM.
fn emit_call_strlen(asm: &mut Aarch64Assembler, deopt: Label, dst: u32, arg: u32, helper: u64) {
    // 1–2: guard the arg is a string + read its borrowed-pointer payload while
    // X0 is still the slot base.
    emit_string_guard(asm, deopt, arg);
    asm.ldr_x(X4, X0, payload_off(arg));
    // 3: non-leaf frame + 16-byte scratch ([sp+0]=out length, [sp+8]=slot base).
    asm.push_fp_lr();
    asm.sub_imm(SP, SP, 16);
    asm.str_x(X0, SP, 8);
    // 4: C-ABI args — x0 = value_ptr, x1 = &out.
    asm.mov(X0, X4);
    asm.add_imm(X1, SP, 0);
    // 5: call strlen_helper(value_ptr, &out) -> status in w0.
    asm.mov_imm64(X9, helper);
    asm.blr(X9);
    // 6: on a non-OK status, tear the frame down and take the shared side exit.
    let call_deopt = asm.new_label();
    let done = asm.new_label();
    asm.cmp_imm_w(X0, JIT_HELPER_STATUS_OK as u16);
    asm.b_cond(Cond::NotEqual, call_deopt);
    // 7: OK — reload the length and slot base, free the frame, store the Int.
    asm.ldr_x(X6, SP, 0);
    asm.ldr_x(X0, SP, 8);
    asm.add_imm(SP, SP, 16);
    asm.pop_fp_lr();
    emit_store_int(asm, dst, X6);
    asm.b(done);
    asm.bind(call_deopt);
    asm.add_imm(SP, SP, 16);
    asm.pop_fp_lr();
    asm.b(deopt);
    asm.bind(done);
}

/// Guard that `slot`'s tag is `OpaqueObject`, taking the side exit otherwise.
/// Only a genuine object slot reaches the property-load helper — a scalar, a
/// string, an array, `null`, or any other value side-exits here so the
/// interpreter reproduces the exact property-access semantics/errors.
fn emit_object_guard(asm: &mut Aarch64Assembler, deopt: Label, slot: u32) {
    asm.ldr_w(X3, X0, tag_off(slot));
    asm.cmp_imm_w(X3, OBJECT_TAG);
    asm.b_cond(Cond::NotEqual, deopt);
}

/// Emit a guarded native call `slot[dst] = <object>-><scalar property>` through
/// the monomorphic property-load ABI wrapper at `helper` (an `extern "C"
/// fn(value_ptr: usize, metadata_ptr: usize, out: *mut JitCValue) -> i32`).
///
/// Mirrors [`emit_call_count`]'s non-leaf frame discipline, differing in three
/// ways: the guard (the arg must be an `OpaqueObject`), a third C-ABI argument
/// (`metadata_ptr`, the borrowed layout metadata), and a full-`JitCValue` out
/// slot (24 bytes) rather than an `i64`. In order:
/// 1. Guard `slot[arg]` is an `OpaqueObject` (while `X0` still holds the slot
///    base), taking the shared side exit otherwise.
/// 2. Load the object slot's `payload` — a borrowed `*const Value` the VM
///    marshaled for the call's duration — into a scratch register.
/// 3. Enter a non-leaf frame: `push_fp_lr` then a 32-byte scratch reserving
///    `[sp+0 .. sp+24)` for the helper's out `JitCValue` and `[sp+24]` for the
///    saved slot base (`sp` stays 16-byte aligned per AAPCS64).
/// 4. Marshal the C-ABI arguments: `x0 = value_ptr`, `x1 = metadata_ptr`,
///    `x2 = &out` (`sp+0`).
/// 5. Materialize the helper address into `x9` and `blr x9`.
/// 6. Check `w0` (the status): a non-`OK` status — the object's layout does not
///    match the expected class, the property is absent/uninitialized, or the
///    property value is non-scalar — tears the frame down and takes the shared
///    side exit so the interpreter produces the exact value/error.
/// 7. On `OK`: reload the slot base, copy the marshaled 24-byte `JitCValue`
///    result from the scratch into `slot[dst]`, and tear the frame down.
///
/// Nothing is assumed to survive the `blr` except through the stack; the helper
/// only reads the borrowed object's declared property and never mutates, frees,
/// invokes a hook/`__get`, or re-enters the VM.
fn emit_property_load(
    asm: &mut Aarch64Assembler,
    deopt: Label,
    dst: u32,
    arg: u32,
    metadata_ptr: u64,
    helper: u64,
) {
    // 1–2: guard the arg is an object + read its borrowed-pointer payload while
    // X0 is still the slot base.
    emit_object_guard(asm, deopt, arg);
    asm.ldr_x(X4, X0, payload_off(arg));
    // 3: non-leaf frame + 32-byte scratch ([sp+0..24)=out JitCValue,
    // [sp+24]=slot base).
    asm.push_fp_lr();
    asm.sub_imm(SP, SP, 32);
    asm.str_x(X0, SP, 24);
    // 4: C-ABI args — x0 = value_ptr, x1 = metadata_ptr, x2 = &out.
    asm.mov(X0, X4);
    asm.mov_imm64(X1, metadata_ptr);
    asm.add_imm(X2, SP, 0);
    // 5: call helper(value_ptr, metadata_ptr, &out) -> status in w0.
    asm.mov_imm64(X9, helper);
    asm.blr(X9);
    // 6: on a non-OK status, tear the frame down and take the shared side exit.
    let call_deopt = asm.new_label();
    let done = asm.new_label();
    asm.cmp_imm_w(X0, JIT_HELPER_STATUS_OK as u16);
    asm.b_cond(Cond::NotEqual, call_deopt);
    // 7: OK — reload the slot base, copy the 24-byte result (tag+reserved,
    // payload, aux) into slot[dst], free the frame.
    asm.ldr_x(X0, SP, 24);
    asm.ldr_x(X3, SP, 0);
    asm.str_x(X3, X0, tag_off(dst));
    asm.ldr_x(X4, SP, 8);
    asm.str_x(X4, X0, payload_off(dst));
    asm.ldr_x(X5, SP, 16);
    asm.str_x(X5, X0, aux_off(dst));
    asm.add_imm(SP, SP, 32);
    asm.pop_fp_lr();
    asm.b(done);
    asm.bind(call_deopt);
    asm.add_imm(SP, SP, 32);
    asm.pop_fp_lr();
    asm.b(deopt);
    asm.bind(done);
}

/// Emit a pure type-predicate stencil `slot[dst] = (slot[arg].tag ==
/// expected_tag)`, guarding that the marshaled tag is decidable.
///
/// No call and no payload deref: it loads the tag word, side-exits when it is
/// `Uninitialized` (the ambiguous case the bridge collapses `null`/objects/etc.
/// onto — the interpreter must answer), then writes the `Bool` result of the tag
/// comparison. Every definite tag yields a correct true/false, so there is no
/// wrong-answer path: an unrecognized shape is either a decidable non-match or an
/// `Uninitialized` side exit.
fn emit_is_type(asm: &mut Aarch64Assembler, deopt: Label, dst: u32, arg: u32, expected_tag: u16) {
    asm.ldr_w(X3, X0, tag_off(arg));
    // An Uninitialized-marshaled value is ambiguous; defer to the interpreter.
    asm.cmp_imm_w(X3, UNINIT_TAG);
    asm.b_cond(Cond::Equal, deopt);
    // The answer is exactly whether the marshaled tag is the predicate's tag.
    asm.cmp_imm_w(X3, expected_tag);
    asm.cset(X6, Cond::Equal);
    emit_store_bool(asm, dst, X6);
}

/// Write `value` to `slot[dst]` tagged as `Int`.
fn emit_store_int(asm: &mut Aarch64Assembler, dst: u32, value: Reg) {
    asm.movz(X3, INT_TAG);
    asm.str_w(X3, X0, tag_off(dst));
    asm.str_x(value, X0, payload_off(dst));
}

/// Write `value` (0 or 1 in the low bit) to `slot[dst]` tagged as `Bool`.
fn emit_store_bool(asm: &mut Aarch64Assembler, dst: u32, value: Reg) {
    asm.movz(X3, BOOL_TAG);
    asm.str_w(X3, X0, tag_off(dst));
    asm.str_x(value, X0, payload_off(dst));
}

/// Guard that `slot`'s tag is `FloatBits`, taking the side exit otherwise.
fn emit_float_guard(asm: &mut Aarch64Assembler, deopt: Label, slot: u32) {
    asm.ldr_w(X3, X0, tag_off(slot));
    asm.cmp_imm_w(X3, FLOAT_TAG);
    asm.b_cond(Cond::NotEqual, deopt);
}

/// Write the double register `value` to `slot[dst]` tagged as `FloatBits`.
fn emit_store_float(asm: &mut Aarch64Assembler, dst: u32, value: Reg) {
    asm.movz(X3, FLOAT_TAG);
    asm.str_w(X3, X0, tag_off(dst));
    asm.str_d(value, X0, payload_off(dst));
}

/// Emit a native `extern "C" fn(slot_base: *mut JitCValue) -> i32` that applies
/// each scalar-int op in order over the caller's flat slot buffer.
///
/// Returns `0` when every op succeeded. Returns `1` on a side exit: a binary
/// op's operand slot not tagged `Int`, or an add/sub/mul that overflows `i64`.
/// On a side exit, slots written by already-completed ops keep their results —
/// those ops correspond to earlier opcodes that legitimately ran, so the
/// interpreter resumes at the failing op with the prior locals already updated.
/// (This primitive returns a single generic side-exit code; wiring it into VM
/// dispatch adds the per-op resume program point.)
fn check_op_slots(op: ScalarIntOp) -> Result<(), SlotSequenceError> {
    match op {
        ScalarIntOp::Const { dst, .. } => check_slot(dst),
        ScalarIntOp::Copy { dst, src }
        | ScalarIntOp::CallAbsI64 { dst, arg: src }
        | ScalarIntOp::CallCountI64 { dst, arg: src, .. }
        | ScalarIntOp::CallStrlenI64 { dst, arg: src, .. }
        | ScalarIntOp::CallPropertyLoadScalar { dst, arg: src, .. }
        | ScalarIntOp::IsType { dst, arg: src, .. } => {
            check_slot(dst)?;
            check_slot(src)
        }
        ScalarIntOp::BinaryConst { dst, lhs, .. } => {
            check_slot(dst)?;
            check_slot(lhs)
        }
        ScalarIntOp::Binary { dst, lhs, rhs, .. } | ScalarIntOp::Compare { dst, lhs, rhs, .. } => {
            check_slot(dst)?;
            check_slot(lhs)?;
            check_slot(rhs)
        }
    }
}

/// Emit one scalar-int op, reading operands from and writing the result to the
/// slot buffer. `X3`..`X6` are scratch; nothing is kept in registers across ops
/// (values live in slots), so ops compose freely — including inside a loop body.
fn emit_op(asm: &mut Aarch64Assembler, deopt: Label, op: ScalarIntOp) {
    match op {
        ScalarIntOp::Const { dst, value } => {
            asm.mov_imm64(X6, value as u64);
            emit_store_int(asm, dst, X6);
        }
        ScalarIntOp::Copy { dst, src } => emit_value_copy(asm, dst, src),
        ScalarIntOp::CallAbsI64 { dst, arg } => emit_call_abs_i64(asm, deopt, dst, arg),
        ScalarIntOp::CallCountI64 {
            dst,
            arg,
            array_len_helper,
        } => emit_call_count(asm, deopt, dst, arg, array_len_helper),
        ScalarIntOp::CallStrlenI64 {
            dst,
            arg,
            strlen_helper,
        } => emit_call_strlen(asm, deopt, dst, arg, strlen_helper),
        ScalarIntOp::IsType {
            dst,
            arg,
            expected_tag,
        } => emit_is_type(asm, deopt, dst, arg, expected_tag),
        ScalarIntOp::CallPropertyLoadScalar {
            dst,
            arg,
            metadata_ptr,
            helper,
        } => emit_property_load(asm, deopt, dst, arg, metadata_ptr, helper),
        ScalarIntOp::Binary { op, dst, lhs, rhs } => {
            emit_int_guard(asm, deopt, lhs);
            emit_int_guard(asm, deopt, rhs);
            asm.ldr_x(X4, X0, payload_off(lhs));
            asm.ldr_x(X5, X0, payload_off(rhs));
            op.emit(asm, deopt);
            emit_store_int(asm, dst, X6);
        }
        ScalarIntOp::BinaryConst { op, dst, lhs, rhs } => {
            emit_int_guard(asm, deopt, lhs);
            asm.ldr_x(X4, X0, payload_off(lhs));
            asm.mov_imm64(X5, rhs as u64);
            op.emit(asm, deopt);
            emit_store_int(asm, dst, X6);
        }
        ScalarIntOp::Compare {
            cond,
            dst,
            lhs,
            rhs,
        } => {
            emit_int_guard(asm, deopt, lhs);
            emit_int_guard(asm, deopt, rhs);
            asm.ldr_x(X4, X0, payload_off(lhs));
            asm.ldr_x(X5, X0, payload_off(rhs));
            asm.cmp_reg(X4, X5);
            asm.cset(X6, cond);
            emit_store_bool(asm, dst, X6);
        }
    }
}

pub fn emit_scalar_int_ops(ops: &[ScalarIntOp]) -> Result<Vec<u8>, SlotSequenceError> {
    for op in ops {
        check_op_slots(*op)?;
    }
    let mut asm = Aarch64Assembler::new();
    let deopt = asm.new_label();
    for op in ops {
        emit_op(&mut asm, deopt, *op);
    }
    asm.movz(X0, 0);
    asm.ret();
    asm.bind(deopt);
    asm.movz(X0, 1);
    asm.ret();
    Ok(asm.finish())
}

/// Emit a native `extern "C" fn(slot_base: *mut JitCValue) -> i32` that applies
/// each scalar-float op in order over the caller's flat slot buffer. Returns `0`
/// on success, `1` on a side exit (non-`FloatBits` operand or a zero divisor).
pub fn emit_scalar_float_ops(ops: &[ScalarFloatOp]) -> Result<Vec<u8>, SlotSequenceError> {
    for op in ops {
        check_float_op_slots(*op)?;
    }
    let mut asm = Aarch64Assembler::new();
    let deopt = asm.new_label();
    for op in ops {
        emit_float_op(&mut asm, deopt, *op);
    }
    asm.movz(X0, 0);
    asm.ret();
    asm.bind(deopt);
    asm.movz(X0, 1);
    asm.ret();
    Ok(asm.finish())
}

/// A native counted loop over the flat slot buffer: run `prologue` once, then
/// `while slot[counter] < slot[limit] { body; slot[counter] += 1 }`, all
/// executing natively with no per-iteration interpreter dispatch — the shape
/// where the tier's real win lives. Loop-carried values (accumulators, the
/// counter) live in slots, so no cross-block register allocation or phi handling
/// is needed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountedLoop {
    /// Ops run once before the loop (e.g., zero an accumulator and the counter).
    pub prologue: Vec<ScalarIntOp>,
    /// Loop counter slot (compared to `limit`, incremented by 1 each iteration).
    pub counter: u32,
    /// Limit slot; the loop runs while `slot[counter] < slot[limit]`.
    pub limit: u32,
    /// Ops run each iteration (may read the counter and accumulator slots).
    pub body: Vec<ScalarIntOp>,
}

/// Emit a native `extern "C" fn(slot_base: *mut JitCValue) -> i32` for a counted
/// loop. Returns `0` on completion, `1` on a side exit (non-`Int` operand or
/// overflow anywhere in the prologue, body, condition, or increment).
pub fn emit_counted_loop(counted: &CountedLoop) -> Result<Vec<u8>, SlotSequenceError> {
    for op in counted.prologue.iter().chain(counted.body.iter()) {
        check_op_slots(*op)?;
    }
    check_slot(counted.counter)?;
    check_slot(counted.limit)?;

    let mut asm = Aarch64Assembler::new();
    let deopt = asm.new_label();
    let header = asm.new_label();
    let end = asm.new_label();

    for op in &counted.prologue {
        emit_op(&mut asm, deopt, *op);
    }

    asm.bind(header);
    // Condition: while slot[counter] < slot[limit].
    emit_int_guard(&mut asm, deopt, counted.counter);
    emit_int_guard(&mut asm, deopt, counted.limit);
    asm.ldr_x(X4, X0, payload_off(counted.counter));
    asm.ldr_x(X5, X0, payload_off(counted.limit));
    asm.cmp_reg(X4, X5);
    asm.b_cond(Cond::GreaterEqual, end);

    for op in &counted.body {
        emit_op(&mut asm, deopt, *op);
    }

    // slot[counter] += 1 (overflow-guarded).
    asm.ldr_x(X4, X0, payload_off(counted.counter));
    asm.movz(X5, 1);
    asm.adds(X6, X4, X5);
    asm.b_cond(Cond::Overflow, deopt);
    emit_store_int(&mut asm, counted.counter, X6);
    asm.b(header);

    asm.bind(end);
    asm.movz(X0, 0);
    asm.ret();
    asm.bind(deopt);
    asm.movz(X0, 1);
    asm.ret();
    Ok(asm.finish())
}

/// Emit a guarded int-add sequence — the `Add`-only special case of
/// [`emit_scalar_int_ops`].
pub fn emit_guarded_int_add_sequence(
    steps: &[GuardedIntAddStep],
) -> Result<Vec<u8>, SlotSequenceError> {
    let ops: Vec<ScalarIntOp> = steps
        .iter()
        .map(|step| ScalarIntOp::Binary {
            op: IntBinOp::Add,
            dst: step.dst,
            lhs: step.lhs,
            rhs: step.rhs,
        })
        .collect();
    emit_scalar_int_ops(&ops)
}

/// The plan for a native→userland tail call recognized in a scalar-int leaf.
///
/// When the region returns
/// [`JIT_HELPER_STATUS_TAILCALL`], its prefix
/// has already left each positional argument — guaranteed `Int`-tagged — in the
/// buffer slots named by `arg_slots` (in call order). The VM bridge reads those
/// slots as the arguments and performs the call to `callee_name` through the
/// normal interpreter path, so the callee runs identically to a fully
/// interpreted call. The region itself never re-enters the VM.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TailCallPlan {
    /// Callee name exactly as written at the call site; the VM normalizes and
    /// resolves it (and validates it is a plain userland function) at call time.
    pub callee_name: String,
    /// Buffer slots holding the marshaled positional `Int` arguments, in order.
    pub arg_slots: Vec<u32>,
}

/// A region lowered to the scalar-int subset: native code plus the slot-buffer
/// layout the VM must marshal against.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledScalarRegion {
    /// Emitted `extern "C" fn(slot_base: *mut JitCValue) -> i32`.
    pub code: Vec<u8>,
    /// Slot holding the region result after a successful (`0`) return. Unused on
    /// the tail-call path (the VM produces the result from the userland call).
    pub result_slot: u32,
    /// Number of `JitCValue` slots the caller's buffer must provide.
    pub buffer_slots: u32,
    /// `Some` when the region is a native→userland tail call: it returns
    /// [`JIT_HELPER_STATUS_TAILCALL`] rather
    /// than a result, and the VM bridge performs the call named here. `None` for
    /// every ordinary region (result in `result_slot` on a `0` return).
    pub tail_call: Option<TailCallPlan>,
}

/// Reason a region cannot be lowered to the scalar-int subset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegionCompileError {
    /// A value-producing node outside the supported scalar-int subset
    /// (`Param`, `Const`, `Add`, `Sub`, `Mul`).
    UnsupportedNode(&'static str),
    /// A supported arithmetic node whose value type is not `I64`.
    NonIntValue,
    /// An arithmetic node with the wrong input arity, or an input that is not a
    /// value-producing node lowered earlier (the graph is not in dependency
    /// order).
    MalformedInputs,
    /// The requested result node was not lowered to a computed value.
    ResultNotComputed,
    /// A slot index exceeds the addressable scaled-immediate range.
    SlotIndexOutOfRange(u32),
}

impl From<SlotSequenceError> for RegionCompileError {
    fn from(err: SlotSequenceError) -> Self {
        match err {
            SlotSequenceError::SlotIndexOutOfRange(slot) => Self::SlotIndexOutOfRange(slot),
        }
    }
}

/// Lower a straight-line scalar-int region to native code over the flat slot
/// buffer, via [`emit_scalar_int_ops`].
///
/// Supported nodes: `Param { slot }` (a marshaled VM slot); `Const` holding an
/// `I64`, materialized into a fresh temporary; and the guarded integer
/// arithmetic `Add`/`Sub`/`Mul`, each writing its `Int` result to a temporary
/// allocated above the parameter slots. `result` names the node whose slot
/// holds the region output; it must be a computed value (a `Const` or an
/// arithmetic op), not a passed-through parameter, since a bare parameter could
/// be non-`Int` at runtime. Control- and memory-typed nodes are skipped; every
/// other scalar node (comparisons, division, casts, calls, …) is rejected so
/// the interpreter runs that region. Nodes must appear in dependency order (an
/// op's inputs lowered before it), as the region builder emits them; otherwise
/// the input is reported as malformed.
pub fn compile_scalar_int_region(
    graph: &RegionGraph,
    result: NodeId,
) -> Result<CompiledScalarRegion, RegionCompileError> {
    let nodes = graph.nodes();

    // Parameter slots occupy their VM slot indices; temporaries are allocated
    // above the highest parameter slot so the two never collide.
    let mut max_param_slot = 0u32;
    let mut any_param = false;
    for node in nodes {
        if let RegionNodeKind::Param { slot } = node.kind {
            any_param = true;
            max_param_slot = max_param_slot.max(slot.raw());
        }
    }
    let mut next_temp = if any_param { max_param_slot + 1 } else { 0 };

    let mut node_slot: Vec<Option<u32>> = vec![None; nodes.len()];
    let mut ops: Vec<ScalarIntOp> = Vec::new();

    for (index, node) in nodes.iter().enumerate() {
        match node.kind {
            RegionNodeKind::Param { slot } => node_slot[index] = Some(slot.raw()),
            RegionNodeKind::Const(constant) => {
                let value = match graph.constant(constant) {
                    Some(RegionConst::I64(value)) => *value,
                    _ => return Err(RegionCompileError::NonIntValue),
                };
                let dst = next_temp;
                next_temp += 1;
                node_slot[index] = Some(dst);
                ops.push(ScalarIntOp::Const { dst, value });
            }
            RegionNodeKind::Add | RegionNodeKind::Sub | RegionNodeKind::Mul => {
                if node.value_type != RegionValueType::I64 {
                    return Err(RegionCompileError::NonIntValue);
                }
                let op = match node.kind {
                    RegionNodeKind::Sub => IntBinOp::Sub,
                    RegionNodeKind::Mul => IntBinOp::Mul,
                    _ => IntBinOp::Add,
                };
                let [lhs, rhs] = binary_inputs(node)?;
                let lhs = slot_of(&node_slot, lhs)?;
                let rhs = slot_of(&node_slot, rhs)?;
                let dst = next_temp;
                next_temp += 1;
                node_slot[index] = Some(dst);
                ops.push(ScalarIntOp::Binary { op, dst, lhs, rhs });
            }
            RegionNodeKind::Compare(compare_op) => {
                let [lhs, rhs] = binary_inputs(node)?;
                let lhs = slot_of(&node_slot, lhs)?;
                let rhs = slot_of(&node_slot, rhs)?;
                let dst = next_temp;
                next_temp += 1;
                node_slot[index] = Some(dst);
                ops.push(ScalarIntOp::Compare {
                    cond: region_compare_to_cond(compare_op),
                    dst,
                    lhs,
                    rhs,
                });
            }
            // Control/effect tokens carry no scalar value; they are not lowered.
            _ if matches!(
                node.value_type,
                RegionValueType::Control | RegionValueType::Memory
            ) => {}
            _ => return Err(RegionCompileError::UnsupportedNode("non-scalar-int-op")),
        }
    }

    // The result must be a computed value, not a passed-through parameter (which
    // could be non-Int at runtime).
    let result_is_computed = nodes.get(result.index()).is_some_and(|node| {
        matches!(
            node.kind,
            RegionNodeKind::Add
                | RegionNodeKind::Sub
                | RegionNodeKind::Mul
                | RegionNodeKind::Const(_)
                | RegionNodeKind::Compare(_)
        )
    });
    if !result_is_computed {
        return Err(RegionCompileError::ResultNotComputed);
    }
    let result_slot = node_slot
        .get(result.index())
        .copied()
        .flatten()
        .ok_or(RegionCompileError::ResultNotComputed)?;

    let code = emit_scalar_int_ops(&ops)?;
    Ok(CompiledScalarRegion {
        code,
        result_slot,
        buffer_slots: next_temp.max(max_param_slot + 1),
        tail_call: None,
    })
}

fn binary_inputs(node: &RegionNode) -> Result<[NodeId; 2], RegionCompileError> {
    match node.inputs.as_slice() {
        [lhs, rhs] => Ok([*lhs, *rhs]),
        _ => Err(RegionCompileError::MalformedInputs),
    }
}

fn slot_of(node_slot: &[Option<u32>], node: NodeId) -> Result<u32, RegionCompileError> {
    node_slot
        .get(node.index())
        .copied()
        .flatten()
        .ok_or(RegionCompileError::MalformedInputs)
}

/// Resolve an IR operand to the region node holding its value.
fn resolve_operand(
    op: &Operand,
    builder: &mut RegionBuilder,
    reg_nodes: &HashMap<RegId, NodeId>,
    param_nodes: &HashMap<LocalId, NodeId>,
    constants: &[IrConstant],
) -> Option<NodeId> {
    match op {
        Operand::Register(reg) => reg_nodes.get(reg).copied(),
        Operand::Local(local) => param_nodes.get(local).copied(),
        Operand::Constant(constant) => match constants.get(constant.index()) {
            Some(IrConstant::Int(value)) => Some(builder.emit_const_i64(*value)),
            _ => None,
        },
    }
}

/// Recognize a straight-line scalar-int leaf function and build the `RegionGraph`
/// the copy-and-patch compiler lowers. Returns the graph plus the result node,
/// or `None` to reject (the interpreter runs the function).
///
/// Accepts: a single-block free function declared `: int`, with only `int`,
/// by-value, non-variadic, no-default parameters, whose body is exclusively
/// `LoadLocal` (of a parameter), `LoadConst` (of an `Int`), `Move`, and
/// `Binary` `Add`/`Sub`/`Mul`, terminated by `Return` of a register. Every other
/// shape — methods, closures, generators, multiple blocks, branches, calls,
/// arrays, references, non-int values, or `Div`/`Mod`/`Concat`/bitwise/shift —
/// is rejected so the interpreter runs it. Guards and overflow side exits are
/// added by the compiler; recognition only maps proven-int shapes.
pub fn build_scalar_int_region(
    function: &IrFunction,
    constants: &[IrConstant],
    region_id: u32,
) -> Option<(RegionGraph, NodeId)> {
    let flags = function.flags;
    if flags.is_top_level || flags.is_closure || flags.is_method || flags.is_generator {
        return None;
    }
    if function.returns_by_ref || !function.captures.is_empty() {
        return None;
    }
    // An `int` body, or a `bool` body whose result is a comparison. Operands are
    // still `int` and guarded; only the result type differs.
    if !matches!(
        function.return_type,
        Some(IrReturnType::Int | IrReturnType::Bool)
    ) {
        return None;
    }
    for param in &function.params {
        if param.by_ref
            || param.variadic
            || param.default.is_some()
            || param.type_ != Some(IrReturnType::Int)
        {
            return None;
        }
    }

    // Pure straight-line arithmetic is exactly one block.
    let [block] = function.blocks.as_slice() else {
        return None;
    };

    let mut builder = RegionBuilder::new(RegionId::new(region_id), function.name.as_str());
    let start = builder.start();

    // Each parameter local materializes as a region parameter keyed by its slot.
    let mut param_nodes: HashMap<LocalId, NodeId> = HashMap::new();
    for param in &function.params {
        let node = builder.param_i64(VmSlotId::new(param.local.raw()));
        param_nodes.insert(param.local, node);
    }

    let mut reg_nodes: HashMap<RegId, NodeId> = HashMap::new();
    for instruction in &block.instructions {
        match &instruction.kind {
            InstructionKind::LoadLocal { dst, local } => {
                let node = param_nodes.get(local).copied()?;
                reg_nodes.insert(*dst, node);
            }
            InstructionKind::LoadConst { dst, constant } => {
                let value = match constants.get(constant.index()) {
                    Some(IrConstant::Int(value)) => *value,
                    _ => return None,
                };
                let node = builder.emit_const_i64(value);
                reg_nodes.insert(*dst, node);
            }
            InstructionKind::Move { dst, src } => {
                let node = resolve_operand(src, &mut builder, &reg_nodes, &param_nodes, constants)?;
                reg_nodes.insert(*dst, node);
            }
            InstructionKind::Binary { dst, op, lhs, rhs } => {
                let lhs_node =
                    resolve_operand(lhs, &mut builder, &reg_nodes, &param_nodes, constants)?;
                let rhs_node =
                    resolve_operand(rhs, &mut builder, &reg_nodes, &param_nodes, constants)?;
                let node = match op {
                    BinaryOp::Add => builder.emit_add_i64(lhs_node, rhs_node),
                    BinaryOp::Sub => builder.emit_sub_i64(lhs_node, rhs_node),
                    BinaryOp::Mul => builder.emit_mul_i64(lhs_node, rhs_node),
                    _ => return None,
                };
                reg_nodes.insert(*dst, node);
            }
            InstructionKind::Compare { dst, op, lhs, rhs } => {
                let compare_op = ir_compare_to_region(*op)?;
                let lhs_node =
                    resolve_operand(lhs, &mut builder, &reg_nodes, &param_nodes, constants)?;
                let rhs_node =
                    resolve_operand(rhs, &mut builder, &reg_nodes, &param_nodes, constants)?;
                let node = builder.emit_compare_i64(compare_op, lhs_node, rhs_node);
                reg_nodes.insert(*dst, node);
            }
            _ => return None,
        }
    }

    let result = match &block.terminator {
        Some(terminator) => match &terminator.kind {
            TerminatorKind::Return {
                value: Some(Operand::Register(reg)),
                by_ref_local: None,
            } => reg_nodes.get(reg).copied()?,
            _ => return None,
        },
        None => return None,
    };
    builder.emit_return(start, result);
    Some((builder.finish(), result))
}

/// Recognize and lower a scalar-int function to native code in one step, trying
/// the most specialized lowering first: a straight-line leaf
/// (`build_scalar_int_region`), then a canonical counted `for` loop
/// (`compile_counted_loop_function`), then the general control-flow compiler
/// (`compile_scalar_int_cfg`) for arbitrary `if`/`else`/`while` shapes.
/// Returns `None` when the function is outside every subset.
///
/// The leaf and counted-loop paths stay ahead of the general compiler because
/// they emit tighter code (locals read directly, native loop increment) for the
/// shapes they recognize; the CFG compiler is the catch-all that trades a few
/// extra slot copies for covering everything else in the int/bool subset.
pub fn compile_scalar_int_function(
    function: &IrFunction,
    constants: &[IrConstant],
    region_id: u32,
) -> Option<CompiledScalarRegion> {
    compile_scalar_int_function_with_permits(
        function,
        constants,
        region_id,
        NativeCallPermits::default(),
    )
}

/// [`compile_scalar_int_function`] with explicit native-call permission.
///
/// The straight-line leaf, counted-loop, and float-leaf recognizers never lower
/// calls, so a function containing a permitted builtin call is rejected by them
/// and reaches the general CFG compiler, which honors `permits` to lower the
/// call to a guarded native helper (e.g. `abs` → [`ScalarIntOp::CallAbsI64`]).
/// With the default permits this is identical to [`compile_scalar_int_function`].
///
/// The native→userland tail-call recognizer runs *last*, after the CFG compiler,
/// so a call the CFG compiler already lowers natively (e.g. `return abs($x)`)
/// keeps its tighter form and only a call outside every other subset — a
/// non-inlinable userland tail call — reaches `compile_scalar_int_tailcall_leaf`.
pub fn compile_scalar_int_function_with_permits(
    function: &IrFunction,
    constants: &[IrConstant],
    region_id: u32,
    permits: NativeCallPermits,
) -> Option<CompiledScalarRegion> {
    if let Some((graph, result)) = build_scalar_int_region(function, constants, region_id)
        && let Ok(compiled) = compile_scalar_int_region(&graph, result)
    {
        return Some(compiled);
    }
    if let Some(compiled) = compile_counted_loop_function(function, constants) {
        return Some(compiled);
    }
    if let Some(compiled) = compile_scalar_int_cfg(function, constants, permits) {
        return Some(compiled);
    }
    if let Some(compiled) = compile_scalar_float_leaf(function, constants) {
        return Some(compiled);
    }
    compile_scalar_int_tailcall_leaf(function, constants, permits)
}

/// [`compile_scalar_int_function_with_permits`] plus runtime-helper addresses,
/// enabling the heap-handle and type-predicate shapes (currently `count` →
/// `php_jit_array_len`, `strlen` → the string-length wrapper, and the pure
/// `is_int`/`is_string`/`is_array`/`is_float`/`is_bool` predicates;
/// concat/property-load will follow).
///
/// These recognizers run *first*: their shapes (a string/array/untyped parameter
/// feeding a builtin) are disjoint from the int/float scalar subset the other
/// recognizers accept, so trying them first never steals a scalar leaf — and with
/// the default permits (all false) each returns `None`, so this reduces exactly to
/// [`compile_scalar_int_function_with_permits`].
pub fn compile_scalar_int_function_with_permits_and_helpers(
    function: &IrFunction,
    constants: &[IrConstant],
    region_id: u32,
    permits: NativeCallPermits,
    helpers: CopyPatchRuntimeHelpers,
) -> Option<CompiledScalarRegion> {
    if let Some(compiled) = compile_scalar_int_count_leaf(function, permits, helpers) {
        return Some(compiled);
    }
    if let Some(compiled) = compile_scalar_int_strlen_leaf(function, permits, helpers) {
        return Some(compiled);
    }
    if let Some(compiled) = compile_scalar_int_is_type_leaf(function, permits) {
        return Some(compiled);
    }
    compile_scalar_int_function_with_permits(function, constants, region_id, permits)
}

/// The structural match shared by the single-argument builtin-leaf recognizers
/// (`count`/`strlen`/`is_*`): the sole parameter, the un-normalized call name,
/// and the slot layout. Builtin-specific gates (name, permit, declared return
/// and parameter types) are applied by each recognizer.
struct SingleArgBuiltinLeaf<'a> {
    /// The function's sole parameter (by-value, non-variadic, no default).
    param: &'a php_ir::IrParam,
    /// The call name exactly as written at the call site; a namespaced shadow
    /// keeps its `\`, so a bare-name check never matches it.
    call_name: String,
    /// Slot the native result is written to (above locals + registers).
    result_slot: u32,
    /// `JitCValue` slots the caller's buffer must provide.
    buffer_slots: u32,
}

/// Match the `return builtin($x)` leaf shape common to the `count`/`strlen`/
/// `is_*` recognizers: a single-block free function with exactly one by-value,
/// non-variadic, no-default parameter, whose body is exactly `LoadLocal($param)`
/// then a single-positional-argument `CallFunction` on that register, returned
/// unchanged. Returns the parameter, the un-normalized call name, and the slot
/// layout, or `None` for any other shape (methods, closures, generators, wrong
/// arity, named/spread args, by-ref params, extra body). Callers still apply the
/// builtin-specific name/permit/type gates.
fn match_single_arg_builtin_leaf(function: &IrFunction) -> Option<SingleArgBuiltinLeaf<'_>> {
    let flags = function.flags;
    if flags.is_top_level || flags.is_closure || flags.is_method || flags.is_generator {
        return None;
    }
    if function.returns_by_ref || !function.captures.is_empty() {
        return None;
    }
    // Exactly one parameter: by-value, non-variadic, no default.
    let [param] = function.params.as_slice() else {
        return None;
    };
    if param.by_ref || param.variadic || param.default.is_some() {
        return None;
    }

    // Single-block leaf: load the parameter, then call the builtin on it, return.
    let [block] = function.blocks.as_slice() else {
        return None;
    };
    let kinds = meaningful_kinds(block);
    let [
        InstructionKind::LoadLocal {
            dst: load_reg,
            local: load_local,
        },
        InstructionKind::CallFunction {
            dst: call_dst,
            name,
            args,
        },
    ] = kinds.as_slice()
    else {
        return None;
    };
    if *load_local != param.local {
        return None;
    }
    // A single plain positional argument, the loaded parameter register. A
    // by-reference *arg* annotation only tracks the variable's source location
    // for potential write-back; these pure builtins never write back, so it is
    // moot (as in the `abs` path).
    let [arg] = args.as_slice() else {
        return None;
    };
    if arg.name.is_some() || arg.unpack {
        return None;
    }
    let Operand::Register(arg_reg) = arg.value else {
        return None;
    };
    if arg_reg != *load_reg {
        return None;
    }
    // The terminator must return exactly the call's result register.
    let TerminatorKind::Return {
        value: Some(Operand::Register(ret_reg)),
        by_ref_local: None,
    } = &block.terminator.as_ref()?.kind
    else {
        return None;
    };
    if ret_reg != call_dst {
        return None;
    }

    // Slot layout matches the other leaf compilers: locals occupy their indices,
    // so the sole parameter is marshaled into `slot[param.local]`; the result
    // lands in a dedicated slot above locals + registers. The stencil reads the
    // argument straight from the parameter slot — no register copy.
    let result_slot = function.local_count.checked_add(function.register_count)?;
    let buffer_slots = result_slot.checked_add(1)?;
    if result_slot > MAX_SLOT {
        return None;
    }
    Some(SingleArgBuiltinLeaf {
        param,
        call_name: name.clone(),
        result_slot,
        buffer_slots,
    })
}

/// Recognize and lower a `count($array)` leaf to a native
/// [`ScalarIntOp::CallCountI64`], gated by `permits.builtin_count` and a resolved
/// `helpers.array_len` address.
///
/// The matched shape is a single-argument builtin leaf (see
/// [`match_single_arg_builtin_leaf`]) named `count`, returning `int` or untyped
/// (`count()` is always int-valued), whose parameter is `array`-typed or untyped
/// — an int/float/etc. parameter could never be an array at runtime, so it is
/// outside this shape (and the guard would always fail). A namespaced `count`
/// shadow keeps its namespace in the call name, so the literal `name == "count"`
/// check never matches it — the interpreter runs the shadow.
///
/// The stencil guards the runtime array tag and the helper's packed-int layout,
/// so only a plain packed all-int array runs natively; every other value
/// side-exits and the interpreter reproduces the exact result/error.
fn compile_scalar_int_count_leaf(
    function: &IrFunction,
    permits: NativeCallPermits,
    helpers: CopyPatchRuntimeHelpers,
) -> Option<CompiledScalarRegion> {
    if !permits.builtin_count || helpers.array_len == 0 {
        return None;
    }
    // `count()` returns int; accept an int-typed or untyped declared return.
    if !matches!(function.return_type, None | Some(IrReturnType::Int)) {
        return None;
    }
    let leaf = match_single_arg_builtin_leaf(function)?;
    if leaf.call_name != "count" {
        return None;
    }
    if !matches!(leaf.param.type_, None | Some(IrReturnType::Array)) {
        return None;
    }
    let code = emit_scalar_int_ops(&[ScalarIntOp::CallCountI64 {
        dst: leaf.result_slot,
        arg: leaf.param.local.raw(),
        array_len_helper: helpers.array_len,
    }])
    .ok()?;
    Some(CompiledScalarRegion {
        code,
        result_slot: leaf.result_slot,
        buffer_slots: leaf.buffer_slots,
        tail_call: None,
    })
}

/// Recognize and lower a `strlen($string)` leaf to a native
/// [`ScalarIntOp::CallStrlenI64`], gated by `permits.builtin_strlen` and a
/// resolved `helpers.strlen` address.
///
/// The matched shape is a single-argument builtin leaf (see
/// [`match_single_arg_builtin_leaf`]) named `strlen`, returning `int` or untyped
/// (`strlen()` is always int-valued), whose parameter is `string`-typed or
/// untyped. A `string`-typed parameter is always a genuine string (PHP coerces
/// the argument at the boundary); an untyped parameter may be anything, and any
/// non-string value side-exits at the tag guard so the interpreter applies
/// `strlen`'s coercion/`TypeError` semantics. A namespaced `strlen` shadow keeps
/// its namespace in the call name, so the literal check never matches it.
///
/// Only a genuine `Value::String` runs natively; its byte length (PHP `strlen`
/// is a byte count) is read by the helper. Every other value side-exits.
fn compile_scalar_int_strlen_leaf(
    function: &IrFunction,
    permits: NativeCallPermits,
    helpers: CopyPatchRuntimeHelpers,
) -> Option<CompiledScalarRegion> {
    if !permits.builtin_strlen || helpers.strlen == 0 {
        return None;
    }
    // `strlen()` returns int; accept an int-typed or untyped declared return.
    if !matches!(function.return_type, None | Some(IrReturnType::Int)) {
        return None;
    }
    let leaf = match_single_arg_builtin_leaf(function)?;
    if leaf.call_name != "strlen" {
        return None;
    }
    if !matches!(leaf.param.type_, None | Some(IrReturnType::String)) {
        return None;
    }
    let code = emit_scalar_int_ops(&[ScalarIntOp::CallStrlenI64 {
        dst: leaf.result_slot,
        arg: leaf.param.local.raw(),
        strlen_helper: helpers.strlen,
    }])
    .ok()?;
    Some(CompiledScalarRegion {
        code,
        result_slot: leaf.result_slot,
        buffer_slots: leaf.buffer_slots,
        tail_call: None,
    })
}

/// Map a canonical type-predicate builtin name to the marshaled [`JitCValueTag`]
/// its argument must carry, but only when the matching per-name permit is set.
///
/// Covers exactly the canonical spellings whose answer is a pure tag check:
/// `is_int`/`is_string`/`is_array`/`is_float`/`is_bool`. The aliases
/// (`is_integer`/`is_long`/`is_double`) and the predicates with non-tag semantics
/// (`is_null`/`is_object`/`is_numeric`/`is_scalar`/`is_callable`/`is_iterable`/
/// `is_a`) return `None` and fall through to the interpreter. A namespaced shadow
/// keeps its `\`, so it never matches a bare name here.
fn is_type_predicate_tag(name: &str, permits: NativeCallPermits) -> Option<u16> {
    match name {
        "is_int" if permits.builtin_is_int => Some(INT_TAG),
        "is_string" if permits.builtin_is_string => Some(STRING_TAG),
        "is_array" if permits.builtin_is_array => Some(ARRAY_TAG),
        "is_float" if permits.builtin_is_float => Some(FLOAT_TAG),
        "is_bool" if permits.builtin_is_bool => Some(BOOL_TAG),
        _ => None,
    }
}

/// Recognize and lower an `is_TYPE($x)` leaf for a canonical type predicate to a
/// native [`ScalarIntOp::IsType`], gated by the predicate's own permit (see
/// [`is_type_predicate_tag`]).
///
/// The matched shape is a single-argument builtin leaf (see
/// [`match_single_arg_builtin_leaf`]) whose name is one of the canonical
/// predicates and returning `bool` or untyped. The parameter may be of *any* (or
/// no) declared type: the answer is the marshaled tag, which is correct for every
/// definite tag, and an `Uninitialized`-marshaled argument (`null`, an object,
/// etc.) side-exits so the interpreter answers — so no declared type can produce
/// a wrong result. No helper is needed (the stencil only reads the tag word).
fn compile_scalar_int_is_type_leaf(
    function: &IrFunction,
    permits: NativeCallPermits,
) -> Option<CompiledScalarRegion> {
    // `is_*()` returns bool; accept a bool-typed or untyped declared return.
    if !matches!(function.return_type, None | Some(IrReturnType::Bool)) {
        return None;
    }
    let leaf = match_single_arg_builtin_leaf(function)?;
    let expected_tag = is_type_predicate_tag(&leaf.call_name, permits)?;
    let code = emit_scalar_int_ops(&[ScalarIntOp::IsType {
        dst: leaf.result_slot,
        arg: leaf.param.local.raw(),
        expected_tag,
    }])
    .ok()?;
    Some(CompiledScalarRegion {
        code,
        result_slot: leaf.result_slot,
        buffer_slots: leaf.buffer_slots,
        tail_call: None,
    })
}

/// Lower a recognized monomorphic scalar property-load leaf to native code: a
/// single [`ScalarIntOp::CallPropertyLoadScalar`] over the flat slot buffer.
///
/// The VM bridge (which owns class/property resolution) recognizes the
/// `return $o->prop;` leaf shape and builds the `JitPropertyLoadMetadata` the
/// layout guard needs, passing its (borrowed, VM-owned, outlives-the-leaf)
/// pointer as `metadata_ptr` and the property-load ABI wrapper address as
/// `helper`. `object_slot` is the object parameter's VM slot (its `LocalId`
/// index); `result_slot` is a dedicated slot above locals + registers;
/// `buffer_slots` sizes the caller's marshaled buffer. Returns `None` if either
/// address is unavailable (`0`) or a slot is outside the addressable range.
///
/// `php_jit` never recognizes this shape itself (it cannot resolve classes), so
/// with no bridge caller this is dead-inert; it only lowers the exact `(slot,
/// metadata, helper)` triple the bridge hands it.
pub fn compile_property_load_leaf(
    object_slot: u32,
    result_slot: u32,
    buffer_slots: u32,
    metadata_ptr: u64,
    helper: u64,
) -> Option<CompiledScalarRegion> {
    if helper == 0 || metadata_ptr == 0 {
        return None;
    }
    let code = emit_scalar_int_ops(&[ScalarIntOp::CallPropertyLoadScalar {
        dst: result_slot,
        arg: object_slot,
        metadata_ptr,
        helper,
    }])
    .ok()?;
    Some(CompiledScalarRegion {
        code,
        result_slot,
        buffer_slots,
        tail_call: None,
    })
}

/// Non-`Discard` instruction kinds of a block. `Discard` is a register-lifetime
/// hint with no scalar-int semantics, so filtering it makes the shape matching
/// direct.
fn meaningful_kinds(block: &BasicBlock) -> Vec<InstructionKind> {
    block
        .instructions
        .iter()
        .map(|instruction| instruction.kind.clone())
        .filter(|kind| !matches!(kind, InstructionKind::Discard { .. }))
        .collect()
}

fn int_constant(constants: &[IrConstant], id: ConstId) -> Option<i64> {
    match constants.get(id.index()) {
        Some(IrConstant::Int(value)) => Some(*value),
        _ => None,
    }
}

/// Recognize a canonical counted `for` loop and lower it to a native
/// [`CountedLoop`]. The matched shape (as the frontend lowers
/// `for ($i = c; $i < $n; $i++) { $acc = $acc <op> $var; … }` in an
/// `int`-returning free function) is exactly five blocks:
///
/// - entry: `[LoadConst; StoreLocal]*` initializers, then `jump header`;
/// - header: `load counter; load limit; compare less; jump_if body exit`;
/// - body: one or more `$L = $A <op> $B` accumulator statements, then `jump incr`;
/// - incr: `$counter = $counter + 1`, then `jump header`;
/// - exit: `load result; return result`.
///
/// Locals map to slots by index (`LocalId::raw`), matching the marshaling
/// convention. Returns `None` for any other shape (the interpreter runs it).
fn compile_counted_loop_function(
    function: &IrFunction,
    constants: &[IrConstant],
) -> Option<CompiledScalarRegion> {
    let flags = function.flags;
    if flags.is_top_level || flags.is_closure || flags.is_method || flags.is_generator {
        return None;
    }
    if function.returns_by_ref || !function.captures.is_empty() {
        return None;
    }
    if function.return_type != Some(IrReturnType::Int) {
        return None;
    }
    for param in &function.params {
        if param.by_ref
            || param.variadic
            || param.default.is_some()
            || param.type_ != Some(IrReturnType::Int)
        {
            return None;
        }
    }

    let blocks = &function.blocks;
    if blocks.len() != 5 {
        return None;
    }

    // Entry block: initializer stores, then jump to the loop header.
    let entry = blocks.first()?;
    let entry_kinds = meaningful_kinds(entry);
    if !entry_kinds.len().is_multiple_of(2) {
        return None;
    }
    let mut prologue = Vec::new();
    for pair in entry_kinds.chunks_exact(2) {
        let (
            InstructionKind::LoadConst { dst, constant },
            InstructionKind::StoreLocal {
                local,
                src: Operand::Register(store_reg),
            },
        ) = (&pair[0], &pair[1])
        else {
            return None;
        };
        if store_reg != dst {
            return None;
        }
        prologue.push(ScalarIntOp::Const {
            dst: local.raw(),
            value: int_constant(constants, *constant)?,
        });
    }
    let TerminatorKind::Jump { target: header_id } = entry.terminator.as_ref()?.kind else {
        return None;
    };

    // Header block: `counter < limit`, branching to body / exit.
    let header = blocks.get(header_id.index())?;
    let (counter, limit, body_id, exit_id) = match meaningful_kinds(header).as_slice() {
        [
            InstructionKind::LoadLocal {
                dst: counter_reg,
                local: counter,
            },
            InstructionKind::LoadLocal {
                dst: limit_reg,
                local: limit,
            },
            InstructionKind::Compare {
                dst: cmp,
                op: CompareOp::Less,
                lhs: Operand::Register(cmp_lhs),
                rhs: Operand::Register(cmp_rhs),
            },
        ] if cmp_lhs == counter_reg && cmp_rhs == limit_reg => {
            let TerminatorKind::JumpIf {
                condition: Operand::Register(cond),
                if_true,
                if_false,
            } = header.terminator.as_ref()?.kind
            else {
                return None;
            };
            if cond != *cmp {
                return None;
            }
            (*counter, *limit, if_true, if_false)
        }
        _ => return None,
    };

    // Body block: accumulator statements, then jump to the increment block.
    let body = blocks.get(body_id.index())?;
    let body_kinds = meaningful_kinds(body);
    if body_kinds.is_empty() || !body_kinds.len().is_multiple_of(4) {
        return None;
    }
    let mut loop_body = Vec::new();
    for stmt in body_kinds.chunks_exact(4) {
        // Every body statement is `local = local <op> operand`, lowered as
        // load-lhs / load-rhs / binary / store. The right operand is either
        // another local (`Binary`) or a constant (`BinaryConst`).
        let (
            InstructionKind::LoadLocal {
                dst: lhs_reg,
                local: lhs_local,
            },
            InstructionKind::Binary {
                dst: result_reg,
                op,
                lhs: Operand::Register(bin_lhs),
                rhs: Operand::Register(bin_rhs),
            },
            InstructionKind::StoreLocal {
                local: store_local,
                src: Operand::Register(store_reg),
            },
        ) = (&stmt[0], &stmt[2], &stmt[3])
        else {
            return None;
        };
        if bin_lhs != lhs_reg || store_reg != result_reg {
            return None;
        }
        let op = int_bin_op(*op)?;
        let dst = store_local.raw();
        let lhs = lhs_local.raw();
        match &stmt[1] {
            InstructionKind::LoadLocal {
                dst: rhs_reg,
                local: rhs_local,
            } if bin_rhs == rhs_reg => {
                loop_body.push(ScalarIntOp::Binary {
                    op,
                    dst,
                    lhs,
                    rhs: rhs_local.raw(),
                });
            }
            InstructionKind::LoadConst {
                dst: rhs_reg,
                constant,
            } if bin_rhs == rhs_reg => {
                let value = int_constant(constants, *constant)?;
                loop_body.push(ScalarIntOp::BinaryConst {
                    op,
                    dst,
                    lhs,
                    rhs: value,
                });
            }
            _ => return None,
        }
    }
    let TerminatorKind::Jump { target: incr_id } = body.terminator.as_ref()?.kind else {
        return None;
    };

    // Increment block: `counter = counter + 1`, then back to the header.
    let incr = blocks.get(incr_id.index())?;
    match meaningful_kinds(incr).as_slice() {
        [
            InstructionKind::LoadLocal {
                dst: load_reg,
                local: incr_local,
            },
            InstructionKind::LoadConst {
                dst: one_reg,
                constant: one,
            },
            InstructionKind::Binary {
                dst: sum_reg,
                op: BinaryOp::Add,
                lhs: Operand::Register(add_lhs),
                rhs: Operand::Register(add_rhs),
            },
            InstructionKind::StoreLocal {
                local: store_local,
                src: Operand::Register(store_reg),
            },
        ] if *incr_local == counter
            && *store_local == counter
            && add_lhs == load_reg
            && add_rhs == one_reg
            && store_reg == sum_reg
            && int_constant(constants, *one) == Some(1) => {}
        _ => return None,
    }
    let TerminatorKind::Jump {
        target: incr_target,
    } = incr.terminator.as_ref()?.kind
    else {
        return None;
    };
    if incr_target != header_id {
        return None;
    }

    // Exit block: return a local.
    let exit = blocks.get(exit_id.index())?;
    let result_local = match meaningful_kinds(exit).as_slice() {
        [InstructionKind::LoadLocal { dst, local }] => {
            let TerminatorKind::Return {
                value: Some(Operand::Register(ret_reg)),
                by_ref_local: None,
            } = exit.terminator.as_ref()?.kind
            else {
                return None;
            };
            if ret_reg != *dst {
                return None;
            }
            *local
        }
        _ => return None,
    };

    let counted = CountedLoop {
        prologue,
        counter: counter.raw(),
        limit: limit.raw(),
        body: loop_body,
    };
    let code = emit_counted_loop(&counted).ok()?;
    Some(CompiledScalarRegion {
        code,
        result_slot: result_local.raw(),
        buffer_slots: function.local_count,
        tail_call: None,
    })
}

/// Lower a value-producing move into `dst` from an operand: a register or local
/// becomes a whole-value [`ScalarIntOp::Copy`], an int constant becomes a
/// [`ScalarIntOp::Const`]. `None` for any other operand form.
fn move_to_slot(
    dst: u32,
    src: &Operand,
    constants: &[IrConstant],
    local_count: u32,
) -> Option<ScalarIntOp> {
    match src {
        Operand::Register(r) => Some(ScalarIntOp::Copy {
            dst,
            src: local_count + r.raw(),
        }),
        Operand::Local(l) => Some(ScalarIntOp::Copy { dst, src: l.raw() }),
        Operand::Constant(c) => Some(ScalarIntOp::Const {
            dst,
            value: int_constant(constants, *c)?,
        }),
    }
}

/// Lower one non-call scalar-int instruction to its slot op, using the CFG slot
/// mapping (`reg_slot` for registers, raw index for locals). Shared by the
/// general CFG compiler and the tail-call recognizer for the prefix subset —
/// `LoadLocal`/`LoadLocalQuiet`, `LoadConst`(int), `Move`, `StoreLocal`,
/// `Binary`, and `Compare`. Returns `None` for any instruction outside the
/// int/bool subset, including every `CallFunction` (each caller handles calls —
/// or rejects them — itself).
fn scalar_int_prefix_op(
    kind: &InstructionKind,
    constants: &[IrConstant],
    local_count: u32,
    reg_slot: impl Fn(RegId) -> u32,
) -> Option<ScalarIntOp> {
    match kind {
        InstructionKind::LoadLocal { dst, local }
        | InstructionKind::LoadLocalQuiet { dst, local } => Some(ScalarIntOp::Copy {
            dst: reg_slot(*dst),
            src: local.raw(),
        }),
        InstructionKind::LoadConst { dst, constant } => Some(ScalarIntOp::Const {
            dst: reg_slot(*dst),
            value: int_constant(constants, *constant)?,
        }),
        InstructionKind::Move { dst, src } => {
            move_to_slot(reg_slot(*dst), src, constants, local_count)
        }
        InstructionKind::StoreLocal { local, src } => {
            move_to_slot(local.raw(), src, constants, local_count)
        }
        InstructionKind::Binary {
            dst,
            op,
            lhs: Operand::Register(lhs),
            rhs,
        } => {
            let op = int_bin_op(*op)?;
            let dst = reg_slot(*dst);
            let lhs = reg_slot(*lhs);
            match rhs {
                Operand::Register(rhs) => Some(ScalarIntOp::Binary {
                    op,
                    dst,
                    lhs,
                    rhs: reg_slot(*rhs),
                }),
                Operand::Constant(c) => Some(ScalarIntOp::BinaryConst {
                    op,
                    dst,
                    lhs,
                    rhs: int_constant(constants, *c)?,
                }),
                Operand::Local(_) => None,
            }
        }
        InstructionKind::Compare {
            dst,
            op,
            lhs: Operand::Register(lhs),
            rhs: Operand::Register(rhs),
        } => Some(ScalarIntOp::Compare {
            cond: region_compare_to_cond(ir_compare_to_region(*op)?),
            dst: reg_slot(*dst),
            lhs: reg_slot(*lhs),
            rhs: reg_slot(*rhs),
        }),
        _ => None,
    }
}

/// Lower an arbitrary scalar-int control-flow graph (`if`/`else`, `while`, and
/// their combinations) to native code. Every SSA register and every local gets
/// its own slot, so values flow through the slot buffer and control flow is just
/// branches between per-block labels — no cross-block register allocation or phi
/// handling is needed. This is the general fallback after the straight-line leaf
/// and canonical counted-loop recognizers; it accepts the int/bool instruction
/// subset plus `Jump` / `JumpIf` / `Return(value)` terminators and returns `None`
/// (the interpreter runs the function) for anything else.
///
/// Slot layout: locals occupy `[0, local_count)`, registers occupy
/// `[local_count, local_count + register_count)`, and every `Return` copies its
/// value to a dedicated result slot just above them, so the caller always finds
/// the result in the same place regardless of which return fired.
fn compile_scalar_int_cfg(
    function: &IrFunction,
    constants: &[IrConstant],
    permits: NativeCallPermits,
) -> Option<CompiledScalarRegion> {
    let flags = function.flags;
    if flags.is_top_level || flags.is_closure || flags.is_method || flags.is_generator {
        return None;
    }
    if function.returns_by_ref || !function.captures.is_empty() {
        return None;
    }
    if !matches!(
        function.return_type,
        Some(IrReturnType::Int) | Some(IrReturnType::Bool)
    ) {
        return None;
    }
    for param in &function.params {
        if param.by_ref
            || param.variadic
            || param.default.is_some()
            || param.type_ != Some(IrReturnType::Int)
        {
            return None;
        }
    }
    if function.blocks.is_empty() {
        return None;
    }

    let local_count = function.local_count;
    let result_slot = local_count.checked_add(function.register_count)?;
    let buffer_slots = result_slot.checked_add(1)?;
    if result_slot > MAX_SLOT {
        return None;
    }
    let reg_slot = |r: RegId| local_count + r.raw();

    // Translate one instruction to its slot op. `None` means "outside the
    // subset", which aborts the whole compile (the interpreter runs it). The
    // non-call subset is shared with the tail-call recognizer via
    // `scalar_int_prefix_op`; only the `abs` helper call is CFG-specific.
    let to_op = |kind: &InstructionKind| -> Option<ScalarIntOp> {
        // A call to the real builtin `abs` (confirmed by the VM bridge via
        // `permits`) on a single by-value register argument lowers to the
        // guarded native helper call. Any other name, arity, argument form,
        // or an unconfirmed `abs` returns `None` so the interpreter runs it.
        if let InstructionKind::CallFunction { dst, name, args } = kind
            && permits.builtin_abs
            && name.as_str() == "abs"
        {
            let [arg] = args.as_slice() else {
                return None;
            };
            if arg.name.is_some() || arg.unpack {
                return None;
            }
            let Operand::Register(arg_reg) = arg.value else {
                return None;
            };
            return Some(ScalarIntOp::CallAbsI64 {
                dst: reg_slot(*dst),
                arg: reg_slot(arg_reg),
            });
        }
        scalar_int_prefix_op(kind, constants, local_count, reg_slot)
    };

    let mut asm = Aarch64Assembler::new();
    let deopt = asm.new_label();
    let block_labels: Vec<Label> = function.blocks.iter().map(|_| asm.new_label()).collect();
    // Block IDs need not equal their vec position, so index labels by id.
    let mut label_of = HashMap::new();
    for (pos, block) in function.blocks.iter().enumerate() {
        label_of.insert(block.id.index(), block_labels[pos]);
    }

    for (pos, block) in function.blocks.iter().enumerate() {
        asm.bind(block_labels[pos]);
        for kind in meaningful_kinds(block) {
            emit_op(&mut asm, deopt, to_op(&kind)?);
        }
        match &block.terminator.as_ref()?.kind {
            TerminatorKind::Jump { target } => {
                asm.b(*label_of.get(&target.index())?);
            }
            TerminatorKind::JumpIf {
                condition: Operand::Register(cond),
                if_true,
                if_false,
            } => {
                let slot = reg_slot(*cond);
                emit_bool_guard(&mut asm, deopt, slot);
                asm.ldr_x(X4, X0, payload_off(slot));
                asm.cmp_imm_x(X4, 0);
                asm.b_cond(Cond::NotEqual, *label_of.get(&if_true.index())?);
                asm.b(*label_of.get(&if_false.index())?);
            }
            TerminatorKind::Return {
                value: Some(Operand::Register(reg)),
                by_ref_local: None,
            } => {
                emit_op(
                    &mut asm,
                    deopt,
                    ScalarIntOp::Copy {
                        dst: result_slot,
                        src: reg_slot(*reg),
                    },
                );
                asm.movz(X0, 0);
                asm.ret();
            }
            _ => return None,
        }
    }

    asm.bind(deopt);
    asm.movz(X0, 1);
    asm.ret();

    Some(CompiledScalarRegion {
        code: asm.finish(),
        result_slot,
        buffer_slots,
        tail_call: None,
    })
}

/// One positional argument of a native→userland tail call: how the emitted
/// prefix produces it (guaranteed `Int`) into its fresh buffer slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TailArgSource {
    /// Guard `src` is `Int` (side exit otherwise), then copy it to the arg slot.
    GuardedCopy { src: u32 },
    /// Materialize a statically-known `Int` into the arg slot.
    Const { value: i64 },
}

/// Emit the tail-call region: guard every parameter slot as `Int`, run the
/// argument-computing `prefix`, marshal each argument into a fresh buffer slot at
/// `first_arg_slot + i` (guarding params as `Int` so a non-int actual argument
/// side-exits), then return [`JIT_HELPER_STATUS_TAILCALL`] without writing a
/// result. A guard failure anywhere takes the shared side exit (status `1`).
///
/// The entry parameter guards ensure the region only requests the tail call when
/// *every* parameter is a genuine `Int` — even a parameter that never feeds an
/// argument. That is what lets the VM materialize the caller's stack frame with
/// the raw call arguments (they equal the interpreter's coerced `int` values), so
/// a throwing or stack-inspecting callee observes a byte-identical backtrace.
///
/// Returns the machine code and the argument slots (in call order) the VM reads.
fn emit_tailcall_region(
    prefix: &[ScalarIntOp],
    args: &[TailArgSource],
    param_slots: &[u32],
    first_arg_slot: u32,
) -> Result<(Vec<u8>, Vec<u32>), SlotSequenceError> {
    for op in prefix {
        check_op_slots(*op)?;
    }
    for &slot in param_slots {
        check_slot(slot)?;
    }
    let mut arg_slots = Vec::with_capacity(args.len());
    for (index, arg) in args.iter().enumerate() {
        let slot = first_arg_slot
            .checked_add(
                u32::try_from(index)
                    .map_err(|_| SlotSequenceError::SlotIndexOutOfRange(first_arg_slot))?,
            )
            .ok_or(SlotSequenceError::SlotIndexOutOfRange(first_arg_slot))?;
        check_slot(slot)?;
        if let TailArgSource::GuardedCopy { src } = arg {
            check_slot(*src)?;
        }
        arg_slots.push(slot);
    }

    let mut asm = Aarch64Assembler::new();
    let deopt = asm.new_label();
    // Guard every parameter is `Int` up front (see the note above).
    for &slot in param_slots {
        emit_int_guard(&mut asm, deopt, slot);
    }
    for op in prefix {
        emit_op(&mut asm, deopt, *op);
    }
    for (arg, &slot) in args.iter().zip(arg_slots.iter()) {
        match arg {
            TailArgSource::GuardedCopy { src } => {
                emit_int_guard(&mut asm, deopt, *src);
                emit_value_copy(&mut asm, slot, *src);
            }
            TailArgSource::Const { value } => {
                asm.mov_imm64(X6, *value as u64);
                emit_store_int(&mut asm, slot, X6);
            }
        }
    }
    // Tail call requested: the arguments are in their slots and the VM performs
    // the call. No result is written — the region returns before the call.
    asm.movz(X0, JIT_HELPER_STATUS_TAILCALL as u16);
    asm.ret();
    asm.bind(deopt);
    asm.movz(X0, 1);
    asm.ret();
    Ok((asm.finish(), arg_slots))
}

/// Recognize a native→userland *tail-call* leaf and lower its argument-computing
/// prefix to native code, gated by `permits.allow_userland_tailcall`.
///
/// The matched shape is a single-block `int`/`bool`-returning free function
/// (int, by-value, non-variadic, no-default parameters) whose meaningful
/// instructions are a scalar-int prefix (`LoadLocal`, `LoadConst`(int), `Move`,
/// `StoreLocal`, `Binary`, `Compare`) followed by a final
/// `CallFunction { dst, name, args }` whose result register is returned
/// unchanged — `Return(%dst)`. Because the call is the last instruction, `%dst`
/// is used only by that `Return`: a genuine tail call.
///
/// The emitted region computes each positional argument into a fresh buffer slot
/// (guarding params as `Int`, so a non-int actual argument side-exits rather than
/// mis-marshaling), then returns
/// [`JIT_HELPER_STATUS_TAILCALL`] and records
/// a [`TailCallPlan`]. The VM bridge reads the argument slots and performs the
/// call through its normal interpreter path — so behavior matches the interpreter
/// by construction, with no native re-entry.
///
/// Rejected: named or spread arguments, a pure by-reference argument send
/// (`ByRefLocationPlaceholder`), a non-register/non-int-constant argument value,
/// a nested call or any out-of-subset prefix instruction, and any post-call work
/// (which would need on-stack replacement — out of scope). The callee's identity
/// (userland vs. builtin, arity, by-reference parameters, method/closure) is not
/// checked here — `php_jit` has no function registry — but by the VM at call
/// time, which interprets the whole leaf when the callee is out of scope.
fn compile_scalar_int_tailcall_leaf(
    function: &IrFunction,
    constants: &[IrConstant],
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
    if !matches!(
        function.return_type,
        Some(IrReturnType::Int) | Some(IrReturnType::Bool)
    ) {
        return None;
    }
    for param in &function.params {
        if param.by_ref
            || param.variadic
            || param.default.is_some()
            || param.type_ != Some(IrReturnType::Int)
        {
            return None;
        }
    }

    // Single-block leaf: prefix + tail call + return. A multi-block prefix would
    // need the general CFG lowering with a tail-call terminator — a later step.
    let [block] = function.blocks.as_slice() else {
        return None;
    };

    // The final meaningful instruction must be the tail call; everything before
    // it is the argument-computing prefix.
    let kinds = meaningful_kinds(block);
    let (call_kind, prefix_kinds) = kinds.split_last()?;
    let InstructionKind::CallFunction {
        dst: call_dst,
        name,
        args,
    } = call_kind
    else {
        return None;
    };

    // The terminator must return exactly the call's result register.
    let TerminatorKind::Return {
        value: Some(Operand::Register(ret_reg)),
        by_ref_local: None,
    } = &block.terminator.as_ref()?.kind
    else {
        return None;
    };
    if ret_reg != call_dst {
        return None;
    }

    let local_count = function.local_count;
    // Registers sit above locals; the fresh argument slots sit above the
    // registers, so an argument slot never aliases a value the prefix computes.
    let first_arg_slot = local_count.checked_add(function.register_count)?;
    let reg_slot = |r: RegId| local_count + r.raw();

    // Lower the prefix with the shared scalar-int lowering. A nested call or any
    // out-of-subset instruction rejects the whole leaf.
    let mut prefix = Vec::with_capacity(prefix_kinds.len());
    for kind in prefix_kinds {
        prefix.push(scalar_int_prefix_op(
            kind,
            constants,
            local_count,
            reg_slot,
        )?);
    }

    // Each argument must be a plain positional value whose operand is a
    // scalar-int register or int constant.
    let mut arg_sources = Vec::with_capacity(args.len());
    for arg in args {
        if arg.name.is_some()
            || arg.unpack
            || arg.value_kind == IrCallArgValueKind::ByRefLocationPlaceholder
        {
            return None;
        }
        let source = match arg.value {
            Operand::Register(reg) => TailArgSource::GuardedCopy { src: reg_slot(reg) },
            Operand::Constant(constant) => TailArgSource::Const {
                value: int_constant(constants, constant)?,
            },
            Operand::Local(_) => return None,
        };
        arg_sources.push(source);
    }

    let arg_count = u32::try_from(args.len()).ok()?;
    let buffer_slots = first_arg_slot.checked_add(arg_count)?;

    // Guard every parameter as `Int` at entry so the tail call is only requested
    // when the leaf's arguments are all genuine ints (see `emit_tailcall_region`).
    let param_slots: Vec<u32> = function
        .params
        .iter()
        .map(|param| param.local.raw())
        .collect();

    let (code, arg_slots) =
        emit_tailcall_region(&prefix, &arg_sources, &param_slots, first_arg_slot).ok()?;
    Some(CompiledScalarRegion {
        code,
        // Unused on the tail-call path (the VM produces the result); slot 0 is
        // always present, keeping the field well-formed.
        result_slot: 0,
        buffer_slots,
        tail_call: Some(TailCallPlan {
            callee_name: name.clone(),
            arg_slots,
        }),
    })
}

/// Map an IR `BinaryOp` to the native scalar-float subset (`Div` is included
/// because float `/` is float-typed, unlike int `/`).
fn float_bin_op(op: BinaryOp) -> Option<FloatBinOp> {
    match op {
        BinaryOp::Add => Some(FloatBinOp::Add),
        BinaryOp::Sub => Some(FloatBinOp::Sub),
        BinaryOp::Mul => Some(FloatBinOp::Mul),
        BinaryOp::Div => Some(FloatBinOp::Div),
        _ => None,
    }
}

fn float_constant(constants: &[IrConstant], id: ConstId) -> Option<u64> {
    match constants.get(id.index()) {
        Some(IrConstant::Float(value)) => Some(value.to_bits()),
        _ => None,
    }
}

/// Recognize and lower a straight-line scalar-**float** leaf function to native
/// code: a single block of `float`-typed by-value params returning `float`,
/// whose body is `LoadLocal` / `LoadConst`(float) / `Move` / `Binary`
/// (`Add`/`Sub`/`Mul`/`Div`) over register operands, then `Return`. Every SSA
/// register gets its own slot (like [`compile_scalar_int_cfg`]); the returned
/// value is copied to a dedicated result slot. Returns `None` for any other
/// shape (the interpreter runs it). Div carries a zero-divisor side exit.
fn compile_scalar_float_leaf(
    function: &IrFunction,
    constants: &[IrConstant],
) -> Option<CompiledScalarRegion> {
    let flags = function.flags;
    if flags.is_top_level || flags.is_closure || flags.is_method || flags.is_generator {
        return None;
    }
    if function.returns_by_ref || !function.captures.is_empty() {
        return None;
    }
    if function.return_type != Some(IrReturnType::Float) {
        return None;
    }
    for param in &function.params {
        if param.by_ref
            || param.variadic
            || param.default.is_some()
            || param.type_ != Some(IrReturnType::Float)
        {
            return None;
        }
    }
    // Single-block leaf only; branches/loops over floats are a later step.
    if function.blocks.len() != 1 {
        return None;
    }
    let block = function.blocks.first()?;

    let local_count = function.local_count;
    let result_slot = local_count.checked_add(function.register_count)?;
    let buffer_slots = result_slot.checked_add(1)?;
    if result_slot > MAX_SLOT {
        return None;
    }
    let reg_slot = |r: RegId| local_count + r.raw();

    let mut ops = Vec::new();
    for kind in meaningful_kinds(block) {
        let op = match &kind {
            InstructionKind::LoadLocal { dst, local }
            | InstructionKind::LoadLocalQuiet { dst, local } => ScalarFloatOp::Copy {
                dst: reg_slot(*dst),
                src: local.raw(),
            },
            InstructionKind::LoadConst { dst, constant } => ScalarFloatOp::Const {
                dst: reg_slot(*dst),
                bits: float_constant(constants, *constant)?,
            },
            InstructionKind::Move {
                dst,
                src: Operand::Register(src),
            } => ScalarFloatOp::Copy {
                dst: reg_slot(*dst),
                src: reg_slot(*src),
            },
            InstructionKind::Move {
                dst,
                src: Operand::Constant(c),
            } => ScalarFloatOp::Const {
                dst: reg_slot(*dst),
                bits: float_constant(constants, *c)?,
            },
            InstructionKind::Binary {
                dst,
                op,
                lhs: Operand::Register(lhs),
                rhs: Operand::Register(rhs),
            } => ScalarFloatOp::Binary {
                op: float_bin_op(*op)?,
                dst: reg_slot(*dst),
                lhs: reg_slot(*lhs),
                rhs: reg_slot(*rhs),
            },
            _ => return None,
        };
        ops.push(op);
    }

    // Terminator: return a register value, copied into the result slot.
    let TerminatorKind::Return {
        value: Some(Operand::Register(reg)),
        by_ref_local: None,
    } = &block.terminator.as_ref()?.kind
    else {
        return None;
    };
    ops.push(ScalarFloatOp::Copy {
        dst: result_slot,
        src: reg_slot(*reg),
    });

    let code = emit_scalar_float_ops(&ops).ok()?;
    Some(CompiledScalarRegion {
        code,
        result_slot,
        buffer_slots,
        tail_call: None,
    })
}

/// Map an IR `BinaryOp` to the native scalar-int subset.
fn int_bin_op(op: BinaryOp) -> Option<IntBinOp> {
    match op {
        BinaryOp::Add => Some(IntBinOp::Add),
        BinaryOp::Sub => Some(IntBinOp::Sub),
        BinaryOp::Mul => Some(IntBinOp::Mul),
        BinaryOp::Mod => Some(IntBinOp::Mod),
        BinaryOp::BitAnd => Some(IntBinOp::BitAnd),
        BinaryOp::BitOr => Some(IntBinOp::BitOr),
        BinaryOp::BitXor => Some(IntBinOp::BitXor),
        BinaryOp::ShiftLeft => Some(IntBinOp::Shl),
        BinaryOp::ShiftRight => Some(IntBinOp::Shr),
        _ => None,
    }
}

/// Map a region comparison op to the aarch64 condition after `cmp lhs, rhs`.
fn region_compare_to_cond(op: RegionCompareOp) -> Cond {
    match op {
        RegionCompareOp::Eq => Cond::Equal,
        RegionCompareOp::NotEq => Cond::NotEqual,
        RegionCompareOp::Lt => Cond::LessThan,
        RegionCompareOp::Lte => Cond::LessEqual,
        RegionCompareOp::Gt => Cond::GreaterThan,
        RegionCompareOp::Gte => Cond::GreaterEqual,
    }
}

/// Map an IR comparison op to the region comparison subset, or `None` for ops
/// outside guarded integer comparison. `Identical`/`NotIdentical` behave like
/// loose `==`/`!=` once both operands are guarded `Int`; `Spaceship` yields an
/// int, not a bool, so it is rejected.
fn ir_compare_to_region(op: CompareOp) -> Option<RegionCompareOp> {
    match op {
        CompareOp::Equal | CompareOp::Identical => Some(RegionCompareOp::Eq),
        CompareOp::NotEqual | CompareOp::NotIdentical => Some(RegionCompareOp::NotEq),
        CompareOp::Less => Some(RegionCompareOp::Lt),
        CompareOp::LessEqual => Some(RegionCompareOp::Lte),
        CompareOp::Greater => Some(RegionCompareOp::Gt),
        CompareOp::GreaterEqual => Some(RegionCompareOp::Gte),
        CompareOp::Spaceship => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CopyPatchRuntimeHelpers, GuardedIntAddStep, MAX_SLOT, NativeCallPermits,
        RegionCompileError, SlotSequenceError, build_scalar_int_region,
        compile_counted_loop_function, compile_scalar_int_function,
        compile_scalar_int_function_with_permits,
        compile_scalar_int_function_with_permits_and_helpers, compile_scalar_int_region,
        emit_guarded_int_add_sequence, int_bin_op,
    };
    use crate::region_ir::{
        NodeId, RegionConst, RegionEffects, RegionGraph, RegionId, RegionNode, RegionNodeKind,
        RegionPlacement, RegionValueType, VmSlotId,
    };
    use php_ir::instruction::{IrCallArg, IrCallArgValueKind, TerminatorKind};
    use php_ir::{
        BasicBlock, BinaryOp, BlockId, CompareOp, ConstId, FunctionFlags, InstrId, Instruction,
        InstructionKind, IrConstant, IrParam, IrReturnType, IrSpan, LocalId, Operand, RegId,
        Terminator,
    };

    fn int_param(name: &str, local: u32) -> IrParam {
        IrParam {
            name: name.to_string(),
            local: LocalId::new(local),
            required: true,
            default: None,
            type_: Some(IrReturnType::Int),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        }
    }

    /// `function f($a, $b): <return_type> { return $a <op> $b; }`
    fn binary_leaf(op: BinaryOp, return_type: IrReturnType) -> php_ir::IrFunction {
        let span = IrSpan::default();
        php_ir::IrFunction {
            name: "f".to_string(),
            params: vec![int_param("a", 0), int_param("b", 1)],
            locals: vec!["a".to_string(), "b".to_string()],
            local_count: 2,
            register_count: 3,
            blocks: vec![BasicBlock {
                id: BlockId::new(0),
                instructions: vec![
                    Instruction {
                        id: InstrId::new(0),
                        span,
                        kind: InstructionKind::LoadLocal {
                            dst: RegId::new(0),
                            local: LocalId::new(0),
                        },
                    },
                    Instruction {
                        id: InstrId::new(1),
                        span,
                        kind: InstructionKind::LoadLocal {
                            dst: RegId::new(1),
                            local: LocalId::new(1),
                        },
                    },
                    Instruction {
                        id: InstrId::new(2),
                        span,
                        kind: InstructionKind::Binary {
                            dst: RegId::new(2),
                            op,
                            lhs: Operand::Register(RegId::new(0)),
                            rhs: Operand::Register(RegId::new(1)),
                        },
                    },
                ],
                terminator: Some(Terminator {
                    span,
                    kind: TerminatorKind::Return {
                        value: Some(Operand::Register(RegId::new(2))),
                        by_ref_local: None,
                    },
                }),
            }],
            span,
            flags: FunctionFlags::default(),
            return_type: Some(return_type),
            returns_by_ref: false,
            captures: Vec::new(),
            attributes: Vec::new(),
        }
    }

    fn param(graph: &mut RegionGraph, slot: u32) -> NodeId {
        graph.add_node(RegionNode::new(
            RegionNodeKind::Param {
                slot: VmSlotId::new(slot),
            },
            Vec::new(),
            None,
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        ))
    }

    fn bin(graph: &mut RegionGraph, kind: RegionNodeKind, lhs: NodeId, rhs: NodeId) -> NodeId {
        graph.add_node(RegionNode::new(
            kind,
            vec![lhs, rhs],
            None,
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        ))
    }

    fn add(graph: &mut RegionGraph, lhs: NodeId, rhs: NodeId) -> NodeId {
        bin(graph, RegionNodeKind::Add, lhs, rhs)
    }

    fn const_i64(graph: &mut RegionGraph, value: i64) -> NodeId {
        let constant = graph.add_constant(RegionConst::I64(value));
        graph.add_node(RegionNode::new(
            RegionNodeKind::Const(constant),
            Vec::new(),
            None,
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        ))
    }

    #[test]
    fn empty_sequence_emits_only_the_return_epilogue() {
        // movz x0,#0 ; ret ; movz x0,#1 ; ret = four 32-bit instructions.
        let code = emit_guarded_int_add_sequence(&[]).expect("empty sequence emits");
        assert_eq!(code.len(), 4 * 4);
    }

    #[test]
    fn sequence_length_grows_with_each_step() {
        let one = emit_guarded_int_add_sequence(&[GuardedIntAddStep {
            dst: 2,
            lhs: 0,
            rhs: 1,
        }])
        .expect("one step emits");
        let two = emit_guarded_int_add_sequence(&[
            GuardedIntAddStep {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
            GuardedIntAddStep {
                dst: 4,
                lhs: 2,
                rhs: 3,
            },
        ])
        .expect("two steps emit");
        // Each step emits the same fixed-size stencil, so two steps add exactly
        // one step's worth of instructions over one step.
        assert_eq!(two.len() - one.len(), one.len() - 4 * 4);
        assert!(one.len().is_multiple_of(4) && two.len().is_multiple_of(4));
    }

    #[test]
    fn out_of_range_slot_is_rejected_not_miscompiled() {
        let bad = MAX_SLOT + 1;
        assert_eq!(
            emit_guarded_int_add_sequence(&[GuardedIntAddStep {
                dst: bad,
                lhs: 0,
                rhs: 1,
            }]),
            Err(SlotSequenceError::SlotIndexOutOfRange(bad)),
        );
        // The last addressable slot is accepted.
        assert!(
            emit_guarded_int_add_sequence(&[GuardedIntAddStep {
                dst: MAX_SLOT,
                lhs: 0,
                rhs: 1,
            }])
            .is_ok()
        );
    }

    #[test]
    fn compiles_scalar_int_region_to_slot_layout() {
        // result = (p0 + p1) + p2; params in slots 0..3, temporaries above.
        let mut graph = RegionGraph::new(RegionId::new(1), "add-region");
        let p0 = param(&mut graph, 0);
        let p1 = param(&mut graph, 1);
        let p2 = param(&mut graph, 2);
        let sum01 = add(&mut graph, p0, p1);
        let total = add(&mut graph, sum01, p2);

        let compiled = compile_scalar_int_region(&graph, total).expect("region compiles");
        // Temps are allocated above the max param slot (2): sum01 -> 3, total -> 4.
        assert_eq!(compiled.result_slot, 4);
        assert_eq!(compiled.buffer_slots, 5);
        assert!(!compiled.code.is_empty());
    }

    #[test]
    fn compiles_sub_mul_and_const_nodes() {
        // result = (p0 - p1) * 10
        let mut graph = RegionGraph::new(RegionId::new(4), "sub-mul-const");
        let p0 = param(&mut graph, 0);
        let p1 = param(&mut graph, 1);
        let diff = bin(&mut graph, RegionNodeKind::Sub, p0, p1);
        let ten = const_i64(&mut graph, 10);
        let scaled = bin(&mut graph, RegionNodeKind::Mul, diff, ten);

        let compiled = compile_scalar_int_region(&graph, scaled).expect("region compiles");
        // Params 0,1 -> temps from 2: diff -> 2, const 10 -> 3, scaled -> 4.
        assert_eq!(compiled.result_slot, 4);
        assert_eq!(compiled.buffer_slots, 5);
        assert!(!compiled.code.is_empty());
    }

    #[test]
    fn rejects_a_node_outside_the_subset() {
        // Div is not in the scalar-int subset (no divide-by-zero guard yet).
        let mut graph = RegionGraph::new(RegionId::new(2), "div-region");
        let p0 = param(&mut graph, 0);
        let p1 = param(&mut graph, 1);
        let bad = bin(&mut graph, RegionNodeKind::Div, p0, p1);
        assert_eq!(
            compile_scalar_int_region(&graph, bad),
            Err(RegionCompileError::UnsupportedNode("non-scalar-int-op")),
        );
    }

    #[test]
    fn rejects_a_passed_through_parameter_result() {
        let mut graph = RegionGraph::new(RegionId::new(3), "id-region");
        let p0 = param(&mut graph, 0);
        assert_eq!(
            compile_scalar_int_region(&graph, p0),
            Err(RegionCompileError::ResultNotComputed),
        );
    }

    #[test]
    fn recognizes_add_of_two_int_params() {
        // function f($a, $b): int { return $a + $b; }
        let function = binary_leaf(BinaryOp::Add, IrReturnType::Int);
        let compiled =
            compile_scalar_int_function(&function, &[], 1).expect("scalar-int leaf recognized");
        // Params occupy slots 0,1; the add result lands in temp slot 2.
        assert_eq!(compiled.result_slot, 2);
        assert_eq!(compiled.buffer_slots, 3);
        assert!(!compiled.code.is_empty());
    }

    /// `function f(int $x): int { return abs($x) + 1; }` as the frontend lowers
    /// it (see `php-vm dump-ir`): a single block with a `CallFunction "abs"` on a
    /// register argument, then `+ 1`.
    fn abs_plus_one_function(call_name: &str) -> php_ir::IrFunction {
        let span = IrSpan::default();
        let ins = |kind| Instruction {
            id: InstrId::new(0),
            span,
            kind,
        };
        let positional = |reg: u32| IrCallArg {
            name: None,
            value: Operand::Register(RegId::new(reg)),
            unpack: false,
            value_kind: IrCallArgValueKind::Direct,
            by_ref_local: Some(LocalId::new(0)),
            by_ref_dim: None,
            by_ref_property: None,
            by_ref_property_dim: None,
        };
        php_ir::IrFunction {
            name: "f".to_string(),
            params: vec![int_param("x", 0)],
            locals: vec!["x".to_string()],
            local_count: 1,
            register_count: 4,
            blocks: vec![BasicBlock {
                id: BlockId::new(0),
                instructions: vec![
                    ins(InstructionKind::LoadLocal {
                        dst: RegId::new(1),
                        local: LocalId::new(0),
                    }),
                    ins(InstructionKind::CallFunction {
                        dst: RegId::new(0),
                        name: call_name.to_string(),
                        args: vec![positional(1)],
                    }),
                    ins(InstructionKind::LoadConst {
                        dst: RegId::new(2),
                        constant: ConstId::new(0),
                    }),
                    ins(InstructionKind::Binary {
                        dst: RegId::new(3),
                        op: BinaryOp::Add,
                        lhs: Operand::Register(RegId::new(0)),
                        rhs: Operand::Register(RegId::new(2)),
                    }),
                ],
                terminator: Some(Terminator {
                    span,
                    kind: TerminatorKind::Return {
                        value: Some(Operand::Register(RegId::new(3))),
                        by_ref_local: None,
                    },
                }),
            }],
            span,
            flags: FunctionFlags::default(),
            return_type: Some(IrReturnType::Int),
            returns_by_ref: false,
            captures: Vec::new(),
            attributes: Vec::new(),
        }
    }

    #[test]
    fn recognizes_builtin_abs_call_only_with_permit() {
        let function = abs_plus_one_function("abs");
        let constants = [IrConstant::Int(1)];
        let permits = NativeCallPermits {
            builtin_abs: true,
            ..NativeCallPermits::default()
        };

        // Without permission the `abs` call is out of subset -> interpreter.
        assert!(compile_scalar_int_function(&function, &constants, 1).is_none());

        // With the VM's confirmation, the CFG compiler lowers it natively.
        let compiled = compile_scalar_int_function_with_permits(&function, &constants, 1, permits)
            .expect("abs leaf recognized when the builtin is confirmed");
        // Locals: $x = slot 0; registers r0..r3 -> slots 1..4; result slot 5.
        assert_eq!(compiled.result_slot, 5);
        assert_eq!(compiled.buffer_slots, 6);
        assert!(!compiled.code.is_empty());
    }

    #[test]
    fn rejects_a_namespaced_abs_call_even_with_permit() {
        // A namespaced call keeps its `\` in the lowered name, so it is never
        // matched as the builtin regardless of the permit.
        let function = abs_plus_one_function("app\\abs");
        let constants = [IrConstant::Int(1)];
        let permits = NativeCallPermits {
            builtin_abs: true,
            ..NativeCallPermits::default()
        };
        assert!(
            compile_scalar_int_function_with_permits(&function, &constants, 1, permits).is_none()
        );
    }

    /// `function f($x): $ret { return $name($x); }` as the frontend lowers it:
    /// one by-value parameter loaded into a register, a single-positional
    /// `CallFunction` on it, returned unchanged. The shared shape behind the
    /// count/strlen/is_* recognizer tests; `call_name`, `param_type`, and
    /// `return_type` vary to exercise the shadow, typed-parameter, and
    /// return-type cases.
    fn single_arg_builtin_leaf_function(
        call_name: &str,
        param_type: Option<IrReturnType>,
        return_type: Option<IrReturnType>,
    ) -> php_ir::IrFunction {
        let span = IrSpan::default();
        let ins = |kind| Instruction {
            id: InstrId::new(0),
            span,
            kind,
        };
        let positional = |reg: u32| IrCallArg {
            name: None,
            value: Operand::Register(RegId::new(reg)),
            unpack: false,
            value_kind: IrCallArgValueKind::Direct,
            by_ref_local: Some(LocalId::new(0)),
            by_ref_dim: None,
            by_ref_property: None,
            by_ref_property_dim: None,
        };
        php_ir::IrFunction {
            name: "f".to_string(),
            params: vec![IrParam {
                name: "a".to_string(),
                local: LocalId::new(0),
                required: true,
                default: None,
                type_: param_type,
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            }],
            locals: vec!["a".to_string()],
            local_count: 1,
            register_count: 2,
            blocks: vec![BasicBlock {
                id: BlockId::new(0),
                instructions: vec![
                    ins(InstructionKind::LoadLocal {
                        dst: RegId::new(1),
                        local: LocalId::new(0),
                    }),
                    ins(InstructionKind::CallFunction {
                        dst: RegId::new(0),
                        name: call_name.to_string(),
                        args: vec![positional(1)],
                    }),
                ],
                terminator: Some(Terminator {
                    span,
                    kind: TerminatorKind::Return {
                        value: Some(Operand::Register(RegId::new(0))),
                        by_ref_local: None,
                    },
                }),
            }],
            span,
            flags: FunctionFlags::default(),
            return_type,
            returns_by_ref: false,
            captures: Vec::new(),
            attributes: Vec::new(),
        }
    }

    /// The count-specific shape: `function f($a) { return count($a); }` with an
    /// untyped return (like the microbench). Delegates to the shared builder.
    fn count_leaf_function(
        call_name: &str,
        param_type: Option<IrReturnType>,
    ) -> php_ir::IrFunction {
        single_arg_builtin_leaf_function(call_name, param_type, None)
    }

    #[test]
    fn recognizes_builtin_count_leaf_only_with_permit_and_helper() {
        let function = count_leaf_function("count", None);
        // A non-zero placeholder address; the recognizer only embeds it (the
        // emitted `blr` is exercised end-to-end in code_memory.rs).
        let helpers = CopyPatchRuntimeHelpers {
            array_len: 0x1000,
            strlen: 0,
        };
        let permits = NativeCallPermits {
            builtin_count: true,
            ..NativeCallPermits::default()
        };

        // No permit -> outside every subset (the interpreter runs it).
        assert!(
            compile_scalar_int_function_with_permits_and_helpers(
                &function,
                &[],
                1,
                NativeCallPermits::default(),
                helpers,
            )
            .is_none()
        );
        // Permit but no resolved helper address -> not lowered.
        assert!(
            compile_scalar_int_function_with_permits_and_helpers(
                &function,
                &[],
                1,
                permits,
                CopyPatchRuntimeHelpers::default(),
            )
            .is_none()
        );

        // Permit + helper -> recognized. $a=slot 0; regs r0,r1 -> slots 1,2; the
        // count result lands in the dedicated result slot 3.
        let compiled = compile_scalar_int_function_with_permits_and_helpers(
            &function,
            &[],
            1,
            permits,
            helpers,
        )
        .expect("count leaf recognized when the builtin is confirmed and the helper resolves");
        assert_eq!(compiled.result_slot, 3);
        assert_eq!(compiled.buffer_slots, 4);
        assert!(compiled.tail_call.is_none());
        assert!(!compiled.code.is_empty());
    }

    #[test]
    fn recognizes_count_leaf_with_array_typed_parameter() {
        let function = count_leaf_function("count", Some(IrReturnType::Array));
        let helpers = CopyPatchRuntimeHelpers {
            array_len: 0x1000,
            strlen: 0,
        };
        let permits = NativeCallPermits {
            builtin_count: true,
            ..NativeCallPermits::default()
        };
        assert!(
            compile_scalar_int_function_with_permits_and_helpers(
                &function,
                &[],
                1,
                permits,
                helpers
            )
            .is_some()
        );
    }

    #[test]
    fn rejects_a_namespaced_count_call_even_with_permit() {
        // A namespaced call keeps its `\` in the lowered name, so it is never
        // matched as the builtin `count` regardless of the permit.
        let function = count_leaf_function("app\\count", None);
        let helpers = CopyPatchRuntimeHelpers {
            array_len: 0x1000,
            strlen: 0,
        };
        let permits = NativeCallPermits {
            builtin_count: true,
            ..NativeCallPermits::default()
        };
        assert!(
            compile_scalar_int_function_with_permits_and_helpers(
                &function,
                &[],
                1,
                permits,
                helpers
            )
            .is_none()
        );
    }

    /// Runtime helpers with only the `strlen` address resolved (a non-zero
    /// placeholder; the emitted `blr` is exercised end-to-end in code_memory.rs).
    fn strlen_helpers() -> CopyPatchRuntimeHelpers {
        CopyPatchRuntimeHelpers {
            array_len: 0,
            strlen: 0x2000,
        }
    }

    #[test]
    fn recognizes_builtin_strlen_leaf_only_with_permit_and_helper() {
        let function = single_arg_builtin_leaf_function("strlen", None, Some(IrReturnType::Int));
        let helpers = strlen_helpers();
        let permits = NativeCallPermits {
            builtin_strlen: true,
            ..NativeCallPermits::default()
        };

        // No permit -> outside every subset (the interpreter runs it).
        assert!(
            compile_scalar_int_function_with_permits_and_helpers(
                &function,
                &[],
                1,
                NativeCallPermits::default(),
                helpers,
            )
            .is_none()
        );
        // Permit but no resolved helper address -> not lowered.
        assert!(
            compile_scalar_int_function_with_permits_and_helpers(
                &function,
                &[],
                1,
                permits,
                CopyPatchRuntimeHelpers::default(),
            )
            .is_none()
        );

        // Permit + helper -> recognized. $a=slot 0; regs r0,r1 -> slots 1,2; the
        // strlen result lands in the dedicated result slot 3.
        let compiled = compile_scalar_int_function_with_permits_and_helpers(
            &function,
            &[],
            1,
            permits,
            helpers,
        )
        .expect("strlen leaf recognized when the builtin is confirmed and the helper resolves");
        assert_eq!(compiled.result_slot, 3);
        assert_eq!(compiled.buffer_slots, 4);
        assert!(compiled.tail_call.is_none());
        assert!(!compiled.code.is_empty());
    }

    #[test]
    fn recognizes_strlen_leaf_with_string_typed_parameter() {
        let function = single_arg_builtin_leaf_function(
            "strlen",
            Some(IrReturnType::String),
            Some(IrReturnType::Int),
        );
        let permits = NativeCallPermits {
            builtin_strlen: true,
            ..NativeCallPermits::default()
        };
        assert!(
            compile_scalar_int_function_with_permits_and_helpers(
                &function,
                &[],
                1,
                permits,
                strlen_helpers()
            )
            .is_some()
        );
    }

    #[test]
    fn rejects_strlen_leaf_with_int_typed_parameter() {
        // An int-typed parameter can never be a string at runtime, so it is
        // outside the strlen shape (the tag guard would always fail); the
        // interpreter runs it.
        let function = single_arg_builtin_leaf_function(
            "strlen",
            Some(IrReturnType::Int),
            Some(IrReturnType::Int),
        );
        let permits = NativeCallPermits {
            builtin_strlen: true,
            ..NativeCallPermits::default()
        };
        assert!(
            compile_scalar_int_function_with_permits_and_helpers(
                &function,
                &[],
                1,
                permits,
                strlen_helpers()
            )
            .is_none()
        );
    }

    #[test]
    fn rejects_a_namespaced_strlen_call_even_with_permit() {
        let function =
            single_arg_builtin_leaf_function("app\\strlen", None, Some(IrReturnType::Int));
        let permits = NativeCallPermits {
            builtin_strlen: true,
            ..NativeCallPermits::default()
        };
        assert!(
            compile_scalar_int_function_with_permits_and_helpers(
                &function,
                &[],
                1,
                permits,
                strlen_helpers()
            )
            .is_none()
        );
    }

    #[test]
    fn recognizes_is_type_leaves_only_with_matching_permit() {
        // Each canonical predicate is recognized only when its own permit is set,
        // and needs no runtime helper (the stencil only reads the tag).
        let cases = [
            (
                "is_int",
                NativeCallPermits {
                    builtin_is_int: true,
                    ..NativeCallPermits::default()
                },
            ),
            (
                "is_string",
                NativeCallPermits {
                    builtin_is_string: true,
                    ..NativeCallPermits::default()
                },
            ),
            (
                "is_array",
                NativeCallPermits {
                    builtin_is_array: true,
                    ..NativeCallPermits::default()
                },
            ),
            (
                "is_float",
                NativeCallPermits {
                    builtin_is_float: true,
                    ..NativeCallPermits::default()
                },
            ),
            (
                "is_bool",
                NativeCallPermits {
                    builtin_is_bool: true,
                    ..NativeCallPermits::default()
                },
            ),
        ];
        for (name, permits) in cases {
            let function = single_arg_builtin_leaf_function(name, None, Some(IrReturnType::Bool));

            // No permit -> the interpreter runs it.
            assert!(
                compile_scalar_int_function_with_permits_and_helpers(
                    &function,
                    &[],
                    1,
                    NativeCallPermits::default(),
                    CopyPatchRuntimeHelpers::default(),
                )
                .is_none(),
                "{name} without its permit falls back"
            );

            // Matching permit -> recognized; no helper address required.
            let compiled = compile_scalar_int_function_with_permits_and_helpers(
                &function,
                &[],
                1,
                permits,
                CopyPatchRuntimeHelpers::default(),
            )
            .unwrap_or_else(|| panic!("{name} recognized with its permit"));
            assert_eq!(compiled.result_slot, 3);
            assert_eq!(compiled.buffer_slots, 4);
            assert!(compiled.tail_call.is_none());
            assert!(!compiled.code.is_empty());
        }
    }

    #[test]
    fn is_type_permits_do_not_cross() {
        // `is_string` must not be enabled by the `is_int` permit.
        let function =
            single_arg_builtin_leaf_function("is_string", None, Some(IrReturnType::Bool));
        let permits = NativeCallPermits {
            builtin_is_int: true,
            ..NativeCallPermits::default()
        };
        assert!(
            compile_scalar_int_function_with_permits_and_helpers(
                &function,
                &[],
                1,
                permits,
                CopyPatchRuntimeHelpers::default()
            )
            .is_none()
        );
    }

    #[test]
    fn recognizes_is_type_leaf_with_typed_parameter() {
        // The tag check is correct for any declared parameter type, so a typed
        // parameter is still recognized (unlike strlen/count, no type restriction).
        let function = single_arg_builtin_leaf_function(
            "is_int",
            Some(IrReturnType::Mixed),
            Some(IrReturnType::Bool),
        );
        let permits = NativeCallPermits {
            builtin_is_int: true,
            ..NativeCallPermits::default()
        };
        assert!(
            compile_scalar_int_function_with_permits_and_helpers(
                &function,
                &[],
                1,
                permits,
                CopyPatchRuntimeHelpers::default()
            )
            .is_some()
        );
    }

    #[test]
    fn rejects_is_type_aliases_and_unhandled_predicates() {
        // Every type-predicate permit ON, to prove it is the NAME (not a missing
        // permit) that leaves these to the interpreter: aliases (is_integer/
        // is_long/is_double) and the non-tag predicates.
        let permits = NativeCallPermits {
            builtin_is_int: true,
            builtin_is_string: true,
            builtin_is_array: true,
            builtin_is_float: true,
            builtin_is_bool: true,
            ..NativeCallPermits::default()
        };
        for name in [
            "is_integer",
            "is_long",
            "is_double",
            "is_null",
            "is_object",
            "is_numeric",
            "is_scalar",
            "is_iterable",
            "is_callable",
            "is_a",
        ] {
            let function = single_arg_builtin_leaf_function(name, None, Some(IrReturnType::Bool));
            assert!(
                compile_scalar_int_function_with_permits_and_helpers(
                    &function,
                    &[],
                    1,
                    permits,
                    CopyPatchRuntimeHelpers::default()
                )
                .is_none(),
                "{name} is left to the interpreter"
            );
        }
    }

    #[test]
    fn rejects_a_namespaced_is_type_call() {
        let function =
            single_arg_builtin_leaf_function("app\\is_int", None, Some(IrReturnType::Bool));
        let permits = NativeCallPermits {
            builtin_is_int: true,
            ..NativeCallPermits::default()
        };
        assert!(
            compile_scalar_int_function_with_permits_and_helpers(
                &function,
                &[],
                1,
                permits,
                CopyPatchRuntimeHelpers::default()
            )
            .is_none()
        );
    }

    /// `function f($a, $b): int { return g($a + $b); }` — the native→userland
    /// tail-call shape: the prefix computes `$a + $b`, then a final
    /// `CallFunction` whose result register is returned unchanged.
    fn tailcall_leaf_function(call_name: &str) -> php_ir::IrFunction {
        let span = IrSpan::default();
        let ins = |kind| Instruction {
            id: InstrId::new(0),
            span,
            kind,
        };
        let positional = |reg: u32| IrCallArg {
            name: None,
            value: Operand::Register(RegId::new(reg)),
            unpack: false,
            value_kind: IrCallArgValueKind::Direct,
            by_ref_local: None,
            by_ref_dim: None,
            by_ref_property: None,
            by_ref_property_dim: None,
        };
        php_ir::IrFunction {
            name: "f".to_string(),
            params: vec![int_param("a", 0), int_param("b", 1)],
            locals: vec!["a".to_string(), "b".to_string()],
            local_count: 2,
            register_count: 4,
            blocks: vec![BasicBlock {
                id: BlockId::new(0),
                instructions: vec![
                    ins(InstructionKind::LoadLocal {
                        dst: RegId::new(1),
                        local: LocalId::new(0),
                    }),
                    ins(InstructionKind::LoadLocal {
                        dst: RegId::new(2),
                        local: LocalId::new(1),
                    }),
                    ins(InstructionKind::Binary {
                        dst: RegId::new(3),
                        op: BinaryOp::Add,
                        lhs: Operand::Register(RegId::new(1)),
                        rhs: Operand::Register(RegId::new(2)),
                    }),
                    ins(InstructionKind::CallFunction {
                        dst: RegId::new(0),
                        name: call_name.to_string(),
                        args: vec![positional(3)],
                    }),
                ],
                terminator: Some(Terminator {
                    span,
                    kind: TerminatorKind::Return {
                        value: Some(Operand::Register(RegId::new(0))),
                        by_ref_local: None,
                    },
                }),
            }],
            span,
            flags: FunctionFlags::default(),
            return_type: Some(IrReturnType::Int),
            returns_by_ref: false,
            captures: Vec::new(),
            attributes: Vec::new(),
        }
    }

    #[test]
    fn recognizes_userland_tailcall_shape_only_with_permit() {
        let function = tailcall_leaf_function("classify");
        let constants: [IrConstant; 0] = [];

        // Without the permit the tail-call shape is out of every subset, so the
        // interpreter runs it (no native lowering).
        assert!(compile_scalar_int_function(&function, &constants, 1).is_none());

        // With the permit, the recognizer lowers the argument prefix and records
        // the plan. The callee's identity is validated later, by the VM.
        let permits = NativeCallPermits {
            allow_userland_tailcall: true,
            ..NativeCallPermits::default()
        };
        let compiled = compile_scalar_int_function_with_permits(&function, &constants, 1, permits)
            .expect("tail-call leaf recognized with the permit");
        let plan = compiled
            .tail_call
            .as_ref()
            .expect("a tail-call region records its plan");
        assert_eq!(plan.callee_name, "classify");
        // Locals $a,$b = slots 0,1; regs r0..r3 = slots 2..5; the fresh argument
        // slot sits just above the registers at slot 6.
        assert_eq!(plan.arg_slots, vec![6]);
        assert_eq!(compiled.buffer_slots, 7);
        assert!(!compiled.code.is_empty());
    }

    #[test]
    fn rejects_non_tail_call_shape_even_with_permit() {
        // `return callee($x) + 1`: the returned value is the *add* result, not the
        // call's result, so it is not a tail call. With the tail-call permit set
        // (but no `abs` permit) it is rejected by every subset -> interpreter.
        let function = abs_plus_one_function("classify");
        let constants = [IrConstant::Int(1)];
        let permits = NativeCallPermits {
            allow_userland_tailcall: true,
            ..NativeCallPermits::default()
        };
        assert!(
            compile_scalar_int_function_with_permits(&function, &constants, 1, permits).is_none(),
            "post-call work is not a tail call"
        );
    }

    #[test]
    fn rejects_a_non_int_return_type() {
        let function = binary_leaf(BinaryOp::Add, IrReturnType::Float);
        assert!(build_scalar_int_region(&function, &[], 1).is_none());
    }

    #[test]
    fn rejects_an_out_of_subset_binary_op() {
        // Concatenation is a valid BinaryOp but outside the scalar-int subset.
        let function = binary_leaf(BinaryOp::Concat, IrReturnType::Int);
        assert!(build_scalar_int_region(&function, &[], 1).is_none());
    }

    /// Hand-build the IR the frontend lowers for
    /// `function sum_to(int $n): int { $s = 0; for ($i=0; $i<$n; $i++) { $s = $s + $i; } return $s; }`
    /// (locals: 0=$n, 1=$s, 2=$i), matching `php-vm dump-ir`.
    fn sum_to_loop_function() -> php_ir::IrFunction {
        let span = IrSpan::default();
        let ins = |kind| Instruction {
            id: InstrId::new(0),
            span,
            kind,
        };
        let load_local = |dst, local| {
            ins(InstructionKind::LoadLocal {
                dst: RegId::new(dst),
                local: LocalId::new(local),
            })
        };
        let load_const = |dst, constant| {
            ins(InstructionKind::LoadConst {
                dst: RegId::new(dst),
                constant: ConstId::new(constant),
            })
        };
        let store_local = |local, reg| {
            ins(InstructionKind::StoreLocal {
                local: LocalId::new(local),
                src: Operand::Register(RegId::new(reg)),
            })
        };
        let discard = |reg| {
            ins(InstructionKind::Discard {
                src: Operand::Register(RegId::new(reg)),
            })
        };
        let add = |dst, lhs, rhs| {
            ins(InstructionKind::Binary {
                dst: RegId::new(dst),
                op: BinaryOp::Add,
                lhs: Operand::Register(RegId::new(lhs)),
                rhs: Operand::Register(RegId::new(rhs)),
            })
        };
        let term = |kind| Some(Terminator { span, kind });
        let jump = |target| {
            term(TerminatorKind::Jump {
                target: BlockId::new(target),
            })
        };
        let block = |id, instructions, terminator| BasicBlock {
            id: BlockId::new(id),
            instructions,
            terminator,
        };

        php_ir::IrFunction {
            name: "sum_to".to_string(),
            params: vec![int_param("n", 0)],
            locals: vec!["n".to_string(), "s".to_string(), "i".to_string()],
            local_count: 3,
            register_count: 12,
            blocks: vec![
                block(
                    0,
                    vec![
                        load_const(0, 0),
                        store_local(1, 0),
                        discard(0),
                        load_const(1, 0),
                        store_local(2, 1),
                        discard(1),
                    ],
                    jump(1),
                ),
                block(
                    1,
                    vec![
                        load_local(2, 2),
                        load_local(3, 0),
                        ins(InstructionKind::Compare {
                            dst: RegId::new(4),
                            op: CompareOp::Less,
                            lhs: Operand::Register(RegId::new(2)),
                            rhs: Operand::Register(RegId::new(3)),
                        }),
                    ],
                    term(TerminatorKind::JumpIf {
                        condition: Operand::Register(RegId::new(4)),
                        if_true: BlockId::new(3),
                        if_false: BlockId::new(2),
                    }),
                ),
                block(
                    2,
                    vec![load_local(11, 1)],
                    term(TerminatorKind::Return {
                        value: Some(Operand::Register(RegId::new(11))),
                        by_ref_local: None,
                    }),
                ),
                block(
                    3,
                    vec![
                        load_local(5, 1),
                        load_local(6, 2),
                        add(7, 5, 6),
                        store_local(1, 7),
                        discard(7),
                    ],
                    jump(4),
                ),
                block(
                    4,
                    vec![
                        load_local(8, 2),
                        load_const(9, 1),
                        add(10, 8, 9),
                        store_local(2, 10),
                        discard(8),
                    ],
                    jump(1),
                ),
            ],
            span,
            flags: FunctionFlags::default(),
            return_type: Some(IrReturnType::Int),
            returns_by_ref: false,
            captures: Vec::new(),
            attributes: Vec::new(),
        }
    }

    #[test]
    fn recognizes_a_counted_for_loop() {
        let function = sum_to_loop_function();
        let constants = [IrConstant::Int(0), IrConstant::Int(1)];
        let compiled = compile_scalar_int_function(&function, &constants, 1)
            .expect("counted for-loop recognized and compiled");
        // Locals map to slots: $n=0 (limit), $s=1 (result), $i=2 (counter).
        assert_eq!(compiled.result_slot, 1);
        assert_eq!(compiled.buffer_slots, 3);
        assert!(!compiled.code.is_empty());
    }

    #[test]
    fn counted_loop_recognizer_rejects_a_non_one_step() {
        // Change the increment constant from 1 to 2: no longer a canonical `$i++`
        // loop, so the specialized counted-loop recognizer declines it...
        let function = sum_to_loop_function();
        let constants = [IrConstant::Int(0), IrConstant::Int(2)];
        assert!(compile_counted_loop_function(&function, &constants).is_none());
        // ...but the general CFG compiler still lowers it (it is a valid `while`).
        assert!(compile_scalar_int_function(&function, &constants, 1).is_some());
    }

    /// `function acc(int $n): int { $s = 0; for ($i=0; $i<$n; $i++) { $s = $s <op> C; } return $s; }`
    /// where the loop body applies a constant right operand (`BinaryConst`).
    /// The body's second load is `LoadConst` (const index 2) instead of a local.
    fn const_body_loop_function(op: BinaryOp) -> php_ir::IrFunction {
        let span = IrSpan::default();
        let ins = |kind| Instruction {
            id: InstrId::new(0),
            span,
            kind,
        };
        let load_local = |dst, local| {
            ins(InstructionKind::LoadLocal {
                dst: RegId::new(dst),
                local: LocalId::new(local),
            })
        };
        let load_const = |dst, constant| {
            ins(InstructionKind::LoadConst {
                dst: RegId::new(dst),
                constant: ConstId::new(constant),
            })
        };
        let store_local = |local, reg| {
            ins(InstructionKind::StoreLocal {
                local: LocalId::new(local),
                src: Operand::Register(RegId::new(reg)),
            })
        };
        let discard = |reg| {
            ins(InstructionKind::Discard {
                src: Operand::Register(RegId::new(reg)),
            })
        };
        let binary = |dst, op, lhs, rhs| {
            ins(InstructionKind::Binary {
                dst: RegId::new(dst),
                op,
                lhs: Operand::Register(RegId::new(lhs)),
                rhs: Operand::Register(RegId::new(rhs)),
            })
        };
        let add = |dst, lhs, rhs| binary(dst, BinaryOp::Add, lhs, rhs);
        let term = |kind| Some(Terminator { span, kind });
        let jump = |target| {
            term(TerminatorKind::Jump {
                target: BlockId::new(target),
            })
        };
        let block = |id, instructions, terminator| BasicBlock {
            id: BlockId::new(id),
            instructions,
            terminator,
        };

        php_ir::IrFunction {
            name: "acc".to_string(),
            params: vec![int_param("n", 0)],
            locals: vec!["n".to_string(), "s".to_string(), "i".to_string()],
            local_count: 3,
            register_count: 12,
            blocks: vec![
                block(
                    0,
                    vec![
                        load_const(0, 0),
                        store_local(1, 0),
                        discard(0),
                        load_const(1, 0),
                        store_local(2, 1),
                        discard(1),
                    ],
                    jump(1),
                ),
                block(
                    1,
                    vec![
                        load_local(2, 2),
                        load_local(3, 0),
                        ins(InstructionKind::Compare {
                            dst: RegId::new(4),
                            op: CompareOp::Less,
                            lhs: Operand::Register(RegId::new(2)),
                            rhs: Operand::Register(RegId::new(3)),
                        }),
                    ],
                    term(TerminatorKind::JumpIf {
                        condition: Operand::Register(RegId::new(4)),
                        if_true: BlockId::new(3),
                        if_false: BlockId::new(2),
                    }),
                ),
                block(
                    2,
                    vec![load_local(11, 1)],
                    term(TerminatorKind::Return {
                        value: Some(Operand::Register(RegId::new(11))),
                        by_ref_local: None,
                    }),
                ),
                block(
                    3,
                    vec![
                        load_local(5, 1),
                        load_const(6, 2),
                        binary(7, op, 5, 6),
                        store_local(1, 7),
                        discard(7),
                    ],
                    jump(4),
                ),
                block(
                    4,
                    vec![
                        load_local(8, 2),
                        load_const(9, 1),
                        add(10, 8, 9),
                        store_local(2, 10),
                        discard(8),
                    ],
                    jump(1),
                ),
            ],
            span,
            flags: FunctionFlags::default(),
            return_type: Some(IrReturnType::Int),
            returns_by_ref: false,
            captures: Vec::new(),
            attributes: Vec::new(),
        }
    }

    #[test]
    fn recognizes_a_loop_with_a_const_operand_body() {
        // for (...) { $s = $s + 5; } — a BinaryConst body statement.
        let function = const_body_loop_function(BinaryOp::Add);
        let constants = [IrConstant::Int(0), IrConstant::Int(1), IrConstant::Int(5)];
        let compiled = compile_scalar_int_function(&function, &constants, 1)
            .expect("const-operand loop body recognized and compiled");
        assert_eq!(compiled.result_slot, 1);
        assert_eq!(compiled.buffer_slots, 3);
        assert!(!compiled.code.is_empty());
    }

    #[test]
    fn recognizes_a_loop_with_a_bitwise_const_body() {
        // for (...) { $s = $s | 5; } — bitwise op with a constant right operand.
        let function = const_body_loop_function(BinaryOp::BitOr);
        let constants = [IrConstant::Int(0), IrConstant::Int(1), IrConstant::Int(5)];
        let compiled = compile_scalar_int_function(&function, &constants, 1)
            .expect("bitwise const-operand loop body recognized and compiled");
        assert_eq!(compiled.result_slot, 1);
        assert!(!compiled.code.is_empty());
    }

    #[test]
    fn recognizes_mod_and_shift_loop_bodies() {
        // Mod and both shifts are in the native integer subset with a guard.
        let constants = [IrConstant::Int(0), IrConstant::Int(1), IrConstant::Int(5)];
        for op in [BinaryOp::Mod, BinaryOp::ShiftLeft, BinaryOp::ShiftRight] {
            let function = const_body_loop_function(op);
            let compiled = compile_scalar_int_function(&function, &constants, 1)
                .unwrap_or_else(|| panic!("{op:?} const-operand loop body should compile"));
            assert_eq!(compiled.result_slot, 1);
            assert!(!compiled.code.is_empty());
        }
    }

    #[test]
    fn int_bin_op_still_rejects_out_of_subset_ops() {
        // Div (float-typed result) and Pow are not in the integer subset.
        assert_eq!(int_bin_op(BinaryOp::Div), None);
        assert_eq!(int_bin_op(BinaryOp::Pow), None);
        assert_eq!(int_bin_op(BinaryOp::Concat), None);
    }

    /// `function max2(int $a, int $b): int { if ($a > $b) { return $a; } return $b; }`
    /// (locals: 0=$a, 1=$b) — an if/else diamond the counted-loop and leaf
    /// recognizers reject, exercising the general CFG compiler.
    fn max2_function() -> php_ir::IrFunction {
        let span = IrSpan::default();
        let ins = |kind| Instruction {
            id: InstrId::new(0),
            span,
            kind,
        };
        let block = |id, instructions, terminator| BasicBlock {
            id: BlockId::new(id),
            instructions,
            terminator,
        };
        let ret = |reg| {
            Some(Terminator {
                span,
                kind: TerminatorKind::Return {
                    value: Some(Operand::Register(RegId::new(reg))),
                    by_ref_local: None,
                },
            })
        };

        php_ir::IrFunction {
            name: "max2".to_string(),
            params: vec![int_param("a", 0), int_param("b", 1)],
            locals: vec!["a".to_string(), "b".to_string()],
            local_count: 2,
            register_count: 6,
            blocks: vec![
                // entry: cmp = $a > $b; jump_if then else.
                block(
                    0,
                    vec![
                        ins(InstructionKind::LoadLocal {
                            dst: RegId::new(0),
                            local: LocalId::new(0),
                        }),
                        ins(InstructionKind::LoadLocal {
                            dst: RegId::new(1),
                            local: LocalId::new(1),
                        }),
                        ins(InstructionKind::Compare {
                            dst: RegId::new(2),
                            op: CompareOp::Greater,
                            lhs: Operand::Register(RegId::new(0)),
                            rhs: Operand::Register(RegId::new(1)),
                        }),
                    ],
                    Some(Terminator {
                        span,
                        kind: TerminatorKind::JumpIf {
                            condition: Operand::Register(RegId::new(2)),
                            if_true: BlockId::new(1),
                            if_false: BlockId::new(2),
                        },
                    }),
                ),
                // then: return $a.
                block(
                    1,
                    vec![ins(InstructionKind::LoadLocal {
                        dst: RegId::new(3),
                        local: LocalId::new(0),
                    })],
                    ret(3),
                ),
                // else: return $b.
                block(
                    2,
                    vec![ins(InstructionKind::LoadLocal {
                        dst: RegId::new(4),
                        local: LocalId::new(1),
                    })],
                    ret(4),
                ),
            ],
            span,
            flags: FunctionFlags::default(),
            return_type: Some(IrReturnType::Int),
            returns_by_ref: false,
            captures: Vec::new(),
            attributes: Vec::new(),
        }
    }

    #[test]
    fn general_cfg_compiler_recognizes_if_else_diamond() {
        let function = max2_function();
        let compiled = compile_scalar_int_function(&function, &[], 1)
            .expect("if/else diamond compiled by the general CFG compiler");
        // Slot layout: locals 0,1; registers 2..8; result slot at 2 + 6 = 8.
        assert_eq!(compiled.result_slot, 8);
        assert_eq!(compiled.buffer_slots, 9);
        assert!(!compiled.code.is_empty());
    }

    fn float_param(name: &str, local: u32) -> IrParam {
        IrParam {
            name: name.to_string(),
            local: LocalId::new(local),
            required: true,
            default: None,
            type_: Some(IrReturnType::Float),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        }
    }

    /// `function fma(float $a, float $b): float { return $a * $b + $a; }`
    fn float_leaf_function(op: BinaryOp, return_type: IrReturnType) -> php_ir::IrFunction {
        let span = IrSpan::default();
        let ins = |kind| Instruction {
            id: InstrId::new(0),
            span,
            kind,
        };
        php_ir::IrFunction {
            name: "f".to_string(),
            params: vec![float_param("a", 0), float_param("b", 1)],
            locals: vec!["a".to_string(), "b".to_string()],
            local_count: 2,
            register_count: 4,
            blocks: vec![BasicBlock {
                id: BlockId::new(0),
                instructions: vec![
                    ins(InstructionKind::LoadLocal {
                        dst: RegId::new(0),
                        local: LocalId::new(0),
                    }),
                    ins(InstructionKind::LoadLocal {
                        dst: RegId::new(1),
                        local: LocalId::new(1),
                    }),
                    ins(InstructionKind::Binary {
                        dst: RegId::new(2),
                        op,
                        lhs: Operand::Register(RegId::new(0)),
                        rhs: Operand::Register(RegId::new(1)),
                    }),
                ],
                terminator: Some(Terminator {
                    span,
                    kind: TerminatorKind::Return {
                        value: Some(Operand::Register(RegId::new(2))),
                        by_ref_local: None,
                    },
                }),
            }],
            span,
            flags: FunctionFlags::default(),
            return_type: Some(return_type),
            returns_by_ref: false,
            captures: Vec::new(),
            attributes: Vec::new(),
        }
    }

    #[test]
    fn recognizes_a_scalar_float_leaf() {
        // function f(float $a, float $b): float { return $a / $b; }
        let function = float_leaf_function(BinaryOp::Div, IrReturnType::Float);
        let compiled = compile_scalar_int_function(&function, &[], 1)
            .expect("scalar-float leaf recognized and compiled");
        // Locals 0,1; registers 0..4 map to slots 2..6; result slot at 2+4 = 6.
        assert_eq!(compiled.result_slot, 6);
        assert_eq!(compiled.buffer_slots, 7);
        assert!(!compiled.code.is_empty());
    }

    #[test]
    fn float_leaf_rejects_a_non_float_return() {
        // A float-param body returning int is not the float-leaf shape (and the
        // int leaf rejects float params), so no native lowering applies.
        let function = float_leaf_function(BinaryOp::Add, IrReturnType::Int);
        assert!(compile_scalar_int_function(&function, &[], 1).is_none());
    }

    #[test]
    fn float_bin_op_rejects_out_of_subset_ops() {
        // Modulo and concat are not float-native; only +,-,*,/ are.
        assert_eq!(super::float_bin_op(BinaryOp::Mod), None);
        assert_eq!(super::float_bin_op(BinaryOp::Concat), None);
        assert!(super::float_bin_op(BinaryOp::Div).is_some());
    }

    #[test]
    fn general_cfg_compiler_rejects_a_non_int_body_op() {
        // Swap the comparison for a string concat: outside the int/bool subset.
        let mut function = max2_function();
        function.blocks[0].instructions[2].kind = InstructionKind::Binary {
            dst: RegId::new(2),
            op: BinaryOp::Concat,
            lhs: Operand::Register(RegId::new(0)),
            rhs: Operand::Register(RegId::new(1)),
        };
        // The JumpIf now reads a non-bool from a rejected op — but the op itself
        // is out of subset, so the whole compile bails to the interpreter.
        assert!(compile_scalar_int_function(&function, &[], 1).is_none());
    }

    #[test]
    fn recognizes_int_comparison_returning_bool() {
        // function lt(int $a, int $b): bool { return $a < $b; }
        let span = IrSpan::default();
        let ins = |kind| Instruction {
            id: InstrId::new(0),
            span,
            kind,
        };
        let function = php_ir::IrFunction {
            name: "lt".to_string(),
            params: vec![int_param("a", 0), int_param("b", 1)],
            locals: vec!["a".to_string(), "b".to_string()],
            local_count: 2,
            register_count: 3,
            blocks: vec![BasicBlock {
                id: BlockId::new(0),
                instructions: vec![
                    ins(InstructionKind::LoadLocal {
                        dst: RegId::new(0),
                        local: LocalId::new(0),
                    }),
                    ins(InstructionKind::LoadLocal {
                        dst: RegId::new(1),
                        local: LocalId::new(1),
                    }),
                    ins(InstructionKind::Compare {
                        dst: RegId::new(2),
                        op: CompareOp::Less,
                        lhs: Operand::Register(RegId::new(0)),
                        rhs: Operand::Register(RegId::new(1)),
                    }),
                ],
                terminator: Some(Terminator {
                    span,
                    kind: TerminatorKind::Return {
                        value: Some(Operand::Register(RegId::new(2))),
                        by_ref_local: None,
                    },
                }),
            }],
            span,
            flags: FunctionFlags::default(),
            return_type: Some(IrReturnType::Bool),
            returns_by_ref: false,
            captures: Vec::new(),
            attributes: Vec::new(),
        };
        let compiled = compile_scalar_int_function(&function, &[], 1)
            .expect("int comparison returning bool recognized");
        // params 0,1 + the compare result temporary at slot 2.
        assert_eq!(compiled.buffer_slots, 3);
        assert!(!compiled.code.is_empty());
    }
}
