//! Incremental request-root membership for native runtime handles.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

use php_runtime::api::Value;
use php_runtime::experimental::{WeakArrayHandle, WeakObjectHandle, WeakReferenceHandle};

/// Object, array, and reference IDs are runtime-generated identities rather
/// than PHP-controlled keys. Hash them directly so call-root maintenance does
/// not run SipHash millions of times per request.
#[derive(Default)]
struct IdentityHasher(u64);

impl Hasher for IdentityHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0 = bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    }

    fn write_u64(&mut self, value: u64) {
        // Hashbrown consumes both bucket and high control bits. Runtime IDs
        // are often sequential, so a multiplicative mixer avoids the severe
        // clustering caused by a literal identity hash.
        self.0 = value.wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

type IdentityMap<V> = HashMap<u64, V, BuildHasherDefault<IdentityHasher>>;
type IdentitySet = HashSet<u64, BuildHasherDefault<IdentityHasher>>;

/// Why a request root changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RootMutationReason {
    GlobalOrStatic,
    Session,
    CallbackOrHandler,
    PendingThrowable,
    EnumOrStaticObject,
    NativeFrame,
    Suspension,
    ResourceOwned,
    RootedContainer,
}

/// Request-local object membership rebuilt only after a root mutation.
#[derive(Default)]
pub(super) struct RequestRootIndex {
    object_counts: IdentityMap<u32>,
    array_counts: IdentityMap<u32>,
    reference_counts: IdentityMap<u32>,
    roots: Vec<RootMembership>,
    root_handles: Vec<RootHandle>,
    fingerprints: Vec<RootFingerprint>,
    pending_containers: HashSet<RootFingerprint>,
    dirty: bool,
    generation: u64,
    rebuilds: u64,
    membership_traversals: u64,
    last_reason: Option<RootMutationReason>,
}

fn increment(counts: &mut IdentityMap<u32>, id: u64) {
    let count = counts.entry(id).or_default();
    *count = count.saturating_add(1);
}

fn decrement(counts: &mut IdentityMap<u32>, id: u64) {
    let Some(count) = counts.get_mut(&id) else {
        debug_assert!(false, "incremental root membership underflow for {id}");
        return;
    };
    *count -= 1;
    if *count == 0 {
        counts.remove(&id);
    }
}

impl RequestRootIndex {
    pub(super) fn new_dirty() -> Self {
        Self {
            dirty: true,
            ..Self::default()
        }
    }

    pub(super) fn mark_dirty(&mut self, reason: RootMutationReason) {
        debug_assert!(RootMutationReason::ALL.contains(&reason));
        self.dirty = true;
        self.last_reason = Some(reason);
    }

    pub(super) const fn is_dirty(&self) -> bool {
        self.dirty
    }

    #[cfg(test)]
    pub(super) fn replace(&mut self, membership: RootMembership) {
        self.object_counts.clear();
        self.array_counts.clear();
        self.reference_counts.clear();
        self.roots.clear();
        self.root_handles.clear();
        self.fingerprints.clear();
        self.pending_containers.clear();
        add_request_membership_counts(self, &membership);
        self.roots.push(membership);
        self.root_handles.push(RootHandle::Scalar);
        self.fingerprints.push(RootFingerprint::Scalar);
        self.dirty = false;
        self.generation = self.generation.saturating_add(1);
        self.rebuilds = self.rebuilds.saturating_add(1);
    }

    pub(super) fn rebuild(&mut self, roots: &[Value]) {
        self.object_counts.clear();
        self.array_counts.clear();
        self.reference_counts.clear();
        self.roots.clear();
        self.root_handles.clear();
        self.fingerprints.clear();
        self.pending_containers.clear();
        for root in roots {
            let membership = collect_root_membership(std::iter::once(root));
            self.membership_traversals = self.membership_traversals.saturating_add(1);
            add_request_membership_counts(self, &membership);
            self.roots.push(membership);
            self.root_handles.push(RootHandle::of(root));
            self.fingerprints.push(RootFingerprint::of(root));
        }
        self.dirty = false;
        self.generation = self.generation.saturating_add(1);
        self.rebuilds = self.rebuilds.saturating_add(1);
    }

