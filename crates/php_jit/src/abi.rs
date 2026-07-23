//! Safe native compiler/runtime ABI boundary types.
//!
//! These types are intentionally handle-based. They do not expose raw pointers,
//! Rust references, frame internals, GC cells, refcount state, or COW storage to
//! future native code.

use std::cell::Cell;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use php_ir::{FunctionId, LocalId, RegId};

/// Version for the C-compatible runtime ABI records.
pub const JIT_RUNTIME_ABI_VERSION: u32 = 78;

/// Stable ABI fingerprint for Cranelift ABI.
///
/// This is updated only when a `repr(C)` boundary type changes layout or tag
/// meaning. It is intentionally independent from Rust type names.
pub const JIT_RUNTIME_ABI_HASH: u64 = 0x0dc1_a843_0000_006e;

/// No stable length is published for this runtime value slot.
pub const JIT_NATIVE_VALUE_VIEW_NONE: u32 = 0;
/// Stable byte-length descriptor for one request-owned PHP string.
pub const JIT_NATIVE_VALUE_VIEW_STRING: u32 = 1;
/// Layout/meaning version for the string payload in [`JitNativeValueSlot`].
pub const JIT_NATIVE_STRING_VIEW_ABI_VERSION: u32 = 2;
/// The published string payload contains the PHP-false string `"0"`.
pub const JIT_NATIVE_STRING_VALUE_ZERO: u32 = 1;
/// Stable element-count descriptor for one request-owned PHP array.
pub const JIT_NATIVE_VALUE_VIEW_ARRAY: u32 = 2;
/// Stable scalar-only descriptor for one request-owned PHP reference cell.
pub const JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR: u32 = 3;
/// Stable by-value array-iteration snapshot owned by the request arena.
pub const JIT_NATIVE_VALUE_VIEW_FOREACH_DIRECT: u32 = 4;
/// Compact mutable array allocated and owned entirely by the native request arena.
pub const JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY: u32 = 5;
/// Request-owned by-value foreach cursor over a direct native array.
pub const JIT_NATIVE_VALUE_VIEW_DIRECT_FOREACH: u32 = 7;
/// Direct-mapped cache of plain declared properties for one request object.
// Value-view kind 8 was the deleted copied declared-property cache.
/// Direct request slot owning one intrusive `PhpArray` COW storage reference.
pub const JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY: u32 = 9;
/// Non-escaping array view borrowed from a PHP reference cell.
pub const JIT_NATIVE_VALUE_VIEW_BORROWED_REFERENCE_ARRAY: u32 = 10;
/// Authoritative immediate scalar reference owned entirely by the direct slot.
pub const JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR: u32 = 11;
/// Authoritative IEEE-754 payload owned entirely by the direct slot.
pub const JIT_NATIVE_VALUE_VIEW_FLOAT: u32 = 12;
/// Request-owned object identity backed by the stable slot-parallel owner
/// arena published in the native runtime view.
pub const JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT: u32 = 13;
/// Request-owned callable record with authoritative encoded object/capture owners.
pub const JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE: u32 = 14;
/// Layout/meaning version for a prepared callable direct value slot.
pub const JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION: u32 = 1;
/// Request-owned Fiber lifecycle with encoded callable and return owners.
pub const JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER: u32 = 15;
/// Layout/meaning version for a direct Fiber value slot.
pub const JIT_NATIVE_DIRECT_FIBER_ABI_VERSION: u32 = 1;
/// Fiber that crossed an explicit cold boundary and is now backed by a
/// cached runtime `FiberRef`; optimizing handlers do not admit this kind.
pub const JIT_NATIVE_VALUE_VIEW_MATERIALIZED_FIBER: u32 = 16;
pub const JIT_NATIVE_SHARED_ARRAY_ABI_VERSION: u32 = 1;
pub const JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION: u32 = 1;
pub const JIT_NATIVE_TRUSTED_PROPERTY_SLOT_EMPTY: u32 = 0;
pub const JIT_NATIVE_TRUSTED_PROPERTY_SLOT_PUBLISHED: u32 = 1;
pub const JIT_NATIVE_TRUSTED_PROPERTY_SLOT_WRITABLE: u32 = 2;
/// Exact declared slot admitted for reference identity publication. Typed,
/// readonly, hook, magic, dynamic, and inaccessible properties never publish
/// this state and retain their single baseline continuation.
pub const JIT_NATIVE_TRUSTED_PROPERTY_SLOT_REFERENCEABLE: u32 = 3;
/// Exact declared array-valued slot admitted for direct dimension mutation.
/// Publication follows readonly/type/hook/access checks in the baseline
/// continuation; generated code consumes only the numeric slot and COW plan.
pub const JIT_NATIVE_TRUSTED_PROPERTY_SLOT_DIMENSION_WRITABLE: u32 = 4;
/// Layout/meaning version for the array payload in [`JitNativeValueSlot`].
pub const JIT_NATIVE_ARRAY_VIEW_ABI_VERSION: u32 = 3;
/// Layout/meaning version for [`JitNativeReferenceScalarView`].
pub const JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION: u32 = 1;
/// Layout/meaning version for [`JitNativeForeachView`].
pub const JIT_NATIVE_FOREACH_VIEW_ABI_VERSION: u32 = 1;
pub const JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION: u32 = 1;
pub const JIT_NATIVE_DIRECT_ARRAY_FLAGS_VERSION_MASK: u32 = 0xff;
pub const JIT_NATIVE_DIRECT_ARRAY_CURSOR_SHIFT: u32 = 8;
pub const JIT_NATIVE_DIRECT_ARRAY_CURSOR_NONE: u32 = 0x00ff_ffff;

#[must_use]
pub const fn jit_native_direct_array_flags(cursor: Option<u32>) -> u32 {
    JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION
        | (match cursor {
            Some(cursor) => cursor,
            None => JIT_NATIVE_DIRECT_ARRAY_CURSOR_NONE,
        } << JIT_NATIVE_DIRECT_ARRAY_CURSOR_SHIFT)
}

#[must_use]
pub const fn jit_native_direct_array_cursor(flags: u32) -> Option<u32> {
    let cursor = flags >> JIT_NATIVE_DIRECT_ARRAY_CURSOR_SHIFT;
    if cursor == JIT_NATIVE_DIRECT_ARRAY_CURSOR_NONE {
        None
    } else {
        Some(cursor)
    }
}
/// Encoded runtime indexes at or above this value address the direct native slot arena.
pub const JIT_NATIVE_DIRECT_VALUE_INDEX_BASE: u32 = 0x4000_0000;
// The direct plane is the canonical request value store, not a small optimizer
// cache.  Keep enough stable address space for a complete large application
// request. Measured production requests cross the former reservation after
// the direct plane becomes canonical, before request-end recycling can run.
// StableNativeArena reserves this address range demand-backed, while ordinary
// last-owner releases still return slots to the in-request free list.
pub const JIT_NATIVE_DIRECT_VALUE_CAPACITY: usize = 2_097_152;
pub const JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY: usize = 4_194_304;
pub const JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY: u32 = 8;
pub const JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS: usize = 32;
pub const JIT_NATIVE_DIRECT_ARRAY_FREE_NONE: u32 = u32::MAX;
// Large framework requests can transiently cross the former 32 MiB string
// reservation before their ordinary last-owner releases recycle spans.
pub const JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY: usize = 64 * 1024 * 1024;
pub const JIT_NATIVE_DIRECT_STRING_FREE_BUCKETS: usize = 32;
pub const JIT_NATIVE_DIRECT_STRING_MIN_CAPACITY: u32 = 4;
pub const JIT_NATIVE_DIRECT_STRING_CAPACITY_SHIFT: u32 = 1;

#[must_use]
pub const fn jit_native_direct_string_reserved(capacity: u32, zero: bool) -> u32 {
    (capacity << JIT_NATIVE_DIRECT_STRING_CAPACITY_SHIFT)
        | (zero as u32 * JIT_NATIVE_STRING_VALUE_ZERO)
}

#[must_use]
pub const fn jit_native_direct_string_capacity(reserved: u32) -> u32 {
    reserved >> JIT_NATIVE_DIRECT_STRING_CAPACITY_SHIFT
}
/// The reference view has no cached immediate value.
pub const JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY: u32 = 0;
/// The reference view contains a cached immediate value.
pub const JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED: u32 = 1;
pub const JIT_NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_INT: u32 = 2;
pub const JIT_NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_NULL: u32 = 3;
pub const JIT_NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_FALSE: u32 = 4;
pub const JIT_NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_TRUE: u32 = 5;
pub const JIT_NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_UNINITIALIZED: u32 = 6;
pub const JIT_NATIVE_REFERENCE_ARRAY_VIEW_ABI_VERSION: u32 = 2;
pub const JIT_NATIVE_REFERENCE_ARRAY_VIEW_EMPTY: u32 = 0;
pub const JIT_NATIVE_REFERENCE_ARRAY_VIEW_PUBLISHED: u32 = 1;
pub const JIT_NATIVE_REFERENCE_ARRAY_KEY_INT: u32 = 1;
pub const JIT_NATIVE_REFERENCE_ARRAY_KEY_STRING: u32 = 2;
pub const JIT_NATIVE_REFERENCE_ARRAY_VALUE_UNSUPPORTED: u32 = 0;
pub const JIT_NATIVE_REFERENCE_ARRAY_VALUE_NULL: u32 = 1;
pub const JIT_NATIVE_REFERENCE_ARRAY_VALUE_UNINITIALIZED: u32 = 2;
pub const JIT_NATIVE_REFERENCE_ARRAY_VALUE_FALSE: u32 = 3;
pub const JIT_NATIVE_REFERENCE_ARRAY_VALUE_TRUE: u32 = 4;
pub const JIT_NATIVE_REFERENCE_ARRAY_VALUE_INT: u32 = 5;
pub const JIT_NATIVE_REFERENCE_ARRAY_VALUE_STRING: u32 = 6;

/// Stable request-owned value slot inspected by generated code.
///
/// The PHP value itself remains opaque to Cranelift.  Refcount, type view and
/// the two type-specific payload words deliberately live in one cache-local
/// record so hot native operations do not chase three parallel Rust vectors.
/// For arrays `payload` is the current length and `aux` points at the complete
/// insertion-ordered native entry array. For strings `payload` is the byte
/// length and `aux` points at immutable bytes. Reference
/// and foreach views place their descriptor address in `payload`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeValueSlot {
    pub refcount: u32,
    pub kind: u32,
    pub flags: u32,
    pub reserved: u32,
    pub payload: u64,
    pub aux: u64,
}

// SAFETY: this repr(C) record is plain integers and its derived Default is the
// all-zero representation supplied by the demand-backed native arena.
unsafe impl php_runtime::api::NativeZeroed for JitNativeValueSlot {}

/// One mutable key/value cell in a direct native array.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeDirectArrayEntry {
    pub key: i64,
    pub value: i64,
}

