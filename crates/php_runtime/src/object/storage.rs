use super::{
    ClassEntry, ClassEnumBackingType, ObjectIdGuard, debug::property_debug_label, next_object_id,
};
use crate::Value;
use std::cell::{BorrowError, BorrowMutError, Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::rc::{Rc, Weak};
use std::sync::Arc;

/// Class-owned declared-property layout, shared across instances of the
/// same class through a thread-local cache. The layout maps storage names
/// (private names arrive pre-mangled as `private:Owner:prop`) to slot
/// indices and carries the per-class debug labels; per-object state is a
/// plain slot vector plus a side map for dynamic properties.
struct PropertyLayout {
    /// Process-thread-unique identity used as the slot-access guard.
    layout_id: u64,
    /// Declared storage names in declaration order, slot-index aligned.
    slot_names: Vec<String>,
    /// storage name -> slot index.
    slot_by_name: HashMap<String, u32>,
    /// var_dump labels for every class property name (including statics and
    /// virtual hook properties, matching the previous per-object map).
    debug_labels: HashMap<String, String>,
}

thread_local! {
    static LAYOUT_CACHE: RefCell<HashMap<String, Vec<Rc<PropertyLayout>>>> =
        RefCell::new(HashMap::new());
    static NEXT_LAYOUT_ID: Cell<u64> = const { Cell::new(1) };
}

fn next_layout_id() -> u64 {
    NEXT_LAYOUT_ID.with(|next| {
        let id = next.get();
        next.set(id.wrapping_add(1));
        id
    })
}

/// Returns true when a property entry occupies backed instance storage.
fn is_backed_instance_property(property: &super::ClassPropertyEntry) -> bool {
    !property.flags.is_static
        && !((property.hooks.get_function_id.is_some() || property.hooks.set_function_id.is_some())
            && !property.hooks.backed)
}

/// Materializes the default declared-slot vector for a fresh instance of
/// `class` under `layout`. Slot defaults always come from the caller's class
/// entry (a cached layout may have been built from an earlier, identical
/// shape), so this reads defaults live rather than storing them on the shared
/// layout. When two properties share a storage name (redeclaration through
/// inheritance or trait composition), the later occurrence wins, matching the
/// slot the shared layout assigned to the first occurrence.
fn build_declared_slots(class: &ClassEntry, layout: &PropertyLayout) -> Vec<Option<Value>> {
    let mut declared_slots: Vec<Option<Value>> = vec![None; layout.slot_names.len()];
    for property in &class.properties {
        if !is_backed_instance_property(property) {
            continue;
        }
        if let Some(slot) = layout.slot_by_name.get(&property.name) {
            declared_slots[*slot as usize] = Some(property.default.clone());
        }
    }
    declared_slots
}

/// Builds or reuses the shared layout for a class. Conditional classes can
/// redefine a name with a different shape, so a cached layout is only
/// shared when the declared names and debug labels match exactly; slot
/// defaults always come from the caller's class entry.
fn class_layout(class: &ClassEntry, display_name: &str) -> Rc<PropertyLayout> {
    let mut slot_names = Vec::new();
    for property in &class.properties {
        if is_backed_instance_property(property) && !slot_names.contains(&property.name) {
            slot_names.push(property.name.clone());
        }
    }
    let debug_labels: HashMap<String, String> = class
        .properties
        .iter()
        .map(|property| {
            (
                property.name.clone(),
                property_debug_label(property, display_name),
            )
        })
        .collect();
    LAYOUT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let candidates = cache.entry(class.name.to_string()).or_default();
        if let Some(existing) = candidates
            .iter()
            .find(|layout| layout.slot_names == slot_names && layout.debug_labels == debug_labels)
        {
            return Rc::clone(existing);
        }
        let slot_by_name = slot_names
            .iter()
            .enumerate()
            .map(|(index, name)| (name.clone(), index as u32))
            .collect();
        let layout = Rc::new(PropertyLayout {
            layout_id: next_layout_id(),
            slot_names,
            slot_by_name,
            debug_labels,
        });
        candidates.push(Rc::clone(&layout));
        layout
    })
}

