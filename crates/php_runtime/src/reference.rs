//! Reference, slot, and temporary-value scaffolding for runtime semantics.
//!
//! The VM should not pass `Rc<RefCell<Value>>` through public APIs. This module
//! keeps the shared storage private behind `ReferenceCell` and keeps local-slot
//! aliasing explicit through `Slot`. Temporaries are represented by `TempValue`
//! so register values cannot accidentally become reference aliases.

use crate::Value;
use std::cell::{BorrowError, BorrowMutError, Ref, RefCell};
use std::rc::{Rc, Weak};

/// Shared cell used for the simple local-reference MVP.
#[derive(Clone, Debug)]
pub struct ReferenceCell {
    inner: Rc<RefCell<Value>>,
}

/// Weak debug handle to reference-cell storage for GC tests.
#[derive(Clone, Debug)]
pub struct WeakReferenceHandle {
    id: usize,
    inner: Weak<RefCell<Value>>,
}

impl WeakReferenceHandle {
    /// Returns the process-local debug ID for this handle.
    #[must_use]
    pub const fn id(&self) -> usize {
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
            inner: Rc::new(RefCell::new(value)),
        }
    }

    /// Reads the contained value by cloning it.
    #[must_use]
    pub fn get(&self) -> Value {
        self.inner.borrow().clone()
    }

    /// Attempts to read the contained value by cloning it.
    ///
    /// This checked accessor is preferred outside low-level runtime internals.
    /// It returns `Err` if another caller currently holds a mutable borrow.
    pub fn try_get(&self) -> Result<Value, BorrowError> {
        self.inner.try_borrow().map(|value| value.clone())
    }

    /// Borrows the contained value for read-only inspection.
    ///
    /// This can panic if the same reference cell is already mutably borrowed.
    /// Prefer [`Self::try_get`] or [`Self::try_with_value`] outside VM-internal
    /// paths that already control borrow ordering.
    #[doc(hidden)]
    #[must_use]
    pub fn borrow(&self) -> Ref<'_, Value> {
        self.inner.borrow()
    }

    /// Runs `f` with a checked immutable borrow of the contained value.
    pub fn try_with_value<T>(&self, f: impl FnOnce(&Value) -> T) -> Result<T, BorrowError> {
        self.inner.try_borrow().map(|value| f(&value))
    }

    /// Replaces the contained value.
    pub fn set(&self, value: Value) {
        *self.inner.borrow_mut() = value;
    }

    /// Attempts to replace the contained value.
    ///
    /// This checked accessor is preferred outside low-level runtime internals.
    /// It returns `Err` if another caller currently holds a borrow.
    pub fn try_set(&self, value: Value) -> Result<(), BorrowMutError> {
        self.inner.try_borrow_mut().map(|mut slot| {
            *slot = value;
        })
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
    pub fn gc_debug_id(&self) -> usize {
        Rc::as_ptr(&self.inner).cast::<()>() as usize
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
            Self::Value(value) => value.clone(),
            Self::Reference(cell) => cell.get(),
        }
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

    /// Converts an ordinary slot into a reference cell or returns its existing
    /// cell. This is the only runtime operation that creates local aliases.
    pub fn ensure_reference_cell(&mut self) -> ReferenceCell {
        match self {
            Self::Value(value) => {
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
}

/// Backwards-compatible exported name for runtime slot users.
pub type ValueSlot = Slot;

/// Backwards-compatible exported name for earlier placeholder references.
pub type ReferencePlaceholder = ReferenceCell;

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
    use super::{ReferenceCell, Slot, TempValue};
    use crate::{ArrayKey, PhpArray, Value};

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
}
