//! Reference, slot, and temporary-value scaffolding for runtime semantics.
//!
//! The VM should not pass `Rc<RefCell<Value>>` through public APIs. This module
//! keeps the shared storage private behind `ReferenceCell` and keeps local-slot
//! aliasing explicit through `Slot`. Temporaries are represented by `TempValue`
//! so register values cannot accidentally become reference aliases.

use crate::{
    ArrayKey, Value,
    layout_stats::{
        SOURCE_BY_REF_ARGUMENT_BINDING, SOURCE_REFERENCE_DEREFERENCE,
        SOURCE_STACK_REGISTER_LOCAL_MOVE, enter_default_layout_source_family,
    },
    object::ObjectRef,
};
use std::cell::{BorrowError, BorrowMutError, Cell, Ref, RefCell};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_REFERENCE_CELL_ID: AtomicU64 = AtomicU64::new(1);

/// ABI version for the scalar-only native reference view.
///
/// The view deliberately publishes only immediate encoded values. Opaque
/// handles remain owned by the VM value table and must use the typed reference
/// slow path so reference replacement cannot delay PHP-visible destruction or
/// outlive a reused value-table slot.
pub const NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION: u32 = 1;
/// The scalar view has no currently published immediate value.
pub const NATIVE_REFERENCE_SCALAR_VIEW_EMPTY: u32 = 0;
/// The scalar view contains a valid immediate encoded value.
pub const NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED: u32 = 1;
/// Native code replaced the reference with an immediate integer.
pub const NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_INT: u32 = 2;
/// Native code replaced the reference with PHP null.
pub const NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_NULL: u32 = 3;
/// Native code replaced the reference with PHP false.
pub const NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_FALSE: u32 = 4;
/// Native code replaced the reference with PHP true.
pub const NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_TRUE: u32 = 5;
/// Native code replaced the reference with the internal uninitialized value.
pub const NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_UNINITIALIZED: u32 = 6;
/// ABI version for a reference-owned, read-only array `isset` view.
pub const NATIVE_REFERENCE_ARRAY_VIEW_ABI_VERSION: u32 = 2;
pub const NATIVE_REFERENCE_ARRAY_VIEW_EMPTY: u32 = 0;
pub const NATIVE_REFERENCE_ARRAY_VIEW_PUBLISHED: u32 = 1;
pub const NATIVE_REFERENCE_ARRAY_KEY_INT: u32 = 1;
pub const NATIVE_REFERENCE_ARRAY_KEY_STRING: u32 = 2;
pub const NATIVE_REFERENCE_ARRAY_VALUE_UNSUPPORTED: u32 = 0;
pub const NATIVE_REFERENCE_ARRAY_VALUE_NULL: u32 = 1;
pub const NATIVE_REFERENCE_ARRAY_VALUE_UNINITIALIZED: u32 = 2;
pub const NATIVE_REFERENCE_ARRAY_VALUE_FALSE: u32 = 3;
pub const NATIVE_REFERENCE_ARRAY_VALUE_TRUE: u32 = 4;
pub const NATIVE_REFERENCE_ARRAY_VALUE_INT: u32 = 5;
pub const NATIVE_REFERENCE_ARRAY_VALUE_STRING: u32 = 6;

/// One key and PHP-nullness record in a reference-owned array view.
///
/// String bytes remain owned by the immutable array value in the reference
/// cell. Every mutation invalidates the view before that value can move or be
/// released, so generated code never observes stale pointers.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeReferenceArrayEntry {
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

/// Stable descriptor for direct native `isset($reference[$key])` reads.
#[repr(C)]
#[derive(Debug)]
pub struct NativeReferenceArrayView {
    pub abi_version: u32,
    pub state: Cell<u32>,
    pub length: Cell<u64>,
    pub entries: Cell<u64>,
    pub storage_refcount: Cell<u64>,
    pub dirty: Cell<u32>,
    pub reserved: u32,
}

impl Default for NativeReferenceArrayView {
    fn default() -> Self {
        Self {
            abi_version: NATIVE_REFERENCE_ARRAY_VIEW_ABI_VERSION,
            state: Cell::new(NATIVE_REFERENCE_ARRAY_VIEW_EMPTY),
            length: Cell::new(0),
            entries: Cell::new(0),
            storage_refcount: Cell::new(0),
            dirty: Cell::new(0),
            reserved: 0,
        }
    }
}

/// Stable, versioned view that native code may inspect without depending on
/// `ReferenceStorage`, `RefCell`, or `Value` layout.
#[repr(C)]
#[derive(Debug)]
pub struct NativeReferenceScalarView {
    /// Layout/meaning version for this descriptor.
    pub abi_version: u32,
    /// One of the `NATIVE_REFERENCE_SCALAR_VIEW_*` states.
    pub state: Cell<u32>,
    /// Encoded null, bool, int, or uninitialized value when published.
    pub encoded: Cell<i64>,
}

impl Default for NativeReferenceScalarView {
    fn default() -> Self {
        Self {
            abi_version: NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
            state: Cell::new(NATIVE_REFERENCE_SCALAR_VIEW_EMPTY),
            encoded: Cell::new(0),
        }
    }
}

fn next_reference_cell_id() -> u64 {
    NEXT_REFERENCE_CELL_ID.fetch_add(1, Ordering::Relaxed)
}

