//! VM-side bridge for the copy-and-patch native tier (default-off, behind the
//! `jit-copy-patch` feature).
//!
//! It marshals a frame's locals into the flat `JitCValue` slot buffer a
//! [`CompiledScalarRegion`](php_jit::copy_patch::CompiledScalarRegion) expects,
//! runs the emitted native code, and marshals
//! the result back to a VM [`Value`](php_runtime::Value). Non-scalar locals are
//! marshaled as `Uninitialized` so the region's `Int` guards take the
//! interpreter side exit rather than misreading a heap handle as an integer.
//!
//! This is the execution mechanism only. It is deliberately NOT yet triggered
//! from the interpreter's function-entry fork: doing that needs an IR /
//! dense-bytecode → `RegionGraph` builder (see
//! `docs/research/copy-and-patch-stencil-tier.md`), which is the next step. The
//! bridge is exercised by unit tests over a real [`LocalFile`](crate::frame::LocalFile)
//! so the marshal-in / marshal-out ABI is proven end-to-end, and it stays inert
//! unless both the `jit-copy-patch` feature and a caller opt in.

// This is php_vm's single sanctioned native-execution boundary: marshaling
// raw `Value`/metadata pointers across the JIT ABI and calling emitted machine
// code are irreducibly `unsafe`, each guarded by a local `// SAFETY:`
// contract. The `runtime-hardening-lints` gate denies `unsafe` across the
// interpreter core; scope the exemption to this one module so the invariant
// still holds for the rest of php_vm.
#![allow(unsafe_code)]

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
use php_ir::module::{ClassEntry, ClassPropertyEntry, IrUnit, normalize_class_name};
#[cfg(all(unix, target_arch = "aarch64"))]
use php_ir::{
    FunctionId, InstrId, Instruction, InstructionKind, IrConstant, IrFunction, IrReturnType,
    IrSpan, Operand, RegId,
};
#[cfg(all(unix, target_arch = "aarch64"))]
use php_jit::copy_patch::{CopyPatchRuntimeHelpers, TailCallPlan};
#[cfg(all(unix, target_arch = "aarch64"))]
use php_jit::{
    JIT_HELPER_STATUS_FALLBACK, JIT_HELPER_STATUS_OK, JIT_HELPER_STATUS_TAILCALL, JitCValue,
    JitCValueTag,
};
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
/// Scalar ints, bools, and floats cross as themselves. An `Array` crosses as a
/// read-only borrowed `OpaqueArray` handle, a `String` as a read-only borrowed
/// `OpaqueString` handle, and an `Object` as a read-only borrowed `OpaqueObject`
/// handle — in every case an opaque-tagged slot whose `payload` is
/// `value as *const Value`, a pointer the helpers read but never mutate, free, or
/// store. Every other value (references, null, uninitialized, resources,
/// callables, …) becomes `Uninitialized`, so a region expecting a scalar or a
/// different heap shape takes the interpreter side exit instead of
/// misinterpreting a handle.
///
/// The real (non-`Uninitialized`) tag is also exactly what the pure `is_*` type
/// predicates read: `is_int` = `Int`, `is_bool` = `Bool`, `is_float` =
/// `FloatBits`, `is_string` = `OpaqueString`, `is_array` = `OpaqueArray`; an
/// `Uninitialized`-marshaled argument is ambiguous, so those stencils side-exit.
/// An `OpaqueObject` argument is a *definite non-match* for all five of those
/// predicates — an object is none of int/bool/float/string/array — so they answer
/// a correct `false` natively (they only read the tag word and never dereference
/// the object payload); it likewise fails the `Int`/`OpaqueString`/`OpaqueArray`
/// tag guards of every arithmetic/`count`/`strlen` stencil, so those side-exit on
/// an object exactly as they did when objects marshaled as `Uninitialized`.
///
/// SAFETY / POINTER-LIFETIME CONTRACT: the returned `JitCValue` may embed a raw
/// pointer *into* `value`. The caller MUST keep the pointed-to `Value` alive and
/// unmoved for the entire duration of the native `run` call its buffer is passed
/// to, and the native code MUST NOT retain the pointer past that call. Both call
/// sites uphold this — [`run_scalar_int_region`] marshals pointers into an owned
/// backing `Vec<Option<Value>>` that outlives the call, and
/// [`NativeLeaf::run_outcome`] marshals pointers into the caller's `&[Value]`
/// params slice, which likewise outlives the call. The consumers of a borrowed
/// handle payload are the `count`/`strlen`/property-load stencils, whose helpers
/// ([`copy_patch_array_len_abi`] / [`copy_patch_strlen_abi`] /
/// [`copy_patch_property_load_abi`]) perform a synchronous read-only query — a
/// length, or one declared property-slot read — with no mutation, free, hook/
/// `__get` invocation, or VM re-entry (the `is_*` stencils never deref the payload
/// at all — they only read the tag word).
#[cfg(all(unix, target_arch = "aarch64"))]
fn marshal_local(value: &Value) -> JitCValue {
    match value {
        Value::Int(int) => JitCValue::int(*int),
        Value::Bool(boolean) => JitCValue::bool(*boolean),
        Value::Float(float) => JitCValue::float(float.to_f64()),
        // Read-only borrowed array handle: the payload is a `*const Value` valid
        // only for the synchronous native call (see the contract above).
        Value::Array(_) => JitCValue {
            tag: JitCValueTag::OpaqueArray,
            reserved: 0,
            payload: value as *const Value as u64,
            aux: 0,
        },
        // Read-only borrowed string handle, same lifetime contract as the array
        // handle above; the payload is read only for its byte length.
        Value::String(_) => JitCValue {
            tag: JitCValueTag::OpaqueString,
            reserved: 0,
            payload: value as *const Value as u64,
            aux: 0,
        },
        // Read-only borrowed object handle, same lifetime contract; the payload
        // is read only through the monomorphic property-load helper, which guards
        // the object's layout and reads one declared property slot.
        Value::Object(_) => JitCValue {
            tag: JitCValueTag::OpaqueObject,
            reserved: 0,
            payload: value as *const Value as u64,
            aux: 0,
        },
        _ => JitCValue::uninitialized(),
    }
}

