//! VM-side bridge for the copy-and-patch native tier (default-off, behind the
//! `jit-copy-patch` feature).
//!
//! It marshals a frame's locals into the flat `JitCValue` slot buffer a
//! [`CompiledScalarRegion`] expects, runs the emitted native code, and marshals
//! the result back to a VM [`Value`]. Non-scalar locals are marshaled as
//! `Uninitialized` so the region's `Int` guards take the interpreter side exit
//! rather than misreading a heap handle as an integer.
//!
//! This is the execution mechanism only. It is deliberately NOT yet triggered
//! from the interpreter's function-entry fork: doing that needs an IR /
//! dense-bytecode → `RegionGraph` builder (see
//! `docs/research/copy-and-patch-stencil-tier.md`), which is the next step. The
//! bridge is exercised by unit tests over a real [`LocalFile`] so the
//! marshal-in / marshal-out ABI is proven end-to-end, and it stays inert unless
//! both the `jit-copy-patch` feature and a caller opt in.

use std::sync::OnceLock;

use php_jit::copy_patch::CompiledScalarRegion;
use php_runtime::Value;

use crate::frame::LocalFile;

// The marshaling types, local addressing, and compiled-leaf cache are only
// reachable on the aarch64 path; the non-aarch64 fallback returns `None`.
#[cfg(all(unix, target_arch = "aarch64"))]
use php_ir::ids::LocalId;
#[cfg(all(unix, target_arch = "aarch64"))]
use php_ir::instruction::{IrCallArg, TerminatorKind};
#[cfg(all(unix, target_arch = "aarch64"))]
use php_ir::{
    FunctionId, InstrId, Instruction, InstructionKind, IrConstant, IrFunction, IrReturnType,
    IrSpan, Operand, RegId,
};
#[cfg(all(unix, target_arch = "aarch64"))]
use php_jit::{JitCValue, JitCValueTag};
#[cfg(all(unix, target_arch = "aarch64"))]
use std::cell::RefCell;
#[cfg(all(unix, target_arch = "aarch64"))]
use std::collections::{HashMap, HashSet};
#[cfg(all(unix, target_arch = "aarch64"))]
use std::rc::Rc;

#[cfg(all(unix, target_arch = "aarch64"))]
use crate::compiled_unit::CompiledUnit;

/// Marshal a VM `Value` into the flat-buffer `JitCValue` the native tier reads.
///
/// Only scalar ints, bools, and floats cross as themselves. Every other value
/// (strings, arrays, objects, references, null, uninitialized, …) becomes
/// `Uninitialized`, so the region's type guard takes the interpreter side exit
/// instead of misinterpreting a heap handle or wrong-typed scalar.
#[cfg(all(unix, target_arch = "aarch64"))]
fn marshal_local(value: &Value) -> JitCValue {
    match value {
        Value::Int(int) => JitCValue::int(*int),
        Value::Bool(boolean) => JitCValue::bool(*boolean),
        Value::Float(float) => JitCValue::float(float.to_f64()),
        _ => JitCValue::uninitialized(),
    }
}

/// Marshal a native result `JitCValue` back to a VM `Value`. Returns `None` for
/// any tag the scalar tier does not produce as a committed result.
#[cfg(all(unix, target_arch = "aarch64"))]
fn unmarshal_result(value: &JitCValue) -> Option<Value> {
    match value.tag {
        JitCValueTag::Int => Some(Value::Int(value.payload as i64)),
        JitCValueTag::Bool => Some(Value::Bool(value.payload != 0)),
        JitCValueTag::FloatBits => Some(Value::Float(php_runtime::FloatValue::from_f64(
            f64::from_bits(value.payload),
        ))),
        _ => None,
    }
}