/// Shared cell used for the simple local-reference MVP.
#[derive(Clone, Debug)]
pub struct ReferenceCell {
    inner: Rc<ReferenceStorage>,
}

#[derive(Debug)]
struct ReferenceStorage {
    id: u64,
    value: RefCell<Value>,
    native_scalar: NativeReferenceScalarView,
    native_array: NativeReferenceArrayView,
    native_array_entries: RefCell<Vec<NativeReferenceArrayEntry>>,
}

/// Weak debug handle to reference-cell storage for GC tests.
#[derive(Clone, Debug)]
pub struct WeakReferenceHandle {
    id: u64,
    inner: Weak<ReferenceStorage>,
}

impl WeakReferenceHandle {
    /// Returns the process-local debug ID for this handle.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Returns true when the reference cell is still alive.
    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.inner.strong_count() > 0
    }

    /// Upgrades this weak handle into a reference cell when still alive.
    #[must_use]
    pub fn upgrade(&self) -> Option<ReferenceCell> {
        self.inner.upgrade().map(|inner| ReferenceCell { inner })
    }
}

impl ReferenceCell {
    /// Creates a reference cell containing `value`.
    #[must_use]
    pub fn new(value: Value) -> Self {
        crate::layout_stats::record_reference_cell_creation();
        Self {
            inner: Rc::new(ReferenceStorage {
                id: next_reference_cell_id(),
                value: RefCell::new(value),
                native_scalar: NativeReferenceScalarView::default(),
                native_array: NativeReferenceArrayView::default(),
                native_array_entries: RefCell::new(Vec::new()),
            }),
        }
    }

    /// Reads the contained value by cloning it.
    #[must_use]
    pub fn get(&self) -> Value {
        self.materialize_native_scalar_overlay();
        self.materialize_native_array_overlay();
        let _source = enter_default_layout_source_family(SOURCE_REFERENCE_DEREFERENCE);
        self.inner.value.borrow().clone()
    }

    /// Attempts to read the contained value by cloning it.
    ///
    /// This checked accessor is preferred outside low-level runtime internals.
    /// It returns `Err` if another caller currently holds a mutable borrow.
    pub fn try_get(&self) -> Result<Value, BorrowError> {
        self.materialize_native_scalar_overlay();
        self.materialize_native_array_overlay();
        let _source = enter_default_layout_source_family(SOURCE_REFERENCE_DEREFERENCE);
        self.inner.value.try_borrow().map(|value| value.clone())
    }

