//! Minimal object storage and class metadata for runtime.

mod attribute;
mod class;
mod debug;
mod member;
mod storage;
mod types;

pub use attribute::AttributeEntry;
pub use class::{ClassEntry, ClassFlags, display_class_name, normalize_class_name};
pub use member::{
    ClassConstantEntry, ClassConstantFlags, ClassEnumBackingType, ClassEnumCaseEntry,
    ClassMethodEntry, ClassMethodFlags, ClassPropertyEntry, ClassPropertyFlags, ClassPropertyHooks,
};
pub use storage::{ObjectRef, WeakObjectHandle};
pub use types::RuntimeType;

use std::cell::{Cell, RefCell};

thread_local! {
    static NEXT_OBJECT_ID: Cell<u64> = const { Cell::new(1) };
    static FREE_OBJECT_IDS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn next_object_id() -> u64 {
    if let Some(id) = FREE_OBJECT_IDS.with_borrow_mut(Vec::pop) {
        return id;
    }
    NEXT_OBJECT_ID.with(|next_id| {
        let id = next_id.get();
        next_id.set(id + 1);
        id
    })
}

#[derive(Debug)]
pub(crate) struct ObjectIdGuard {
    id: u64,
}

impl ObjectIdGuard {
    #[must_use]
    pub(crate) const fn new(id: u64) -> Self {
        Self { id }
    }
}

impl Drop for ObjectIdGuard {
    fn drop(&mut self) {
        FREE_OBJECT_IDS.with_borrow_mut(|free_ids| free_ids.push(self.id));
    }
}

#[cfg(test)]
mod tests;