/// Run a compiled scalar-int region over a frame's locals, behind the guards the
/// region emitted.
///
/// Returns `Some(result)` on the native success path, or `None` to fall back to
/// the interpreter — an unsupported host, a guard/overflow side exit, or an
/// unrepresentable result. Buffer slot `i` is marshaled from local `i`, matching
/// the region compiler's convention that a `Param`'s `VmSlotId` is the `LocalId`
/// index.
#[cfg(all(unix, target_arch = "aarch64"))]
pub fn run_scalar_int_region(compiled: &CompiledScalarRegion, locals: &LocalFile) -> Option<Value> {
    use php_jit::code_memory::CodeMemory;

    let mut buffer: Vec<JitCValue> = (0..compiled.buffer_slots)
        .map(|slot| {
            locals
                .get(LocalId::new(slot))
                .map_or_else(JitCValue::uninitialized, |value| marshal_local(&value))
        })
        .collect();

    let mem = CodeMemory::new(&compiled.code).ok()?;
    // SAFETY: `compiled.code` is machine code emitted by `php_jit::copy_patch`
    // as a valid `extern "C" fn(*mut JitCValue) -> i32`, finalized read-execute
    // by `CodeMemory`. `buffer` is a live, aligned, contiguous `[JitCValue]` of
    // `buffer_slots` entries that outlives the call, and the region only
    // addresses slots `< buffer_slots`.
    let run: extern "C" fn(*mut JitCValue) -> i32 = unsafe {
        core::mem::transmute::<*const u8, extern "C" fn(*mut JitCValue) -> i32>(mem.as_ptr())
    };
    if run(buffer.as_mut_ptr()) != 0 {
        return None; // guard/overflow side exit → interpreter fallback
    }
    unmarshal_result(buffer.get(compiled.result_slot as usize)?)
}

/// Hosts without a copy-and-patch emitter (non-aarch64 / non-unix) always fall
/// back to the interpreter.
#[cfg(not(all(unix, target_arch = "aarch64")))]
pub fn run_scalar_int_region(
    _compiled: &CompiledScalarRegion,
    _locals: &LocalFile,
) -> Option<Value> {
    None
}

/// Process-global enable for the copy-patch leaf tier, read once. Default off;
/// set the `PHRUST_JIT_COPY_PATCH` environment variable (to any value) to opt in.
/// Gated additionally by the `jit-copy-patch` cargo feature, so it is inert in a
/// default build.
#[must_use]
pub fn copy_patch_leaf_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("PHRUST_JIT_COPY_PATCH").is_some())
}

/// A recognized + compiled scalar-int leaf function, ready to invoke natively.
///
/// Holds the finalized executable mapping so a function is recognized and lowered
/// once, then reused across calls (the [`cached_leaf`] cache owns these).
#[cfg(all(unix, target_arch = "aarch64"))]
pub struct NativeLeaf {
    code: php_jit::code_memory::CodeMemory,
    result_slot: u32,
    buffer_slots: u32,
}

#[cfg(all(unix, target_arch = "aarch64"))]
impl NativeLeaf {
    /// Recognize and lower `function` to native code, or `None` if it is outside
    /// the scalar-int subset or the executable-memory finalize fails.
    ///
    /// Before recognition, a pre-inline pass (`inline_scalar_leaf_calls`) tries
    /// to splice the bodies of same-unit scalar-leaf callees into `function`. A
    /// caller that only delegates to recognized scalar leaves becomes a call-free
    /// leaf itself and compiles natively; if the pass finds nothing to inline (or
    /// a non-inlinable call remains) the original `function` is compiled unchanged
    /// — which the recognizer then rejects if a call is left, exactly as before.
    pub fn compile(
        unit: &CompiledUnit,
        function: &IrFunction,
        constants: &[IrConstant],
        region_id: u32,
    ) -> Option<Self> {
        let inlined = inline_scalar_leaf_calls(function, unit, constants);
        let target = inlined.as_ref().unwrap_or(function);
        let compiled =
            php_jit::copy_patch::compile_scalar_int_function(target, constants, region_id)?;
        let code = php_jit::code_memory::CodeMemory::new(&compiled.code).ok()?;
        Some(Self {
            code,
            result_slot: compiled.result_slot,
            buffer_slots: compiled.buffer_slots,
        })
    }