    /// Borrows the contained value for read-only inspection.
    ///
    /// This can panic if the same reference cell is already mutably borrowed.
    /// Prefer [`Self::try_get`] or [`Self::try_with_value`] outside VM-internal
    /// paths that already control borrow ordering.
    #[doc(hidden)]
    #[must_use]
    pub fn borrow(&self) -> Ref<'_, Value> {
        self.materialize_native_scalar_overlay();
        self.materialize_native_array_overlay();
        self.inner.value.borrow()
    }

    /// Runs `f` with a checked immutable borrow of the contained value.
    pub fn try_with_value<T>(&self, f: impl FnOnce(&Value) -> T) -> Result<T, BorrowError> {
        self.materialize_native_scalar_overlay();
        self.materialize_native_array_overlay();
        self.inner.value.try_borrow().map(|value| f(&value))
    }

    /// Runs `f` with a checked mutable borrow of the contained value.
    ///
    /// In-place mutation preserves the cell's copy-on-write handle instead of
    /// forcing a clone/separate/write-back cycle; callers fall back to the
    /// clone-based path when the cell is already borrowed.
    pub fn try_with_value_mut<T>(
        &self,
        f: impl FnOnce(&mut Value) -> T,
    ) -> Result<T, BorrowMutError> {
        self.materialize_native_scalar_overlay();
        self.materialize_native_array_overlay();
        let result = {
            let mut value = self.inner.value.try_borrow_mut()?;
            self.invalidate_native_scalar();
            f(&mut value)
        };
        self.publish_native_array_view();
        Ok(result)
    }

    /// Replaces the contained value.
    pub fn set(&self, value: Value) {
        self.inner.native_array.dirty.set(0);
        self.invalidate_native_scalar();
        {
            *self.inner.value.borrow_mut() = value;
        }
        self.publish_native_array_view();
    }

    /// Attempts to replace the contained value.
    ///
    /// This checked accessor is preferred outside low-level runtime internals.
    /// It returns `Err` if another caller currently holds a borrow.
    pub fn try_set(&self, value: Value) -> Result<(), BorrowMutError> {
        self.inner.native_array.dirty.set(0);
        {
            let mut slot = self.inner.value.try_borrow_mut()?;
            self.invalidate_native_scalar();
            *slot = value;
        }
        self.publish_native_array_view();
        Ok(())
    }

    /// Returns the address of the stable scalar-only native view.
    ///
    /// The reference cell owns this address for its full lifetime. Native code
    /// reaches it only through a request-owned, versioned VM descriptor that
    /// also keeps this reference cell alive.
    #[doc(hidden)]
    #[must_use]
    pub fn native_scalar_view_address(&self) -> usize {
        std::ptr::from_ref(&self.inner.native_scalar) as usize
    }

    /// Returns a stable descriptor for direct read-only array `isset` access.
    /// Publication is rebuilt only after mutation invalidates the cell.
    #[doc(hidden)]
    #[must_use]
    pub fn native_array_view_address(&self) -> usize {
        self.publish_native_array_view();
        std::ptr::from_ref(&self.inner.native_array) as usize
    }

    /// Publishes one immediate encoded value for native reference reads.
    /// Opaque runtime handles must never be passed here.
    #[doc(hidden)]
    pub fn publish_native_scalar(&self, encoded: i64) {
        self.inner.native_scalar.encoded.set(encoded);
        self.inner
            .native_scalar
            .state
            .set(NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED);
    }

    fn invalidate_native_scalar(&self) {
        self.inner
            .native_scalar
            .state
            .set(NATIVE_REFERENCE_SCALAR_VIEW_EMPTY);
        self.inner
            .native_array
            .state
            .set(NATIVE_REFERENCE_ARRAY_VIEW_EMPTY);
    }

    fn publish_native_array_view(&self) {
        self.materialize_native_scalar_overlay();
        if self.inner.native_array.state.get() == NATIVE_REFERENCE_ARRAY_VIEW_PUBLISHED {
            return;
        }
        let Ok(value) = self.inner.value.try_borrow() else {
            return;
        };
        let Value::Array(array) = &*value else {
            return;
        };
        let mut entries = self.inner.native_array_entries.borrow_mut();
        entries.clear();
        entries.reserve(array.len());
        for (key, value) in array.iter() {
            let Some(non_null) = native_isset_value(value) else {
                entries.clear();
                return;
            };
            let (value_kind, value_flags, value_payload, value_length, value_bytes) =
                native_array_value_view(value);
            let entry = match key {
                ArrayKey::Int(integer) => NativeReferenceArrayEntry {
                    kind: NATIVE_REFERENCE_ARRAY_KEY_INT,
                    non_null: u32::from(non_null),
                    integer,
                    string_length: 0,
                    string_bytes: 0,
                    value_kind,
                    value_flags,
                    value_payload,
                    value_length,
                    value_bytes,
                },
                ArrayKey::String(string) => NativeReferenceArrayEntry {
                    kind: NATIVE_REFERENCE_ARRAY_KEY_STRING,
                    non_null: u32::from(non_null),
                    integer: 0,
                    string_length: string.len() as u64,
                    string_bytes: string.as_bytes().as_ptr() as usize as u64,
                    value_kind,
                    value_flags,
                    value_payload,
                    value_length,
                    value_bytes,
                },
            };
            entries.push(entry);
        }
        self.inner
            .native_array
            .entries
            .set(entries.as_ptr() as usize as u64);
        self.inner.native_array.length.set(entries.len() as u64);
        self.inner
            .native_array
            .storage_refcount
            .set(array.native_storage_refcount_address() as u64);
        self.inner
            .native_array
            .state
            .set(NATIVE_REFERENCE_ARRAY_VIEW_PUBLISHED);
    }

    fn materialize_native_array_overlay(&self) {
        if self.inner.native_array.dirty.get() == 0 {
            return;
        }
        self.inner
            .native_array
            .state
            .set(NATIVE_REFERENCE_ARRAY_VIEW_EMPTY);
        let entries = self.inner.native_array_entries.borrow().clone();
        {
            let mut stored = self.inner.value.borrow_mut();
            let Value::Array(array) = &mut *stored else {
                self.inner.native_array.dirty.set(0);
                return;
            };
            let original = array
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<Vec<_>>();
            for ((key, original), entry) in original.into_iter().zip(entries) {
                let value = match entry.value_kind {
                    NATIVE_REFERENCE_ARRAY_VALUE_NULL => Value::Null,
                    NATIVE_REFERENCE_ARRAY_VALUE_UNINITIALIZED => Value::Uninitialized,
                    NATIVE_REFERENCE_ARRAY_VALUE_FALSE => Value::Bool(false),
                    NATIVE_REFERENCE_ARRAY_VALUE_TRUE => Value::Bool(true),
                    NATIVE_REFERENCE_ARRAY_VALUE_INT => Value::Int(entry.value_payload),
                    NATIVE_REFERENCE_ARRAY_VALUE_STRING => original,
                    _ => continue,
                };
                array.insert(key, value);
            }
        }
        self.inner.native_array.dirty.set(0);
        self.publish_native_array_view();
    }

    /// Commits an immediate scalar written by native code into the cold Rust
    /// value only when a cold semantic consumer crosses that boundary.
    /// Native reads continue to use the encoded scalar in the stable view.
    fn materialize_native_scalar_overlay(&self) {
        let state = self.inner.native_scalar.state.get();
        let value = match state {
            NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_INT => {
                Value::Int(self.inner.native_scalar.encoded.get())
            }
            NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_NULL => Value::Null,
            NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_FALSE => Value::Bool(false),
            NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_TRUE => Value::Bool(true),
            NATIVE_REFERENCE_SCALAR_VIEW_DIRTY_UNINITIALIZED => Value::Uninitialized,
            _ => return,
        };
        self.inner.native_array.dirty.set(0);
        self.inner
            .native_array
            .state
            .set(NATIVE_REFERENCE_ARRAY_VIEW_EMPTY);
        *self.inner.value.borrow_mut() = value;
        self.inner
            .native_scalar
            .state
            .set(NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED);
    }

    /// Returns true when both cells point at the same shared storage.
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }

    /// Returns a process-local cell identity for GC debug snapshots.
    ///
    /// This is not a PHP-visible handle and must only be used by runtime tests
    /// and diagnostics.
    #[must_use]
    pub fn gc_debug_id(&self) -> u64 {
        self.inner.id
    }

    /// Returns the current `Rc` strong count for GC debug metadata.
    #[must_use]
    pub fn gc_refcount_estimate(&self) -> usize {
        Rc::strong_count(&self.inner)
    }

    /// Returns a weak debug handle for GC tests.
    #[must_use]
    pub fn weak_handle(&self) -> WeakReferenceHandle {
        WeakReferenceHandle {
            id: self.gc_debug_id(),
            inner: Rc::downgrade(&self.inner),
        }
    }

    /// Clears this cell as an internal GC action.
    ///
    /// This is not PHP-visible `unset()` semantics; it is only used by the
    /// runtime-semantics cycle-collection test hook after proving the cell is not rooted.
    pub fn gc_clear(&self) {
        self.set(Value::Uninitialized);
    }
}