// SAFETY: both i64 fields admit the all-zero representation.
unsafe impl php_runtime::api::NativeZeroed for JitNativeDirectArrayEntry {}

/// Slot-parallel PHP auto-index state for an authoritative direct array.
///
/// This cannot be reconstructed from the live keys: `unset` preserves the
/// next auto-index while `array_pop` may move it backwards. Generated code
/// indexes this stable arena with the direct value-slot index.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeDirectArrayState {
    pub next_append_key: i64,
    pub has_next_append_key: u32,
    pub reserved: u32,
}

// SAFETY: the sole i64 field admits the all-zero representation.
unsafe impl php_runtime::api::NativeZeroed for JitNativeDirectArrayState {}

/// One exact `(function, continuation)` declared-property plan. Publication
/// performs name/visibility/layout resolution once; generated code consumes
/// only the numeric slot and guarded layout identity.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeTrustedPropertySlot {
    pub state: u32,
    pub slot_index: u32,
    pub layout_id: u64,
}

/// One request-owned exact function-static reference. The slot owns one
/// encoded direct-reference handle for the active unit.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeTrustedStaticLocalSlot {
    pub encoded: i64,
    pub state: u32,
    pub reserved: u32,
}

pub const JIT_NATIVE_TRUSTED_STATIC_LOCAL_EMPTY: u32 = 0;
pub const JIT_NATIVE_TRUSTED_STATIC_LOCAL_PUBLISHED: u32 = 1;

/// One request-owned exact `global $name` reference. Publication resolves the
/// name once; generated code consumes only this dense continuation slot.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeTrustedGlobalReferenceSlot {
    pub encoded: i64,
    pub reference_identity: u64,
    pub state: u32,
    pub reserved: u32,
    pub reserved_wide: u64,
}

pub const JIT_NATIVE_TRUSTED_GLOBAL_REFERENCE_EMPTY: u32 = 0;
pub const JIT_NATIVE_TRUSTED_GLOBAL_REFERENCE_PUBLISHED: u32 = 1;

/// One immutable exact-class result in a prepared static `instanceof` table.
/// Layout id zero is the empty-bucket sentinel; object layout ids start at one.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeInstanceOfEntry {
    pub layout_id: u64,
    pub result: u32,
    pub reserved: u32,
}

/// One exact `(function, continuation)` static `instanceof` plan. The entry
/// range is an open-addressed table whose capacity is `mask + 1`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeInstanceOfPlan {
    pub entry_offset: u32,
    pub mask: u32,
    pub state: u32,
    pub reserved: u32,
}

pub const JIT_NATIVE_INSTANCEOF_PLAN_PUBLISHED: u32 = 1;

/// Hashes a published object layout into one power-of-two `instanceof` table.
#[must_use]
pub const fn jit_native_instanceof_index(layout_id: u64, mask: u32) -> u32 {
    let mut mixed = layout_id;
    mixed ^= mixed >> 30;
    mixed = mixed.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed ^= mixed >> 27;
    mixed = mixed.wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^= mixed >> 31;
    (mixed as u32) & mask
}

/// One request-owned authoritative static-property cell. The encoded value
/// owns exactly one native handle; generated code retains a second owner when
/// it publishes a fetched value into SSA or a frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeStaticPropertySlot {
    pub value: i64,
    pub initialized: u32,
    pub reserved: u32,
}

/// One authoritative request/include-scope local. The encoded value is a
/// direct native reference handle, so generated code preserves alias identity
/// while the cold `ReferenceCell` remains only the include/export sidecar.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeRequestLocalSlot {
    pub encoded: i64,
    pub state: u32,
    pub reserved: u32,
}

pub const JIT_NATIVE_REQUEST_LOCAL_EMPTY: u32 = 0;
pub const JIT_NATIVE_REQUEST_LOCAL_PUBLISHED: u32 = 1;

// SAFETY: this repr(C) record is plain integers and its all-zero state is an
// unpublished cell.
unsafe impl php_runtime::api::NativeZeroed for JitNativeStaticPropertySlot {}

/// One class layout prepared at request publication for exact object
/// allocation. `prepared` is an opaque request-owned pointer consumed only by
/// the matching runtime allocator; generated code merely indexes and forwards
/// it, so class lookup and validation never occur per allocation.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativePreparedClassPlan {
    pub prepared: u64,
    /// Immutable display-name bytes owned by the prepared request class.
    pub display_name_bytes: u64,
    pub display_name_length: u64,
    pub state: u32,
    pub reserved: u32,
}

pub const JIT_NATIVE_PREPARED_CLASS_EMPTY: u32 = 0;
pub const JIT_NATIVE_PREPARED_CLASS_ALLOCATABLE: u32 = 1;

/// Exact `(function, continuation)` static-property plan. Class/name,
/// inheritance, visibility, and type admission are resolved before native
/// execution; the optimizing artifact consumes only the numeric storage slot.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeTrustedStaticPropertySlot {
    pub state: u32,
    pub slot_index: u32,
}

pub const JIT_NATIVE_TRUSTED_STATIC_PROPERTY_EMPTY: u32 = 0;
pub const JIT_NATIVE_TRUSTED_STATIC_PROPERTY_READABLE: u32 = 1;
pub const JIT_NATIVE_TRUSTED_STATIC_PROPERTY_WRITABLE: u32 = 3;
pub const JIT_NATIVE_STATIC_PROPERTY_CAPACITY: usize = 65_536;

/// Immutable, publication-owned view of one source-unit constant.
///
/// Generated code uses string views for literal array keys. The pointed-to
/// bytes remain owned by the immutable `CompiledUnit`; no process address is
/// persisted in the relocatable artifact itself.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeConstantView {
    pub kind: u32,
    pub reserved: u32,
    pub length: u64,
    pub bytes: u64,
}

pub const JIT_NATIVE_CONSTANT_VIEW_NONE: u32 = 0;
pub const JIT_NATIVE_CONSTANT_VIEW_STRING: u32 = 1;
pub const JIT_NATIVE_CONSTANT_VIEW_NULL: u32 = 2;
pub const JIT_NATIVE_CONSTANT_VIEW_BOOL: u32 = 3;
pub const JIT_NATIVE_CONSTANT_VIEW_INT: u32 = 4;
pub const JIT_NATIVE_CONSTANT_VIEW_FLOAT: u32 = 5;

/// One exact `(function, continuation)` global-constant cache entry. The
/// baseline continuation resolves the name once and publishes one owned
/// encoded value; subsequent optimizing executions retain it directly.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeTrustedConstantSlot {
    pub value: i64,
    pub state: u32,
    pub reserved: u32,
}

pub const JIT_NATIVE_TRUSTED_CONSTANT_EMPTY: u32 = 0;
pub const JIT_NATIVE_TRUSTED_CONSTANT_PUBLISHED: u32 = 1;

/// One immutable, already-encoded PHP key/value pair. The owning iterator
/// retains both handles until cleanup; generated code retains only values it
/// publishes into the current native frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeForeachEntry {
    pub key: i64,
    pub value: i64,
}

/// Request-owned cursor over an immutable by-value array snapshot. This
/// descriptor is deliberately independent from `PhpArray` and Rust iterator
/// layouts so generated code can advance it directly.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeForeachView {
    pub cursor: u64,
    pub length: u64,
    pub entries: u64,
}

/// Layout contract for the scalar-only reference view owned by `php_runtime`.
/// Generated code reaches this record through [`JitNativeValueSlot::payload`]
/// only after validating both ABI versions and the reference descriptor kind.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeReferenceScalarView {
    pub abi_version: u32,
    pub state: u32,
    pub encoded: i64,
}

/// Layout contract for one key in a reference-owned array `isset` view.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeReferenceArrayEntry {
    pub kind: u32,
    pub non_null: u32,
    pub integer: i64,
    pub string_length: u64,
    pub string_bytes: u64,
    pub value_kind: u32,
    pub value_flags: u32,
    pub value_payload: i64,
    pub value_length: u64,
    pub value_bytes: u64,
}

/// Layout contract for the reference-owned array view published through
/// [`JitNativeValueSlot::aux`].
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeReferenceArrayView {
    pub abi_version: u32,
    pub state: u32,
    pub length: u64,
    pub entries: u64,
    pub storage_refcount: u64,
    pub dirty: u32,
    pub reserved: u32,
}

