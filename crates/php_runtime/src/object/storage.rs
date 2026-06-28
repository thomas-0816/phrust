use super::{ClassEntry, debug::property_debug_label, next_object_id};
use crate::Value;
use std::cell::{BorrowError, BorrowMutError, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::rc::{Rc, Weak};

#[derive(Debug)]
struct ObjectStorage {
    class_name: String,
    display_name: String,
    properties: HashMap<String, Value>,
    property_order: Vec<String>,
    property_debug_labels: HashMap<String, String>,
}

/// Reference to runtime object storage.
#[derive(Clone)]
pub struct ObjectRef {
    id: u64,
    storage: Rc<RefCell<ObjectStorage>>,
}

/// Weak debug handle to object storage for GC tests.
#[derive(Clone, Debug)]
pub struct WeakObjectHandle {
    id: u64,
    storage: Weak<RefCell<ObjectStorage>>,
}

impl WeakObjectHandle {
    /// Returns the stable object identity.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Returns true when the object storage is still alive.
    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.storage.strong_count() > 0
    }

    /// Upgrades this weak handle into an object reference when still alive.
    #[must_use]
    pub fn upgrade(&self) -> Option<ObjectRef> {
        self.storage.upgrade().map(|storage| ObjectRef {
            id: self.id,
            storage,
        })
    }
}

impl ObjectRef {
    /// Creates an object with properties initialized from the class entry.
    #[must_use]
    pub fn new(class: &ClassEntry) -> Self {
        Self::new_with_display_name(class, class.name.clone())
    }

    /// Creates an object with an explicit source-spelled display class name.
    #[must_use]
    pub fn new_with_display_name(class: &ClassEntry, display_name: impl Into<String>) -> Self {
        crate::layout_stats::record_object_allocation();
        let display_name = display_name.into();
        let property_entries = class
            .properties
            .iter()
            .filter(|property| {
                !property.flags.is_static
                    && !((property.hooks.get_function_id.is_some()
                        || property.hooks.set_function_id.is_some())
                        && !property.hooks.backed)
            })
            .map(|property| (property.name.clone(), property.default.clone()))
            .collect::<Vec<_>>();
        let property_debug_labels = class
            .properties
            .iter()
            .map(|property| {
                (
                    property.name.clone(),
                    property_debug_label(property, &display_name),
                )
            })
            .collect();
        let mut property_order = Vec::new();
        for (name, _) in &property_entries {
            if !property_order.iter().any(|entry| entry == name) {
                property_order.push(name.clone());
            }
        }
        let properties = property_entries.into_iter().collect();
        Self {
            id: next_object_id(),
            storage: Rc::new(RefCell::new(ObjectStorage {
                class_name: class.name.clone(),
                display_name,
                properties,
                property_order,
                property_debug_labels,
            })),
        }
    }

    /// Returns the stable object identity for tests and diagnostics.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Returns the current `Rc` strong count for GC debug metadata.
    #[must_use]
    pub fn gc_refcount_estimate(&self) -> usize {
        Rc::strong_count(&self.storage)
    }

    /// Returns a weak debug handle for GC tests.
    #[must_use]
    pub fn weak_handle(&self) -> WeakObjectHandle {
        WeakObjectHandle {
            id: self.id,
            storage: Rc::downgrade(&self.storage),
        }
    }

    /// Returns the object's class name.
    #[must_use]
    pub fn class_name(&self) -> String {
        self.storage.borrow().class_name.clone()
    }

    /// Returns the source-spelled display class name for diagnostics and dumps.
    #[must_use]
    pub fn display_name(&self) -> String {
        self.storage.borrow().display_name.clone()
    }

    /// Creates a new object identity with a shallow copy of the property map.
    #[must_use]
    pub fn clone_shallow(&self) -> Self {
        crate::layout_stats::record_object_allocation();
        let storage = self.storage.borrow();
        Self {
            id: next_object_id(),
            storage: Rc::new(RefCell::new(ObjectStorage {
                class_name: storage.class_name.clone(),
                display_name: storage.display_name.clone(),
                properties: storage.properties.clone(),
                property_order: storage.property_order.clone(),
                property_debug_labels: storage.property_debug_labels.clone(),
            })),
        }
    }

    /// Reads a property value.
    #[must_use]
    pub fn get_property(&self, name: &str) -> Option<Value> {
        self.storage.borrow().properties.get(name).cloned()
    }

    /// Attempts to read a property value without panicking on nested borrows.
    pub fn try_get_property(&self, name: &str) -> Result<Option<Value>, BorrowError> {
        self.storage
            .try_borrow()
            .map(|storage| storage.properties.get(name).cloned())
    }

    /// Writes a property value.
    pub fn set_property(&self, name: impl Into<String>, value: Value) {
        let name = name.into();
        let mut storage = self.storage.borrow_mut();
        if !storage.properties.contains_key(&name) {
            storage.property_order.push(name.clone());
        }
        storage.properties.insert(name, value);
    }

    /// Attempts to write a property value without panicking on nested borrows.
    pub fn try_set_property(
        &self,
        name: impl Into<String>,
        value: Value,
    ) -> Result<(), BorrowMutError> {
        let name = name.into();
        self.storage.try_borrow_mut().map(|mut storage| {
            if !storage.properties.contains_key(&name) {
                storage.property_order.push(name.clone());
            }
            storage.properties.insert(name, value);
        })
    }

    /// Returns the `var_dump` property label for a stored property name.
    #[must_use]
    pub fn property_debug_label(&self, name: &str) -> String {
        self.storage
            .borrow()
            .property_debug_labels
            .get(name)
            .cloned()
            .unwrap_or_else(|| format!("\"{name}\""))
    }

    /// Removes a property value, returning whether it existed.
    pub fn unset_property(&self, name: &str) -> bool {
        let mut storage = self.storage.borrow_mut();
        let removed = storage.properties.remove(name).is_some();
        if removed {
            storage.property_order.retain(|entry| entry != name);
        }
        removed
    }

    /// Clears all stored properties as an internal GC action.
    ///
    /// This is not PHP-visible `unset()` semantics; it is only used by the
    /// runtime-semantics cycle-collection test hook after proving the object is not
    /// rooted.
    pub fn gc_clear_properties(&self) {
        let mut storage = self.storage.borrow_mut();
        storage.properties.clear();
        storage.property_order.clear();
    }

    /// Returns a snapshot of runtime properties in PHP insertion/declaration order.
    #[must_use]
    pub fn properties_snapshot(&self) -> Vec<(String, Value)> {
        let storage = self.storage.borrow();
        storage
            .property_order
            .iter()
            .filter_map(|name| {
                storage
                    .properties
                    .get(name)
                    .map(|value| (name.clone(), value.clone()))
            })
            .collect()
    }

    /// Attempts to snapshot runtime properties without panicking on nested borrows.
    pub fn try_properties_snapshot(&self) -> Result<Vec<(String, Value)>, BorrowError> {
        self.storage.try_borrow().map(|storage| {
            storage
                .property_order
                .iter()
                .filter_map(|name| {
                    storage
                        .properties
                        .get(name)
                        .map(|value| (name.clone(), value.clone()))
                })
                .collect()
        })
    }
}

impl fmt::Debug for ObjectRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObjectRef")
            .field("id", &self.id)
            .field("class_name", &self.class_name())
            .finish()
    }
}

impl PartialEq for ObjectRef {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for ObjectRef {}
