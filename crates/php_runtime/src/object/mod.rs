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

use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_OBJECT_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_object_id() -> u64 {
    NEXT_OBJECT_ID.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests;