/// Versioned request-owned view used by generated code to reach the compact
/// native value plane. The slots are an ABI array, not a Rust container
/// layout, and remain stable for the activation lifetime.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeRuntimeView {
    pub abi_version: u32,
    pub value_slot_capacity: u32,
    pub value_slots: u64,
    pub direct_value_slots: u64,
    pub direct_value_next: u64,
    pub direct_value_free_head: u64,
    /// Diagnostics-only cumulative bytes served from the direct value free
    /// list. Generated allocation updates this counter only on a reuse hit.
    pub direct_value_reused_bytes: u64,
    /// Slot-parallel stable pointers to the Rust object owner. Object values
    /// remain authoritative in the direct slot plane; this table supplies
    /// only the immovable backing identity needed by exact object operations.
    pub direct_object_owners: u64,
    /// Slot-parallel exact PHP auto-index state for direct arrays.
    pub direct_array_states: u64,
    pub direct_array_entries: u64,
    pub direct_array_next: u64,
    /// Exact-power-of-two free lists. A released range stores the preceding
    /// head index in its first entry, so both Rust and generated code can
    /// recycle growth storage without a helper boundary.
    pub direct_array_free_heads: u64,
    /// Diagnostics-only cumulative entry bytes served from array free lists.
    pub direct_array_reused_bytes: u64,
    pub direct_string_bytes: u64,
    pub direct_string_next: u64,
    /// Exact-power-of-two byte-range free lists. Released ranges store the
    /// preceding head offset in their first four bytes.
    pub direct_string_free_heads: u64,
    /// Diagnostics-only cumulative byte capacity served from string free lists.
    pub direct_string_reused_bytes: u64,
    /// One request-owned `$GLOBALS` proxy handle. Optimizing functions load
    /// this stable value directly instead of treating the special local as an
    /// uninitialized ordinary SSA slot.
    pub trusted_globals_proxy: i64,
    /// Dense per-function offsets followed by authoritative numeric slots for
    /// top-level/include-scope locals and superglobals.
    pub trusted_request_local_function_offsets: u64,
    pub trusted_request_local_function_count: u32,
    pub trusted_request_local_reserved: u32,
    pub trusted_request_local_slots: u64,
    pub trusted_request_local_slot_count: u32,
    pub trusted_request_local_slot_reserved: u32,
    /// Immutable constant descriptors for the currently active source unit.
    pub trusted_constant_views: u64,
    pub trusted_constant_view_count: u32,
    pub trusted_constant_view_reserved: u32,
    /// Request-owned immutable values for already-resolved `FetchConst`
    /// callsites. The property continuation-offset table indexes this dense
    /// parallel plan array as well.
    pub trusted_constant_slots: u64,
    pub trusted_constant_slot_count: u32,
    pub trusted_constant_slot_reserved: u32,
    /// Dense exact class-allocation plans for the active source unit.
    pub trusted_class_plans: u64,
    pub trusted_class_plan_count: u32,
    pub trusted_class_plan_reserved: u32,
    /// Dense process-owned baseline entry cells indexed by `FunctionId`.
    pub trusted_function_entries: u64,
    pub trusted_function_entry_count: u32,
    pub trusted_function_entry_reserved: u32,
    /// Optional optimizing entries for the same functions. Generated
    /// optimizing callers select these entries and resume a rejected callee
    /// directly through `trusted_function_entries`.
    pub trusted_optimizing_function_entries: u64,
    pub trusted_optimizing_function_entry_count: u32,
    pub trusted_optimizing_function_entry_reserved: u32,
    /// Demand-backed caller snapshots captured only when a compiled native
    /// call returns `SUSPEND_FIBER`. Generated callers push the callee state,
    /// publish their own continuation, and link the two without entering the
    /// cold coordinator or a runtime helper.
    pub fiber_suspension_states: u64,
    pub fiber_suspension_next: u64,
    pub fiber_suspension_capacity: u32,
    pub fiber_suspension_reserved: u32,
    /// Request-local loop-header counter. Generated code performs a deadline
    /// poll on the first header visit and at a fixed bounded cadence.
    pub poll_counter: u64,
    /// Set by direct container stores before releasing a replaced child.
    pub root_mutation_pending: u64,
    /// Dense offsets indexed by FunctionId, followed by exact numeric plans
    /// indexed by continuation ID.
    pub trusted_property_function_offsets: u64,
    pub trusted_property_function_count: u32,
    pub trusted_property_reserved: u32,
    pub trusted_property_slots: u64,
    pub trusted_property_slot_count: u32,
    pub trusted_property_slot_reserved: u32,
    /// Exact `global $name` references indexed by function/continuation.
    pub trusted_global_reference_slots: u64,
    pub trusted_global_reference_slot_count: u32,
    pub trusted_global_reference_slot_reserved: u32,
    /// Exact function-static references indexed through the same dense
    /// function/continuation offsets as the other immutable callsite plans.
    pub trusted_static_local_slots: u64,
    pub trusted_static_local_slot_count: u32,
    pub trusted_static_local_slot_reserved: u32,
    /// Authoritative request static-property storage plus exact continuation
    /// plans. The existing function-offset table indexes both declared-object
    /// and static-property plans.
    pub static_property_slots: u64,
    pub static_property_slot_count: u32,
    pub static_property_slot_reserved: u32,
    pub trusted_static_property_slots: u64,
    pub trusted_static_property_slot_count: u32,
    pub trusted_static_property_slot_reserved: u32,
    /// Exact static `instanceof` plans indexed through the existing dense
    /// function/continuation offset table, plus their immutable hash entries.
    pub trusted_instanceof_plans: u64,
    pub trusted_instanceof_plan_count: u32,
    pub trusted_instanceof_plan_reserved: u32,
    pub trusted_instanceof_entries: u64,
    pub trusted_instanceof_entry_count: u32,
    pub trusted_instanceof_entry_reserved: u32,
    /// Exact request capability consumed by the `error_reporting` intrinsic.
    pub error_reporting: u64,
}

thread_local! {
    static ACTIVE_NATIVE_RUNTIME_VIEW: Cell<JitNativeRuntimeView> =
        const { Cell::new(JitNativeRuntimeView { abi_version: 0, value_slot_capacity: 0, value_slots: 0, direct_value_slots: 0, direct_value_next: 0, direct_value_free_head: 0, direct_value_reused_bytes: 0, direct_object_owners: 0, direct_array_states: 0, direct_array_entries: 0, direct_array_next: 0, direct_array_free_heads: 0, direct_array_reused_bytes: 0, direct_string_bytes: 0, direct_string_next: 0, direct_string_free_heads: 0, direct_string_reused_bytes: 0, trusted_globals_proxy: 0, trusted_request_local_function_offsets: 0, trusted_request_local_function_count: 0, trusted_request_local_reserved: 0, trusted_request_local_slots: 0, trusted_request_local_slot_count: 0, trusted_request_local_slot_reserved: 0, trusted_constant_views: 0, trusted_constant_view_count: 0, trusted_constant_view_reserved: 0, trusted_constant_slots: 0, trusted_constant_slot_count: 0, trusted_constant_slot_reserved: 0, trusted_class_plans: 0, trusted_class_plan_count: 0, trusted_class_plan_reserved: 0, trusted_function_entries: 0, trusted_function_entry_count: 0, trusted_function_entry_reserved: 0, trusted_optimizing_function_entries: 0, trusted_optimizing_function_entry_count: 0, trusted_optimizing_function_entry_reserved: 0, fiber_suspension_states: 0, fiber_suspension_next: 0, fiber_suspension_capacity: 0, fiber_suspension_reserved: 0, poll_counter: 0, root_mutation_pending: 0, trusted_property_function_offsets: 0, trusted_property_function_count: 0, trusted_property_reserved: 0, trusted_property_slots: 0, trusted_property_slot_count: 0, trusted_property_slot_reserved: 0, trusted_global_reference_slots: 0, trusted_global_reference_slot_count: 0, trusted_global_reference_slot_reserved: 0, trusted_static_local_slots: 0, trusted_static_local_slot_count: 0, trusted_static_local_slot_reserved: 0, static_property_slots: 0, static_property_slot_count: 0, static_property_slot_reserved: 0, trusted_static_property_slots: 0, trusted_static_property_slot_count: 0, trusted_static_property_slot_reserved: 0, trusted_instanceof_plans: 0, trusted_instanceof_plan_count: 0, trusted_instanceof_plan_reserved: 0, trusted_instanceof_entries: 0, trusted_instanceof_entry_count: 0, trusted_instanceof_entry_reserved: 0, error_reporting: 0 }) };
    // Standalone compiler tests may publish only the arena fields they
    // exercise. Production activation always supplies its request-owned head.
    static FALLBACK_DIRECT_VALUE_FREE_HEAD: Cell<u32> =
        const { Cell::new(JIT_NATIVE_DIRECT_ARRAY_FREE_NONE) };
    static FALLBACK_DIRECT_VALUE_REUSED_BYTES: Cell<u64> = const { Cell::new(0) };
    static FALLBACK_DIRECT_ARRAY_STATES: std::cell::RefCell<Box<[JitNativeDirectArrayState]>> =
        std::cell::RefCell::new(
            vec![JitNativeDirectArrayState::default(); 4_096].into_boxed_slice()
        );
    static FALLBACK_DIRECT_ARRAY_REUSED_BYTES: Cell<u64> = const { Cell::new(0) };
    static FALLBACK_DIRECT_STRING_REUSED_BYTES: Cell<u64> = const { Cell::new(0) };
}

static EMPTY_NATIVE_FUNCTION_ENTRIES: [std::sync::atomic::AtomicUsize; 4_096] =
    [const { std::sync::atomic::AtomicUsize::new(0) }; 4_096];

/// Restores the preceding request view when native execution leaves the
/// current synchronous activation.
pub struct JitNativeRuntimeViewGuard {
    previous: JitNativeRuntimeView,
}

impl Drop for JitNativeRuntimeViewGuard {
    fn drop(&mut self) {
        ACTIVE_NATIVE_RUNTIME_VIEW.with(|active| active.set(self.previous));
    }
}

#[must_use]
pub fn activate_native_runtime_view(mut view: JitNativeRuntimeView) -> JitNativeRuntimeViewGuard {
    if view.direct_value_free_head == 0 {
        FALLBACK_DIRECT_VALUE_FREE_HEAD.with(|head| {
            head.set(JIT_NATIVE_DIRECT_ARRAY_FREE_NONE);
            view.direct_value_free_head = head.as_ptr() as usize as u64;
        });
    }
    if view.direct_value_reused_bytes == 0 {
        FALLBACK_DIRECT_VALUE_REUSED_BYTES.with(|bytes| {
            bytes.set(0);
            view.direct_value_reused_bytes = bytes.as_ptr() as usize as u64;
        });
    }
    if view.direct_array_states == 0 {
        FALLBACK_DIRECT_ARRAY_STATES.with(|states| {
            let mut states = states.borrow_mut();
            states.fill(JitNativeDirectArrayState::default());
            if view.direct_value_slots != 0 && view.direct_value_next != 0 {
                // SAFETY: this compatibility branch exists only for standalone
                // compiler fixtures that publish the slot arena but predate the
                // slot-parallel array-state field. Production activation always
                // supplies its demand-backed state arena explicitly.
                let used = unsafe { *(view.direct_value_next as usize as *const u32) as usize }
                    .min(states.len());
                let slots = unsafe {
                    std::slice::from_raw_parts(
                        view.direct_value_slots as usize as *const JitNativeValueSlot,
                        used,
                    )
                };
                for (index, slot) in slots.iter().enumerate() {
                    if slot.refcount == 0
                        || slot.kind != JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
                        || slot.aux == 0
                    {
                        continue;
                    }
                    let length = usize::try_from(slot.payload).unwrap_or(0);
                    let entries = unsafe {
                        std::slice::from_raw_parts(
                            slot.aux as usize as *const JitNativeDirectArrayEntry,
                            length,
                        )
                    };
                    let next_append_key = entries
                        .iter()
                        .filter_map(|entry| {
                            (crate::jit_decode_runtime_value(entry.key).is_none()
                                && crate::jit_decode_constant(entry.key).is_none())
                            .then_some(entry.key)
                        })
                        .map(|key| key.saturating_add(1))
                        .max();
                    states[index] = JitNativeDirectArrayState {
                        next_append_key: next_append_key.unwrap_or(0),
                        has_next_append_key: u32::from(next_append_key.is_some()),
                        reserved: 0,
                    };
                }
            }
            view.direct_array_states = states.as_mut_ptr() as usize as u64;
        });
    }
    if view.direct_array_reused_bytes == 0 {
        FALLBACK_DIRECT_ARRAY_REUSED_BYTES.with(|bytes| {
            bytes.set(0);
            view.direct_array_reused_bytes = bytes.as_ptr() as usize as u64;
        });
    }
    if view.direct_string_reused_bytes == 0 {
        FALLBACK_DIRECT_STRING_REUSED_BYTES.with(|bytes| {
            bytes.set(0);
            view.direct_string_reused_bytes = bytes.as_ptr() as usize as u64;
        });
    }
    let previous = ACTIVE_NATIVE_RUNTIME_VIEW.with(|active| active.replace(view));
    JitNativeRuntimeViewGuard { previous }
}