#[derive(Debug)]
struct ObjectStorage {
    class_name: Arc<str>,
    display_name: Arc<str>,
    is_enum: bool,
    enum_backing_type: Option<ClassEnumBackingType>,
    id_guard: Option<ObjectIdGuard>,
    layout: Rc<PropertyLayout>,
    /// Declared property slots; `None` means unset (absent), which is
    /// distinct from a present `Value::Uninitialized` typed slot.
    declared_slots: Vec<Option<Value>>,
    /// Dynamic (undeclared) properties; declared names never live here.
    dynamic_properties: HashMap<String, Value>,
    /// Insertion order of dynamic properties. Declared properties iterate
    /// in declaration (slot) order — even after unset and re-assignment,
    /// matching reference slot semantics — followed by dynamic entries.
    dynamic_order: Vec<String>,
    /// Labels for debug-view entries that are not part of the class layout.
    dynamic_debug_labels: HashMap<String, String>,
}

impl fmt::Debug for PropertyLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PropertyLayout")
            .field("layout_id", &self.layout_id)
            .field("slot_names", &self.slot_names)
            .finish()
    }
}

impl ObjectStorage {
    fn get(&self, name: &str) -> Option<&Value> {
        if let Some(slot) = self.layout.slot_by_name.get(name) {
            crate::layout_stats::record_object_declared_slot_read();
            return self.declared_slots[*slot as usize].as_ref();
        }
        // Most objects never grow dynamic properties; skip the second hash
        // (and its telemetry) when the map is provably empty.
        if self.dynamic_properties.is_empty() {
            return None;
        }
        crate::layout_stats::record_object_dynamic_property_map_read();
        self.dynamic_properties.get(name)
    }

    fn get_mut(&mut self, name: &str) -> Option<&mut Value> {
        if let Some(slot) = self.layout.slot_by_name.get(name).copied() {
            return self.declared_slots[slot as usize].as_mut();
        }
        if self.dynamic_properties.is_empty() {
            return None;
        }
        self.dynamic_properties.get_mut(name)
    }

    fn set(&mut self, name: String, value: Value) {
        if let Some(slot) = self.layout.slot_by_name.get(&name).copied() {
            crate::layout_stats::record_object_declared_slot_write();
            self.declared_slots[slot as usize] = Some(value);
            return;
        }
        crate::layout_stats::record_object_dynamic_property_map_write();
        if !self.dynamic_properties.contains_key(&name) {
            self.dynamic_order.push(name.clone());
        }
        self.dynamic_properties.insert(name, value);
    }

    fn unset(&mut self, name: &str) -> bool {
        if let Some(slot) = self.layout.slot_by_name.get(name).copied() {
            let slot_value = &mut self.declared_slots[slot as usize];
            if slot_value.is_none() {
                return false;
            }
            *slot_value = None;
            return true;
        }
        let removed = self.dynamic_properties.remove(name).is_some();
        if removed {
            self.dynamic_order.retain(|entry| entry != name);
        }
        removed
    }

    fn snapshot(&self) -> Vec<(String, Value)> {
        let declared = self
            .layout
            .slot_names
            .iter()
            .zip(&self.declared_slots)
            .filter_map(|(name, slot)| slot.as_ref().map(|value| (name.clone(), value.clone())));
        let dynamic = self.dynamic_order.iter().filter_map(|name| {
            self.dynamic_properties
                .get(name)
                .map(|value| (name.clone(), value.clone()))
        });
        declared.chain(dynamic).collect()
    }
}

/// Shared object cell: the stable identity lives beside the storage inside
/// one allocation so the handle itself stays pointer-sized — `Value` embeds
/// it in every register and local slot.
#[derive(Debug)]
struct ObjectCell {
    id: u64,
    storage: RefCell<ObjectStorage>,
}

/// Reference to runtime object storage.
#[derive(Clone)]
pub struct ObjectRef {
    cell: Rc<ObjectCell>,
}