    /// Recompute only stable request roots that reach a mutated container.
    /// Root-set additions/removals mark the index dirty separately, so an
    /// unchanged root count preserves the per-root slot mapping here.
    pub(super) fn refresh_container(&mut self, value: &Value) {
        if self.dirty {
            if let Some(fingerprint) = RootFingerprint::container(value) {
                self.pending_containers.insert(fingerprint);
            }
            return;
        }
        let affected = self
            .roots
            .iter()
            .enumerate()
            .filter_map(|(index, membership)| membership.contains_container(value).then_some(index))
            .collect::<Vec<_>>();
        for index in affected {
            let Some(root) = self.root_handles[index].upgrade() else {
                self.mark_dirty(RootMutationReason::RootedContainer);
                return;
            };
            let replacement = collect_root_membership(std::iter::once(&root));
            self.membership_traversals = self.membership_traversals.saturating_add(1);
            let previous = std::mem::replace(&mut self.roots[index], replacement);
            remove_request_membership_counts(self, &previous);
            let replacement = std::mem::take(&mut self.roots[index]);
            add_request_membership_counts(self, &replacement);
            self.roots[index] = replacement;
            self.fingerprints[index] = RootFingerprint::of(&root);
        }
    }

    pub(super) fn add_nested_container(&mut self, parent: &Value, child: &Value) {
        if self.dirty {
            if let Some(fingerprint) = RootFingerprint::container(parent) {
                self.pending_containers.insert(fingerprint);
            }
            return;
        }
        let affected = self
            .roots
            .iter()
            .enumerate()
            .filter_map(|(index, membership)| {
                membership.contains_container(parent).then_some(index)
            })
            .collect::<Vec<_>>();
        for index in affected {
            if let Some(fingerprint) = self.roots[index].insert_container(child) {
                increment_fingerprint(
                    &mut self.object_counts,
                    &mut self.array_counts,
                    &mut self.reference_counts,
                    fingerprint,
                );
            }
        }
    }

    /// Reconcile root replacements after a helper mutates globals, statics,
    /// callbacks, handlers, or other stable root slots. Unchanged slots are
    /// identified without traversing their object graphs; only replaced or
    /// explicitly mutated roots are rescanned.
    pub(super) fn synchronize(&mut self, roots: &[Value]) -> bool {
        if self.roots.len() != self.root_handles.len()
            || self.roots.len() != self.fingerprints.len()
            || self.roots.is_empty()
        {
            self.rebuild(roots);
            return true;
        }
        if self.roots.len() != roots.len() {
            self.synchronize_changed_shape(roots);
            return true;
        }
        let affected = roots
            .iter()
            .enumerate()
            .filter_map(|(index, root)| {
                let identity_changed = self.fingerprints[index] != RootFingerprint::of(root);
                let contains_pending = self
                    .pending_containers
                    .iter()
                    .any(|container| self.roots[index].contains_fingerprint(*container));
                (identity_changed || contains_pending).then_some(index)
            })
            .collect::<Vec<_>>();
        for index in affected {
            let replacement = collect_root_membership(std::iter::once(&roots[index]));
            self.membership_traversals = self.membership_traversals.saturating_add(1);
            let previous = std::mem::replace(&mut self.roots[index], replacement);
            remove_request_membership_counts(self, &previous);
            let replacement = std::mem::take(&mut self.roots[index]);
            add_request_membership_counts(self, &replacement);
            self.roots[index] = replacement;
            self.root_handles[index] = RootHandle::of(&roots[index]);
            self.fingerprints[index] = RootFingerprint::of(&roots[index]);
        }
        self.pending_containers.clear();
        self.dirty = false;
        true
    }