/// C-ABI wrapper the `count` stencil `blr`s: read the borrowed `Value` at
/// `value_ptr` and write its packed-array length through `out`. Mirrors the
/// Cranelift `jit_array_len_abi` (identical `extern "C" fn(usize, *mut i64) ->
/// i32` shape) so both native tiers reuse the one runtime helper
/// `php_runtime::php_jit_array_len` rather than re-implementing array length.
///
/// Read-only and non-re-entrant: it only reads the borrowed value's length and
/// never mutates, frees, or re-enters the VM. It returns a non-OK status (so the
/// stencil side-exits to the interpreter) for a null pointer, a non-packed-int
/// array, or a length that does not fit an `i64`.
///
/// SAFETY: `value_ptr` is the `payload` of an `OpaqueArray` slot the bridge
/// marshaled as `&Value as *const Value` into the live params/backing buffer, so
/// it is a valid `Value` pointer for this synchronous call (see `marshal_local`).
/// `out` is the stencil's stack out-slot, non-null and valid for the call.
#[cfg(all(unix, target_arch = "aarch64"))]
extern "C" fn copy_patch_array_len_abi(value_ptr: usize, out: *mut i64) -> i32 {
    if value_ptr == 0 || out.is_null() {
        return php_runtime::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT;
    }
    // SAFETY: a live, borrowed `Value` valid for this call (see the doc above).
    let value = unsafe { &*(value_ptr as *const Value) };
    let mut length = 0_usize;
    if php_runtime::php_jit_array_len(value, &mut length) != php_runtime::PHP_JIT_ARRAY_STATUS_OK {
        return php_runtime::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT;
    }
    let Ok(length) = i64::try_from(length) else {
        return php_runtime::PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT;
    };
    // SAFETY: `out` is non-null and valid for this synchronous call (checked).
    unsafe {
        *out = length;
    }
    php_runtime::PHP_JIT_ARRAY_STATUS_OK
}

/// C-ABI wrapper the `strlen` stencil `blr`s: read the borrowed `Value` at
/// `value_ptr` and write its byte length through `out`. Mirrors
/// [`copy_patch_array_len_abi`] (identical `extern "C" fn(usize, *mut i64) ->
/// i32` shape) and the Cranelift `jit_strlen_known_abi`, so both native tiers
/// reuse the one string-length primitive (`PhpString::len`, exactly what the
/// `strlen` builtin returns) rather than re-implementing it. PHP `strlen` is a
/// *byte* count (not a multibyte length), which is precisely `PhpString::len`.
///
/// Read-only and non-re-entrant: it only reads the borrowed value's byte length
/// and never mutates, frees, or re-enters the VM. It returns a non-OK status (so
/// the stencil side-exits to the interpreter) for a null pointer, a non-string
/// value (the interpreter then applies `strlen`'s coercion/`TypeError`
/// semantics), or a length that does not fit an `i64`. The bridge only marshals a
/// genuine `Value::String` as `OpaqueString`, so the string-tag guard normally
/// ensures a string reaches here; the type check is defense in depth.
///
/// SAFETY: `value_ptr` is the `payload` of an `OpaqueString` slot the bridge
/// marshaled as `&Value as *const Value` into the live params/backing buffer, so
/// it is a valid `Value` pointer for this synchronous call (see `marshal_local`).
/// `out` is the stencil's stack out-slot, non-null and valid for the call.
#[cfg(all(unix, target_arch = "aarch64"))]
extern "C" fn copy_patch_strlen_abi(value_ptr: usize, out: *mut i64) -> i32 {
    if value_ptr == 0 || out.is_null() {
        return JIT_HELPER_STATUS_FALLBACK;
    }
    // SAFETY: a live, borrowed `Value` valid for this call (see the doc above).
    let value = unsafe { &*(value_ptr as *const Value) };
    let Value::String(string) = value else {
        return JIT_HELPER_STATUS_FALLBACK;
    };
    let Ok(length) = i64::try_from(string.len()) else {
        return JIT_HELPER_STATUS_FALLBACK;
    };
    // SAFETY: `out` is non-null and valid for this synchronous call (checked).
    unsafe {
        *out = length;
    }
    JIT_HELPER_STATUS_OK
}