    /// Invoke over positional parameter values (parameter `i` supplies buffer
    /// slot `i`). Returns `None` on any guard/overflow side exit or an
    /// unrepresentable result, so the caller falls back to the interpreter.
    #[must_use]
    pub fn run(&self, params: &[Value]) -> Option<Value> {
        let mut buffer: Vec<JitCValue> = (0..self.buffer_slots)
            .map(|slot| {
                params
                    .get(slot as usize)
                    .map_or_else(JitCValue::uninitialized, marshal_local)
            })
            .collect();
        // SAFETY: `self.code` is machine code emitted by `php_jit::copy_patch`
        // as a valid `extern "C" fn(*mut JitCValue) -> i32`, finalized
        // read-execute by `CodeMemory`; `buffer` is a live, aligned, contiguous
        // `[JitCValue; buffer_slots]` that outlives the call.
        let run: extern "C" fn(*mut JitCValue) -> i32 = unsafe {
            core::mem::transmute::<*const u8, extern "C" fn(*mut JitCValue) -> i32>(
                self.code.as_ptr(),
            )
        };
        if run(buffer.as_mut_ptr()) != 0 {
            return None;
        }
        unmarshal_result(buffer.get(self.result_slot as usize)?)
    }
}

/// `(unit id, function id)` → compiled leaf, or `None` for a function proven
/// outside the subset (so it is not re-recognized on every call).
#[cfg(all(unix, target_arch = "aarch64"))]
type LeafCache = HashMap<(u32, u32), Option<Rc<NativeLeaf>>>;

#[cfg(all(unix, target_arch = "aarch64"))]
thread_local! {
    /// Native code depends only on the function's immutable IR, so no epoch
    /// invalidation is needed within a process.
    static LEAF_CACHE: RefCell<LeafCache> = RefCell::new(HashMap::new());
}

/// Look up — or recognize, compile, and cache — the native leaf for a function.
///
/// `unit` supplies the sibling functions the pre-inline pass may splice in; the
/// cache key stays `(unit id, function id)`. Compilation (including the inline
/// pass) never re-enters this cache, so holding the borrow across it is safe.
#[cfg(all(unix, target_arch = "aarch64"))]
pub fn cached_leaf(
    unit: &CompiledUnit,
    function_id: u32,
    function: &IrFunction,
    constants: &[IrConstant],
) -> Option<Rc<NativeLeaf>> {
    let unit_id = unit.unit().id.raw();
    LEAF_CACHE.with(|cache| {
        cache
            .borrow_mut()
            .entry((unit_id, function_id))
            .or_insert_with(|| {
                let leaf = NativeLeaf::compile(unit, function, constants, function_id).map(Rc::new);
                if std::env::var_os("PHRUST_JIT_COPY_PATCH_DEBUG").is_some() {
                    eprintln!(
                        "[copy-patch] fn={} (id={}) recognized={}",
                        function.name,
                        function_id,
                        leaf.is_some()
                    );
                }
                leaf
            })
            .clone()
    })
}

/// Recursion budget for the transitive inline pass. Bounds mutually-recursive
/// call chains (direct self-recursion is rejected outright); a chain deeper than
/// this is simply left un-inlined and runs in the interpreter.
#[cfg(all(unix, target_arch = "aarch64"))]
const MAX_INLINE_DEPTH: u32 = 8;

/// Outcome of inlining a function's `CallFunction`s.
#[cfg(all(unix, target_arch = "aarch64"))]
enum InlineOutcome {
    /// The function contains no `CallFunction`; compile it as-is.
    NoCalls,
    /// Every `CallFunction` was inlined away; here is the call-free rewrite.
    Inlined(Box<IrFunction>),
    /// A `CallFunction` is outside the inlinable shape; do not transform.
    Rejected,
}