fn native_array_value_view(value: &Value) -> (u32, u32, i64, u64, u64) {
    match value {
        Value::Null => (NATIVE_REFERENCE_ARRAY_VALUE_NULL, 0, 0, 0, 0),
        Value::Uninitialized => (NATIVE_REFERENCE_ARRAY_VALUE_UNINITIALIZED, 0, 0, 0, 0),
        Value::Bool(false) => (NATIVE_REFERENCE_ARRAY_VALUE_FALSE, 0, 0, 0, 0),
        Value::Bool(true) => (NATIVE_REFERENCE_ARRAY_VALUE_TRUE, 0, 0, 0, 0),
        Value::Int(value) if !matches!(((*value as u64) >> 48) as u16, 0x7ff1 | 0x7ff2) => {
            (NATIVE_REFERENCE_ARRAY_VALUE_INT, 0, *value, 0, 0)
        }
        Value::String(value) => (
            NATIVE_REFERENCE_ARRAY_VALUE_STRING,
            u32::from(value.as_bytes() == b"0"),
            0,
            value.len() as u64,
            value.as_bytes().as_ptr() as usize as u64,
        ),
        _ => (NATIVE_REFERENCE_ARRAY_VALUE_UNSUPPORTED, 0, 0, 0, 0),
    }
}

fn native_isset_value(value: &Value) -> Option<bool> {
    match value {
        Value::Null | Value::Uninitialized => Some(false),
        Value::Reference(reference) => reference
            .inner
            .value
            .try_borrow()
            .ok()
            .and_then(|value| native_isset_value(&value)),
        _ => Some(true),
    }
}

impl Eq for ReferenceCell {}

impl PartialEq for ReferenceCell {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other)
    }
}

/// Runtime storage slot for variables, properties, and array elements.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Slot {
    /// Ordinary by-value storage.
    Value(Value),
    /// Alias to a shared reference cell.
    Reference(ReferenceCell),
}

impl Slot {
    /// Creates an ordinary slot initialized to PHP `null`.
    #[must_use]
    pub const fn null() -> Self {
        Self::Value(Value::Null)
    }

    /// Creates an ordinary value slot.
    #[must_use]
    pub const fn value(value: Value) -> Self {
        Self::Value(value)
    }

    /// Creates an uninitialized ordinary slot.
    #[must_use]
    pub const fn uninitialized() -> Self {
        Self::Value(Value::Uninitialized)
    }

    /// Reads the effective value. Reference slots dereference their cell.
    #[must_use]
    pub fn read(&self) -> Value {
        match self {
            Self::Value(value) => {
                let _source = enter_default_layout_source_family(SOURCE_STACK_REGISTER_LOCAL_MOVE);
                value.clone()
            }
            Self::Reference(cell) => cell.get(),
        }
    }

    /// Reads the effective value for PHP value reads.
    #[must_use]
    pub fn read_value(&self) -> Value {
        self.read()
    }

    /// Returns true when the effective value is uninitialized.
    #[must_use]
    pub fn is_uninitialized(&self) -> bool {
        self.read().is_uninitialized()
    }

    /// Writes through the slot. Reference slots update the shared cell.
    pub fn write(&mut self, value: Value) {
        match self {
            Self::Value(slot) => *slot = value,
            Self::Reference(cell) => cell.set(value),
        }
    }

    /// Runs `f` with in-place mutable access to the effective value, so
    /// copy-on-write containers are not forced to separate by a transient
    /// read clone. Returns `None` when a reference cell is already borrowed;
    /// callers then take the clone-based read/write path.
    pub fn try_with_effective_value_mut<T>(
        &mut self,
        f: impl FnOnce(&mut Value) -> T,
    ) -> Option<T> {
        match self {
            Self::Value(value) => Some(f(value)),
            Self::Reference(cell) => cell.try_with_value_mut(f).ok(),
        }
    }

    /// Writes the PHP value through this slot.
    pub fn write_value(&mut self, value: Value) {
        self.write(value);
    }

    /// Unsets this slot name without mutating an aliased reference cell.
    pub fn unset(&mut self) {
        *self = Self::uninitialized();
    }