    /// Reconcile root insertions and removals without rebuilding every root
    /// shifted by the shape change. Request roots are emitted in stable map
    /// order, so their unchanged prefix and suffix retain exact memberships;
    /// only the changed middle needs graph traversal.
    fn synchronize_changed_shape(&mut self, roots: &[Value]) {
        let old_len = self.roots.len();
        let new_len = roots.len();
        let reusable = |old_index: usize, root: &Value| {
            self.fingerprints[old_index] == RootFingerprint::of(root)
                && !self
                    .pending_containers
                    .iter()
                    .any(|container| self.roots[old_index].contains_fingerprint(*container))
        };
        let mut prefix = 0;
        while prefix < old_len && prefix < new_len && reusable(prefix, &roots[prefix]) {
            prefix += 1;
        }
        let mut suffix = 0;
        while suffix < old_len.saturating_sub(prefix)
            && suffix < new_len.saturating_sub(prefix)
            && reusable(old_len - suffix - 1, &roots[new_len - suffix - 1])
        {
            suffix += 1;
        }

        let old_middle_end = old_len - suffix;
        let mut old_roots = std::mem::take(&mut self.roots);
        let mut old_handles = std::mem::take(&mut self.root_handles);
        let mut old_fingerprints = std::mem::take(&mut self.fingerprints);
        let old_middle_roots = old_roots.split_off(prefix);
        let old_middle_handles = old_handles.split_off(prefix);
        let old_middle_fingerprints = old_fingerprints.split_off(prefix);
        let old_middle_len = old_middle_end - prefix;
        let mut old_middle_roots = old_middle_roots;
        let mut old_middle_handles = old_middle_handles;
        let mut old_middle_fingerprints = old_middle_fingerprints;
        let suffix_roots = old_middle_roots.split_off(old_middle_len);
        let suffix_handles = old_middle_handles.split_off(old_middle_len);
        let suffix_fingerprints = old_middle_fingerprints.split_off(old_middle_len);
        for membership in &old_middle_roots {
            remove_request_membership_counts(self, membership);
        }

        let new_middle_end = new_len - suffix;
        old_roots.reserve(new_len.saturating_sub(old_roots.len()));
        old_handles.reserve(new_len.saturating_sub(old_handles.len()));
        old_fingerprints.reserve(new_len.saturating_sub(old_fingerprints.len()));
        for root in &roots[prefix..new_middle_end] {
            let membership = collect_root_membership(std::iter::once(root));
            self.membership_traversals = self.membership_traversals.saturating_add(1);
            add_request_membership_counts(self, &membership);
            old_roots.push(membership);
            old_handles.push(RootHandle::of(root));
            old_fingerprints.push(RootFingerprint::of(root));
        }
        old_roots.extend(suffix_roots);
        old_handles.extend(suffix_handles);
        old_fingerprints.extend(suffix_fingerprints);
        self.roots = old_roots;
        self.root_handles = old_handles;
        self.fingerprints = old_fingerprints;
        self.pending_containers.clear();
        self.dirty = false;
        self.generation = self.generation.saturating_add(1);
    }

    pub(super) fn contains(&self, object_id: u64) -> bool {
        self.object_counts.contains_key(&object_id)
    }

    pub(super) fn contains_container(&self, value: &Value) -> bool {
        match value {
            Value::Object(object) => self.object_counts.contains_key(&object.id()),
            Value::Array(array) => self.array_counts.contains_key(&array.gc_debug_id()),
            Value::Reference(reference) => {
                self.reference_counts.contains_key(&reference.gc_debug_id())
            }
            _ => false,
        }
    }

    pub(super) fn last_reason(&self) -> RootMutationReason {
        self.last_reason
            .unwrap_or(RootMutationReason::RootedContainer)
    }

    pub(super) const fn membership_traversals(&self) -> u64 {
        self.membership_traversals
    }

    #[cfg(test)]
    pub(super) const fn rebuilds(&self) -> u64 {
        self.rebuilds
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum RootFingerprint {
    Scalar,
    Object(u64),
    Array(u64),
    Reference(u64),
}

#[derive(Clone, Debug)]
enum RootHandle {
    Scalar,
    Object(WeakObjectHandle),
    Array(WeakArrayHandle),
    Reference(WeakReferenceHandle),
}

impl RootHandle {
    fn of(value: &Value) -> Self {
        match value {
            Value::Object(object) => Self::Object(object.weak_handle()),
            Value::Array(array) => Self::Array(array.weak_handle()),
            Value::Reference(reference) => Self::Reference(reference.weak_handle()),
            _ => Self::Scalar,
        }
    }

    fn upgrade(&self) -> Option<Value> {
        match self {
            Self::Scalar => Some(Value::Null),
            Self::Object(object) => object.upgrade().map(Value::Object),
            Self::Array(array) => array.upgrade().map(Value::Array),
            Self::Reference(reference) => reference.upgrade().map(Value::Reference),
        }
    }
}

impl RootFingerprint {
    fn of(value: &Value) -> Self {
        Self::container(value).unwrap_or(Self::Scalar)
    }

    fn container(value: &Value) -> Option<Self> {
        match value {
            Value::Object(object) => Some(Self::Object(object.id())),
            Value::Array(array) => Some(Self::Array(array.gc_debug_id())),
            Value::Reference(reference) => Some(Self::Reference(reference.gc_debug_id())),
            _ => None,
        }
    }
}

fn add_request_membership_counts(index: &mut RequestRootIndex, membership: &RootMembership) {
    for object in membership.objects.iter() {
        increment(&mut index.object_counts, *object);
    }
    for array in membership.arrays.iter() {
        increment(&mut index.array_counts, *array);
    }
    for reference in membership.references.iter() {
        increment(&mut index.reference_counts, *reference);
    }
}

fn remove_request_membership_counts(index: &mut RequestRootIndex, membership: &RootMembership) {
    for object in membership.objects.iter() {
        decrement(&mut index.object_counts, *object);
    }
    for array in membership.arrays.iter() {
        decrement(&mut index.array_counts, *array);
    }
    for reference in membership.references.iter() {
        decrement(&mut index.reference_counts, *reference);
    }
}

impl RootMutationReason {
    const ALL: [Self; 9] = [
        Self::GlobalOrStatic,
        Self::Session,
        Self::CallbackOrHandler,
        Self::PendingThrowable,
        Self::EnumOrStaticObject,
        Self::NativeFrame,
        Self::Suspension,
        Self::ResourceOwned,
        Self::RootedContainer,
    ];

    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::GlobalOrStatic => "global_or_static",
            Self::Session => "session",
            Self::CallbackOrHandler => "callback_or_handler",
            Self::PendingThrowable => "pending_throwable",
            Self::EnumOrStaticObject => "enum_or_static_object",
            Self::NativeFrame => "native_frame",
            Self::Suspension => "suspension",
            Self::ResourceOwned => "resource_owned",
            Self::RootedContainer => "rooted_container",
        }
    }
}