pub(crate) fn current_native_runtime_view() -> JitNativeRuntimeView {
    let mut view = ACTIVE_NATIVE_RUNTIME_VIEW.with(Cell::get);
    let empty = EMPTY_NATIVE_FUNCTION_ENTRIES.as_ptr() as usize as u64;
    if view.trusted_function_entries == 0 {
        view.trusted_function_entries = empty;
    }
    if view.trusted_optimizing_function_entries == 0 {
        view.trusted_optimizing_function_entries = empty;
    }
    view
}

/// Stable prefix of the request-owned native runtime state passed to every
/// generated entry, fragment, and compiled callee.
///
/// Generated code treats the pointer as opaque today and consumes the copied
/// runtime view from `JitDeoptState`. Keeping this prefix in the codegen crate
/// lets entry setup obtain that view directly from the native ABI instead of
/// recovering request state through TLS or depending on the VM coordinator's
/// Rust layout.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct JitNativeFastStateHeader {
    pub abi_version: u32,
    pub flags: u32,
    pub runtime_view: JitNativeRuntimeView,
}

pub(crate) fn native_runtime_view(runtime: *mut std::ffi::c_void) -> JitNativeRuntimeView {
    if runtime.is_null() {
        return current_native_runtime_view();
    }
    // SAFETY: internal native entries receive a pointer whose stable prefix is
    // `JitNativeFastStateHeader`. Publication validates the runtime ABI once;
    // warm entry does not repeat an ABI or pointer check.
    unsafe {
        runtime
            .cast::<JitNativeFastStateHeader>()
            .read()
            .runtime_view
    }
}

/// Maximum number of scalar VM locals materialized by one native side exit.
pub const JIT_DEOPT_MAX_SLOTS: usize = 256;
pub const JIT_DEOPT_LOCAL_MASK_WORDS: usize = JIT_DEOPT_MAX_SLOTS / u64::BITS as usize;
pub const JIT_DEOPT_MAX_REGISTERS: usize = 64;
/// The native frame arena admits at most 768 active allocations. A suspended
/// compiled call can therefore never require more caller snapshots than this
/// request-local demand-backed stack.
pub const JIT_NATIVE_FIBER_SUSPENSION_CAPACITY: usize = 768;

/// Diagnostic detail written only when an optimizing array access leaves the
/// native tier. These values classify the rejected native representation; they
/// are not consulted by production execution.
pub const JIT_OPTIMIZING_EXIT_ARRAY_NOT_TAGGED: u32 = 0x1001;
pub const JIT_OPTIMIZING_EXIT_ARRAY_VIEW_MISSING: u32 = 0x1002;
pub const JIT_OPTIMIZING_EXIT_ARRAY_KEY_UNSUPPORTED: u32 = 0x1003;

/// Caller-owned state buffer populated before a native side exit returns.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JitDeoptState {
    /// Stable IR function ID that owns `continuation_id`.
    pub function_id: u32,
    /// Stable continuation ID in the compiled region metadata.
    pub continuation_id: u32,
    /// Number of addressable local slots in the compiled region.
    pub slot_count: u32,
    /// Native source-version identity reconstructed at a transition.
    pub native_version: u32,
    /// Bit `n` is set when `slots[n]` contains a materialized value for the
    /// first 64 locals.
    pub initialized_mask: u64,
    /// Initialization masks for locals 64 through 255, in ascending chunks.
    pub initialized_masks_high: [u64; JIT_DEOPT_LOCAL_MASK_WORDS - 1],
    /// Materialized scalar locals indexed by their VM local ID.
    pub slots: [i64; JIT_DEOPT_MAX_SLOTS],
    /// Explicit PHP control resumed at a catch/finally native entry.
    pub control_status: JitCallStatus,
    pub control_reserved: u32,
    pub control_value: i64,
    /// Suspension metadata used by generator/fiber native resume entries.
    pub suspend_kind: u32,
    pub suspend_flags: u32,
    pub yielded_key: i64,
    pub delegation_handle: u64,
    /// Dense initialization mask for sparse register snapshot slots.
    pub initialized_register_mask: u64,
    /// Stable `RegId` for every sparse snapshot slot. Optimizing and baseline
    /// liveness can differ, so tier transitions must never infer identity
    /// from snapshot position.
    pub register_ids: [u32; JIT_DEOPT_MAX_REGISTERS],
    /// Sparse register values paired with `register_ids`.
    pub registers: [i64; JIT_DEOPT_MAX_REGISTERS],
    /// Stable request view copied at native entry. Direct callees receive the
    /// same state pointer, so refcount cells remain available across calls and
    /// fragment transitions without exposing Rust container offsets.
    pub runtime_view: JitNativeRuntimeView,
}

// SAFETY: `JitDeoptState` is a plain C-layout ABI record, has no destructor,
// and every field accepts an all-zero bit pattern. Runtime publication fills
// the active prefix before generated code can consume a captured state.
#[allow(unsafe_code)]
unsafe impl php_runtime::api::NativeZeroed for JitDeoptState {}

impl Default for JitDeoptState {
    fn default() -> Self {
        Self {
            function_id: u32::MAX,
            continuation_id: u32::MAX,
            slot_count: 0,
            native_version: 0,
            initialized_mask: 0,
            initialized_masks_high: [0; JIT_DEOPT_LOCAL_MASK_WORDS - 1],
            slots: [0; JIT_DEOPT_MAX_SLOTS],
            control_status: JitCallStatus::CONTINUE,
            control_reserved: 0,
            control_value: 0,
            suspend_kind: 0,
            suspend_flags: 0,
            yielded_key: 0,
            delegation_handle: 0,
            initialized_register_mask: 0,
            register_ids: [u32::MAX; JIT_DEOPT_MAX_REGISTERS],
            registers: [0; JIT_DEOPT_MAX_REGISTERS],
            runtime_view: current_native_runtime_view(),
        }
    }
}

impl JitDeoptState {
    #[must_use]
    pub fn local_initialized(&self, local: LocalId) -> bool {
        let index = local.index();
        if index >= JIT_DEOPT_MAX_SLOTS {
            return false;
        }
        let bit = 1_u64 << (index % u64::BITS as usize);
        if index < u64::BITS as usize {
            self.initialized_mask & bit != 0
        } else {
            self.initialized_masks_high[index / u64::BITS as usize - 1] & bit != 0
        }
    }

    pub fn mark_local_initialized(&mut self, local: LocalId) {
        let index = local.index();
        if index >= JIT_DEOPT_MAX_SLOTS {
            return;
        }
        let bit = 1_u64 << (index % u64::BITS as usize);
        if index < u64::BITS as usize {
            self.initialized_mask |= bit;
        } else {
            self.initialized_masks_high[index / u64::BITS as usize - 1] |= bit;
        }
    }
}

/// State reconstructed for a native-to-native version transition.
///
/// This compatibility alias retains the established C layout while public
/// compiler/runtime APIs use native-transition terminology.
pub type JitNativeTransitionState = JitDeoptState;

/// Stable status returned by native calls and runtime helpers.
///
/// Native code must compare the numeric constants below. It must never depend
/// on a Rust enum discriminant.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct JitCallStatus(pub u32);

impl Default for JitCallStatus {
    fn default() -> Self {
        Self::CONTINUE
    }
}

impl JitCallStatus {
    pub const CONTINUE: Self = Self(0);
    pub const RETURN: Self = Self(1);
    pub const RETURN_REFERENCE: Self = Self(2);
    pub const THROW: Self = Self(3);
    pub const EXIT: Self = Self(4);
    pub const SUSPEND_GENERATOR: Self = Self(5);
    pub const SUSPEND_FIBER: Self = Self(6);
    pub const RUNTIME_ERROR: Self = Self(7);
    pub const COMPILE_REQUIRED: Self = Self(8);
    pub const RECOMPILE_REQUESTED: Self = Self(9);
    /// Boundary validation failed before generated code was entered.
    pub const ABI_MISMATCH: Self = Self(10);

    #[must_use]
    pub const fn is_terminal_return(self) -> bool {
        self.0 == Self::RETURN.0 || self.0 == Self::RETURN_REFERENCE.0
    }
}

/// Stable tagged value passed across the generic helper boundary.
///
/// `payload` is either an immediate bit pattern or an opaque VM-owned handle,
/// as selected by `tag`. No Rust `Value` layout crosses the boundary.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitAbiSlot {
    pub tag: u32,
    pub flags: u32,
    pub payload: u64,
}

/// Compact result record shared by native entries and helper dispatch.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JitCallResult {
    pub status: JitCallStatus,
    pub detail: u32,
    pub value: JitAbiSlot,
}

impl Default for JitCallResult {
    fn default() -> Self {
        Self {
            status: JitCallStatus::RETURN,
            detail: 0,
            value: JitAbiSlot::default(),
        }
    }
}

/// Register-returned control result used by exact prepared native handlers.
///
/// The first eight-byte ABI word contains `status` and `detail`; the second
/// contains the encoded native value.  The 16-byte shape is returned in
/// integer registers by both supported product ABIs (AMD64 SysV and AArch64),
/// avoiding the generic call-result out pointer entirely.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JitNativeControlResult {
    pub status: JitCallStatus,
    pub detail: u32,
    pub value: i64,
}

impl JitNativeControlResult {
    #[must_use]
    pub const fn returning(value: i64) -> Self {
        Self {
            status: JitCallStatus::RETURN,
            detail: 0,
            value,
        }
    }

    #[must_use]
    pub const fn control(status: JitCallStatus, detail: u32, value: i64) -> Self {
        Self {
            status,
            detail,
            value,
        }
    }
}

/// ABI-visible reason why native PHP control left the current frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeControlRecord {
    pub status: JitCallStatus,
    /// Source continuation at which the status originated.
    pub continuation_id: u32,
    /// Return value, reference cell, throwable, exit code, or suspend value.
    pub value: JitAbiSlot,
    /// Opaque VM-owned throwable handle when `status` is `THROW`.
    pub exception_handle: u64,
    /// Native continuation selected by explicit unwind, or `u32::MAX`.
    pub resume_continuation_id: u32,
    pub handler_depth: u32,
}

/// One exception region published by a native PHP frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeExceptionHandler {
    pub enter_continuation: u32,
    pub catch_block: u32,
    pub finally_block: u32,
    pub after_block: u32,
    pub exception_local: u32,
    pub catch_type_start: u32,
    pub catch_type_count: u32,
}

/// GC root representation for a live value at a native safepoint.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct JitNativeRootKind(pub u32);

impl JitNativeRootKind {
    /// Baseline frame slot in the frame's published tagged-slot table.
    pub const FRAME_SLOT: Self = Self(1);
    /// Optimized-frame root in a compiler stack map.
    pub const STACK_MAP: Self = Self(2);
    /// Optimized-frame root mirrored into a shadow slot.
    pub const SHADOW_SLOT: Self = Self(3);
}

/// One heap handle visible to GC at a native safepoint.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeRootEntry {
    pub kind: JitNativeRootKind,
    pub slot: u32,
    pub stack_offset: i32,
    pub value_tag: u32,
}