    /// Converts an ordinary slot into a reference cell or returns its existing
    /// cell. This is the only runtime operation that creates local aliases.
    pub fn ensure_reference_cell(&mut self) -> ReferenceCell {
        match self {
            Self::Value(value) => {
                let _source = enter_default_layout_source_family(SOURCE_BY_REF_ARGUMENT_BINDING);
                let cell = ReferenceCell::new(value.clone());
                *self = Self::Reference(cell.clone());
                cell
            }
            Self::Reference(cell) => cell.clone(),
        }
    }

    /// Binds this slot to an existing reference cell.
    pub fn bind_reference(&mut self, cell: ReferenceCell) {
        *self = Self::Reference(cell);
    }

    /// Returns true when two slots are aliases of the same reference cell.
    ///
    /// This is for tests, tracing, and conservative optimization guards. It is
    /// not a PHP-visible identity operation.
    #[must_use]
    pub fn aliases(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Reference(left), Self::Reference(right)) => left.ptr_eq(right),
            _ => false,
        }
    }
}

/// Backwards-compatible exported name for runtime slot users.
pub type ValueSlot = Slot;

/// Backwards-compatible exported name for earlier placeholder references.
pub type ReferencePlaceholder = ReferenceCell;

/// Resolved assignable runtime location kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LvalueKind {
    LocalVariable,
    GlobalVariable,
    ArrayElement,
    ArrayAppendElement,
    ObjectProperty,
    StaticLocal,
    StaticProperty,
}

impl LvalueKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalVariable => "local_variable",
            Self::GlobalVariable => "global_variable",
            Self::ArrayElement => "array_element",
            Self::ArrayAppendElement => "array_append_element",
            Self::ObjectProperty => "object_property",
            Self::StaticLocal => "static_local",
            Self::StaticProperty => "static_property",
        }
    }
}

enum LvalueTarget<'a> {
    Slot(&'a mut Slot),
    Value(&'a mut Value),
    Cell(ReferenceCell),
    ObjectProperty { object: ObjectRef, name: String },
}

/// Runtime lvalue facade for PHP-visible storage locations.
///
/// This type keeps PHP read/write/reference operations centralized without
/// exposing `Rc<RefCell<Value>>` to VM instructions. It intentionally models
/// only resolved storage; name lookup, visibility, property hooks, and
/// diagnostic routing stay in the VM/frontend layers that already own them.
pub struct Lvalue<'a> {
    kind: LvalueKind,
    target: LvalueTarget<'a>,
}

impl<'a> Lvalue<'a> {
    /// Creates an lvalue over a variable slot.
    pub fn slot(slot: &'a mut Slot, kind: LvalueKind) -> Self {
        Self {
            kind,
            target: LvalueTarget::Slot(slot),
        }
    }

    /// Creates an lvalue over a direct value field such as an array element or
    /// static-property table entry.
    pub fn value(value: &'a mut Value, kind: LvalueKind) -> Self {
        Self {
            kind,
            target: LvalueTarget::Value(value),
        }
    }

    /// Creates an lvalue over an existing shared reference cell.
    #[must_use]
    pub fn cell(cell: ReferenceCell, kind: LvalueKind) -> Self {
        Self {
            kind,
            target: LvalueTarget::Cell(cell),
        }
    }

    /// Creates an lvalue over object property storage.
    #[must_use]
    pub fn object_property(object: ObjectRef, name: impl Into<String>, kind: LvalueKind) -> Self {
        Self {
            kind,
            target: LvalueTarget::ObjectProperty {
                object,
                name: name.into(),
            },
        }
    }

    /// Returns the lvalue kind.
    #[must_use]
    pub const fn kind(&self) -> LvalueKind {
        self.kind
    }

    /// Reads the effective PHP value.
    #[must_use]
    pub fn read_value(&self) -> Value {
        match &self.target {
            LvalueTarget::Slot(slot) => slot.read_value(),
            LvalueTarget::Value(value) => deref_value_for_read(value),
            LvalueTarget::Cell(cell) => cell.get(),
            LvalueTarget::ObjectProperty { object, name } => object
                .get_property(name)
                .map(deref_owned_value_for_read)
                .unwrap_or(Value::Uninitialized),
        }
    }

    /// Writes a PHP value through this lvalue.
    pub fn write_value(&mut self, value: Value) -> Result<(), LvalueError> {
        match &mut self.target {
            LvalueTarget::Slot(slot) => slot.write_value(value),
            LvalueTarget::Value(target) => write_value(target, value),
            LvalueTarget::Cell(cell) => cell.set(value),
            LvalueTarget::ObjectProperty { object, name } => match object.get_property(name) {
                Some(Value::Reference(cell)) => cell.set(value),
                _ => object.set_property(name.clone(), value),
            },
        }
        Ok(())
    }

    /// Converts this lvalue to reference storage and returns the shared cell.
    pub fn ensure_reference_cell(&mut self) -> Result<ReferenceCell, LvalueError> {
        match &mut self.target {
            LvalueTarget::Slot(slot) => Ok(slot.ensure_reference_cell()),
            LvalueTarget::Value(value) => Ok(ensure_value_reference_cell(value)),
            LvalueTarget::Cell(cell) => Ok(cell.clone()),
            LvalueTarget::ObjectProperty { object, name } => {
                let current = object.get_property(name).unwrap_or(Value::Uninitialized);
                if let Value::Reference(cell) = current {
                    return Ok(cell);
                }
                let cell = ReferenceCell::new(current);
                object.set_property(name.clone(), Value::Reference(cell.clone()));
                Ok(cell)
            }
        }
    }