/// C-ABI wrapper the property-load stencil `blr`s: read the borrowed object
/// `Value` at `value_ptr`, apply the monomorphic layout guard + declared-property
/// read described by the borrowed `JitPropertyLoadMetadata` at `metadata_ptr`,
/// and — only for a *scalar* result — marshal it into the out `JitCValue`.
///
/// The property-load itself is not reimplemented here: it delegates to
/// [`crate::vm::jit_property_load_fetch`], the exact fetch core the Cranelift
/// property-load helper (`jit_property_load_monomorphic_fast`) uses, which does
/// the class (layout) guard, the declared-property read, and the uninitialized
/// guard. This wrapper only adds the copy-patch result scoping.
///
/// Scalar-result scoping: the copy-patch result slot commits only `Int`, `Bool`,
/// and `Float` ([`unmarshal_result`]). So `OK` is returned only when the property
/// value is one of those scalars (marshaled into `*out`); a non-scalar value
/// (string/array/object/null/…), an uninitialized typed property, an absent
/// property, a class (layout) mismatch, or a non-object value all return
/// [`JIT_HELPER_STATUS_FALLBACK`] so the stencil side-exits and the interpreter
/// produces the exact value/error. A single fallback code suffices because the
/// stencil only distinguishes `OK` from non-`OK`.
///
/// Read-only and non-re-entrant: the fetch core only reads a declared property
/// slot and never mutates, frees, invokes a hook/`__get`, or re-enters the VM
/// (hooked/magic properties are excluded at recognition time, so they never reach
/// here — they side-exit as unrecognized and the interpreter runs them).
///
/// SAFETY: `value_ptr` is the `payload` of an `OpaqueObject` slot the bridge
/// marshaled as `&Value as *const Value` into the live params/backing buffer, so
/// it is a valid `Value` pointer for this synchronous call (see `marshal_local`).
/// `metadata_ptr` is the borrowed, VM-owned `JitPropertyLoadMetadata` the
/// [`NativeLeaf`] keeps alive for the whole life of the compiled leaf. `out` is
/// the stencil's stack out-slot, a non-null, valid `JitCValue` for the call.
#[cfg(all(unix, target_arch = "aarch64"))]
extern "C" fn copy_patch_property_load_abi(
    value_ptr: usize,
    metadata_ptr: usize,
    out: *mut JitCValue,
) -> i32 {
    if value_ptr == 0 || metadata_ptr == 0 || out.is_null() {
        return JIT_HELPER_STATUS_FALLBACK;
    }
    // SAFETY: a live, borrowed object `Value` valid for this call (see the doc).
    let value = unsafe { &*(value_ptr as *const Value) };
    // SAFETY: a live, borrowed metadata record valid for this call (see the doc).
    let metadata = unsafe { &*(metadata_ptr as *const php_jit::JitPropertyLoadMetadata) };
    let Ok(value) = crate::vm::jit_property_load_fetch(value, metadata) else {
        return JIT_HELPER_STATUS_FALLBACK;
    };
    // Scalar-result scoping: only Int/Bool/Float can be committed to the result
    // slot; every other property value side-exits to the interpreter.
    let marshaled = match value {
        Value::Int(int) => JitCValue::int(int),
        Value::Bool(boolean) => JitCValue::bool(boolean),
        Value::Float(float) => JitCValue::float(float.to_f64()),
        _ => return JIT_HELPER_STATUS_FALLBACK,
    };
    // SAFETY: `out` is non-null and a valid `JitCValue` for this synchronous call.
    unsafe {
        *out = marshaled;
    }
    JIT_HELPER_STATUS_OK
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

    // Own each marshaled local's `Value` in a backing store that outlives the
    // native call: `LocalFile::get` returns an owned clone, so marshaling an
    // `Array` handle as a pointer to a temporary would dangle. Pointers embedded
    // by `marshal_local` therefore point into `owned`, kept alive across `run`.
    let owned: Vec<Option<Value>> = (0..compiled.buffer_slots)
        .map(|slot| locals.get(LocalId::new(slot)))
        .collect();
    let mut buffer: Vec<JitCValue> = owned
        .iter()
        .map(|value| {
            value
                .as_ref()
                .map_or_else(JitCValue::uninitialized, marshal_local)
        })
        .collect();

    let mem = CodeMemory::new(&compiled.code).ok()?;
    // SAFETY: `compiled.code` is machine code emitted by `php_jit::copy_patch`
    // as a valid `extern "C" fn(*mut JitCValue) -> i32`, finalized read-execute
    // by `CodeMemory`. `buffer` is a live, aligned, contiguous `[JitCValue]` of
    // `buffer_slots` entries that outlives the call, and the region only
    // addresses slots `< buffer_slots`. Any borrowed array pointer in it points
    // into `owned`, which also outlives the call (dropped at scope end below).
    let run: extern "C" fn(*mut JitCValue) -> i32 = unsafe {
        core::mem::transmute::<*const u8, extern "C" fn(*mut JitCValue) -> i32>(mem.as_ptr())
    };
    let status = run(buffer.as_mut_ptr());
    // `owned` must stay live until the native call returns (it may hold arrays
    // the region read by pointer); reference it here to pin that lifetime.
    drop(owned);
    if status != 0 {
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

/// Process-global enable for the copy-patch leaf tier, read once. Default **on**
/// (the `jit-copy-patch` cargo feature is in the default feature set, and this
/// tier engages unless explicitly disabled). Set `PHRUST_JIT_COPY_PATCH` to a
/// falsey value (`0`, `off`, `false`, `no`, or empty) to disable it at runtime —
/// e.g. for the differential harness's off-vs-on comparison or to isolate the
/// interpreter on a workload. Any other value (or leaving it unset) keeps the
/// tier on. On a non-aarch64/non-unix host the tier is inert regardless.
#[must_use]
pub fn copy_patch_leaf_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| match std::env::var("PHRUST_JIT_COPY_PATCH") {
        Ok(value) => !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no" | ""
        ),
        // Unset (or non-UTF-8) → on by default.
        Err(_) => true,
    })
}