/// PHP-visible points at which native code may release the last object root
/// and must invoke `__destruct` through a native call entry.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct JitNativeDestructorPoint(pub u32);

impl JitNativeDestructorPoint {
    pub const LOCAL_OVERWRITE: Self = Self(1);
    pub const DISCARD: Self = Self(2);
    pub const FRAME_RETURN: Self = Self(3);
    pub const EXCEPTION_UNWIND: Self = Self(4);
    pub const REQUEST_SHUTDOWN: Self = Self(5);
}

/// Native suspension family stored in generator/fiber heap state.
#[repr(transparent)]
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize,
)]
pub struct JitNativeSuspendKind(pub u32);

impl JitNativeSuspendKind {
    pub const GENERATOR_YIELD: Self = Self(1);
    pub const GENERATOR_DELEGATE: Self = Self(2);
    pub const FIBER_SUSPEND: Self = Self(3);
}

/// Input delivered to a native suspension continuation.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct JitNativeResumeInputKind(pub u32);

impl JitNativeResumeInputKind {
    pub const START: Self = Self(1);
    pub const VALUE: Self = Self(2);
    pub const THROW: Self = Self(3);
}

/// Native-generation ownership policy for a suspended heap object.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct JitNativeSuspensionGenerationPolicy(pub u32);

impl JitNativeSuspensionGenerationPolicy {
    pub const KEEP_OWNING_GENERATION: Self = Self(1);
    pub const RECOMPILE_AT_SAFE_BOUNDARY: Self = Self(2);
}

/// Heap header retained by a suspended native generator.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeGeneratorState {
    pub abi_version: u32,
    pub struct_size: u32,
    pub function_id: u32,
    pub native_version: u32,
    pub owning_generation: u64,
    pub continuation_id: u32,
    pub resume_id: u32,
    pub lifecycle_state: u32,
    pub generation_policy: JitNativeSuspensionGenerationPolicy,
    pub local_slots: u64,
    pub local_count: u32,
    pub temporary_count: u32,
    pub temporary_slots: u64,
    pub yielded_key: JitAbiSlot,
    pub yielded_value: JitAbiSlot,
    pub delegation_state: u64,
    pub exception_state: u64,
    pub root_entries: u64,
    pub root_count: u32,
    pub flags: u32,
}

impl JitNativeGeneratorState {
    #[must_use]
    pub fn new(
        function_id: u32,
        native_version: u32,
        owning_generation: u64,
        continuation_id: u32,
        resume_id: u32,
    ) -> Self {
        Self {
            abi_version: JIT_RUNTIME_ABI_VERSION,
            struct_size: std::mem::size_of::<Self>() as u32,
            function_id,
            native_version,
            owning_generation,
            continuation_id,
            resume_id,
            generation_policy: JitNativeSuspensionGenerationPolicy::KEEP_OWNING_GENERATION,
            ..Self::default()
        }
    }

    /// Changes artifact ownership only at an explicit suspension boundary.
    pub fn transition_generation_at_suspension(
        &mut self,
        native_version: u32,
        owning_generation: u64,
        resume_id: u32,
    ) {
        self.native_version = native_version;
        self.owning_generation = owning_generation;
        self.resume_id = resume_id;
        self.generation_policy = JitNativeSuspensionGenerationPolicy::RECOMPILE_AT_SAFE_BOUNDARY;
    }
}

/// Heap header retained by a suspended native fiber stack.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeFiberState {
    pub abi_version: u32,
    pub struct_size: u32,
    pub fiber_id: u64,
    pub function_id: u32,
    pub native_version: u32,
    pub owning_generation: u64,
    pub continuation_id: u32,
    pub resume_id: u32,
    pub lifecycle_state: u32,
    pub generation_policy: JitNativeSuspensionGenerationPolicy,
    pub frame_slots: u64,
    pub frame_slot_count: u32,
    pub frame_count: u32,
    pub exception_state: u64,
    pub root_entries: u64,
    pub root_count: u32,
    pub flags: u32,
}

impl JitNativeFiberState {
    #[must_use]
    pub fn new(
        fiber_id: u64,
        function_id: u32,
        native_version: u32,
        owning_generation: u64,
        continuation_id: u32,
        resume_id: u32,
    ) -> Self {
        Self {
            abi_version: JIT_RUNTIME_ABI_VERSION,
            struct_size: std::mem::size_of::<Self>() as u32,
            fiber_id,
            function_id,
            native_version,
            owning_generation,
            continuation_id,
            resume_id,
            generation_policy: JitNativeSuspensionGenerationPolicy::KEEP_OWNING_GENERATION,
            ..Self::default()
        }
    }

    pub fn transition_generation_at_suspension(
        &mut self,
        native_version: u32,
        owning_generation: u64,
        resume_id: u32,
    ) {
        self.native_version = native_version;
        self.owning_generation = owning_generation;
        self.resume_id = resume_id;
        self.generation_policy = JitNativeSuspensionGenerationPolicy::RECOMPILE_AT_SAFE_BOUNDARY;
    }
}

/// Precise source/backtrace record associated with a generated PC range.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativePcMetadata {
    pub function_id: u32,
    pub continuation_id: u32,
    pub native_start: u32,
    pub native_end: u32,
    pub file_id: u32,
    pub source_start: u32,
    pub source_end: u32,
    pub handler_depth: u32,
    pub root_map_start: u32,
    pub root_map_count: u32,
}

/// Published native frame header. Generated code and runtime helpers exchange
/// only pointers/counts; Rust containers never cross this boundary.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeFrameHeader {
    pub abi_version: u32,
    pub struct_size: u32,
    pub function_id: u32,
    pub generation: u32,
    pub caller_frame: u64,
    pub slots: u64,
    pub slot_count: u32,
    pub active_handler_depth: u32,
    pub handlers: u64,
    pub handler_count: u32,
    pub roots: u64,
    pub root_count: u32,
    pub pc_metadata: u64,
    pub pc_metadata_count: u32,
    pub flags: u32,
}

/// Stable native-call target family. Numeric values are ABI-visible.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct JitNativeCallKind(pub u32);

impl JitNativeCallKind {
    pub const FUNCTION: Self = Self(1);
    pub const METHOD: Self = Self(2);
    pub const STATIC_METHOD: Self = Self(3);
    pub const CLOSURE: Self = Self(4);
    pub const CALLABLE: Self = Self(5);
    pub const PIPE: Self = Self(6);
    pub const CONSTRUCTOR: Self = Self(7);
    pub const DYNAMIC_CONSTRUCTOR: Self = Self(8);
    pub const MAGIC_METHOD: Self = Self(9);
    pub const PROPERTY_HOOK: Self = Self(10);
    pub const AUTOLOAD_CALLBACK: Self = Self(11);
    pub const ERROR_HANDLER: Self = Self(12);
    pub const SHUTDOWN_FUNCTION: Self = Self(13);
    pub const DESTRUCTOR: Self = Self(14);
    pub const BUILTIN_CALLBACK: Self = Self(15);
    /// RegionSemanticOperationId is stored in `target.function_id`.
    pub const SEMANTIC_OPERATION: Self = Self(16);
}

/// ABI-visible flags for one prepared native argument.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct JitNativeArgFlags(pub u32);

impl JitNativeArgFlags {
    pub const NAMED: Self = Self(1 << 0);
    pub const UNPACK: Self = Self(1 << 1);
    pub const BY_REFERENCE: Self = Self(1 << 2);
    pub const INDIRECT_TEMPORARY: Self = Self(1 << 3);
    pub const BY_REF_RETURN_DESTINATION: Self = Self(1 << 4);

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// One argument slot written directly into a native callee-frame buffer.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeCallArgument {
    pub value: JitAbiSlot,
    /// Stable symbol hash for a named argument, zero for positional arguments.
    pub name_hash: u64,
    pub flags: JitNativeArgFlags,
    /// Caller local/lvalue index for by-reference binding, or `u32::MAX`.
    pub source_slot: u32,
    /// Encoded receiver for a deferred object-property lvalue, or zero.
    ///
    /// Cross-unit userland signatures may not be published when the caller is
    /// compiled. The runtime binder uses this receiver together with immutable
    /// IR call metadata only when the resolved parameter is by-reference.
    pub property_receiver: i64,
}

/// Stable target descriptor resolved through generation-safe indirection.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JitNativeCallTarget {
    pub kind: JitNativeCallKind,
    /// Known `FunctionId`, or `u32::MAX` for dynamic resolution.
    pub function_id: u32,
    /// Stable deployment generation expected by the caller.
    pub generation: u64,
    /// Function/method/callable symbol hash; no persisted absolute address.
    pub symbol_hash: u64,
    /// Class/receiver-context symbol hash when applicable.
    pub class_hash: u64,
}

/// One ABI-stable PHP native frame shared by direct and dynamic calls.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JitNativeCallFrame {
    pub abi_version: u32,
    pub struct_size: u32,
    pub function_id: u32,
    pub region_id: u32,
    pub continuation_id: u32,
    pub source_block_id: u32,
    pub source_instruction_id: u32,
    pub result_slot: u32,
    pub local_count: u32,
    pub temporary_count: u32,
    pub argument_count: u32,
    pub flags: u32,
    /// Caller-owned `JitAbiSlot` table.
    pub local_slots: u64,
    /// Caller-owned `JitAbiSlot` table.
    pub temporary_slots: u64,
    /// Caller-owned `JitNativeCallArgument` table.
    pub arguments: u64,
    pub caller_frame: u64,
    pub receiver_handle: u64,
    pub class_context: u64,
    pub exception_metadata: u64,
    /// Caller-owned `JitDeoptState` populated only when a nested native
    /// activation suspends. The generated caller immediately moves it into
    /// the native suspension stack before publishing its own continuation.
    pub transition_state: u64,
    pub trace_metadata: u64,
    pub generator_handle: u64,
    pub fiber_handle: u64,
    pub target: JitNativeCallTarget,
}

impl Default for JitNativeCallFrame {
    fn default() -> Self {
        Self {
            abi_version: JIT_RUNTIME_ABI_VERSION,
            struct_size: std::mem::size_of::<Self>() as u32,
            function_id: u32::MAX,
            region_id: u32::MAX,
            continuation_id: u32::MAX,
            source_block_id: u32::MAX,
            source_instruction_id: u32::MAX,
            result_slot: u32::MAX,
            local_count: 0,
            temporary_count: 0,
            argument_count: 0,
            flags: 0,
            local_slots: 0,
            temporary_slots: 0,
            arguments: 0,
            caller_frame: 0,
            receiver_handle: 0,
            class_context: 0,
            exception_metadata: 0,
            transition_state: 0,
            trace_metadata: 0,
            generator_handle: 0,
            fiber_handle: 0,
            target: JitNativeCallTarget::default(),
        }
    }
}