    /// Binds this lvalue to an existing reference cell when the lvalue storage
    /// can itself be rebound.
    pub fn bind_reference_cell(&mut self, cell: ReferenceCell) -> Result<(), LvalueError> {
        match &mut self.target {
            LvalueTarget::Slot(slot) => slot.bind_reference(cell),
            LvalueTarget::Value(value) => **value = Value::Reference(cell),
            LvalueTarget::ObjectProperty { object, name } => {
                object.set_property(name.clone(), Value::Reference(cell));
            }
            LvalueTarget::Cell(_) => {
                return Err(LvalueError::CannotRebindCell { kind: self.kind });
            }
        }
        Ok(())
    }

    /// Unsets this lvalue without corrupting still-live aliases.
    pub fn unset(&mut self) -> Result<(), LvalueError> {
        match &mut self.target {
            LvalueTarget::Slot(slot) => slot.unset(),
            LvalueTarget::Value(value) => **value = Value::Uninitialized,
            LvalueTarget::Cell(cell) => cell.set(Value::Uninitialized),
            LvalueTarget::ObjectProperty { object, name } => {
                object.unset_property(name);
            }
        }
        Ok(())
    }
}

/// Lvalue operation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LvalueError {
    CannotRebindCell { kind: LvalueKind },
}

impl std::fmt::Display for LvalueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CannotRebindCell { kind } => write!(
                f,
                "E_PHP_RUNTIME_LVALUE_REBIND_CELL: cannot rebind {} cell storage",
                kind.as_str()
            ),
        }
    }
}

impl std::error::Error for LvalueError {}

fn deref_value_for_read(value: &Value) -> Value {
    match value {
        Value::Reference(cell) => cell.get(),
        value => {
            let _source = enter_default_layout_source_family(SOURCE_STACK_REGISTER_LOCAL_MOVE);
            value.clone()
        }
    }
}

fn deref_owned_value_for_read(value: Value) -> Value {
    match value {
        Value::Reference(cell) => cell.get(),
        value => value,
    }
}

fn write_value(target: &mut Value, value: Value) {
    match target {
        Value::Reference(cell) => cell.set(value),
        target => *target = value,
    }
}

fn ensure_value_reference_cell(value: &mut Value) -> ReferenceCell {
    match value {
        Value::Reference(cell) => cell.clone(),
        value => {
            let _source = enter_default_layout_source_family(SOURCE_BY_REF_ARGUMENT_BINDING);
            let cell = ReferenceCell::new(value.clone());
            *value = Value::Reference(cell.clone());
            cell
        }
    }
}

/// VM temporary value.
///
/// Temporaries are snapshots of effective values. If a reference value is
/// written into a temporary, the cell is dereferenced immediately so the temp
/// cannot become a writable alias.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TempValue {
    value: Value,
}

impl TempValue {
    /// Creates a temporary from an effective PHP value.
    #[must_use]
    pub fn new(value: Value) -> Self {
        let value = match value {
            Value::Reference(cell) => cell.get(),
            value => value,
        };
        Self { value }
    }

    /// Creates an uninitialized temporary.
    #[must_use]
    pub const fn uninitialized() -> Self {
        Self {
            value: Value::Uninitialized,
        }
    }

    /// Reads the temporary value.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// Mutates the temporary's private value.
    ///
    /// This never writes through a `ReferenceCell`; temporaries are not
    /// referenceable storage locations.
    pub fn value_mut(&mut self) -> &mut Value {
        &mut self.value
    }

    /// Replaces the temporary value, dereferencing reference cells first.
    pub fn set(&mut self, value: Value) {
        *self = Self::new(value);
    }

    /// Consumes the temporary into its effective value.
    #[must_use]
    pub fn into_value(self) -> Value {
        self.value
    }
}

impl From<Value> for TempValue {
    fn from(value: Value) -> Self {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Lvalue, LvalueKind, NATIVE_REFERENCE_ARRAY_KEY_STRING, NATIVE_REFERENCE_ARRAY_VALUE_INT,
        NATIVE_REFERENCE_ARRAY_VIEW_ABI_VERSION, NATIVE_REFERENCE_ARRAY_VIEW_EMPTY,
        NATIVE_REFERENCE_ARRAY_VIEW_PUBLISHED, NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
        NATIVE_REFERENCE_SCALAR_VIEW_EMPTY, NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED,
        NativeReferenceScalarView, ReferenceCell, Slot, TempValue,
    };
    use crate::{ArrayKey, ClassEntry, ClassFlags, ObjectRef, PhpArray, Value};

    fn empty_class(name: &str) -> ClassEntry {
        ClassEntry {
            name: name.to_owned().into(),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        }
    }

    #[test]
    fn reference_cell_aliases_updates() {
        let cell = ReferenceCell::new(Value::Int(1));
        let alias = cell.clone();

        alias.set(Value::Int(2));

        assert_eq!(cell.get(), Value::Int(2));
        assert!(cell.ptr_eq(&alias));
    }

    #[test]
    fn reference_cell_checked_accessors_preserve_aliasing() {
        let cell = ReferenceCell::new(Value::Int(1));
        let alias = cell.clone();

        alias.try_set(Value::Int(4)).expect("checked write");

        assert_eq!(cell.try_get().expect("checked read"), Value::Int(4));
        assert_eq!(
            cell.try_with_value(|value| value.clone())
                .expect("checked closure read"),
            Value::Int(4)
        );
    }