/// Native-call permissions for a compile, derived from the unit's function
/// registry — the VM owns function-name resolution, so it (not `php_jit`)
/// decides whether a call name is the real builtin.
///
/// The builtin permissions today are `abs`, `count`, `strlen`, and the canonical
/// type predicates `is_int`/`is_string`/`is_array`/`is_float`/`is_bool`. An
/// unqualified call to any of these is the real builtin unless the unit defines a
/// *global* function literally named that; PHP forbids redeclaring a builtin at
/// global scope, so a shadow is only reachable via a namespace, where the
/// call/registry name carries the namespace (`ns\abs`, `ns\count`, `ns\strlen`,
/// `ns\is_int`, …) and never matches the bare
/// name the native path lowers. Checking the registry for a bare-name user
/// function is therefore defense in depth, and mirrors the interpreter, which
/// also resolves an unqualified builtin name to the builtin ahead of user
/// functions. Any resolution doubt leaves the call unrecognized (interpreter
/// fallback).
#[cfg(all(unix, target_arch = "aarch64"))]
fn native_call_permits(unit: &CompiledUnit) -> php_jit::copy_patch::NativeCallPermits {
    php_jit::copy_patch::NativeCallPermits {
        builtin_abs: unit.lookup_function("abs").is_none(),
        // Permit lowering the native→userland tail-call *shape*. The recognizer
        // only produces a tail call for a `CallFunction`; the actual safety gate
        // (the callee is a plain userland function with a matching, by-value
        // signature) is enforced by the VM at call time, which interprets the
        // whole leaf when the callee is out of scope. So this is unconditionally
        // true — correctness does not depend on any registry check here.
        allow_userland_tailcall: true,
        builtin_count: unit.lookup_function("count").is_none(),
        builtin_strlen: unit.lookup_function("strlen").is_none(),
        builtin_is_int: unit.lookup_function("is_int").is_none(),
        builtin_is_string: unit.lookup_function("is_string").is_none(),
        builtin_is_array: unit.lookup_function("is_array").is_none(),
        builtin_is_float: unit.lookup_function("is_float").is_none(),
        builtin_is_bool: unit.lookup_function("is_bool").is_none(),
    }
}

/// A recognized monomorphic scalar property-load leaf: the object parameter's
/// slot, the result slot layout, and the layout-guard metadata the helper reads.
#[cfg(all(unix, target_arch = "aarch64"))]
struct PropertyLoadLeaf {
    /// VM slot (its `LocalId` index) the object parameter is marshaled into.
    object_slot: u32,
    /// Slot the marshaled scalar result is written to (above locals + registers).
    result_slot: u32,
    /// `JitCValue` slots the caller's buffer must provide.
    buffer_slots: u32,
    /// Layout-guard + storage metadata (receiver class, storage name), built from
    /// the unit's class table — the same shape the Cranelift path passes.
    metadata: php_jit::JitPropertyLoadMetadata,
}

/// Recognize `function f(SomeClass $o): T { return $o->prop; }` — a single-block
/// free-function leaf that loads a by-value object parameter and returns one of
/// its declared, plain (backed, public, instance, non-hooked) properties named by
/// a compile-time constant. Builds the [`JitPropertyLoadMetadata`] the layout
/// guard needs from the unit's class table (the VM owns class resolution; the
/// bare `php_jit` copy-patch layer cannot resolve classes).
///
/// Rejected (→ `None`, so the interpreter runs it, matching the Cranelift
/// property-load recognizer): methods/closures/generators (a leaf *method* whose
/// receiver is `$this` is a deliberate follow-up — this admits only a
/// free-function object parameter), a by-ref/variadic/defaulted parameter, a
/// parameter without a class type, a void/never return, a dynamic `->$var`
/// (lowered as `FetchDynamicProperty`, not `FetchProperty`), a null-safe `?->` or
/// chained `$a->b->c` (extra blocks/instructions), a static property, a property
/// with a get/set hook or whose class hierarchy has a public `__get`, and a
/// non-public/static property. The declared class or property being absent from
/// the unit also rejects.
#[cfg(all(unix, target_arch = "aarch64"))]
fn recognize_property_load_leaf(
    unit: &CompiledUnit,
    function: &IrFunction,
) -> Option<PropertyLoadLeaf> {
    let flags = function.flags;
    if flags.is_top_level || flags.is_closure || flags.is_method || flags.is_generator {
        return None;
    }
    if function.returns_by_ref || !function.captures.is_empty() {
        return None;
    }
    // A value return type: a property load produces a value.
    if matches!(
        function.return_type,
        None | Some(IrReturnType::Void | IrReturnType::Never)
    ) {
        return None;
    }
    // Exactly one by-value, non-variadic, no-default object parameter.
    let [param] = function.params.as_slice() else {
        return None;
    };
    if param.by_ref || param.variadic || param.default.is_some() {
        return None;
    }
    let Some(IrReturnType::Class {
        name: class_name, ..
    }) = param.type_.as_ref()
    else {
        return None;
    };
    let ir = unit.unit();
    let class = lookup_ir_class(ir, class_name)?;

    // Single-block leaf (ignoring `Discard` lifetime hints): load the object
    // parameter, fetch a static-named property of that loaded register, and
    // return exactly that value.
    let [block] = function.blocks.as_slice() else {
        return None;
    };
    let kinds: Vec<&InstructionKind> = block
        .instructions
        .iter()
        .map(|instruction| &instruction.kind)
        .filter(|kind| !matches!(kind, InstructionKind::Discard { .. }))
        .collect();
    let [
        InstructionKind::LoadLocal {
            dst: load_reg,
            local: load_local,
        },
        InstructionKind::FetchProperty {
            dst: fetch_dst,
            object: Operand::Register(object_reg),
            property,
        },
    ] = kinds.as_slice()
    else {
        return None;
    };
    if *load_local != param.local {
        return None;
    }
    if object_reg != load_reg {
        return None;
    }
    let TerminatorKind::Return {
        value: Some(Operand::Register(ret_reg)),
        by_ref_local: None,
    } = &block.terminator.as_ref()?.kind
    else {
        return None;
    };
    if ret_reg != fetch_dst {
        return None;
    }

    // Resolve the declared property and guard it is a plain (backed, public,
    // instance, non-hooked) property with no public `__get` anywhere in the
    // hierarchy — so the load never invokes user code.
    let (declaring_class, property_entry) = lookup_property_in_unit(ir, class, property)?;
    if property_entry.flags.is_static
        || property_entry.flags.is_private
        || property_entry.flags.is_protected
    {
        return None;
    }
    if property_entry.hooks.get.is_some() || property_entry.hooks.set.is_some() {
        return None;
    }
    if class_or_parent_has_public_magic_get(ir, class) {
        return None;
    }
    let property_slot_index = declaring_class
        .properties
        .iter()
        .position(|entry| entry.name == property_entry.name)?;

    // Slot layout mirrors the single-arg builtin leaves: locals occupy their
    // indices (so the object parameter is marshaled into `slot[param.local]`) and
    // the result lands in a dedicated slot above locals + registers. The compiler
    // rejects an out-of-range slot, so no bound check is needed here.
    let result_slot = function.local_count.checked_add(function.register_count)?;
    let buffer_slots = result_slot.checked_add(1)?;

    let metadata = php_jit::JitPropertyLoadMetadata {
        receiver_class: normalize_class_name(&class.name),
        class_id: class.id.raw(),
        property: property_entry.name.clone(),
        storage_name: property_storage_name(declaring_class, property_entry),
        property_slot_index,
        layout_version: 0,
    };
    Some(PropertyLoadLeaf {
        object_slot: param.local.raw(),
        result_slot,
        buffer_slots,
        metadata,
    })
}