impl JitNativeCallFrame {
    pub const FLAG_STRICT_TYPES: u32 = 1 << 0;
    pub const FLAG_RETURN_REFERENCE: u32 = 1 << 1;
    pub const FLAG_DIRECT_BUILTIN: u32 = 1 << 2;
    pub const FLAG_DIRECT_EXTERNAL: u32 = 1 << 3;
    /// `arguments` points to a contiguous `i64` payload table. All argument
    /// names, unpack flags and by-reference bindings are known absent.
    pub const FLAG_COMPACT_ARGUMENTS: u32 = 1 << 4;
}

/// Dynamic call resolver/invoker. It may compile and retry a native entry, but
/// it must never invoke a bytecode or IR interpreter.
pub type JitNativeDispatchTrampoline = unsafe extern "C" fn(
    vm_context: u64,
    frame: *mut JitNativeCallFrame,
    out: *mut JitCallResult,
) -> i32;

/// Dynamic source/declaration operation executed from generated code.
#[repr(transparent)]
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize,
)]
pub struct JitNativeDynamicCodeKind(pub u32);

impl JitNativeDynamicCodeKind {
    pub const INCLUDE: Self = Self(1);
    pub const INCLUDE_ONCE: Self = Self(2);
    pub const REQUIRE: Self = Self(3);
    pub const REQUIRE_ONCE: Self = Self(4);
    pub const EVAL: Self = Self(5);
    pub const DECLARE_FUNCTION: Self = Self(6);
    pub const DECLARE_CLASS: Self = Self(7);
    pub const MAKE_CLOSURE: Self = Self(8);
    pub const REGISTER_CONSTANT: Self = Self(9);
    pub const EMIT_DIAGNOSTIC: Self = Self(10);
}

/// Native compile/publication request for dynamic PHP code.
///
/// Source values are tagged scalar values or opaque VM handles. The runtime
/// resolves and validates them, compiles the complete unit, publishes all
/// native entries, and only then invokes the requested entry. No instruction
/// stream or opcode identity crosses this boundary.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JitNativeDynamicCodeRequest {
    pub abi_version: u32,
    pub struct_size: u32,
    pub kind: JitNativeDynamicCodeKind,
    pub flags: u32,
    pub caller_function_id: u32,
    pub continuation_id: u32,
    pub result_slot: u32,
    /// Known declaration/closure body, or `u32::MAX` for source compilation.
    pub declared_function_id: u32,
    pub source: JitAbiSlot,
    /// Exact semantic compiler configuration identity.
    pub config_hash: u64,
    /// Exact dependency/source-generation identity known by generated code.
    pub dependency_identity: u64,
    /// Stable symbol identity for runtime declaration publication.
    pub symbol_hash: u64,
    /// Caller-owned native frame/slot handle used for closure captures.
    pub caller_frame: u64,
}

impl Default for JitNativeDynamicCodeRequest {
    fn default() -> Self {
        Self {
            abi_version: JIT_RUNTIME_ABI_VERSION,
            struct_size: std::mem::size_of::<Self>() as u32,
            kind: JitNativeDynamicCodeKind::default(),
            flags: 0,
            caller_function_id: u32::MAX,
            continuation_id: u32::MAX,
            result_slot: u32::MAX,
            declared_function_id: u32::MAX,
            source: JitAbiSlot::default(),
            config_hash: 0,
            dependency_identity: 0,
            symbol_hash: 0,
            caller_frame: 0,
        }
    }
}

/// Runtime dynamic compiler/invoker. Successful return means the requested
/// PHP code ran through a published native entry. Compile errors use
/// `RUNTIME_ERROR`; a missing compiler uses `COMPILE_REQUIRED` and may never
/// execute the dynamic unit before native compilation succeeds.
pub type JitNativeDynamicCodeTrampoline = unsafe extern "C" fn(
    vm_context: u64,
    request: *mut JitNativeDynamicCodeRequest,
    out: *mut JitCallResult,
) -> i32;

/// Generation-safe process-local indirection entry. Persisted code stores only
/// `function_id` and generation; absolute addresses remain in this live table.
#[derive(Debug)]
pub struct JitNativeIndirectionEntry {
    function_id: u32,
    generation: AtomicU64,
    address: AtomicUsize,
}

impl JitNativeIndirectionEntry {
    #[must_use]
    pub const fn new(function_id: u32) -> Self {
        Self {
            function_id,
            generation: AtomicU64::new(0),
            address: AtomicUsize::new(0),
        }
    }

    #[must_use]
    pub const fn function_id(&self) -> u32 {
        self.function_id
    }

    /// Publishes a new generation after the native entry address is visible.
    pub fn publish(&self, generation: u64, address: usize) {
        self.address.store(address, Ordering::Release);
        self.generation.store(generation, Ordering::Release);
    }

    /// Resolves only the exact generation expected by compiled code.
    #[must_use]
    pub fn resolve(&self, expected_generation: u64) -> Option<usize> {
        (self.generation.load(Ordering::Acquire) == expected_generation)
            .then(|| self.address.load(Ordering::Acquire))
            .filter(|address| *address != 0)
    }
}

/// Opaque non-zero handle owned by the VM side of the ABI.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct JitOpaqueHandle(NonZeroU64);

impl JitOpaqueHandle {
    /// Creates an opaque handle. Zero is reserved for "no handle".
    #[must_use]
    pub fn new(raw: u64) -> Option<Self> {
        NonZeroU64::new(raw).map(Self)
    }

    /// Returns the stable raw value for logging and test snapshots.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0.get()
    }
}

/// Opaque VM request/context handle.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct JitVmContextHandle(JitOpaqueHandle);

impl JitVmContextHandle {
    /// Creates a VM context handle.
    #[must_use]
    pub fn new(raw: u64) -> Option<Self> {
        JitOpaqueHandle::new(raw).map(Self)
    }

    /// Returns the stable raw handle value.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0.raw()
    }
}

/// Opaque frame handle.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct JitFrameHandle(JitOpaqueHandle);

impl JitFrameHandle {
    /// Creates a frame handle.
    #[must_use]
    pub fn new(raw: u64) -> Option<Self> {
        JitOpaqueHandle::new(raw).map(Self)
    }

    /// Returns the stable raw handle value.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0.raw()
    }
}

/// Read-only frame/register metadata exported to future JIT code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitFrameView {
    /// VM context that owns the frame.
    pub context: JitVmContextHandle,
    /// Opaque active-frame handle.
    pub frame: JitFrameHandle,
    /// IR function represented by this frame.
    pub function: FunctionId,
    /// Number of VM registers available to this frame.
    pub register_count: u32,
    /// Number of local slots available to this frame.
    pub local_count: u32,
}

impl JitFrameView {
    /// Creates a frame view from opaque VM-owned handles and arena sizes.
    #[must_use]
    pub const fn new(
        context: JitVmContextHandle,
        frame: JitFrameHandle,
        function: FunctionId,
        register_count: u32,
        local_count: u32,
    ) -> Self {
        Self {
            context,
            frame,
            function,
            register_count,
            local_count,
        }
    }

    /// Returns true when a register can be addressed through this view.
    #[must_use]
    pub const fn contains_register(&self, register: RegId) -> bool {
        register.raw() < self.register_count
    }

    /// Returns true when a local can be addressed through this view.
    #[must_use]
    pub const fn contains_local(&self, local: LocalId) -> bool {
        local.raw() < self.local_count
    }
}

/// Heap-backed PHP value categories crossing the ABI as opaque handles only.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JitOpaqueValueKind {
    /// PHP string storage.
    String,
    /// PHP array storage.
    Array,
    /// PHP object storage.
    Object,
    /// PHP resource storage.
    Resource,
    /// PHP reference cell.
    Reference,
    /// PHP callable/closure value.
    Callable,
    /// PHP generator value.
    Generator,
    /// PHP fiber value.
    Fiber,
}

impl JitOpaqueValueKind {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Array => "array",
            Self::Object => "object",
            Self::Resource => "resource",
            Self::Reference => "reference",
            Self::Callable => "callable",
            Self::Generator => "generator",
            Self::Fiber => "fiber",
        }
    }
}

/// ABI-safe value representation.
#[derive(Clone, Debug, PartialEq)]
pub enum JitAbiValue {
    /// PHP null.
    Null,
    /// PHP bool.
    Bool(bool),
    /// PHP int.
    Int(i64),
    /// PHP float as raw IEEE-754 bits.
    FloatBits(u64),
    /// Uninitialized register/local marker.
    Uninitialized,
    /// VM-owned heap value represented by an opaque handle.
    Opaque {
        /// Heap value family.
        kind: JitOpaqueValueKind,
        /// VM-owned handle.
        handle: JitOpaqueHandle,
    },
}

impl JitAbiValue {
    /// Creates a float value while preserving exact bits.
    #[must_use]
    pub const fn float(value: f64) -> Self {
        Self::FloatBits(value.to_bits())
    }

    /// Returns true for heap-backed values that require VM side handling.
    #[must_use]
    pub const fn is_opaque(&self) -> bool {
        matches!(self, Self::Opaque { .. })
    }
}

/// Why future native code left the compiled region.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JitBailoutKind {
    /// Type/value guard failed.
    GuardFailed,
    /// Encountered a value outside the primitive subset.
    UnsupportedValue,
    /// Runtime callout requested a generic native path.
    RuntimeCallout,
    /// A less-specialized native continuation was requested.
    NativeTransition,
}

impl JitBailoutKind {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GuardFailed => "guard_failed",
            Self::UnsupportedValue => "unsupported_value",
            Self::RuntimeCallout => "runtime_callout",
            Self::NativeTransition => "native_transition",
        }
    }
}

/// Stable reason codes for native version exits.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SideExitReason {
    /// Runtime value type did not match the compiled specialization.
    TypeMismatch = 1,
    /// Checked arithmetic or conversion overflowed.
    Overflow = 2,
    /// Runtime value shape is outside the compiled subset.
    UnsupportedValue = 3,
    /// A generated guard failed.
    GuardFailed = 4,
    /// Runtime helper returned a non-OK status.
    HelperStatus = 5,
    /// PHP exception/error state is pending.
    ExceptionPending = 6,
    /// VM/JIT ABI hash or call boundary did not match.
    AbiMismatch = 7,
}

impl SideExitReason {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TypeMismatch => "type_mismatch",
            Self::Overflow => "overflow",
            Self::UnsupportedValue => "unsupported_value",
            Self::GuardFailed => "guard_failed",
            Self::HelperStatus => "helper_status",
            Self::ExceptionPending => "exception_pending",
            Self::AbiMismatch => "abi_mismatch",
        }
    }

    /// Stable numeric ABI code.
    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }
}

/// Structured side-exit metadata observed before a native transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitSideExit {
    /// Stable reason.
    pub reason: SideExitReason,
    /// Optional baseline-native continuation entry.
    pub continuation_id: Option<u32>,
    /// Optional stable IR/source position associated with the continuation.
    pub source_position: Option<u32>,
    /// Optional helper status or guard code.
    pub status_code: Option<i32>,
}

