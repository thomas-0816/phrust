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

use php_ir::instruction::TerminatorKind;
use php_ir::{
    BinaryOp, InstructionKind, IrConstant, IrFunction, IrReturnType, LocalId, Operand, RegId,
};

use crate::aarch64::{Aarch64Assembler, Cond, Label, Reg, X0, X3, X4, X5, X6};
use crate::abi::JitCValueTag;
use crate::region_ir::{
    NodeId, RegionBuilder, RegionConst, RegionGraph, RegionId, RegionNode, RegionNodeKind,
    RegionValueType, VmSlotId,
};

const INT_TAG: u16 = JitCValueTag::Int as u16;

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

/// The `tag` word load (`ldr_w` at `slot * 24`, encoding `imm12 = slot * 6`) is
/// the binding scaled-immediate constraint: `slot * 6 <= 4095`. The payload
/// double-word (`imm12 = slot * 3 + 1`) is looser, so this bound covers both.
const MAX_SLOT: u32 = 4095 / 6;

const fn tag_off(slot: u32) -> u32 {
    slot * STRIDE + TAG_OFF
}

const fn payload_off(slot: u32) -> u32 {
    slot * STRIDE + PAYLOAD_OFF
}

/// A binary PHP integer operation lowered with a type + overflow guard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntBinOp {
    /// `lhs + rhs`, side exit on signed overflow.
    Add,
    /// `lhs - rhs`, side exit on signed overflow.
    Sub,
    /// `lhs * rhs`, side exit on signed overflow.
    Mul,
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

/// Write `value` to `slot[dst]` tagged as `Int`.
fn emit_store_int(asm: &mut Aarch64Assembler, dst: u32, value: Reg) {
    asm.movz(X3, INT_TAG);
    asm.str_w(X3, X0, tag_off(dst));
    asm.str_x(value, X0, payload_off(dst));
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
        ScalarIntOp::Binary { dst, lhs, rhs, .. } => {
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
        ScalarIntOp::Binary { op, dst, lhs, rhs } => {
            emit_int_guard(asm, deopt, lhs);
            emit_int_guard(asm, deopt, rhs);
            asm.ldr_x(X4, X0, payload_off(lhs));
            asm.ldr_x(X5, X0, payload_off(rhs));
            match op {
                IntBinOp::Add => {
                    asm.adds(X6, X4, X5);
                    asm.b_cond(Cond::Overflow, deopt);
                }
                IntBinOp::Sub => {
                    asm.subs(X6, X4, X5);
                    asm.b_cond(Cond::Overflow, deopt);
                }
                IntBinOp::Mul => {
                    // Overflow when the product high bits differ from the
                    // sign extension of the low bits (see cmp_shifted_asr63).
                    asm.mul(X6, X4, X5);
                    asm.smulh(X3, X4, X5);
                    asm.cmp_shifted_asr63(X3, X6);
                    asm.b_cond(Cond::NotEqual, deopt);
                }
            }
            emit_store_int(asm, dst, X6);
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

/// A region lowered to the scalar-int subset: native code plus the slot-buffer
/// layout the VM must marshal against.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledScalarRegion {
    /// Emitted `extern "C" fn(slot_base: *mut JitCValue) -> i32`.
    pub code: Vec<u8>,
    /// Slot holding the region result after a successful (`0`) return.
    pub result_slot: u32,
    /// Number of `JitCValue` slots the caller's buffer must provide.
    pub buffer_slots: u32,
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

/// Recognize and lower a scalar-int leaf function to native code in one step.
///
/// Returns `None` when the function is outside the subset ([`build_scalar_int_region`])
/// or the region cannot be compiled ([`compile_scalar_int_region`]).
pub fn compile_scalar_int_function(
    function: &IrFunction,
    constants: &[IrConstant],
    region_id: u32,
) -> Option<CompiledScalarRegion> {
    let (graph, result) = build_scalar_int_region(function, constants, region_id)?;
    compile_scalar_int_region(&graph, result).ok()
}

#[cfg(test)]
mod tests {
    use super::{
        GuardedIntAddStep, MAX_SLOT, RegionCompileError, SlotSequenceError,
        build_scalar_int_region, compile_scalar_int_function, compile_scalar_int_region,
        emit_guarded_int_add_sequence,
    };
    use crate::region_ir::{
        NodeId, RegionConst, RegionEffects, RegionGraph, RegionId, RegionNode, RegionNodeKind,
        RegionPlacement, RegionValueType, VmSlotId,
    };
    use php_ir::instruction::TerminatorKind;
    use php_ir::{
        BasicBlock, BinaryOp, BlockId, FunctionFlags, InstrId, Instruction, InstructionKind,
        IrParam, IrReturnType, IrSpan, LocalId, Operand, RegId, Terminator,
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
}