/// Find `name`'s class entry in the unit by normalized name (mirrors the
/// Cranelift property-load recognizer's `lookup_class`).
#[cfg(all(unix, target_arch = "aarch64"))]
fn lookup_ir_class<'a>(unit: &'a IrUnit, name: &str) -> Option<&'a ClassEntry> {
    let normalized = normalize_class_name(name);
    unit.classes
        .iter()
        .find(|class| normalize_class_name(&class.name) == normalized)
}

/// Resolve `property` on `class`, walking parents (mirrors the Cranelift
/// recognizer's `lookup_property_in_unit`). Returns the declaring class and the
/// property entry.
#[cfg(all(unix, target_arch = "aarch64"))]
fn lookup_property_in_unit<'a>(
    unit: &'a IrUnit,
    class: &'a ClassEntry,
    property: &str,
) -> Option<(&'a ClassEntry, &'a ClassPropertyEntry)> {
    if let Some(entry) = class.properties.iter().find(|entry| entry.name == property) {
        return Some((class, entry));
    }
    let parent = class
        .parent
        .as_deref()
        .and_then(|parent| lookup_ir_class(unit, parent))?;
    lookup_property_in_unit(unit, parent, property)
}

/// True when `class` or an ancestor declares a public instance `__get`, which
/// would make a "missing" property read call user code (mirrors the Cranelift
/// recognizer's `class_or_parent_has_public_magic_get`).
#[cfg(all(unix, target_arch = "aarch64"))]
fn class_or_parent_has_public_magic_get(unit: &IrUnit, class: &ClassEntry) -> bool {
    if class.methods.iter().any(|method| {
        method.name.eq_ignore_ascii_case("__get")
            && !method.flags.is_static
            && !method.flags.is_private
            && !method.flags.is_protected
    }) {
        return true;
    }
    class
        .parent
        .as_deref()
        .and_then(|parent| lookup_ir_class(unit, parent))
        .is_some_and(|parent| class_or_parent_has_public_magic_get(unit, parent))
}

/// Runtime storage name for a declared property (mirrors the Cranelift
/// recognizer's `property_storage_name`), so the helper's `get_property` reads
/// the same slot the VM stores under. A private property is name-mangled; the
/// recognizer rejects non-public properties, so in practice this is the plain
/// name, but the full mapping is kept for faithful parity.
#[cfg(all(unix, target_arch = "aarch64"))]
fn property_storage_name(class: &ClassEntry, property: &ClassPropertyEntry) -> String {
    if property.flags.is_private {
        format!(
            "private:{}:{}",
            normalize_class_name(&class.name),
            property.name
        )
    } else {
        property.name.clone()
    }
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
    /// `Some` when the leaf is a native→userland tail call: running it leaves the
    /// arguments in the plan's buffer slots and returns
    /// [`JIT_HELPER_STATUS_TAILCALL`], and the VM performs the userland call.
    tail_call: Option<TailCallPlan>,
    /// `Some` for a monomorphic property-load leaf: the layout-guard metadata the
    /// stencil's helper reads. The emitted code embeds a borrowed pointer *into*
    /// this box, so the `NativeLeaf` must own it for the whole life of `code`
    /// (kept together in the `Rc` the leaf cache holds). Never dereferenced after
    /// `code` is dropped, since the two drop together. Read only through that
    /// baked pointer inside native code, so it is dead from Rust's view — the box
    /// exists purely to pin the pointee's address and lifetime.
    #[allow(dead_code)]
    property_metadata: Option<Box<php_jit::JitPropertyLoadMetadata>>,
}