    #[test]
    fn value_slot_writes_through_reference_cells() {
        let mut left = Slot::value(Value::Int(1));
        let cell = left.ensure_reference_cell();
        let mut right = Slot::uninitialized();
        right.bind_reference(cell);

        right.write(Value::Int(3));

        assert_eq!(left.read(), Value::Int(3));
        assert_eq!(right.read(), Value::Int(3));
        assert!(left.aliases(&right));
    }

    #[test]
    fn slot_alias_and_copy_semantics_are_distinct() {
        let mut original = Slot::value(Value::Int(1));
        let mut copy = Slot::value(original.read());

        copy.write(Value::Int(2));
        assert_eq!(original.read(), Value::Int(1));
        assert_eq!(copy.read(), Value::Int(2));

        let cell = original.ensure_reference_cell();
        let mut alias = Slot::uninitialized();
        alias.bind_reference(cell);
        alias.write(Value::Int(3));

        assert_eq!(original.read(), Value::Int(3));
        assert_eq!(alias.read(), Value::Int(3));
        assert!(original.aliases(&alias));
    }

    #[test]
    fn lvalue_slot_write_bind_and_unset_preserve_alias_model() {
        let mut left = Slot::null();
        let mut right = Slot::value(Value::Int(1));
        let cell = Lvalue::slot(&mut right, LvalueKind::LocalVariable)
            .ensure_reference_cell()
            .expect("reference cell");

        Lvalue::slot(&mut left, LvalueKind::LocalVariable)
            .bind_reference_cell(cell)
            .expect("bind reference");
        Lvalue::slot(&mut left, LvalueKind::LocalVariable)
            .write_value(Value::Int(9))
            .expect("write through lvalue");

        assert_eq!(right.read(), Value::Int(9));
        assert!(left.aliases(&right));

        Lvalue::slot(&mut left, LvalueKind::LocalVariable)
            .unset()
            .expect("unset lvalue");
        assert_eq!(left.read(), Value::Uninitialized);
        assert_eq!(right.read(), Value::Int(9));
    }

    #[test]
    fn lvalue_array_element_converts_to_reference_cell() {
        let mut value = Value::Int(1);
        let cell = Lvalue::value(&mut value, LvalueKind::ArrayElement)
            .ensure_reference_cell()
            .expect("reference cell");

        cell.set(Value::Int(4));

        assert_eq!(
            Lvalue::value(&mut value, LvalueKind::ArrayElement).read_value(),
            Value::Int(4)
        );
    }

    #[test]
    fn lvalue_object_property_reference_writes_through_aliases() {
        let class = empty_class("Box");
        let object = ObjectRef::new(&class);
        object.set_property("value", Value::Int(1));
        let cell = Lvalue::object_property(object.clone(), "value", LvalueKind::ObjectProperty)
            .ensure_reference_cell()
            .expect("property reference cell");
        let mut local = Slot::uninitialized();

        local.bind_reference(cell);
        local.write(Value::Int(8));

        assert_eq!(
            object.get_property("value"),
            Some(Value::Reference(match local {
                Slot::Reference(ref cell) => cell.clone(),
                Slot::Value(_) => panic!("expected local reference"),
            }))
        );
        assert_eq!(
            Lvalue::object_property(object, "value", LvalueKind::ObjectProperty).read_value(),
            Value::Int(8)
        );
    }

    #[test]
    fn temp_values_snapshot_reference_cells_without_aliasing() {
        let cell = ReferenceCell::new(Value::Int(1));
        let mut temp = TempValue::new(Value::Reference(cell.clone()));

        cell.set(Value::Int(2));
        assert_eq!(temp.value(), &Value::Int(1));

        temp.set(Value::Int(3));
        assert_eq!(cell.get(), Value::Int(2));
        assert_eq!(temp.into_value(), Value::Int(3));
    }

    #[test]
    fn cow_array_reference_cells_write_through_aliases() {
        let mut array = crate::PhpArray::from_packed(vec![Value::Int(1)]);
        array.append(Value::Int(2));
        let cell = ReferenceCell::new(Value::Array(array));
        let mut left = Slot::Reference(cell.clone());
        let right = Slot::Reference(cell);

        let mut current = left.read();
        let Value::Array(ref mut array) = current else {
            panic!("expected array");
        };
        array.append(Value::Int(3));
        left.write(current);

        let Value::Array(array) = right.read() else {
            panic!("expected aliased array");
        };
        assert_eq!(array.packed_elements().expect("packed").len(), 3);
    }

    #[test]
    fn safe_model_reference_alias_matches_slot_model() {
        let cell = ReferenceCell::new(Value::Int(1));
        let mut left = Slot::Reference(cell.clone());
        let mut right = Slot::Reference(cell);
        let mut model_cell = 1;

        assert_eq!(left.read(), Value::Int(model_cell));
        assert_eq!(right.read(), Value::Int(model_cell));

        right.write(Value::Int(7));
        model_cell = 7;
        assert_eq!(left.read(), Value::Int(model_cell));
        assert_eq!(right.read(), Value::Int(model_cell));

        left.write(Value::Int(9));
        model_cell = 9;
        assert_eq!(left.read(), Value::Int(model_cell));
        assert_eq!(right.read(), Value::Int(model_cell));

        let mut by_value_copy = Slot::value(left.read());
        by_value_copy.write(Value::Int(11));
        assert_eq!(by_value_copy.read(), Value::Int(11));
        assert_eq!(left.read(), Value::Int(model_cell));
        assert_eq!(right.read(), Value::Int(model_cell));
    }

