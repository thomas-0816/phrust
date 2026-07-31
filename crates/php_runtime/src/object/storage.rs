use super::{
    ClassEntry, ClassEnumBackingType, ObjectIdGuard, debug::property_debug_label, next_object_id,
};
use crate::Value;
use std::cell::{BorrowError, BorrowMutError, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::rc::{Rc, Weak};
use std::sync::{
    Arc, Mutex, MutexGuard, OnceLock,
    atomic::{AtomicU64, Ordering},
};

/// One authoritative declared-property cell while an object is admitted to
/// native execution. `initialized == 0` represents an absent/unset slot;
/// otherwise `value` is the request-native encoded owner.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeDeclaredPropertySlot {
    pub initialized: u32,
    pub reserved: u32,
    pub value: i64,
}

/// One stable authoritative dynamic-property cell.
///
/// `slot` is the first field so a pointer to this allocation is also the
/// direct [`NativeDeclaredPropertySlot`] data-plane pointer consumed by CLIF.
/// Unset keeps the allocation as a tombstone. A later direct assignment
/// publishes a fresh insertion order through `next_insertion_order`, matching
/// PHP's remove-and-reinsert ordering without invalidating the slot address.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct NativeDynamicPropertyCell {
    pub slot: NativeDeclaredPropertySlot,
    pub insertion_order: u64,
    pub next_insertion_order: *mut u64,
}

impl NativeDynamicPropertyCell {
    fn new(slot: NativeDeclaredPropertySlot, insertion_order: u64) -> Self {
        Self {
            slot,
            insertion_order,
            next_insertion_order: std::ptr::null_mut(),
        }
    }
}

/// Authoritative native values for undeclared object properties. Every cell
/// is separately allocated, so its address survives name-index growth,
/// `unset`, and later reinsertion.
pub type NativeDynamicPropertySlots = HashMap<String, Box<NativeDynamicPropertyCell>>;

type RustPropertySlots = (Vec<Option<Value>>, HashMap<String, Value>);