/// Weak debug handle to object storage for GC tests.
#[derive(Clone, Debug)]
pub struct WeakObjectHandle {
    id: u64,
    cell: Weak<ObjectCell>,
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
        self.cell.strong_count() > 0
    }

    /// Upgrades this weak handle into an object reference when still alive.
    #[must_use]
    pub fn upgrade(&self) -> Option<ObjectRef> {
        self.cell.upgrade().map(|cell| ObjectRef { cell })
    }
}

impl ObjectRef {
    /// Creates an object with properties initialized from the class entry.
    #[must_use]
    pub fn new(class: &ClassEntry) -> Self {
        Self::new_with_display_name(class, class.name.to_string())
    }

    /// Creates an object with an explicit source-spelled display class name.
    #[must_use]
    pub fn new_with_display_name(class: &ClassEntry, display_name: impl Into<String>) -> Self {
        let display_name = display_name.into();
        let layout = class_layout(class, &display_name);
        let declared_slots = build_declared_slots(class, &layout);
        Self::assemble(class, display_name, layout, declared_slots)
    }

    /// Builds the default declared-slot template for a fresh instance of
    /// `class` under the layout selected by `display_name`.
    ///
    /// The returned vector is slot-index aligned with the class's property
    /// layout and byte-identical to the `declared_slots` that
    /// `new_with_display_name` would produce for the same class shape, so a
    /// caller may memoize it (keyed by class identity plus a class-table epoch)
    /// and clone it into fresh instances through [`Self::from_layout_slots`],
    /// skipping the per-property default-materialization loop. The template is
    /// independent of `display_name` (which only selects the debug-label layout
    /// variant, not slot contents or ordering).
    #[must_use]
    pub fn default_declared_slots(class: &ClassEntry, display_name: &str) -> Vec<Option<Value>> {
        let layout = class_layout(class, display_name);
        build_declared_slots(class, &layout)
    }

    /// Creates an object from a precomputed default declared-slot vector,
    /// skipping the per-property default-materialization loop.
    ///
    /// `declared_slots` MUST be slot-index aligned with the layout selected for
    /// `class`/`display_name` — that is, produced by
    /// [`Self::default_declared_slots`] for the same class shape (cloned per
    /// instance). This is the fast instantiation path for the hot `new C(...)`
    /// site; every other caller can keep using `new_with_display_name`, which
    /// builds the slots itself.
    #[must_use]
    pub fn from_layout_slots(
        class: &ClassEntry,
        display_name: impl Into<String>,
        declared_slots: Vec<Option<Value>>,
    ) -> Self {
        let display_name = display_name.into();
        let layout = class_layout(class, &display_name);
        debug_assert_eq!(
            declared_slots.len(),
            layout.slot_names.len(),
            "precomputed declared-slot template length must match the class layout"
        );
        Self::assemble(class, display_name, layout, declared_slots)
    }

    /// Assembles object storage from a resolved layout and declared-slot vector.
    fn assemble(
        class: &ClassEntry,
        display_name: String,
        layout: Rc<PropertyLayout>,
        declared_slots: Vec<Option<Value>>,
    ) -> Self {
        crate::layout_stats::record_object_allocation();
        let id = next_object_id();
        Self {
            cell: Rc::new(ObjectCell {
                id,
                storage: RefCell::new(ObjectStorage {
                    // Shared handle: every instance of one runtime class aliases the
                    // class entry's allocation (no per-instantiation copy, and
                    // the address doubles as a per-class identity).
                    class_name: Arc::clone(&class.name),
                    display_name: Arc::from(display_name),
                    is_enum: class.flags.is_enum,
                    enum_backing_type: class.enum_backing_type,
                    id_guard: Some(ObjectIdGuard::new(id)),
                    layout,
                    declared_slots,
                    dynamic_properties: HashMap::new(),
                    dynamic_order: Vec::new(),
                    dynamic_debug_labels: HashMap::new(),
                }),
            }),
        }
    }

