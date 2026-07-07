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
use php_ir::{IrConstant, IrFunction};
#[cfg(all(unix, target_arch = "aarch64"))]
use php_jit::{JitCValue, JitCValueTag};
#[cfg(all(unix, target_arch = "aarch64"))]
use std::cell::RefCell;
#[cfg(all(unix, target_arch = "aarch64"))]
use std::collections::HashMap;
#[cfg(all(unix, target_arch = "aarch64"))]
use std::rc::Rc;

/// Marshal a VM `Value` into the flat-buffer `JitCValue` the native tier reads.
///
/// Only scalar ints and bools cross as themselves. Every other value (strings,
/// arrays, objects, references, floats, null, uninitialized, …) becomes
/// `Uninitialized`, so the region's `Int` guard takes the interpreter side exit
/// instead of misinterpreting a heap handle or non-int scalar as an integer.
#[cfg(all(unix, target_arch = "aarch64"))]
fn marshal_local(value: &Value) -> JitCValue {
    match value {
        Value::Int(int) => JitCValue::int(*int),
        Value::Bool(boolean) => JitCValue::bool(*boolean),
        _ => JitCValue::uninitialized(),
    }
}

/// Marshal a native result `JitCValue` back to a VM `Value`. Returns `None` for
/// any tag the scalar-int tier does not produce as a committed result.
#[cfg(all(unix, target_arch = "aarch64"))]
fn unmarshal_result(value: &JitCValue) -> Option<Value> {
    match value.tag {
        JitCValueTag::Int => Some(Value::Int(value.payload as i64)),
        JitCValueTag::Bool => Some(Value::Bool(value.payload != 0)),
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
    pub fn compile(
        function: &IrFunction,
        constants: &[IrConstant],
        region_id: u32,
    ) -> Option<Self> {
        let compiled =
            php_jit::copy_patch::compile_scalar_int_function(function, constants, region_id)?;
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
#[cfg(all(unix, target_arch = "aarch64"))]
pub fn cached_leaf(
    unit_id: u32,
    function_id: u32,
    function: &IrFunction,
    constants: &[IrConstant],
) -> Option<Rc<NativeLeaf>> {
    LEAF_CACHE.with(|cache| {
        cache
            .borrow_mut()
            .entry((unit_id, function_id))
            .or_insert_with(|| {
                let leaf = NativeLeaf::compile(function, constants, function_id).map(Rc::new);
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