/// Class-owned declared-property layout, shared across instances of the same
/// class through a thread-local data cache. Its numeric identity is interned
/// process-wide so native code and worker-owned compile records see one stable
/// ABI shape regardless of which server worker materialized the class.
struct PropertyLayout {
    /// Process-wide identity used as the slot-access and method-PIC guard.
    layout_id: u64,
    /// Declared storage names in declaration order, slot-index aligned.
    slot_names: Vec<String>,
    /// storage name -> slot index.
    slot_by_name: HashMap<String, u32>,
    /// PHP array-cast key for each declared slot. Private and protected
    /// properties use Zend's NUL-delimited visibility encoding.
    array_cast_names: Vec<String>,
    /// var_dump labels for every class property name (including statics and
    /// virtual hook properties, matching the previous per-object map).
    debug_labels: HashMap<String, String>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct PropertyLayoutIdentity {
    class_name: String,
    slot_names: Vec<String>,
    array_cast_names: Vec<String>,
    debug_labels: Vec<(String, String)>,
}

thread_local! {
    static LAYOUT_CACHE: RefCell<HashMap<String, Vec<Rc<PropertyLayout>>>> =
        RefCell::new(HashMap::new());
}

fn lock_layout_identities(
    identities: &Mutex<HashMap<PropertyLayoutIdentity, u64>>,
) -> MutexGuard<'_, HashMap<PropertyLayoutIdentity, u64>> {
    identities
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn intern_layout_id(identity: PropertyLayoutIdentity) -> u64 {
    static IDENTITIES: OnceLock<Mutex<HashMap<PropertyLayoutIdentity, u64>>> = OnceLock::new();
    static NEXT_LAYOUT_ID: AtomicU64 = AtomicU64::new(1);

    let mut identities = lock_layout_identities(IDENTITIES.get_or_init(Mutex::default));
    if let Some(id) = identities.get(&identity) {
        return *id;
    }
    let id = NEXT_LAYOUT_ID.fetch_add(1, Ordering::Relaxed);
    if id == 0 {
        std::process::abort();
    }
    identities.insert(identity, id);
    id
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
    let mut array_cast_names = Vec::new();
    for property in &class.properties {
        if is_backed_instance_property(property) && !slot_names.contains(&property.name) {
            slot_names.push(property.name.clone());
            let cast_name = if property.flags.is_private {
                property
                    .name
                    .strip_prefix("private:")
                    .and_then(|rest| rest.split_once(':'))
                    .map_or_else(
                        || format!("\0{display_name}\0{}", property.name),
                        |(owner, name)| {
                            let owner = if owner.eq_ignore_ascii_case(&class.name) {
                                display_name
                            } else {
                                owner
                            };
                            format!("\0{owner}\0{name}")
                        },
                    )
            } else if property.flags.is_protected {
                format!("\0*\0{}", property.name)
            } else {
                property.name.clone()
            };
            array_cast_names.push(cast_name);
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
        if let Some(existing) = candidates.iter().find(|layout| {
            layout.slot_names == slot_names
                && layout.array_cast_names == array_cast_names
                && layout.debug_labels == debug_labels
        }) {
            return Rc::clone(existing);
        }
        let slot_by_name = slot_names
            .iter()
            .enumerate()
            .map(|(index, name)| (name.clone(), index as u32))
            .collect();
        let mut identity_debug_labels = debug_labels
            .iter()
            .map(|(name, label)| (name.clone(), label.clone()))
            .collect::<Vec<_>>();
        identity_debug_labels.sort_unstable();
        let layout_id = intern_layout_id(PropertyLayoutIdentity {
            class_name: class.name.to_string(),
            slot_names: slot_names.clone(),
            array_cast_names: array_cast_names.clone(),
            debug_labels: identity_debug_labels,
        });
        let layout = Rc::new(PropertyLayout {
            layout_id,
            slot_names,
            slot_by_name,
            array_cast_names,
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
    /// Exact class-shape knowledge for non-mutating dynamic property tests.
    /// `None` is reserved for formatter/synthetic views without a complete
    /// runtime method table.
    native_magic_isset: Option<bool>,
    native_countable: bool,
    native_traversable: bool,
    id_guard: Option<ObjectIdGuard>,
    layout: Rc<PropertyLayout>,
    /// Declared property slots; `None` means unset (absent), which is
    /// distinct from a present `Value::Uninitialized` typed slot.
    declared_slots: Vec<Option<Value>>,
    /// Mutually exclusive native representation of `declared_slots`.
    /// Promotion moves every Rust value out before installing this box;
    /// demotion removes the box before restoring Rust values.
    native_declared_slots: Option<Box<[NativeDeclaredPropertySlot]>>,
    /// Dynamic (undeclared) properties; declared names never live here.
    dynamic_properties: HashMap<String, Value>,
    /// Mutually exclusive native value representation of
    /// `dynamic_properties`. Names and insertion order remain object-owned,
    /// while every value is one authoritative request-native encoded owner.
    native_dynamic_properties: Option<NativeDynamicPropertySlots>,
    /// Stable order clock addressed directly by native dynamic cells.
    native_dynamic_next_order: Option<Box<u64>>,
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

    fn set_borrowed(&mut self, name: &str, value: Value) {
        if let Some(slot) = self.layout.slot_by_name.get(name).copied() {
            crate::layout_stats::record_object_declared_slot_write();
            self.declared_slots[slot as usize] = Some(value);
            return;
        }
        self.set(name.to_owned(), value);
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

    fn array_cast_snapshot(&self) -> Vec<(String, Value)> {
        let declared = self
            .layout
            .array_cast_names
            .iter()
            .zip(&self.declared_slots)
            .filter_map(|(name, slot)| {
                slot.as_ref()
                    .filter(|value| !matches!(value, Value::Uninitialized))
                    .map(|value| (name.clone(), value.clone()))
            });
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

    /// Returns the immutable numeric layout identity selected for a runtime
    /// class/display-name pair without allocating an object instance.
    #[must_use]
    pub fn prepared_layout_id(class: &ClassEntry, display_name: &str) -> u64 {
        class_layout(class, display_name).layout_id
    }

    /// Returns the numeric declared-storage slot for a prepared class shape
    /// without allocating an object. Publication uses this once; generated
    /// code subsequently consumes only the slot and layout identity.
    #[must_use]
    pub fn prepared_declared_slot_index(
        class: &ClassEntry,
        display_name: &str,
        storage_name: &str,
    ) -> Option<u32> {
        class_layout(class, display_name)
            .slot_by_name
            .get(storage_name)
            .copied()
    }

    /// Creates an object whose declared properties are already represented by
    /// authoritative native encoded slots. No Rust `Value` slot vector is
    /// constructed at this boundary.
    #[must_use]
    pub fn from_layout_native_slots(
        class: &ClassEntry,
        display_name: impl Into<String>,
        native_declared_slots: Box<[NativeDeclaredPropertySlot]>,
    ) -> Self {
        let display_name = display_name.into();
        let layout = class_layout(class, &display_name);
        debug_assert_eq!(
            native_declared_slots.len(),
            layout.slot_names.len(),
            "prepared native slot template length must match the class layout"
        );
        Self::assemble_with_slots(
            class,
            display_name,
            layout,
            Vec::new(),
            Some(native_declared_slots),
        )
    }

    /// Assembles object storage from a resolved layout and declared-slot vector.
    fn assemble(
        class: &ClassEntry,
        display_name: String,
        layout: Rc<PropertyLayout>,
        declared_slots: Vec<Option<Value>>,
    ) -> Self {
        Self::assemble_with_slots(class, display_name, layout, declared_slots, None)
    }

    fn assemble_with_slots(
        class: &ClassEntry,
        display_name: String,
        layout: Rc<PropertyLayout>,
        declared_slots: Vec<Option<Value>>,
        native_declared_slots: Option<Box<[NativeDeclaredPropertySlot]>>,
    ) -> Self {
        crate::layout_stats::record_object_allocation();
        let id = next_object_id();
        let native_dynamic_properties = native_declared_slots.as_ref().map(|_| HashMap::new());
        let native_dynamic_next_order = native_declared_slots.as_ref().map(|_| Box::new(0));
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
                    native_magic_isset: class
                        .methods
                        .iter()
                        .any(|method| method.name.eq_ignore_ascii_case("__isset"))
                        .then_some(true)
                        .or_else(|| class.flags.has_complete_method_table.then_some(false)),
                    native_countable: class.flags.implements_countable
                        || class
                            .interfaces
                            .iter()
                            .any(|name| name.eq_ignore_ascii_case("countable")),
                    native_traversable: class.flags.implements_traversable
                        || class.interfaces.iter().any(|name| {
                            name.eq_ignore_ascii_case("traversable")
                                || name.eq_ignore_ascii_case("iterator")
                                || name.eq_ignore_ascii_case("iteratoraggregate")
                        }),
                    id_guard: Some(ObjectIdGuard::new(id)),
                    layout,
                    declared_slots,
                    native_declared_slots,
                    dynamic_properties: HashMap::new(),
                    native_dynamic_properties,
                    native_dynamic_next_order,
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
            array_cast_names: Vec::new(),
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
                    native_magic_isset: None,
                    native_countable: source.is_native_countable(),
                    native_traversable: source.is_native_traversable(),
                    id_guard: None,
                    layout: empty_layout,
                    declared_slots: Vec::new(),
                    native_declared_slots: None,
                    dynamic_properties,
                    native_dynamic_properties: None,
                    native_dynamic_next_order: None,
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

    /// Returns publication-time `Countable` classification without consulting
    /// a request class table.
    #[must_use]
    pub fn is_native_countable(&self) -> bool {
        self.cell.storage.borrow().native_countable
    }

    /// Returns publication-time `Traversable` classification without
    /// consulting a request class table.
    #[must_use]
    pub fn is_native_traversable(&self) -> bool {
        self.cell.storage.borrow().native_traversable
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
        let dynamic_order = if let Some(dynamic) = storage.native_dynamic_properties.as_ref() {
            let mut names: Vec<String> = dynamic
                .iter()
                .filter(|(_, cell)| cell.slot.initialized != 0)
                .map(|(name, _)| name.clone())
                .collect();
            names.sort_by_key(|name| {
                dynamic
                    .get(name)
                    .map_or(u64::MAX, |cell| cell.insertion_order)
            });
            names
        } else {
            storage.dynamic_order.clone()
        };
        Self {
            cell: Rc::new(ObjectCell {
                id,
                storage: RefCell::new(ObjectStorage {
                    class_name: storage.class_name.clone(),
                    display_name: storage.display_name.clone(),
                    is_enum: storage.is_enum,
                    enum_backing_type: storage.enum_backing_type,
                    native_magic_isset: storage.native_magic_isset,
                    native_countable: storage.native_countable,
                    native_traversable: storage.native_traversable,
                    id_guard: Some(ObjectIdGuard::new(id)),
                    layout: Rc::clone(&storage.layout),
                    declared_slots: storage.declared_slots.clone(),
                    native_declared_slots: None,
                    dynamic_properties: storage.dynamic_properties.clone(),
                    native_dynamic_properties: None,
                    native_dynamic_next_order: None,
                    dynamic_order,
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

    /// Tests only the actual dynamic-property side map without cloning a PHP
    /// value. Declared-property existence is answered from the shared numeric
    /// layout and remains true for uninitialized typed slots.
    #[must_use]
    pub fn has_dynamic_property(&self, name: &str) -> bool {
        let storage = self.cell.storage.borrow();
        storage.dynamic_properties.contains_key(name)
            || storage
                .native_dynamic_properties
                .as_ref()
                .and_then(|properties| properties.get(name))
                .is_some_and(|cell| cell.slot.initialized != 0)
    }

    /// Returns whether this identity owns any actual dynamic properties.
    #[must_use]
    pub fn has_dynamic_properties(&self) -> bool {
        let storage = self.cell.storage.borrow();
        !storage.dynamic_properties.is_empty()
            || storage
                .native_dynamic_properties
                .as_ref()
                .is_some_and(|properties| {
                    properties.values().any(|cell| cell.slot.initialized != 0)
                })
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

    /// Writes a property while borrowing its already-published name.
    /// Declared slots need no owned key; only a genuinely dynamic property
    /// materializes a `String` for the side map.
    pub fn set_property_borrowed(&self, name: &str, value: Value) {
        self.cell.storage.borrow_mut().set_borrowed(name, value);
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
        debug_assert!(
            storage.native_dynamic_properties.is_none(),
            "native dynamic owners must be retired before cold GC clearing"
        );
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

    /// Returns properties using PHP's object-to-array key encoding.
    /// Uninitialized typed properties are omitted, protected keys are
    /// `\0*\0name`, and private keys are `\0DeclaringClass\0name`.
    #[must_use]
    pub fn array_cast_snapshot(&self) -> Vec<(String, Value)> {
        self.cell.storage.borrow().array_cast_snapshot()
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

    /// Tests present property values without cloning property names or values.
    /// Returns a borrow error instead of panicking when object storage is
    /// already mutably borrowed by a re-entrant runtime operation.
    pub fn try_any_property_value(
        &self,
        mut predicate: impl FnMut(&Value) -> bool,
    ) -> Result<bool, BorrowError> {
        let storage = self.cell.storage.try_borrow()?;
        Ok(storage
            .declared_slots
            .iter()
            .flatten()
            .chain(storage.dynamic_properties.values())
            .any(&mut predicate))
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

    /// Moves every declared and dynamic Rust property value out for
    /// request-native promotion. No Rust property value remains authoritative
    /// afterwards; dynamic names and their insertion order stay object-owned.
    pub fn take_property_slots_for_native(&self, layout_id: u64) -> Option<RustPropertySlots> {
        let mut storage = self.cell.storage.borrow_mut();
        if storage.layout.layout_id != layout_id
            || storage.native_declared_slots.is_some()
            || storage.native_dynamic_properties.is_some()
            || storage.native_dynamic_next_order.is_some()
        {
            return None;
        }
        Some((
            std::mem::take(&mut storage.declared_slots),
            std::mem::take(&mut storage.dynamic_properties),
        ))
    }

    /// Installs all authoritative request-native property cells after every
    /// Rust value has been moved into an encoded owner.
    pub fn install_native_property_slots(
        &self,
        layout_id: u64,
        declared: Box<[NativeDeclaredPropertySlot]>,
        dynamic: NativeDynamicPropertySlots,
    ) -> Result<
        (),
        (
            Box<[NativeDeclaredPropertySlot]>,
            NativeDynamicPropertySlots,
        ),
    > {
        let mut storage = self.cell.storage.borrow_mut();
        if storage.layout.layout_id != layout_id
            || storage.native_declared_slots.is_some()
            || storage.native_dynamic_properties.is_some()
            || storage.native_dynamic_next_order.is_some()
            || !storage.declared_slots.is_empty()
            || !storage.dynamic_properties.is_empty()
            || declared.len() != storage.layout.slot_names.len()
            || dynamic.iter().any(|(name, cell)| {
                cell.slot.initialized != 0 && !storage.dynamic_order.contains(name)
            })
        {
            return Err((declared, dynamic));
        }
        let mut dynamic = dynamic;
        for (order, name) in storage.dynamic_order.iter().enumerate() {
            if let Some(cell) = dynamic.get_mut(name)
                && cell.slot.initialized != 0
            {
                cell.insertion_order = order as u64;
            }
        }
        let next_order = dynamic
            .values()
            .filter(|cell| cell.slot.initialized != 0)
            .map(|cell| cell.insertion_order)
            .max()
            .map_or(0, |order| order.saturating_add(1));
        let mut order_clock = Box::new(next_order);
        let order_clock_pointer = std::ptr::from_mut(&mut *order_clock);
        for cell in dynamic.values_mut() {
            cell.next_insertion_order = order_clock_pointer;
        }
        storage.native_declared_slots = Some(declared);
        storage.native_dynamic_properties = Some(dynamic);
        storage.native_dynamic_next_order = Some(order_clock);
        Ok(())
    }

    /// Installs prepared declared slots for a newly allocated object. Fresh
    /// objects have no dynamic properties, but publish an empty native map so
    /// all property values still have one representation.
    pub fn install_native_declared_slots(
        &self,
        layout_id: u64,
        slots: Box<[NativeDeclaredPropertySlot]>,
    ) -> bool {
        self.install_native_property_slots(layout_id, slots, HashMap::new())
            .is_ok()
    }

    /// Returns the stable native declared-slot base guarded by the layout ID.
    #[must_use]
    pub fn native_declared_slots_view(
        &self,
        layout_id: u64,
    ) -> Option<(*mut NativeDeclaredPropertySlot, usize)> {
        let storage = self.cell.storage.borrow();
        if storage.layout.layout_id != layout_id {
            return None;
        }
        let slots = storage.native_declared_slots.as_ref()?;
        Some((slots.as_ptr().cast_mut(), slots.len()))
    }

    /// Reads one authoritative native dynamic-property cell. The outer
    /// `Option` denotes whether native property storage is active; the inner
    /// value denotes whether the dynamic name exists.
    #[must_use]
    pub fn native_dynamic_property_slot(
        &self,
        layout_id: u64,
        name: &str,
    ) -> Option<Option<NativeDeclaredPropertySlot>> {
        let storage = self.cell.storage.borrow();
        if storage.layout.layout_id != layout_id
            || storage.native_declared_slots.is_none()
            || !storage.declared_slots.is_empty()
            || !storage.dynamic_properties.is_empty()
        {
            return None;
        }
        Some(
            storage
                .native_dynamic_properties
                .as_ref()?
                .get(name)
                .and_then(|cell| (cell.slot.initialized != 0).then_some(cell.slot)),
        )
    }

    /// Locates one stable authoritative native dynamic-property cell.
    ///
    /// The returned pointer remains valid across value mutation, `unset`,
    /// reinsertion, and rehashing of the name index. It must not be retained
    /// across destruction of the object.
    #[must_use]
    pub fn native_dynamic_property_slot_location(
        &self,
        layout_id: u64,
        name: &str,
    ) -> Option<Option<*mut NativeDeclaredPropertySlot>> {
        let storage = self.cell.storage.borrow();
        if storage.layout.layout_id != layout_id
            || storage.native_declared_slots.is_none()
            || !storage.declared_slots.is_empty()
            || !storage.dynamic_properties.is_empty()
        {
            return None;
        }
        Some(
            storage
                .native_dynamic_properties
                .as_ref()?
                .get(name)
                .map(|cell| std::ptr::from_ref(&cell.slot).cast_mut()),
        )
    }

    /// Returns whether a property name belongs to the immutable declared
    /// layout while verifying that native property storage is authoritative.
    #[must_use]
    pub fn native_property_name_is_declared(&self, layout_id: u64, name: &str) -> Option<bool> {
        let storage = self.cell.storage.borrow();
        if storage.layout.layout_id != layout_id
            || storage.native_declared_slots.is_none()
            || !storage.declared_slots.is_empty()
            || !storage.dynamic_properties.is_empty()
        {
            return None;
        }
        Some(storage.layout.slot_by_name.contains_key(name))
    }

    /// Returns exact class metadata for `__isset`, when this is a real runtime
    /// object rather than a synthetic formatter view.
    #[must_use]
    pub fn native_has_magic_isset(&self) -> Option<bool> {
        self.cell.storage.borrow().native_magic_isset
    }

    /// Resolves a stable dynamic-property cell, reserving an uninitialized
    /// tombstone when the name has never existed. Reservation is not
    /// PHP-visible and does not consume an insertion-order number.
    #[must_use]
    pub fn ensure_native_dynamic_property_slot_location(
        &self,
        layout_id: u64,
        name: &str,
    ) -> Option<*mut NativeDeclaredPropertySlot> {
        let mut storage = self.cell.storage.borrow_mut();
        if storage.layout.layout_id != layout_id
            || storage.layout.slot_by_name.contains_key(name)
            || storage.native_declared_slots.is_none()
            || !storage.declared_slots.is_empty()
            || !storage.dynamic_properties.is_empty()
        {
            return None;
        }
        let order_clock = storage.native_dynamic_next_order.as_mut()?;
        let order_clock_pointer = std::ptr::from_mut(&mut **order_clock);
        let properties = storage.native_dynamic_properties.as_mut()?;
        let cell = properties.entry(name.to_owned()).or_insert_with(|| {
            let mut cell = NativeDynamicPropertyCell::new(NativeDeclaredPropertySlot::default(), 0);
            cell.next_insertion_order = order_clock_pointer;
            Box::new(cell)
        });
        Some(std::ptr::from_mut(&mut cell.slot))
    }

    /// Replaces or inserts one authoritative native dynamic-property owner.
    /// On rejection the supplied owner is returned unchanged to its caller.
    pub fn set_native_dynamic_property(
        &self,
        layout_id: u64,
        name: String,
        value: NativeDeclaredPropertySlot,
    ) -> Result<Option<NativeDeclaredPropertySlot>, NativeDeclaredPropertySlot> {
        let mut storage = self.cell.storage.borrow_mut();
        if storage.layout.layout_id != layout_id
            || storage.layout.slot_by_name.contains_key(&name)
            || storage.native_declared_slots.is_none()
            || !storage.declared_slots.is_empty()
            || !storage.dynamic_properties.is_empty()
        {
            return Err(value);
        }
        let active = storage
            .native_dynamic_properties
            .as_ref()
            .and_then(|properties| properties.get(&name))
            .is_some_and(|cell| cell.slot.initialized != 0);
        let Some(order_clock) = storage.native_dynamic_next_order.as_mut() else {
            return Err(value);
        };
        let order_clock_pointer = std::ptr::from_mut(&mut **order_clock);
        let insertion_order = if active {
            None
        } else {
            let order = **order_clock;
            **order_clock = order.saturating_add(1);
            Some(order)
        };
        if !active {
            storage.dynamic_order.retain(|entry| entry != &name);
            storage.dynamic_order.push(name.clone());
        }
        let properties = storage
            .native_dynamic_properties
            .as_mut()
            .expect("native dynamic storage checked above");
        let cell = properties.entry(name).or_insert_with(|| {
            Box::new(NativeDynamicPropertyCell::new(
                NativeDeclaredPropertySlot::default(),
                0,
            ))
        });
        let previous = (cell.slot.initialized != 0).then_some(cell.slot);
        cell.slot = value;
        if let Some(order) = insertion_order {
            cell.insertion_order = order;
        }
        cell.next_insertion_order = order_clock_pointer;
        Ok(previous)
    }

    /// Removes one authoritative native dynamic-property owner. The outer
    /// `Option` denotes whether native storage was available.
    pub fn unset_native_dynamic_property(
        &self,
        layout_id: u64,
        name: &str,
    ) -> Option<Option<NativeDeclaredPropertySlot>> {
        let mut storage = self.cell.storage.borrow_mut();
        if storage.layout.layout_id != layout_id
            || storage.layout.slot_by_name.contains_key(name)
            || storage.native_declared_slots.is_none()
            || !storage.declared_slots.is_empty()
            || !storage.dynamic_properties.is_empty()
        {
            return None;
        }
        let cell = storage.native_dynamic_properties.as_mut()?.get_mut(name);
        let removed = cell.and_then(|cell| {
            (cell.slot.initialized != 0).then(|| {
                let previous = cell.slot;
                cell.slot = NativeDeclaredPropertySlot::default();
                previous
            })
        });
        if removed.is_some() {
            storage.dynamic_order.retain(|entry| entry != name);
            storage.dynamic_debug_labels.remove(name);
        }
        Some(removed)
    }

    /// Borrows the complete authoritative property comparison view.
    ///
    /// Exact native comparison handlers use this to traverse class metadata,
    /// property names, and encoded values without reconstructing a Rust
    /// [`Value`] graph. Declared and dynamic values are both encoded native
    /// owners, while their object-owned names preserve PHP comparison order.
    pub fn with_native_comparison_view<R>(
        &self,
        layout_id: u64,
        compare: impl FnOnce(
            &str,
            &[String],
            &[NativeDeclaredPropertySlot],
            &[String],
            &NativeDynamicPropertySlots,
        ) -> R,
    ) -> Option<R> {
        let storage = self.cell.storage.borrow();
        if storage.layout.layout_id != layout_id || !storage.dynamic_properties.is_empty() {
            return None;
        }
        let slots = storage.native_declared_slots.as_deref()?;
        let dynamic = storage.native_dynamic_properties.as_ref()?;
        let mut dynamic_order: Vec<String> = dynamic
            .iter()
            .filter(|(_, cell)| cell.slot.initialized != 0)
            .map(|(name, _)| name.clone())
            .collect();
        dynamic_order.sort_by_key(|name| {
            dynamic
                .get(name)
                .map_or(u64::MAX, |cell| cell.insertion_order)
        });
        (slots.len() == storage.layout.slot_names.len()).then(|| {
            compare(
                storage.class_name.as_ref(),
                storage.layout.slot_names.as_slice(),
                slots,
                dynamic_order.as_slice(),
                dynamic,
            )
        })
    }

    /// Borrows the authoritative property sequence using PHP's object-to-array
    /// key encoding. Uninitialized declared properties are retained in the
    /// aligned slot slice so the native caller can omit them without
    /// reconstructing a Rust [`Value`] property snapshot.
    pub fn with_native_array_cast_view<R>(
        &self,
        layout_id: u64,
        cast: impl FnOnce(
            &[String],
            &[NativeDeclaredPropertySlot],
            &[String],
            &NativeDynamicPropertySlots,
        ) -> R,
    ) -> Option<R> {
        let storage = self.cell.storage.borrow();
        if storage.layout.layout_id != layout_id || !storage.dynamic_properties.is_empty() {
            return None;
        }
        let slots = storage.native_declared_slots.as_deref()?;
        let dynamic = storage.native_dynamic_properties.as_ref()?;
        let mut dynamic_order: Vec<String> = dynamic
            .iter()
            .filter(|(_, cell)| cell.slot.initialized != 0)
            .map(|(name, _)| name.clone())
            .collect();
        dynamic_order.sort_by_key(|name| {
            dynamic
                .get(name)
                .map_or(u64::MAX, |cell| cell.insertion_order)
        });
        (slots.len() == storage.layout.array_cast_names.len()).then(|| {
            cast(
                storage.layout.array_cast_names.as_slice(),
                slots,
                dynamic_order.as_slice(),
                dynamic,
            )
        })
    }

    /// Copies every authoritative native property record for a prepared
    /// shallow clone. The caller must retain each encoded value before
    /// installing the copy in another object identity.
    #[must_use]
    pub fn clone_native_property_slots(
        &self,
        layout_id: u64,
    ) -> Option<(
        Box<[NativeDeclaredPropertySlot]>,
        NativeDynamicPropertySlots,
    )> {
        let storage = self.cell.storage.borrow();
        if storage.layout.layout_id != layout_id {
            return None;
        }
        Some((
            storage.native_declared_slots.as_ref()?.clone(),
            storage.native_dynamic_properties.as_ref()?.clone(),
        ))
    }

    /// Removes all native property cells before a cold boundary reconstructs
    /// Rust values. Until [`Self::restore_property_slots_from_native`]
    /// succeeds, the object has no property-value representation.
    pub fn take_native_property_slots(
        &self,
        layout_id: u64,
    ) -> Option<(
        Box<[NativeDeclaredPropertySlot]>,
        NativeDynamicPropertySlots,
    )> {
        let mut storage = self.cell.storage.borrow_mut();
        if storage.layout.layout_id != layout_id
            || !storage.declared_slots.is_empty()
            || !storage.dynamic_properties.is_empty()
            || storage.native_declared_slots.is_none()
            || storage.native_dynamic_properties.is_none()
            || storage.native_dynamic_next_order.is_none()
        {
            return None;
        }
        let dynamic = storage
            .native_dynamic_properties
            .take()
            .expect("native dynamic slots checked above");
        let mut dynamic_order: Vec<String> = dynamic
            .iter()
            .filter(|(_, cell)| cell.slot.initialized != 0)
            .map(|(name, _)| name.clone())
            .collect();
        dynamic_order.sort_by_key(|name| {
            dynamic
                .get(name)
                .map_or(u64::MAX, |cell| cell.insertion_order)
        });
        storage.dynamic_order = dynamic_order;
        storage.native_dynamic_next_order = None;
        Some((
            storage
                .native_declared_slots
                .take()
                .expect("native declared slots checked above"),
            dynamic,
        ))
    }

    /// Restores all cold Rust property values after native owners were decoded
    /// and released.
    pub fn restore_property_slots_from_native(
        &self,
        layout_id: u64,
        declared: Vec<Option<Value>>,
        dynamic: HashMap<String, Value>,
    ) -> bool {
        let mut storage = self.cell.storage.borrow_mut();
        if storage.layout.layout_id != layout_id
            || storage.native_declared_slots.is_some()
            || storage.native_dynamic_properties.is_some()
            || storage.native_dynamic_next_order.is_some()
            || !storage.declared_slots.is_empty()
            || !storage.dynamic_properties.is_empty()
            || declared.len() != storage.layout.slot_names.len()
        {
            return false;
        }
        storage.declared_slots = declared;
        storage.dynamic_properties = dynamic;
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