/// Pre-inline pass over a copy-and-patch candidate: splice the bodies of
/// same-unit scalar-leaf callees into `function` so a caller that merely
/// delegates to recognized scalar leaves becomes a call-free leaf itself.
///
/// Returns `Some(rewrite)` only when `function` contained at least one
/// `CallFunction` and every one was inlined away. Returns `None` when there was
/// nothing to inline, or when any call is outside the supported shape — in which
/// case the caller compiles the original `function`, and the recognizer rejects
/// it (a residual call is not in the scalar subset), exactly as before this pass.
///
/// The transform is conservative: a call is inlined only if the callee, after
/// transitively inlining *its* calls, reduces to a single-block, call-free,
/// register-only scalar leaf (recognized by
/// [`compile_scalar_int_function`](php_jit::copy_patch::compile_scalar_int_function))
/// whose body reads only its by-value int/float parameters and returns one
/// register. Arguments must be plain positional register/constant values. Any
/// mismatch leaves the call in place (so it side-exits to the interpreter),
/// preserving observable behavior.
#[cfg(all(unix, target_arch = "aarch64"))]
fn inline_scalar_leaf_calls(
    function: &IrFunction,
    unit: &CompiledUnit,
    constants: &[IrConstant],
) -> Option<IrFunction> {
    let self_id = unit.lookup_function(&function.name);
    match inline_calls(function, unit, constants, self_id, 0) {
        InlineOutcome::Inlined(inlined) => Some(*inlined),
        InlineOutcome::NoCalls | InlineOutcome::Rejected => None,
    }
}

/// True when any block of `function` contains a `CallFunction`.
#[cfg(all(unix, target_arch = "aarch64"))]
fn function_has_calls(function: &IrFunction) -> bool {
    function.blocks.iter().any(|block| {
        block
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.kind, InstructionKind::CallFunction { .. }))
    })
}

/// Rewrite every `CallFunction` in `function` by splicing in its callee's
/// reduced body. `self_id` is the id of `function` itself (to reject direct
/// self-recursion); `depth` bounds the recursion.
#[cfg(all(unix, target_arch = "aarch64"))]
fn inline_calls(
    function: &IrFunction,
    unit: &CompiledUnit,
    constants: &[IrConstant],
    self_id: Option<FunctionId>,
    depth: u32,
) -> InlineOutcome {
    if !function_has_calls(function) {
        return InlineOutcome::NoCalls;
    }
    if depth >= MAX_INLINE_DEPTH {
        return InlineOutcome::Rejected;
    }

    let mut rewrite = function.clone();
    // Callee registers are renamed into disjoint ranges above the caller's own
    // registers; `next_reg` is the running high-water mark. Locals are never
    // extended because an inlinable callee reads only its parameters (bound by
    // register substitution), so it introduces no new local slots.
    let mut next_reg = function.register_count;

    for block in &mut rewrite.blocks {
        let mut rebuilt: Vec<Instruction> = Vec::with_capacity(block.instructions.len());
        for instruction in &block.instructions {
            match &instruction.kind {
                InstructionKind::CallFunction { dst, name, args } => {
                    let Some(spliced) = try_splice_call(
                        *dst,
                        name,
                        args,
                        unit,
                        constants,
                        self_id,
                        depth,
                        &mut next_reg,
                        instruction.span,
                    ) else {
                        return InlineOutcome::Rejected;
                    };
                    rebuilt.extend(spliced);
                }
                _ => rebuilt.push(instruction.clone()),
            }
        }
        block.instructions = rebuilt;
    }

    rewrite.register_count = next_reg;
    InlineOutcome::Inlined(Box::new(rewrite))
}