impl JitSideExit {
    /// Creates side-exit metadata without a resume point.
    #[must_use]
    pub const fn new(reason: SideExitReason) -> Self {
        Self {
            reason,
            continuation_id: None,
            source_position: None,
            status_code: None,
        }
    }

    /// Adds a baseline-native resume point.
    #[must_use]
    pub const fn with_native_continuation(
        mut self,
        continuation_id: u32,
        source_position: u32,
    ) -> Self {
        self.continuation_id = Some(continuation_id);
        self.source_position = Some(source_position);
        self
    }

    /// Adds the raw helper/guard status that caused the exit.
    #[must_use]
    pub const fn with_status(mut self, status_code: i32) -> Self {
        self.status_code = Some(status_code);
        self
    }
}

/// Native version-exit metadata returned to the VM.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitBailout {
    /// Bailout family.
    pub kind: JitBailoutKind,
    /// Optional baseline-native continuation entry.
    pub continuation_id: Option<u32>,
    /// Optional stable IR/source position associated with the continuation.
    pub source_position: Option<u32>,
    /// Stable debug reason.
    pub reason: String,
}

impl JitBailout {
    /// Creates a bailout result.
    #[must_use]
    pub fn new(kind: JitBailoutKind, reason: impl Into<String>) -> Self {
        Self {
            kind,
            continuation_id: None,
            source_position: None,
            reason: reason.into(),
        }
    }

    /// Adds a baseline-native resume point.
    #[must_use]
    pub const fn with_native_continuation(
        mut self,
        continuation_id: u32,
        source_position: u32,
    ) -> Self {
        self.continuation_id = Some(continuation_id);
        self.source_position = Some(source_position);
        self
    }
}

/// Exception marker crossing the ABI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitExceptionMarker {
    /// Stable PHP exception/error class name when known.
    pub class_name: Option<String>,
    /// Stable message snapshot when known.
    pub message: Option<String>,
    /// Opaque VM-owned exception object handle when already allocated.
    pub exception: Option<JitOpaqueHandle>,
}

impl JitExceptionMarker {
    /// Creates a marker from a class/message pair without exposing the object.
    #[must_use]
    pub fn named(class_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            class_name: Some(class_name.into()),
            message: Some(message.into()),
            exception: None,
        }
    }

    /// Creates a marker for an existing VM-owned exception object.
    #[must_use]
    pub fn opaque(exception: JitOpaqueHandle) -> Self {
        Self {
            class_name: None,
            message: None,
            exception: Some(exception),
        }
    }
}

/// Runtime callout identity and arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct JitRuntimeCallout {
    /// Stable callout name.
    pub name: String,
    /// ABI values copied or represented by opaque handles.
    pub args: Vec<JitAbiValue>,
    /// True when the VM side may report an exception marker.
    pub can_throw: bool,
}

impl JitRuntimeCallout {
    /// Creates a runtime callout descriptor.
    #[must_use]
    pub fn new(name: impl Into<String>, args: Vec<JitAbiValue>, can_throw: bool) -> Self {
        Self {
            name: name.into(),
            args,
            can_throw,
        }
    }
}

/// Result returned from a VM runtime callout.
#[derive(Clone, Debug, PartialEq)]
pub enum JitRuntimeCalloutResult {
    /// Callout returned a normal ABI value.
    Returned(JitAbiValue),
    /// Callout requested a generic or less-specialized native continuation.
    Bailout(JitBailout),
    /// Callout propagated a PHP exception/error.
    Exception(JitExceptionMarker),
}

/// Result of a future compiled region.
#[derive(Clone, Debug, PartialEq)]
pub enum JitRegionResult {
    /// Region produced a normal PHP value.
    Returned(JitAbiValue),
    /// Region requested a generic or less-specialized native continuation.
    Bailout(JitBailout),
    /// Region propagated an exception marker to the VM.
    Exception(JitExceptionMarker),
}

/// C-compatible value tags used by native entry and helper call boundaries.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JitCValueTag {
    /// Uninitialized register/local marker.
    Uninitialized = 0,
    /// PHP null.
    Null = 1,
    /// PHP bool; payload is 0 or 1.
    Bool = 2,
    /// PHP int; payload is the two's-complement `i64` bit pattern.
    Int = 3,
    /// PHP float; payload is the raw IEEE-754 bits.
    FloatBits = 4,
    /// VM-owned string handle.
    OpaqueString = 16,
    /// VM-owned array handle.
    OpaqueArray = 17,
    /// VM-owned object handle.
    OpaqueObject = 18,
    /// VM-owned resource handle.
    OpaqueResource = 19,
    /// VM-owned reference handle.
    OpaqueReference = 20,
    /// VM-owned callable handle.
    OpaqueCallable = 21,
    /// VM-owned generator handle.
    OpaqueGenerator = 22,
    /// VM-owned fiber handle.
    OpaqueFiber = 23,
}

/// C-compatible ABI value.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JitCValue {
    /// Value tag.
    pub tag: JitCValueTag,
    /// Reserved for alignment and future ABI-compatible extensions.
    pub reserved: u32,
    /// Primary value payload or opaque handle.
    pub payload: u64,
    /// Auxiliary payload. Zero unless documented by a later ABI revision.
    pub aux: u64,
}

impl JitCValue {
    /// Creates an uninitialized marker.
    #[must_use]
    pub const fn uninitialized() -> Self {
        Self {
            tag: JitCValueTag::Uninitialized,
            reserved: 0,
            payload: 0,
            aux: 0,
        }
    }

    /// Creates a null value.
    #[must_use]
    pub const fn null() -> Self {
        Self {
            tag: JitCValueTag::Null,
            reserved: 0,
            payload: 0,
            aux: 0,
        }
    }

    /// Creates a bool value.
    #[must_use]
    pub const fn bool(value: bool) -> Self {
        Self {
            tag: JitCValueTag::Bool,
            reserved: 0,
            payload: value as u64,
            aux: 0,
        }
    }

    /// Creates an int value.
    #[must_use]
    pub const fn int(value: i64) -> Self {
        Self {
            tag: JitCValueTag::Int,
            reserved: 0,
            payload: value as u64,
            aux: 0,
        }
    }

    /// Creates a float value from exact bits.
    #[must_use]
    pub const fn float_bits(bits: u64) -> Self {
        Self {
            tag: JitCValueTag::FloatBits,
            reserved: 0,
            payload: bits,
            aux: 0,
        }
    }

    /// Creates a float value from an `f64` (stored as its IEEE-754 bits).
    #[must_use]
    pub fn float(value: f64) -> Self {
        Self::float_bits(value.to_bits())
    }

    /// Creates an opaque heap value handle.
    #[must_use]
    pub const fn opaque(kind: JitOpaqueValueKind, handle: JitOpaqueHandle) -> Self {
        let tag = match kind {
            JitOpaqueValueKind::String => JitCValueTag::OpaqueString,
            JitOpaqueValueKind::Array => JitCValueTag::OpaqueArray,
            JitOpaqueValueKind::Object => JitCValueTag::OpaqueObject,
            JitOpaqueValueKind::Resource => JitCValueTag::OpaqueResource,
            JitOpaqueValueKind::Reference => JitCValueTag::OpaqueReference,
            JitOpaqueValueKind::Callable => JitCValueTag::OpaqueCallable,
            JitOpaqueValueKind::Generator => JitCValueTag::OpaqueGenerator,
            JitOpaqueValueKind::Fiber => JitCValueTag::OpaqueFiber,
        };
        Self {
            tag,
            reserved: 0,
            payload: handle.raw(),
            aux: 0,
        }
    }
}

/// C-compatible frame metadata view.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JitCFrameView {
    /// VM context handle.
    pub context: u64,
    /// VM frame handle.
    pub frame: u64,
    /// IR function id.
    pub function: u32,
    /// Number of VM registers available to this frame.
    pub register_count: u32,
    /// Number of local slots available to this frame.
    pub local_count: u32,
    /// Reserved for ABI-compatible expansion.
    pub reserved: u32,
}

impl From<JitFrameView> for JitCFrameView {
    fn from(view: JitFrameView) -> Self {
        Self {
            context: view.context.raw(),
            frame: view.frame.raw(),
            function: view.function.raw(),
            register_count: view.register_count,
            local_count: view.local_count,
            reserved: 0,
        }
    }
}

/// C-compatible region exit tags.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JitCExitTag {
    /// Region returned normally.
    Returned = 0,
    /// Region bailed out before completing.
    Bailout = 1,
    /// Region propagated a PHP exception/error marker.
    Exception = 2,
    /// Region requested a runtime helper call.
    RuntimeCallout = 3,
}

/// C-compatible region exit record.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JitCExit {
    /// Exit tag.
    pub tag: JitCExitTag,
    /// Stable reason or helper-symbol id. Zero means no reason id.
    pub reason_code: u32,
    /// Value returned or associated with the exit.
    pub value: JitCValue,
    /// Baseline-native continuation ID; `u32::MAX` means no continuation.
    pub continuation_id: u32,
    /// Stable IR/source position; `u32::MAX` means no associated position.
    pub source_position: u32,
    /// Reserved for ABI-compatible expansion.
    pub reserved: u32,
}

impl JitCExit {
    /// Creates a normal return exit.
    #[must_use]
    pub const fn returned(value: JitCValue) -> Self {
        Self {
            tag: JitCExitTag::Returned,
            reason_code: 0,
            value,
            continuation_id: u32::MAX,
            source_position: u32::MAX,
            reserved: 0,
        }
    }

    /// Creates a bailout exit with a stable reason id.
    #[must_use]
    pub const fn bailout(reason_code: u32, value: JitCValue) -> Self {
        Self {
            tag: JitCExitTag::Bailout,
            reason_code,
            value,
            continuation_id: u32::MAX,
            source_position: u32::MAX,
            reserved: 0,
        }
    }

    /// Creates a side-exit bailout record with a stable reason.
    #[must_use]
    pub const fn side_exit(reason: SideExitReason, value: JitCValue) -> Self {
        Self::bailout(reason.code(), value)
    }

    /// Adds a baseline-native resume point.
    #[must_use]
    pub const fn with_native_continuation(
        mut self,
        continuation_id: u32,
        source_position: u32,
    ) -> Self {
        self.continuation_id = continuation_id;
        self.source_position = source_position;
        self
    }
}

#[cfg(test)]
mod tests {
    use std::mem::{align_of, size_of};

    use php_ir::{FunctionId, LocalId};