/// Traverse all currently published roots once and collect every reachable
/// object. Cycles through objects and references are bounded by identity.
#[derive(Clone, Default)]
pub(super) struct RootMembership {
    objects: IdentitySet,
    arrays: IdentitySet,
    references: IdentitySet,
}

impl RootMembership {
    fn contains_container(&self, value: &Value) -> bool {
        match value {
            Value::Object(object) => self.objects.contains(&object.id()),
            Value::Array(array) => self.arrays.contains(&array.gc_debug_id()),
            Value::Reference(reference) => self.references.contains(&reference.gc_debug_id()),
            _ => false,
        }
    }

    fn contains_fingerprint(&self, fingerprint: RootFingerprint) -> bool {
        match fingerprint {
            RootFingerprint::Scalar => false,
            RootFingerprint::Object(id) => self.objects.contains(&id),
            RootFingerprint::Array(id) => self.arrays.contains(&id),
            RootFingerprint::Reference(id) => self.references.contains(&id),
        }
    }

    fn insert_container(&mut self, value: &Value) -> Option<RootFingerprint> {
        let fingerprint = RootFingerprint::container(value)?;
        let inserted = match fingerprint {
            RootFingerprint::Scalar => false,
            RootFingerprint::Object(id) => self.objects.insert(id),
            RootFingerprint::Array(id) => self.arrays.insert(id),
            RootFingerprint::Reference(id) => self.references.insert(id),
        };
        inserted.then_some(fingerprint)
    }
}

fn increment_fingerprint(
    objects: &mut IdentityMap<u32>,
    arrays: &mut IdentityMap<u32>,
    references: &mut IdentityMap<u32>,
    fingerprint: RootFingerprint,
) {
    match fingerprint {
        RootFingerprint::Scalar => {}
        RootFingerprint::Object(id) => increment(objects, id),
        RootFingerprint::Array(id) => increment(arrays, id),
        RootFingerprint::Reference(id) => increment(references, id),
    }
}

pub(super) fn collect_root_membership<'a>(
    roots: impl IntoIterator<Item = &'a Value>,
) -> RootMembership {
    fn visit(value: &Value, membership: &mut RootMembership) {
        match value {
            Value::Object(object) => {
                if !membership.objects.insert(object.id()) {
                    return;
                }
                let _ = object.try_any_property_value(|value| {
                    visit(value, membership);
                    false
                });
            }
            Value::Array(array) => {
                if !membership.arrays.insert(array.gc_debug_id()) {
                    return;
                }
                for (_, value) in array.iter() {
                    visit(value, membership);
                }
            }
            Value::Reference(reference) => {
                if !membership.references.insert(reference.gc_debug_id()) {
                    return;
                }
                let _ = reference.try_with_value(|value| visit(value, membership));
            }
            _ => {}
        }
    }

    let mut membership = RootMembership::default();
    for root in roots {
        visit(root, &mut membership);
    }
    membership
}