/// The result of running a [`NativeLeaf`] over its arguments.
#[cfg(all(unix, target_arch = "aarch64"))]
pub enum LeafOutcome {
    /// The leaf computed a committed scalar result natively.
    Value(Value),
    /// The leaf is a tail call: it computed the positional `Int` arguments; the
    /// VM must perform the call to `callee_name` through the interpreter path.
    TailCall {
        callee_name: String,
        args: Vec<Value>,
    },
    /// A guard/overflow side exit or an unrepresentable value: interpret instead.
    Fallback,
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
        // Monomorphic scalar property-load leaf, recognized in the VM (which owns
        // class/property resolution) and lowered to a guarded helper-call
        // stencil. Tried first: its shape (a by-value object parameter returning
        // one of its declared properties) is disjoint from every scalar-int/
        // float/builtin subset, so it never steals another leaf, and on any
        // mismatch it falls through to the existing recognizers unchanged.
        if let Some(leaf) = recognize_property_load_leaf(unit, function) {
            // Box the metadata so its address is stable across the move into
            // `Self`; the stencil bakes in a borrowed pointer to it, and the
            // returned `NativeLeaf` owns the box, keeping the pointer valid for
            // every native invocation (and freed with `code` at drop).
            let metadata = Box::new(leaf.metadata);
            let metadata_ptr = metadata.as_ref() as *const php_jit::JitPropertyLoadMetadata as u64;
            let helper = copy_patch_property_load_abi as *const () as usize as u64;
            if let Some(compiled) = php_jit::copy_patch::compile_property_load_leaf(
                leaf.object_slot,
                leaf.result_slot,
                leaf.buffer_slots,
                metadata_ptr,
                helper,
            ) && let Ok(code) = php_jit::code_memory::CodeMemory::new(&compiled.code)
            {
                return Some(Self {
                    code,
                    result_slot: compiled.result_slot,
                    buffer_slots: compiled.buffer_slots,
                    tail_call: None,
                    property_metadata: Some(metadata),
                });
            }
            // Recognized but could not lower/finalize: fall through. The shape
            // matches no other recognizer, so `compile` returns `None` below.
        }