    #[test]
    fn safe_model_array_cow_matches_php_array_model() {
        let original_model = vec![1, 2];
        let mut copy_model = original_model.clone();
        let original =
            PhpArray::from_packed(original_model.iter().copied().map(Value::Int).collect());
        let mut copy = original.clone();

        copy.append(Value::Int(3));
        copy_model.push(3);

        assert_eq!(original.get(&ArrayKey::Int(2)), None);
        assert_eq!(
            copy.get(&ArrayKey::Int(2)),
            Some(&Value::Int(copy_model[2]))
        );
        assert_eq!(
            original
                .packed_elements()
                .expect("packed original")
                .into_iter()
                .cloned()
                .collect::<Vec<_>>(),
            original_model
                .into_iter()
                .map(Value::Int)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            copy.packed_elements()
                .expect("packed copy")
                .into_iter()
                .cloned()
                .collect::<Vec<_>>(),
            copy_model.into_iter().map(Value::Int).collect::<Vec<_>>()
        );
    }

    #[test]
    fn logical_cell_id_is_shared_by_aliases_and_unique_per_cell() {
        let cell = ReferenceCell::new(Value::Int(1));
        let alias = cell.clone();
        let independent = ReferenceCell::new(Value::Int(1));

        assert_eq!(cell.gc_debug_id(), alias.gc_debug_id());
        assert_ne!(cell.gc_debug_id(), independent.gc_debug_id());
    }

    #[test]
    fn native_scalar_view_is_versioned_stable_and_invalidated_by_mutation() {
        assert_eq!(std::mem::size_of::<NativeReferenceScalarView>(), 16);
        assert_eq!(
            std::mem::offset_of!(NativeReferenceScalarView, abi_version),
            0
        );
        assert_eq!(std::mem::offset_of!(NativeReferenceScalarView, state), 4);
        assert_eq!(std::mem::offset_of!(NativeReferenceScalarView, encoded), 8);

        let cell = ReferenceCell::new(Value::Int(1));
        let view = &cell.inner.native_scalar;
        assert_eq!(view.abi_version, NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION);
        assert_eq!(view.state.get(), NATIVE_REFERENCE_SCALAR_VIEW_EMPTY);

        cell.publish_native_scalar(41);
        assert_eq!(view.state.get(), NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED);
        assert_eq!(view.encoded.get(), 41);

        cell.set(Value::Int(42));
        assert_eq!(view.state.get(), NATIVE_REFERENCE_SCALAR_VIEW_EMPTY);
        cell.publish_native_scalar(42);
        cell.try_with_value_mut(|value| *value = Value::Int(43))
            .expect("mutable reference view");
        assert_eq!(view.state.get(), NATIVE_REFERENCE_SCALAR_VIEW_EMPTY);
    }

    #[test]
    fn native_array_view_is_published_once_and_invalidated_before_mutation() {
        assert_eq!(std::mem::size_of::<super::NativeReferenceArrayEntry>(), 64);
        assert_eq!(std::mem::size_of::<super::NativeReferenceArrayView>(), 40);
        let mut array = PhpArray::new();
        array.insert(
            ArrayKey::String(crate::PhpString::from_bytes(b"post_type".to_vec())),
            Value::Int(1),
        );
        let cell = ReferenceCell::new(Value::Array(array));
        let address = cell.native_array_view_address();
        let view = &cell.inner.native_array;
        assert_eq!(view.abi_version, NATIVE_REFERENCE_ARRAY_VIEW_ABI_VERSION);
        assert_eq!(view.state.get(), NATIVE_REFERENCE_ARRAY_VIEW_PUBLISHED);
        assert_eq!(view.length.get(), 1);
        assert_eq!(address, std::ptr::from_ref(view) as usize);
        let entry = cell.inner.native_array_entries.borrow()[0];
        assert_eq!(entry.kind, NATIVE_REFERENCE_ARRAY_KEY_STRING);
        assert_eq!(entry.non_null, 1);
        assert_eq!(entry.value_kind, NATIVE_REFERENCE_ARRAY_VALUE_INT);
        assert_eq!(entry.value_payload, 1);

        cell.inner.native_array_entries.borrow_mut()[0].value_payload = 42;
        view.dirty.set(1);
        let Value::Array(materialized) = cell.get() else {
            panic!("native overlay did not remain an array");
        };
        assert_eq!(
            materialized.get(&ArrayKey::String(crate::PhpString::from_bytes(
                b"post_type".to_vec()
            ))),
            Some(&Value::Int(42))
        );
        assert_eq!(view.dirty.get(), 0);
        assert_eq!(view.state.get(), NATIVE_REFERENCE_ARRAY_VIEW_PUBLISHED);

        cell.try_with_value_mut(|value| *value = Value::Null)
            .expect("reference mutation");
        assert_eq!(view.state.get(), NATIVE_REFERENCE_ARRAY_VIEW_EMPTY);
    }
}