    use super::{
        JIT_RUNTIME_ABI_HASH, JIT_RUNTIME_ABI_VERSION, JitCExit, JitCExitTag, JitCFrameView,
        JitCValue, JitCValueTag, JitCallStatus, JitDeoptState, JitFrameHandle, JitFrameView,
        JitNativeArgFlags, JitNativeCallArgument, JitNativeCallFrame, JitNativeCallKind,
        JitNativeControlRecord, JitNativeDynamicCodeKind, JitNativeDynamicCodeRequest,
        JitNativeExceptionHandler, JitNativeFiberState, JitNativeFrameHeader,
        JitNativeGeneratorState, JitNativeIndirectionEntry, JitNativePcMetadata,
        JitNativeRootEntry, JitNativeSuspensionGenerationPolicy, JitNativeValueSlot,
        JitOpaqueHandle, JitOpaqueValueKind, JitSideExit, JitVmContextHandle, SideExitReason,
    };

    #[test]
    fn c_abi_layout_is_stable() {
        assert_eq!(JIT_RUNTIME_ABI_VERSION, 78);
        assert_ne!(JIT_RUNTIME_ABI_HASH, 0);
        assert_eq!(size_of::<JitOpaqueHandle>(), 8);
        assert_eq!(size_of::<JitCValueTag>(), 4);
        assert_eq!(size_of::<JitCValue>(), 24);
        assert_eq!(align_of::<JitCValue>(), 8);
        assert_eq!(size_of::<JitCFrameView>(), 32);
        assert_eq!(align_of::<JitCFrameView>(), 8);
        assert_eq!(size_of::<JitCExitTag>(), 4);
        assert_eq!(size_of::<JitCExit>(), 48);
        assert_eq!(align_of::<JitCExit>(), 8);
        assert_eq!(align_of::<JitNativeCallArgument>(), 8);
        assert_eq!(align_of::<JitNativeCallFrame>(), 8);
        assert_eq!(align_of::<JitNativeDynamicCodeRequest>(), 8);
        assert_eq!(align_of::<JitNativeControlRecord>(), 8);
        assert_eq!(align_of::<JitNativeExceptionHandler>(), 4);
        assert_eq!(align_of::<JitNativeFrameHeader>(), 8);
        assert_eq!(align_of::<JitNativePcMetadata>(), 4);
        assert_eq!(align_of::<JitNativeRootEntry>(), 4);
        assert_eq!(align_of::<JitNativeGeneratorState>(), 8);
        assert_eq!(align_of::<JitNativeFiberState>(), 8);
        assert_eq!(size_of::<JitNativeValueSlot>(), 32);
        assert_eq!(align_of::<JitNativeValueSlot>(), 8);
        assert_eq!(std::mem::offset_of!(JitNativeValueSlot, refcount), 0);
        assert_eq!(std::mem::offset_of!(JitNativeValueSlot, payload), 16);
        assert_eq!(std::mem::offset_of!(JitNativeValueSlot, aux), 24);
        assert_eq!(
            JitNativeCallFrame::default().struct_size as usize,
            size_of::<JitNativeCallFrame>()
        );
        assert_eq!(
            JitNativeDynamicCodeRequest::default().struct_size as usize,
            size_of::<JitNativeDynamicCodeRequest>()
        );
    }

    #[test]
    fn native_dynamic_code_kind_numbers_are_stable() {
        assert_eq!(JitNativeDynamicCodeKind::INCLUDE.0, 1);
        assert_eq!(JitNativeDynamicCodeKind::INCLUDE_ONCE.0, 2);
        assert_eq!(JitNativeDynamicCodeKind::REQUIRE.0, 3);
        assert_eq!(JitNativeDynamicCodeKind::REQUIRE_ONCE.0, 4);
        assert_eq!(JitNativeDynamicCodeKind::EVAL.0, 5);
        assert_eq!(JitNativeDynamicCodeKind::DECLARE_FUNCTION.0, 6);
        assert_eq!(JitNativeDynamicCodeKind::DECLARE_CLASS.0, 7);
        assert_eq!(JitNativeDynamicCodeKind::MAKE_CLOSURE.0, 8);
        assert_eq!(JitNativeDynamicCodeKind::REGISTER_CONSTANT.0, 9);
        assert_eq!(JitNativeDynamicCodeKind::EMIT_DIAGNOSTIC.0, 10);
    }

    #[test]
    fn suspended_state_owns_generation_until_safe_transition() {
        let mut generator = JitNativeGeneratorState::new(3, 1, 9, 7, 0x4000_0007);
        assert_eq!(generator.abi_version, JIT_RUNTIME_ABI_VERSION);
        assert_eq!(
            generator.generation_policy,
            JitNativeSuspensionGenerationPolicy::KEEP_OWNING_GENERATION
        );
        generator.transition_generation_at_suspension(2, 10, 0x4000_0008);
        assert_eq!(generator.native_version, 2);
        assert_eq!(generator.owning_generation, 10);
        assert_eq!(
            generator.generation_policy,
            JitNativeSuspensionGenerationPolicy::RECOMPILE_AT_SAFE_BOUNDARY
        );

        let mut fiber = JitNativeFiberState::new(11, 3, 1, 9, 7, 0x4000_0007);
        fiber.transition_generation_at_suspension(2, 10, 0x4000_0008);
        assert_eq!(fiber.owning_generation, 10);
        assert_eq!(fiber.resume_id, 0x4000_0008);
    }

    #[test]
    fn native_control_status_numbers_are_stable() {
        assert_eq!(JitCallStatus::CONTINUE.0, 0);
        assert_eq!(JitCallStatus::RETURN.0, 1);
        assert_eq!(JitCallStatus::RETURN_REFERENCE.0, 2);
        assert_eq!(JitCallStatus::THROW.0, 3);
        assert_eq!(JitCallStatus::EXIT.0, 4);
        assert_eq!(JitCallStatus::SUSPEND_GENERATOR.0, 5);
        assert_eq!(JitCallStatus::SUSPEND_FIBER.0, 6);
        assert_eq!(JitCallStatus::RUNTIME_ERROR.0, 7);
        assert_eq!(JitCallStatus::COMPILE_REQUIRED.0, 8);
        assert_eq!(JitCallStatus::RECOMPILE_REQUESTED.0, 9);
        assert!(JitCallStatus::RETURN.is_terminal_return());
        assert!(JitCallStatus::RETURN_REFERENCE.is_terminal_return());
        assert!(!JitCallStatus::THROW.is_terminal_return());
    }

    #[test]
    fn native_call_frame_and_generation_indirection_are_stable() {
        let mut frame = JitNativeCallFrame {
            function_id: 7,
            continuation_id: 11,
            ..JitNativeCallFrame::default()
        };
        frame.target.kind = JitNativeCallKind::METHOD;
        frame.argument_count = 2;
        assert_eq!(frame.abi_version, JIT_RUNTIME_ABI_VERSION);
        assert_eq!(frame.target.kind, JitNativeCallKind::METHOD);

        let argument = JitNativeCallArgument {
            flags: JitNativeArgFlags::NAMED.union(JitNativeArgFlags::BY_REFERENCE),
            source_slot: 3,
            ..JitNativeCallArgument::default()
        };
        assert_ne!(argument.flags.0 & JitNativeArgFlags::NAMED.0, 0);

        let entry = JitNativeIndirectionEntry::new(7);
        assert_eq!(entry.function_id(), 7);
        assert_eq!(entry.resolve(1), None);
        entry.publish(1, 0x1234);
        assert_eq!(entry.resolve(1), Some(0x1234));
        assert_eq!(entry.resolve(2), None);
        entry.publish(2, 0x5678);
        assert_eq!(entry.resolve(1), None);
        assert_eq!(entry.resolve(2), Some(0x5678));
    }

    #[test]
    fn c_abi_values_encode_scalars_and_opaque_handles() {
        assert_eq!(JitCValue::null().tag, JitCValueTag::Null);
        assert_eq!(JitCValue::bool(true).payload, 1);
        assert_eq!(JitCValue::int(-1).payload, u64::MAX);
        assert_eq!(
            JitCValue::float_bits(1.5f64.to_bits()).payload,
            1.5f64.to_bits()
        );

        let handle = JitOpaqueHandle::new(77).expect("non-zero handle");
        let opaque = JitCValue::opaque(JitOpaqueValueKind::Array, handle);
        assert_eq!(opaque.tag, JitCValueTag::OpaqueArray);
        assert_eq!(opaque.payload, 77);
    }

    #[test]
    fn c_frame_and_exit_records_do_not_expose_rust_references() {
        let context = JitVmContextHandle::new(1).expect("context");
        let frame = JitFrameHandle::new(2).expect("frame");
        let view = JitFrameView::new(context, frame, FunctionId::new(3), 4, 5);
        let c_view = JitCFrameView::from(view);

        assert_eq!(c_view.context, 1);
        assert_eq!(c_view.frame, 2);
        assert_eq!(c_view.function, 3);
        assert_eq!(c_view.register_count, 4);
        assert_eq!(c_view.local_count, 5);
        assert_eq!(c_view.reserved, 0);

        let exit = JitCExit::bailout(9, JitCValue::int(42)).with_native_continuation(7, 8);
        assert_eq!(exit.tag, JitCExitTag::Bailout);
        assert_eq!(exit.reason_code, 9);
        assert_eq!(exit.continuation_id, 7);
        assert_eq!(exit.source_position, 8);
    }

    #[test]
    fn side_exit_reasons_have_stable_report_codes_and_resume_metadata() {
        assert_eq!(SideExitReason::TypeMismatch.as_str(), "type_mismatch");
        assert_eq!(SideExitReason::Overflow.as_str(), "overflow");
        assert_eq!(
            SideExitReason::UnsupportedValue.as_str(),
            "unsupported_value"
        );
        assert_eq!(SideExitReason::GuardFailed.as_str(), "guard_failed");
        assert_eq!(SideExitReason::HelperStatus.as_str(), "helper_status");
        assert_eq!(
            SideExitReason::ExceptionPending.as_str(),
            "exception_pending"
        );
        assert_eq!(SideExitReason::AbiMismatch.as_str(), "abi_mismatch");
        assert_eq!(SideExitReason::HelperStatus.code(), 5);

        let metadata = JitSideExit::new(SideExitReason::HelperStatus)
            .with_status(1)
            .with_native_continuation(2, 3);
        assert_eq!(metadata.reason, SideExitReason::HelperStatus);
        assert_eq!(metadata.status_code, Some(1));
        assert_eq!(metadata.continuation_id, Some(2));
        assert_eq!(metadata.source_position, Some(3));

        let exit = JitCExit::side_exit(SideExitReason::HelperStatus, JitCValue::null());
        assert_eq!(exit.tag, JitCExitTag::Bailout);
        assert_eq!(exit.reason_code, SideExitReason::HelperStatus.code());
    }

    #[test]
    fn deopt_state_tracks_locals_across_all_mask_words() {
        let mut state = JitDeoptState::default();
        for index in [0, 63, 64, 125, 191, 255] {
            let local = LocalId::new(index);
            assert!(!state.local_initialized(local));
            state.mark_local_initialized(local);
            assert!(state.local_initialized(local));
        }
        assert!(!state.local_initialized(LocalId::new(256)));
    }
}