/// Reduce `callee` to the single-block, call-free, register-only scalar leaf the
/// inliner can splice, or `None` if it is outside that shape. The reduction
/// transitively inlines the callee's own calls first, so `poly -> scale -> fma`
/// collapses in one bottom-up walk.
#[cfg(all(unix, target_arch = "aarch64"))]
fn reduce_inlinable_callee(
    callee: &IrFunction,
    callee_id: FunctionId,
    unit: &CompiledUnit,
    constants: &[IrConstant],
    depth: u32,
) -> Option<IrFunction> {
    if depth >= MAX_INLINE_DEPTH {
        return None;
    }
    let flags = callee.flags;
    if flags.is_top_level || flags.is_closure || flags.is_method || flags.is_generator {
        return None;
    }
    if callee.returns_by_ref || !callee.captures.is_empty() {
        return None;
    }

    let reduced = match inline_calls(callee, unit, constants, Some(callee_id), depth + 1) {
        InlineOutcome::NoCalls => callee.clone(),
        InlineOutcome::Inlined(inlined) => *inlined,
        InlineOutcome::Rejected => return None,
    };

    // A splice-able leaf is exactly one block returning one register value.
    if reduced.blocks.len() != 1 {
        return None;
    }
    let block = reduced.blocks.first()?;
    if !matches!(
        block.terminator.as_ref()?.kind,
        TerminatorKind::Return {
            value: Some(Operand::Register(_)),
            by_ref_local: None,
        }
    ) {
        return None;
    }

    // Parameters must be plain by-value int/float scalars (matching the leaf
    // recognizer), so an argument value can be bound by register substitution.
    for param in &reduced.params {
        if param.by_ref || param.variadic || param.default.is_some() {
            return None;
        }
        if !matches!(param.type_, Some(IrReturnType::Int | IrReturnType::Float)) {
            return None;
        }
    }

    // Substitution is only sound when the body reads its parameters and never
    // writes a local: no `StoreLocal`, and every local read is a parameter. This
    // keeps the callee's whole state in renamed registers with no new slots.
    let params: HashSet<LocalId> = reduced.params.iter().map(|param| param.local).collect();
    for instruction in &block.instructions {
        let ok = match &instruction.kind {
            InstructionKind::LoadLocal { local, .. }
            | InstructionKind::LoadLocalQuiet { local, .. } => params.contains(local),
            InstructionKind::LoadConst { .. } => true,
            InstructionKind::Move { src, .. } => operand_is_value_or_param(src, &params),
            InstructionKind::Binary { lhs, rhs, .. }
            | InstructionKind::Compare { lhs, rhs, .. } => {
                operand_is_value_or_param(lhs, &params) && operand_is_value_or_param(rhs, &params)
            }
            InstructionKind::Discard { src } => operand_is_value_or_param(src, &params),
            _ => false,
        };
        if !ok {
            return None;
        }
    }

    // Final gate: the reduced body must be a recognized scalar leaf.
    php_jit::copy_patch::compile_scalar_int_function(&reduced, constants, callee_id.raw())?;
    Some(reduced)
}

/// An operand is safe to keep/rename when it is a register or constant, or a
/// local that is one of the callee's parameters (substituted for its argument).
#[cfg(all(unix, target_arch = "aarch64"))]
fn operand_is_value_or_param(operand: &Operand, params: &HashSet<LocalId>) -> bool {
    match operand {
        Operand::Register(_) | Operand::Constant(_) => true,
        Operand::Local(local) => params.contains(local),
    }
}