        let inlined = inline_scalar_leaf_calls(function, unit, constants);
        let target = inlined.as_ref().unwrap_or(function);
        let permits = native_call_permits(unit);
        // Runtime-owned helper addresses `php_jit` cannot name itself (mirrors the
        // Cranelift `JitRuntimeHelperAddresses` plumbing). `count` reads array
        // length through the wrapper over `php_runtime::php_jit_array_len`, and
        // `strlen` reads string byte length through its wrapper.
        let helpers = CopyPatchRuntimeHelpers {
            array_len: copy_patch_array_len_abi as *const () as usize as u64,
            strlen: copy_patch_strlen_abi as *const () as usize as u64,
        };
        let compiled = php_jit::copy_patch::compile_scalar_int_function_with_permits_and_helpers(
            target, constants, region_id, permits, helpers,
        )?;
        let code = php_jit::code_memory::CodeMemory::new(&compiled.code).ok()?;
        Some(Self {
            code,
            result_slot: compiled.result_slot,
            buffer_slots: compiled.buffer_slots,
            tail_call: compiled.tail_call,
            property_metadata: None,
        })
    }

    /// Invoke over positional parameter values (parameter `i` supplies buffer
    /// slot `i`), returning the full outcome.
    ///
    /// Builds and runs the flat buffer, then dispatches on the region's status:
    /// `0` (OK) unmarshals `result_slot` to a [`LeafOutcome::Value`] (or
    /// `Fallback` when the tag is unrepresentable); `2` — the tail-call status —
    /// reads each argument slot (which the region guaranteed is `Int`) into a
    /// [`LeafOutcome::TailCall`]; any other status (a guard/overflow side exit)
    /// is [`LeafOutcome::Fallback`]. Any defensive mismatch (missing plan, a
    /// non-`Int` argument slot) also falls back rather than misinterpreting.
    #[must_use]
    pub fn run_outcome(&self, params: &[Value]) -> LeafOutcome {
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
        let status = run(buffer.as_mut_ptr());
        if status == JIT_HELPER_STATUS_OK {
            return match buffer
                .get(self.result_slot as usize)
                .and_then(unmarshal_result)
            {
                Some(value) => LeafOutcome::Value(value),
                None => LeafOutcome::Fallback,
            };
        }
        if status == JIT_HELPER_STATUS_TAILCALL
            && let Some(plan) = self.tail_call.as_ref()
        {
            let mut args = Vec::with_capacity(plan.arg_slots.len());
            for &slot in &plan.arg_slots {
                // The region emits an `Int` guard before storing each argument,
                // so the slot is `Int` here; treat anything else as a fallback
                // rather than misreading a payload.
                match buffer.get(slot as usize) {
                    Some(value) if value.tag == JitCValueTag::Int => {
                        args.push(Value::Int(value.payload as i64));
                    }
                    _ => return LeafOutcome::Fallback,
                }
            }
            return LeafOutcome::TailCall {
                callee_name: plan.callee_name.clone(),
                args,
            };
        }
        LeafOutcome::Fallback
    }

    /// Invoke over positional parameter values, returning a committed scalar
    /// result only. A thin wrapper over [`Self::run_outcome`]: a tail call or any
    /// side exit yields `None` so the caller falls back to the interpreter.
    #[must_use]
    pub fn run(&self, params: &[Value]) -> Option<Value> {
        match self.run_outcome(params) {
            LeafOutcome::Value(value) => Some(value),
            LeafOutcome::TailCall { .. } | LeafOutcome::Fallback => None,
        }
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

    /// Build `function f($a) { return count($a); }` as the frontend lowers it and
    /// compile it with the real `copy_patch_array_len_abi` helper wired in.
    fn count_leaf_region() -> php_jit::copy_patch::CompiledScalarRegion {
        use php_ir::instruction::{IrCallArg, IrCallArgValueKind, TerminatorKind};
        use php_ir::{
            BasicBlock, BlockId, FunctionFlags, InstrId, Instruction, InstructionKind, IrParam,
            IrSpan, Operand, RegId,
        };

        let span = IrSpan::default();
        let function = php_ir::IrFunction {
            name: "f".to_string(),
            params: vec![IrParam {
                name: "a".to_string(),
                local: LocalId::new(0),
                required: true,
                default: None,
                type_: None,
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
                    Instruction {
                        id: InstrId::new(0),
                        span,
                        kind: InstructionKind::LoadLocal {
                            dst: RegId::new(1),
                            local: LocalId::new(0),
                        },
                    },
                    Instruction {
                        id: InstrId::new(1),
                        span,
                        kind: InstructionKind::CallFunction {
                            dst: RegId::new(0),
                            name: "count".to_string(),
                            args: vec![IrCallArg {
                                name: None,
                                value: Operand::Register(RegId::new(1)),
                                unpack: false,
                                value_kind: IrCallArgValueKind::Direct,
                                by_ref_local: Some(LocalId::new(0)),
                                by_ref_dim: None,
                                by_ref_property: None,
                                by_ref_property_dim: None,
                            }],
                        },
                    },
                ],
                terminator: Some(php_ir::Terminator {
                    span,
                    kind: TerminatorKind::Return {
                        value: Some(Operand::Register(RegId::new(0))),
                        by_ref_local: None,
                    },
                }),
            }],
            span,
            flags: FunctionFlags::default(),
            return_type: None,
            returns_by_ref: false,
            captures: Vec::new(),
            attributes: Vec::new(),
        };
        let permits = php_jit::copy_patch::NativeCallPermits {
            builtin_count: true,
            ..php_jit::copy_patch::NativeCallPermits::default()
        };
        let helpers = php_jit::copy_patch::CopyPatchRuntimeHelpers {
            array_len: super::copy_patch_array_len_abi as *const () as usize as u64,
            strlen: super::copy_patch_strlen_abi as *const () as usize as u64,
        };
        php_jit::copy_patch::compile_scalar_int_function_with_permits_and_helpers(
            &function,
            &[],
            1,
            permits,
            helpers,
        )
        .expect("count leaf compiles with the array-len helper wired in")
    }

    #[test]
    fn count_stencil_runs_over_a_real_packed_array_handle() {
        // A packed all-int array marshals as an OpaqueArray handle; the stencil
        // guards the tag, calls the real `php_jit_array_len` wrapper over the
        // borrowed pointer, and returns the length natively.
        let compiled = count_leaf_region();
        let mut locals = LocalFile::new(compiled.buffer_slots);
        locals
            .set(
                LocalId::new(0),
                Value::packed_array(vec![Value::Int(10), Value::Int(20), Value::Int(30)]),
            )
            .unwrap();
        assert_eq!(
            run_scalar_int_region(&compiled, &locals),
            Some(Value::Int(3)),
            "count over a real packed-int array handle runs natively"
        );
    }

    #[test]
    fn count_stencil_side_exits_on_a_non_packed_array() {
        // A packed array with a non-int element passes the array-tag guard but the
        // helper reports fallback (not a packed-int layout), so the stencil side-
        // exits and the interpreter computes the length instead.
        let compiled = count_leaf_region();
        let mut locals = LocalFile::new(compiled.buffer_slots);
        locals
            .set(
                LocalId::new(0),
                Value::packed_array(vec![Value::Int(1), Value::string("two")]),
            )
            .unwrap();
        assert_eq!(
            run_scalar_int_region(&compiled, &locals),
            None,
            "a non-packed-int array side-exits after the helper reports fallback"
        );
    }

    #[test]
    fn count_stencil_side_exits_on_a_non_array() {
        // A scalar argument marshals as Int; the array-tag guard fails before the
        // call, so the helper is never reached (the count() TypeError case).
        let compiled = count_leaf_region();
        let mut locals = LocalFile::new(compiled.buffer_slots);
        locals.set(LocalId::new(0), Value::Int(5)).unwrap();
        assert_eq!(
            run_scalar_int_region(&compiled, &locals),
            None,
            "a non-array argument side-exits at the tag guard"
        );
    }

    /// Build `function f($a): $ret { return $name($a); }` (the single-argument
    /// builtin-leaf shape) and compile it with `permits` and the real
    /// `copy_patch_array_len_abi`/`copy_patch_strlen_abi` helpers wired in.
    fn single_arg_builtin_leaf_region(
        call_name: &str,
        param_type: Option<php_ir::IrReturnType>,
        return_type: Option<php_ir::IrReturnType>,
        permits: php_jit::copy_patch::NativeCallPermits,
    ) -> php_jit::copy_patch::CompiledScalarRegion {
        use php_ir::instruction::{IrCallArg, IrCallArgValueKind, TerminatorKind};
        use php_ir::{
            BasicBlock, BlockId, FunctionFlags, InstrId, Instruction, InstructionKind, IrParam,
            IrSpan, Operand, RegId,
        };

        let span = IrSpan::default();
        let function = php_ir::IrFunction {
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
                    Instruction {
                        id: InstrId::new(0),
                        span,
                        kind: InstructionKind::LoadLocal {
                            dst: RegId::new(1),
                            local: LocalId::new(0),
                        },
                    },
                    Instruction {
                        id: InstrId::new(1),
                        span,
                        kind: InstructionKind::CallFunction {
                            dst: RegId::new(0),
                            name: call_name.to_string(),
                            args: vec![IrCallArg {
                                name: None,
                                value: Operand::Register(RegId::new(1)),
                                unpack: false,
                                value_kind: IrCallArgValueKind::Direct,
                                by_ref_local: Some(LocalId::new(0)),
                                by_ref_dim: None,
                                by_ref_property: None,
                                by_ref_property_dim: None,
                            }],
                        },
                    },
                ],
                terminator: Some(php_ir::Terminator {
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
        };
        let helpers = php_jit::copy_patch::CopyPatchRuntimeHelpers {
            array_len: super::copy_patch_array_len_abi as *const () as usize as u64,
            strlen: super::copy_patch_strlen_abi as *const () as usize as u64,
        };
        php_jit::copy_patch::compile_scalar_int_function_with_permits_and_helpers(
            &function,
            &[],
            1,
            permits,
            helpers,
        )
        .expect("single-arg builtin leaf compiles with the helpers wired in")
    }

    #[test]
    fn strlen_stencil_runs_over_real_string_handles() {
        let permits = php_jit::copy_patch::NativeCallPermits {
            builtin_strlen: true,
            ..php_jit::copy_patch::NativeCallPermits::default()
        };
        let compiled = single_arg_builtin_leaf_region(
            "strlen",
            None,
            Some(php_ir::IrReturnType::Int),
            permits,
        );

        // ASCII, empty, embedded-NUL, and multibyte-UTF-8 strings all measure
        // their *byte* length natively (PHP strlen is a byte count): "héllo" is 6
        // bytes (é is two UTF-8 bytes), not 5 characters.
        for (bytes, expected) in [
            (b"hello".to_vec(), 5_i64),
            (Vec::new(), 0),
            (b"a\0b".to_vec(), 3),
            ("héllo".as_bytes().to_vec(), 6),
        ] {
            let mut locals = LocalFile::new(compiled.buffer_slots);
            locals
                .set(LocalId::new(0), Value::string(bytes.clone()))
                .unwrap();
            assert_eq!(
                run_scalar_int_region(&compiled, &locals),
                Some(Value::Int(expected)),
                "strlen byte length runs natively for {bytes:?}"
            );
        }
    }

    #[test]
    fn strlen_stencil_side_exits_on_a_non_string() {
        let permits = php_jit::copy_patch::NativeCallPermits {
            builtin_strlen: true,
            ..php_jit::copy_patch::NativeCallPermits::default()
        };
        let compiled = single_arg_builtin_leaf_region(
            "strlen",
            None,
            Some(php_ir::IrReturnType::Int),
            permits,
        );
        // A non-string local marshals as Int (or Uninitialized), tripping the
        // string-tag guard, so the interpreter applies strlen's coercion/TypeError
        // semantics instead.
        let mut locals = LocalFile::new(compiled.buffer_slots);
        locals.set(LocalId::new(0), Value::Int(123)).unwrap();
        assert_eq!(
            run_scalar_int_region(&compiled, &locals),
            None,
            "a non-string argument side-exits at the tag guard"
        );
    }

    /// True when the canonical predicate `name` holds for `value` (the answer the
    /// native tag check must reproduce for every definite-tag value).
    fn predicate_holds(name: &str, value: &Value) -> bool {
        match name {
            "is_int" => matches!(value, Value::Int(_)),
            "is_string" => matches!(value, Value::String(_)),
            "is_array" => matches!(value, Value::Array(_)),
            "is_float" => matches!(value, Value::Float(_)),
            "is_bool" => matches!(value, Value::Bool(_)),
            _ => unreachable!("unhandled predicate {name}"),
        }
    }

    #[test]
    fn is_type_stencils_answer_true_false_from_the_marshaled_tag() {
        use php_jit::copy_patch::NativeCallPermits;

        // Each predicate with only its own permit set.
        let predicates: [(&str, NativeCallPermits); 5] = [
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
        // One value per definite category — every marshaled tag the stencil can
        // observe (int, string, array, float, bool).
        let definite = || {
            vec![
                Value::Int(7),
                Value::string("hi"),
                Value::packed_array(vec![Value::Int(1), Value::Int(2)]),
                Value::Float(php_runtime::FloatValue::from_f64(1.5)),
                Value::Bool(true),
            ]
        };

        for (name, permits) in predicates {
            let compiled = single_arg_builtin_leaf_region(
                name,
                None,
                Some(php_ir::IrReturnType::Bool),
                permits,
            );

            for value in definite() {
                let expected = predicate_holds(name, &value);
                let mut locals = LocalFile::new(compiled.buffer_slots);
                locals.set(LocalId::new(0), value.clone()).unwrap();
                assert_eq!(
                    run_scalar_int_region(&compiled, &locals),
                    Some(Value::Bool(expected)),
                    "{name}({value:?}) must equal {expected} natively"
                );
            }

            // An ambiguous argument (null, marshaled as Uninitialized) side-exits
            // so the interpreter answers — it could be null/object/etc.
            let mut locals = LocalFile::new(compiled.buffer_slots);
            locals.set(LocalId::new(0), Value::Null).unwrap();
            assert_eq!(
                run_scalar_int_region(&compiled, &locals),
                None,
                "{name}(null) side-exits (Uninitialized is ambiguous)"
            );
        }
    }
}