    /// Creates a formatter-only object view with an existing PHP-visible object
    /// handle and a custom property list.
    ///
    /// This is used for `__debugInfo()` output, where PHP formats the returned
    /// property map as the original object without allocating a new visible
    /// object handle.
    #[must_use]
    pub fn debug_view_with_properties(
        source: &Self,
        properties: Vec<(String, String, Value)>,
    ) -> Self {
        let empty_layout = Rc::new(PropertyLayout {
            layout_id: 0,
            slot_names: Vec::new(),
            slot_by_name: HashMap::new(),
            debug_labels: HashMap::new(),
        });
        let mut dynamic_order = Vec::with_capacity(properties.len());
        let mut dynamic_properties = HashMap::with_capacity(properties.len());
        let mut dynamic_debug_labels = HashMap::with_capacity(properties.len());
        for (name, debug_label, value) in properties {
            if !dynamic_properties.contains_key(&name) {
                dynamic_order.push(name.clone());
            }
            dynamic_debug_labels.insert(name.clone(), debug_label);
            dynamic_properties.insert(name, value);
        }
        Self {
            cell: Rc::new(ObjectCell {
                id: source.id(),
                storage: RefCell::new(ObjectStorage {
                    class_name: source.class_name_handle(),
                    display_name: source.display_name_handle(),
                    is_enum: false,
                    enum_backing_type: None,
                    id_guard: None,
                    layout: empty_layout,
                    declared_slots: Vec::new(),
                    dynamic_properties,
                    dynamic_order,
                    dynamic_debug_labels,
                }),
            }),
        }
    }

    /// Returns the stable object identity for tests and diagnostics.
    #[must_use]
    pub fn id(&self) -> u64 {
        self.cell.id
    }

    /// Returns the current `Rc` strong count for GC debug metadata.
    #[must_use]
    pub fn gc_refcount_estimate(&self) -> usize {
        Rc::strong_count(&self.cell)
    }

    /// Returns a weak debug handle for GC tests.
    #[must_use]
    pub fn weak_handle(&self) -> WeakObjectHandle {
        WeakObjectHandle {
            id: self.cell.id,
            cell: Rc::downgrade(&self.cell),
        }
    }

    /// Returns the object's class name.
    #[must_use]
    pub fn class_name(&self) -> String {
        self.cell.storage.borrow().class_name.to_string()
    }

    /// Returns the object's class name as a shared handle (a refcount bump,
    /// no fresh allocation).
    #[must_use]
    pub fn class_name_handle(&self) -> Arc<str> {
        Arc::clone(&self.cell.storage.borrow().class_name)
    }

    /// Returns the source-spelled display class name for diagnostics and dumps.
    #[must_use]
    pub fn display_name(&self) -> String {
        self.cell.storage.borrow().display_name.to_string()
    }

    /// Returns the display class name as a shared handle (a refcount bump,
    /// no fresh allocation).
    #[must_use]
    pub fn display_name_handle(&self) -> Arc<str> {
        Arc::clone(&self.cell.storage.borrow().display_name)
    }

    /// Returns whether this object represents an enum case.
    #[must_use]
    pub fn is_enum(&self) -> bool {
        self.cell.storage.borrow().is_enum
    }

    /// Returns this object's enum backing type, when it is a backed enum case.
    #[must_use]
    pub fn enum_backing_type(&self) -> Option<ClassEnumBackingType> {
        self.cell.storage.borrow().enum_backing_type
    }

    /// Creates a new object identity with a shallow copy of the property map.
    #[must_use]
    pub fn clone_shallow(&self) -> Self {
        crate::layout_stats::record_object_allocation();
        let storage = self.cell.storage.borrow();
        let id = next_object_id();
        Self {
            cell: Rc::new(ObjectCell {
                id,
                storage: RefCell::new(ObjectStorage {
                    class_name: storage.class_name.clone(),
                    display_name: storage.display_name.clone(),
                    is_enum: storage.is_enum,
                    enum_backing_type: storage.enum_backing_type,
                    id_guard: Some(ObjectIdGuard::new(id)),
                    layout: Rc::clone(&storage.layout),
                    declared_slots: storage.declared_slots.clone(),
                    dynamic_properties: storage.dynamic_properties.clone(),
                    dynamic_order: storage.dynamic_order.clone(),
                    dynamic_debug_labels: storage.dynamic_debug_labels.clone(),
                }),
            }),
        }
    }