/// Try to inline one `CallFunction`. On success returns the instruction sequence
/// that replaces it (argument bindings, the renamed callee body, and a move of
/// the callee's result into the call's destination register) and advances
/// `next_reg` past the callee's renamed register range. Returns `None` for any
/// call outside the supported shape, so the caller is left un-inlined.
#[cfg(all(unix, target_arch = "aarch64"))]
#[allow(clippy::too_many_arguments)]
fn try_splice_call(
    call_dst: RegId,
    name: &str,
    args: &[IrCallArg],
    unit: &CompiledUnit,
    constants: &[IrConstant],
    self_id: Option<FunctionId>,
    depth: u32,
    next_reg: &mut u32,
    call_span: IrSpan,
) -> Option<Vec<Instruction>> {
    let callee_id = unit.lookup_function(name)?;
    if self_id == Some(callee_id) {
        return None; // no direct self-recursion
    }
    let callee = unit.unit().functions.get(callee_id.index())?;
    let reduced = reduce_inlinable_callee(callee, callee_id, unit, constants, depth)?;

    // Plain positional value arguments, one per parameter.
    if args.len() != reduced.params.len() {
        return None;
    }
    let mut param_args: Vec<(LocalId, Operand)> = Vec::with_capacity(reduced.params.len());
    for (param, arg) in reduced.params.iter().zip(args.iter()) {
        if arg.name.is_some() || arg.unpack {
            return None;
        }
        match arg.value {
            Operand::Register(_) | Operand::Constant(_) => {}
            Operand::Local(_) => return None,
        }
        param_args.push((param.local, arg.value));
    }

    let block = reduced.blocks.first()?;
    let TerminatorKind::Return {
        value: Some(Operand::Register(return_reg)),
        by_ref_local: None,
    } = block.terminator.as_ref()?.kind
    else {
        return None;
    };

    let base = *next_reg;
    let mut spliced: Vec<Instruction> = Vec::with_capacity(block.instructions.len() + 1);
    for instruction in &block.instructions {
        let kind = match &instruction.kind {
            InstructionKind::LoadLocal { dst, local }
            | InstructionKind::LoadLocalQuiet { dst, local } => {
                // A parameter read becomes a move of the bound argument value.
                let value = param_arg_value(&param_args, *local)?;
                InstructionKind::Move {
                    dst: rename_reg(base, *dst),
                    src: value,
                }
            }
            InstructionKind::LoadConst { dst, constant } => InstructionKind::LoadConst {
                dst: rename_reg(base, *dst),
                constant: *constant,
            },
            InstructionKind::Move { dst, src } => InstructionKind::Move {
                dst: rename_reg(base, *dst),
                src: rename_operand(*src, base, &param_args)?,
            },
            InstructionKind::Binary { dst, op, lhs, rhs } => InstructionKind::Binary {
                dst: rename_reg(base, *dst),
                op: *op,
                lhs: rename_operand(*lhs, base, &param_args)?,
                rhs: rename_operand(*rhs, base, &param_args)?,
            },
            InstructionKind::Compare { dst, op, lhs, rhs } => InstructionKind::Compare {
                dst: rename_reg(base, *dst),
                op: *op,
                lhs: rename_operand(*lhs, base, &param_args)?,
                rhs: rename_operand(*rhs, base, &param_args)?,
            },
            InstructionKind::Discard { src } => InstructionKind::Discard {
                src: rename_operand(*src, base, &param_args)?,
            },
            _ => return None,
        };
        spliced.push(Instruction {
            id: instruction.id,
            span: instruction.span,
            kind,
        });
    }

    // The callee's return value flows into the call's destination register.
    spliced.push(Instruction {
        id: InstrId::new(0),
        span: call_span,
        kind: InstructionKind::Move {
            dst: call_dst,
            src: Operand::Register(rename_reg(base, return_reg)),
        },
    });

    *next_reg = base.checked_add(reduced.register_count)?;

    if std::env::var_os("PHRUST_JIT_COPY_PATCH_DEBUG").is_some() {
        eprintln!("[copy-patch] fn={name} (inlined leaf callee) recognized=true");
    }

    Some(spliced)
}

/// Rename a callee register into the caller's disjoint high range.
#[cfg(all(unix, target_arch = "aarch64"))]
fn rename_reg(base: u32, reg: RegId) -> RegId {
    RegId::new(base + reg.raw())
}