/// Native handles already retain live values. Only destructor release needs
/// transitive membership, so inspect those values on that cold path.
pub(super) fn values_contain_object<'a>(
    values: impl IntoIterator<Item = &'a Value>,
    object_id: u64,
) -> bool {
    collect_root_membership(values).objects.contains(&object_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_index_reuses_membership_until_marked_dirty() {
        let mut index = RequestRootIndex::new_dirty();
        index.replace(RootMembership {
            objects: [7].into_iter().collect(),
            ..RootMembership::default()
        });
        assert!(index.contains(7));
        assert_eq!(index.rebuilds(), 1);
        index.mark_dirty(RootMutationReason::RootedContainer);
        assert!(index.is_dirty());
    }

    #[test]
    fn nested_container_membership_is_cycle_safe_and_identity_based() {
        let reference = php_runtime::api::ReferenceCell::new(Value::Null);
        let array =
            php_runtime::api::PhpArray::from_packed(vec![Value::Reference(reference.clone())]);
        reference.set(Value::Array(array.clone()));

        let membership = collect_root_membership([&Value::Reference(reference.clone())]);
        let mut index = RequestRootIndex::new_dirty();
        index.replace(membership);

        assert!(index.contains_container(&Value::Reference(reference.clone())));
        assert!(index.contains_container(&Value::Array(array)));
        assert_eq!(index.rebuilds(), 1);
        reference.set(Value::Null);
    }

    #[test]
    fn stable_root_container_refresh_updates_only_affected_root_counts() {
        let array = php_runtime::api::PhpArray::from_packed(vec![Value::Int(1)]);
        let first = php_runtime::api::ReferenceCell::new(Value::Array(array.clone()));
        let second = php_runtime::api::ReferenceCell::new(Value::Array(array.clone()));
        let roots = vec![
            Value::Reference(first.clone()),
            Value::Reference(second.clone()),
        ];
        let mut index = RequestRootIndex::new_dirty();
        index.rebuild(&roots);
        assert!(index.contains_container(&Value::Array(array.clone())));

        first.set(Value::Null);
        index.refresh_container(&Value::Reference(first.clone()));
        assert!(!index.is_dirty());
        assert!(index.contains_container(&Value::Array(array.clone())));

        second.set(Value::Null);
        index.refresh_container(&Value::Reference(second.clone()));
        assert!(!index.is_dirty());
        assert!(!index.contains_container(&Value::Array(array)));
    }

    #[test]
    fn dirty_stable_roots_synchronize_replacements_and_pending_containers() {
        let old = php_runtime::api::PhpArray::from_packed(vec![Value::Int(1)]);
        let new = php_runtime::api::PhpArray::from_packed(vec![Value::Int(2)]);
        let reference = php_runtime::api::ReferenceCell::new(Value::Array(old.clone()));
        let mut roots = vec![Value::Reference(reference.clone()), Value::Int(1)];
        let mut index = RequestRootIndex::new_dirty();
        index.rebuild(&roots);

        index.mark_dirty(RootMutationReason::GlobalOrStatic);
        reference.set(Value::Array(new.clone()));
        index.refresh_container(&Value::Reference(reference.clone()));
        assert!(index.synchronize(&roots));
        assert!(!index.is_dirty());
        assert!(!index.contains_container(&Value::Array(old)));
        assert!(index.contains_container(&Value::Array(new.clone())));

        let replacement = php_runtime::api::PhpArray::from_packed(vec![Value::Int(3)]);
        index.mark_dirty(RootMutationReason::GlobalOrStatic);
        roots[0] = Value::Array(replacement.clone());
        assert!(index.synchronize(&roots));
        assert!(!index.contains_container(&Value::Array(new)));
        assert!(index.contains_container(&Value::Array(replacement.clone())));

        index.mark_dirty(RootMutationReason::GlobalOrStatic);
        roots.push(Value::Null);
        let traversals = index.membership_traversals();
        assert!(index.synchronize(&roots));
        assert!(!index.is_dirty());
        assert!(index.contains_container(&Value::Array(replacement)));
        assert_eq!(index.membership_traversals(), traversals + 1);
    }

    #[test]
    fn live_value_membership_is_computed_only_when_a_destructor_queries_it() {
        let class = php_runtime::api::ClassEntry {
            name: std::sync::Arc::from("nested"),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: php_runtime::api::ClassFlags::default(),
        };
        let object = php_runtime::api::ObjectRef::new(&class);
        let nested = php_runtime::api::PhpArray::from_packed(vec![Value::Object(object.clone())]);
        let values = [Value::Array(nested)];

        assert!(values_contain_object(&values, object.id()));
        assert!(!values_contain_object(&values, object.id() + 1));
    }
}