    /// Reads a property value.
    #[must_use]
    pub fn get_property(&self, name: &str) -> Option<Value> {
        self.cell.storage.borrow().get(name).cloned()
    }

    /// Attempts to read a property value without panicking on nested borrows.
    pub fn try_get_property(&self, name: &str) -> Result<Option<Value>, BorrowError> {
        self.cell
            .storage
            .try_borrow()
            .map(|storage| storage.get(name).cloned())
    }

    /// Writes a property value.
    pub fn set_property(&self, name: impl Into<String>, value: Value) {
        self.cell.storage.borrow_mut().set(name.into(), value);
    }

    /// Attempts to write a property value without panicking on nested borrows.
    pub fn try_set_property(
        &self,
        name: impl Into<String>,
        value: Value,
    ) -> Result<(), BorrowMutError> {
        let name = name.into();
        self.cell
            .storage
            .try_borrow_mut()
            .map(|mut storage| storage.set(name, value))
    }

    /// Runs `f` with a borrowed view of a property value, preferring
    /// `storage_name` and falling back to `fallback_name`, without cloning
    /// the stored value (and therefore without sharing container handles,
    /// which would force copy-on-write separations on later writes).
    /// `Err` means the storage is already mutably borrowed; callers fall
    /// back to the cloning read path.
    pub fn try_with_property_lookup<R>(
        &self,
        storage_name: &str,
        fallback_name: &str,
        f: impl FnOnce(Option<&Value>) -> R,
    ) -> Result<R, BorrowError> {
        let storage = self.cell.storage.try_borrow()?;
        let value = storage
            .get(storage_name)
            .or_else(|| storage.get(fallback_name));
        Ok(f(value))
    }

    /// Modifies an existing property value in place, avoiding the
    /// read-clone → mutate → write-back round trip that separates shared
    /// array storage on every nested dimension write.
    ///
    /// The value is moved out of the slot (leaving `Value::Uninitialized`)
    /// while `f` runs, so `f` may safely touch other objects' storage. `f`
    /// must not trigger PHP-visible reads of this object; the VM only passes
    /// closures that never re-enter PHP code. Returns `Ok(None)` without
    /// calling `f` when the property does not exist. Fails with
    /// `BorrowMutError` when the storage is already borrowed (caller falls
    /// back to the generic clone/write-back path).
    pub fn try_modify_property_value<R>(
        &self,
        name: &str,
        f: impl FnOnce(&mut Value) -> R,
    ) -> Result<Option<R>, BorrowMutError> {
        let mut value = {
            let mut storage = self.cell.storage.try_borrow_mut()?;
            let Some(slot) = storage.get_mut(name) else {
                return Ok(None);
            };
            std::mem::replace(slot, Value::Uninitialized)
        };
        let result = f(&mut value);
        let mut storage = self
            .cell
            .storage
            .try_borrow_mut()
            .expect("object storage re-borrowed across in-place property write");
        if let Some(slot) = storage.get_mut(name) {
            *slot = value;
        } else {
            // `f` cannot remove the slot; restore defensively regardless.
            storage.set(name.to_owned(), value);
        }
        Ok(Some(result))
    }

    /// Returns the `var_dump` property label for a stored property name.
    #[must_use]
    pub fn property_debug_label(&self, name: &str) -> String {
        let storage = self.cell.storage.borrow();
        storage
            .layout
            .debug_labels
            .get(name)
            .or_else(|| storage.dynamic_debug_labels.get(name))
            .cloned()
            .unwrap_or_else(|| format!("\"{name}\""))
    }

    /// Removes a property value, returning whether it existed.
    pub fn unset_property(&self, name: &str) -> bool {
        self.cell.storage.borrow_mut().unset(name)
    }