/// The argument operand bound to a callee parameter local, if any.
#[cfg(all(unix, target_arch = "aarch64"))]
fn param_arg_value(param_args: &[(LocalId, Operand)], local: LocalId) -> Option<Operand> {
    param_args
        .iter()
        .find(|(param, _)| *param == local)
        .map(|(_, value)| *value)
}

/// Rename a callee operand into caller space: registers shift into the disjoint
/// range, constants pass through, and a parameter local becomes its bound
/// argument value. A non-parameter local is unreachable in a reduced leaf, so it
/// aborts the splice.
#[cfg(all(unix, target_arch = "aarch64"))]
fn rename_operand(
    operand: Operand,
    base: u32,
    param_args: &[(LocalId, Operand)],
) -> Option<Operand> {
    match operand {
        Operand::Register(reg) => Some(Operand::Register(rename_reg(base, reg))),
        Operand::Constant(constant) => Some(Operand::Constant(constant)),
        Operand::Local(local) => param_arg_value(param_args, local),
    }
}

#[cfg(test)]
#[cfg(all(unix, target_arch = "aarch64"))]
mod tests {
    use super::{LocalFile, LocalId, Value, run_scalar_int_region};
    use php_jit::copy_patch::compile_scalar_int_region;
    use php_jit::region_ir::{
        NodeId, RegionEffects, RegionGraph, RegionId, RegionNode, RegionNodeKind, RegionPlacement,
        RegionValueType, VmSlotId,
    };

    fn i64_node(graph: &mut RegionGraph, kind: RegionNodeKind, inputs: Vec<NodeId>) -> NodeId {
        graph.add_node(RegionNode::new(
            kind,
            inputs,
            None,
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        ))
    }

    /// Build `local0 + local1` and return its compiled form.
    fn add_two_locals_region() -> php_jit::copy_patch::CompiledScalarRegion {
        let mut graph = RegionGraph::new(RegionId::new(1), "bridge-add");
        let p0 = i64_node(
            &mut graph,
            RegionNodeKind::Param {
                slot: VmSlotId::new(0),
            },
            Vec::new(),
        );
        let p1 = i64_node(
            &mut graph,
            RegionNodeKind::Param {
                slot: VmSlotId::new(1),
            },
            Vec::new(),
        );
        let sum = i64_node(&mut graph, RegionNodeKind::Add, vec![p0, p1]);
        compile_scalar_int_region(&graph, sum).expect("region compiles")
    }

    #[test]
    fn runs_scalar_region_over_frame_locals() {
        let compiled = add_two_locals_region();
        let mut locals = LocalFile::new(compiled.buffer_slots);
        locals.set(LocalId::new(0), Value::Int(40)).unwrap();
        locals.set(LocalId::new(1), Value::Int(2)).unwrap();

        assert_eq!(
            run_scalar_int_region(&compiled, &locals),
            Some(Value::Int(42)),
            "native tier computes local0 + local1 over the marshaled buffer"
        );
    }

    #[test]
    fn overflow_falls_back_to_the_interpreter() {
        let compiled = add_two_locals_region();
        let mut locals = LocalFile::new(compiled.buffer_slots);
        locals.set(LocalId::new(0), Value::Int(i64::MAX)).unwrap();
        locals.set(LocalId::new(1), Value::Int(1)).unwrap();

        assert_eq!(
            run_scalar_int_region(&compiled, &locals),
            None,
            "the overflow side exit marshals no result and defers to the interpreter"
        );
    }

    #[test]
    fn non_int_local_falls_back_to_the_interpreter() {
        let compiled = add_two_locals_region();
        let mut locals = LocalFile::new(compiled.buffer_slots);
        locals.set(LocalId::new(0), Value::Int(1)).unwrap();
        locals
            .set(LocalId::new(1), Value::string("not an int"))
            .unwrap();

        assert_eq!(
            run_scalar_int_region(&compiled, &locals),
            None,
            "a non-Int local is marshaled as Uninitialized and trips the Int guard"
        );
    }
}