    /// Clears all stored properties as an internal GC action.
    ///
    /// This is not PHP-visible `unset()` semantics; it is only used by the
    /// runtime-semantics cycle-collection test hook after proving the object is not
    /// rooted.
    pub fn gc_clear_properties(&self) {
        let mut storage = self.cell.storage.borrow_mut();
        for slot in &mut storage.declared_slots {
            *slot = None;
        }
        storage.dynamic_properties.clear();
        storage.dynamic_order.clear();
    }

    /// Releases the PHP-visible object handle after the VM proves the object has
    /// no PHP-visible roots. Internal stale temporaries may still hold storage
    /// clones until the current frame completes, so handle lifetime is tracked
    /// separately from Rust storage lifetime.
    pub fn release_php_handle(&self) {
        self.cell.storage.borrow_mut().id_guard.take();
    }

    /// Returns a snapshot of runtime properties in PHP insertion/declaration order.
    #[must_use]
    pub fn properties_snapshot(&self) -> Vec<(String, Value)> {
        self.cell.storage.borrow().snapshot()
    }

    /// Visits every present property value (declared slots, then dynamic
    /// properties) without materializing a snapshot vector. Covers the same
    /// value set as [`Self::properties_snapshot`]; property names and order
    /// are not exposed.
    pub fn visit_property_values(&self, mut visit: impl FnMut(&Value)) {
        let storage = self.cell.storage.borrow();
        for value in storage.declared_slots.iter().flatten() {
            visit(value);
        }
        for value in storage.dynamic_properties.values() {
            visit(value);
        }
    }

    /// Attempts to snapshot runtime properties without panicking on nested borrows.
    pub fn try_properties_snapshot(&self) -> Result<Vec<(String, Value)>, BorrowError> {
        self.cell
            .storage
            .try_borrow()
            .map(|storage| storage.snapshot())
    }

    /// Identity of this object's class layout, used as the declared-slot
    /// access guard by inline caches.
    #[must_use]
    pub fn class_layout_epoch(&self) -> u64 {
        self.cell.storage.borrow().layout.layout_id
    }

    /// Slot index for a declared storage name under the current layout.
    #[must_use]
    pub fn declared_slot_index(&self, storage_name: &str) -> Option<u32> {
        self.cell
            .storage
            .borrow()
            .layout
            .slot_by_name
            .get(storage_name)
            .copied()
    }

    /// Declared storage name for a slot under the current layout.
    #[must_use]
    pub fn slot_metadata(&self, slot: u32) -> Option<String> {
        self.cell
            .storage
            .borrow()
            .layout
            .slot_names
            .get(slot as usize)
            .cloned()
    }

    /// Reads a declared slot directly when the layout guard matches.
    /// Returns `None` on guard mismatch or an unset slot; callers fall back
    /// to the generic name-keyed path.
    #[must_use]
    pub fn get_declared_slot(&self, slot: u32, layout_epoch: u64) -> Option<Value> {
        let storage = self.cell.storage.borrow();
        if storage.layout.layout_id != layout_epoch {
            return None;
        }
        let value = storage.declared_slots.get(slot as usize)?.clone();
        if value.is_some() {
            crate::layout_stats::record_object_declared_slot_read();
        }
        value
    }

    /// Writes a declared slot directly when the layout guard matches.
    /// Returns false on guard mismatch so callers fall back to the generic
    /// name-keyed path.
    pub fn set_declared_slot(&self, slot: u32, layout_epoch: u64, value: Value) -> bool {
        let mut storage = self.cell.storage.borrow_mut();
        if storage.layout.layout_id != layout_epoch {
            return false;
        }
        let Some(slot_value) = storage.declared_slots.get_mut(slot as usize) else {
            return false;
        };
        *slot_value = Some(value);
        crate::layout_stats::record_object_declared_slot_write();
        true
    }
}

impl fmt::Debug for ObjectRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObjectRef")
            .field("id", &self.cell.id)
            .field("class_name", &self.class_name())
            .finish()
    }
}

impl PartialEq for ObjectRef {
    fn eq(&self, other: &Self) -> bool {
        self.cell.id == other.cell.id
    }
}

impl Eq for ObjectRef {}
