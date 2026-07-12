//! GC metadata and cycle-candidate scanning for runtime-semantics.
//!
//! This module is intentionally a debug/test surface, not a PHP-visible API.
//! It snapshots the current runtime graph without collecting anything. The VM
//! can use it to prove root tracking and cycle-candidate discovery while full
//! PHP refcount/GC behavior remains a documented later step.

use crate::{
    CallableMethodTarget, CallableValue, Slot, Value, WeakArrayHandle, WeakObjectHandle,
    WeakReferenceHandle,
};
use std::collections::{BTreeMap, BTreeSet};

/// Refcounted runtime entity kind known to the runtime-semantics GC skeleton.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum GcEntityKind {
    /// Ordered PHP array storage.
    Array,
    /// Runtime object storage.
    Object,
    /// Shared reference cell.
    Reference,
    /// Closure value and its captured values.
    Closure,
    /// Reserved generator stack/storage category.
    Generator,
    /// Reserved fiber stack/storage category.
    Fiber,
    /// Strings are currently inline/owned by `PhpString` and are tracked only
    /// as a documented non-refcounted category.
    String,
}

/// Stable debug identity for one GC graph entity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GcEntityId {
    /// Entity kind.
    pub kind: GcEntityKind,
    /// Process-local ID. This is not PHP-visible and not stable across runs.
    pub id: u64,
}

impl GcEntityId {
    /// Creates a GC entity ID.
    #[must_use]
    pub const fn new(kind: GcEntityKind, id: u64) -> Self {
        Self { kind, id }
    }
}

/// Root-set category for GC snapshots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GcRootKind {
    /// VM frame register.
    FrameRegister,
    /// VM frame local slot.
    FrameLocal,
    /// `$GLOBALS` or superglobal storage.
    Global,
    /// Function or method static local.
    StaticLocal,
    /// Static class property or class-table-owned value.
    ClassTable,
    /// Generator suspended stack.
    GeneratorStack,
    /// Fiber suspended stack.
    FiberStack,
    /// VM temporary value.
    Temporary,
    /// Shutdown destructor queue.
    DestructorQueue,
}

/// One root value included in a GC snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GcRoot {
    /// Root kind.
    pub kind: GcRootKind,
    /// Stable debug name such as `frame0.local1`.
    pub name: String,
    /// Effective root value.
    pub value: Value,
}

impl GcRoot {
    /// Creates a root from an effective value.
    #[must_use]
    pub fn value(kind: GcRootKind, name: impl Into<String>, value: Value) -> Self {
        Self {
            kind,
            name: name.into(),
            value,
        }
    }

    /// Creates a root from a slot, dereferencing aliases like the VM would.
    #[must_use]
    pub fn slot(kind: GcRootKind, name: impl Into<String>, slot: &Slot) -> Self {
        Self::value(kind, name, slot.read())
    }
}

/// One scanned refcounted entity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GcNode {
    /// Entity ID.
    pub id: GcEntityId,
    /// Estimated Rust storage refcount when available.
    pub refcount_estimate: Option<usize>,
    /// Outgoing references to other entities.
    pub edges: Vec<GcEntityId>,
    /// Roots that point directly at this entity.
    pub roots: Vec<String>,
}

impl GcNode {
    fn new(id: GcEntityId, refcount_estimate: Option<usize>) -> Self {
        Self {
            id,
            refcount_estimate,
            edges: Vec::new(),
            roots: Vec::new(),
        }
    }
}

/// Cycle candidate discovered by the skeleton scanner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GcCycleCandidate {
    /// Entity that can reach itself through the scanned graph.
    pub root: GcEntityId,
}

/// Immutable GC graph snapshot.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GcSnapshot {
    /// Scanned nodes keyed by entity ID.
    pub nodes: BTreeMap<GcEntityId, GcNode>,
    /// Candidate cyclic entities. This is detection only, not collection.
    pub cycle_candidates: Vec<GcCycleCandidate>,
}

impl GcSnapshot {
    /// Returns true when a node exists.
    #[must_use]
    pub fn contains(&self, id: GcEntityId) -> bool {
        self.nodes.contains_key(&id)
    }
}

/// Scans a set of roots into a deterministic GC graph snapshot.
#[must_use]
pub fn scan_roots(roots: impl IntoIterator<Item = GcRoot>) -> GcSnapshot {
    let mut scanner = GcScanner::default();
    for root in roots {
        let ids = scanner.scan_value(&root.value);
        for id in ids {
            if let Some(node) = scanner.nodes.get_mut(&id) {
                node.roots.push(root.name.clone());
            }
        }
    }
    let cycle_candidates = scanner.cycle_candidates();
    GcSnapshot {
        nodes: scanner.nodes,
        cycle_candidates,
    }
}

/// One entity cleared by the internal cycle-collection test hook.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GcCollectedEntity {
    /// Entity ID.
    pub id: GcEntityId,
}

/// Result from the internal cycle-collection test hook.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GcCollectResult {
    /// Unrooted entities whose outgoing edges were cleared.
    pub collected: Vec<GcCollectedEntity>,
    /// Cycle candidates still visible after root scanning.
    pub remaining_candidates: Vec<GcCycleCandidate>,
}

/// Internal weak-handle heap used by GC tests.
///
/// This is not a production allocator. It tracks weak handles discovered from
/// explicit test values and can break unrooted object/reference cycles so tests
/// can prove the collector collector skeleton is not permanently retaining
/// simple cycles.
#[derive(Clone, Debug, Default)]
pub struct GcTrackedHeap {
    arrays: Vec<WeakArrayHandle>,
    objects: Vec<WeakObjectHandle>,
    references: Vec<WeakReferenceHandle>,
}

impl GcTrackedHeap {
    /// Creates an empty tracked heap.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            arrays: Vec::new(),
            objects: Vec::new(),
            references: Vec::new(),
        }
    }

    /// Tracks weak handles reachable from `value`.
    pub fn track_value(&mut self, value: &Value) {
        self.track_value_inner(value, &mut BTreeSet::new());
    }

    /// Runs the first deterministic cycle-collection test hook.
    ///
    /// Unrooted objects have properties cleared and unrooted reference cells are
    /// reset to `Uninitialized`. This breaks simple object cycles and
    /// reference-mediated array cycles, but it does not attempt Zend-compatible
    /// collection order or public GC function behavior.
    #[must_use]
    pub fn collect_cycles(&mut self, roots: impl IntoIterator<Item = GcRoot>) -> GcCollectResult {
        let snapshot = scan_roots(roots);
        let mut collected = Vec::new();

        for handle in &self.objects {
            let id = GcEntityId::new(GcEntityKind::Object, handle.id());
            if snapshot.contains(id) {
                continue;
            }
            if let Some(object) = handle.upgrade() {
                object.gc_clear_properties();
                collected.push(GcCollectedEntity { id });
            }
        }

        for handle in &self.references {
            let id = GcEntityId::new(GcEntityKind::Reference, handle.id());
            if snapshot.contains(id) {
                continue;
            }
            if let Some(cell) = handle.upgrade() {
                cell.gc_clear();
                collected.push(GcCollectedEntity { id });
            }
        }

        self.retain_live_handles();
        GcCollectResult {
            collected,
            remaining_candidates: snapshot.cycle_candidates,
        }
    }

    /// Counts tracked weak handles that still point at live storage.
    #[must_use]
    pub fn live_handle_count(&self) -> usize {
        self.arrays
            .iter()
            .filter(|handle| handle.is_alive())
            .count()
            + self
                .objects
                .iter()
                .filter(|handle| handle.is_alive())
                .count()
            + self
                .references
                .iter()
                .filter(|handle| handle.is_alive())
                .count()
    }

    fn track_value_inner(&mut self, value: &Value, seen: &mut BTreeSet<GcEntityId>) {
        match value {
            Value::Array(array) => {
                let id = GcEntityId::new(GcEntityKind::Array, array.gc_debug_id());
                if !seen.insert(id) {
                    return;
                }
                if !self
                    .arrays
                    .iter()
                    .any(|handle| handle.id() == array.gc_debug_id())
                {
                    self.arrays.push(array.weak_handle());
                }
                for (_, value) in array.iter() {
                    self.track_value_inner(value, seen);
                }
            }
            Value::Object(object) => {
                let id = GcEntityId::new(GcEntityKind::Object, object.id());
                if !seen.insert(id) {
                    return;
                }
                if !self.objects.iter().any(|handle| handle.id() == object.id()) {
                    self.objects.push(object.weak_handle());
                }
                for (_, value) in object.properties_snapshot() {
                    self.track_value_inner(&value, seen);
                }
            }
            Value::Generator(generator) => {
                let id = GcEntityId::new(GcEntityKind::Generator, generator.id());
                let _ = seen.insert(id);
            }
            Value::Fiber(fiber) => {
                let id = GcEntityId::new(GcEntityKind::Fiber, fiber.id());
                if seen.insert(id) {
                    self.track_value_inner(&fiber.callable(), seen);
                }
            }
            Value::Reference(cell) => {
                let id = GcEntityId::new(GcEntityKind::Reference, cell.gc_debug_id());
                if !seen.insert(id) {
                    return;
                }
                if !self
                    .references
                    .iter()
                    .any(|handle| handle.id() == cell.gc_debug_id())
                {
                    self.references.push(cell.weak_handle());
                }
                let value = cell.get();
                self.track_value_inner(&value, seen);
            }
            Value::Callable(callable) => match callable.as_ref() {
                CallableValue::Closure(payload) => {
                    if let Some(bound_this) = &payload.bound_this {
                        self.track_value_inner(&Value::Object(bound_this.clone()), seen);
                    }
                    for capture in &payload.captures {
                        if let Some(value) = capture.value() {
                            self.track_value_inner(value, seen);
                        }
                        if let Some(cell) = capture.reference() {
                            self.track_value_inner(&Value::Reference(cell), seen);
                        }
                    }
                }
                CallableValue::BoundMethod {
                    target: CallableMethodTarget::Object(object),
                    ..
                } => self.track_value_inner(&Value::Object(object.clone()), seen),
                CallableValue::UserFunction { .. }
                | CallableValue::InternalBuiltin { .. }
                | CallableValue::BoundMethod {
                    target: CallableMethodTarget::Class(_),
                    ..
                }
                | CallableValue::MethodPlaceholder { .. }
                | CallableValue::UnresolvedDynamic { .. } => {}
            },
            Value::Null
            | Value::Bool(_)
            | Value::Int(_)
            | Value::Float(_)
            | Value::String(_)
            | Value::Resource(_)
            | Value::Uninitialized => {}
        }
    }

    fn retain_live_handles(&mut self) {
        self.arrays.retain(WeakArrayHandle::is_alive);
        self.objects.retain(WeakObjectHandle::is_alive);
        self.references.retain(WeakReferenceHandle::is_alive);
    }
}

#[derive(Default)]
struct GcScanner {
    nodes: BTreeMap<GcEntityId, GcNode>,
    scanning: BTreeSet<GcEntityId>,
    next_closure_id: u64,
}

impl GcScanner {
    fn scan_value(&mut self, value: &Value) -> Vec<GcEntityId> {
        match value {
            Value::Array(array) => {
                let id = GcEntityId::new(GcEntityKind::Array, array.gc_debug_id());
                self.ensure_node(id, Some(array.gc_refcount_estimate()));
                if self.scanning.insert(id) {
                    let edges = array
                        .iter()
                        .flat_map(|(_, value)| self.scan_value(value))
                        .collect::<Vec<_>>();
                    self.extend_edges(id, edges);
                    self.scanning.remove(&id);
                }
                vec![id]
            }
            Value::Object(object) => {
                let id = GcEntityId::new(GcEntityKind::Object, object.id());
                self.ensure_node(id, Some(object.gc_refcount_estimate()));
                if self.scanning.insert(id) {
                    let edges = object
                        .properties_snapshot()
                        .iter()
                        .flat_map(|(_, value)| self.scan_value(value))
                        .collect::<Vec<_>>();
                    self.extend_edges(id, edges);
                    self.scanning.remove(&id);
                }
                vec![id]
            }
            Value::Generator(generator) => {
                let id = GcEntityId::new(GcEntityKind::Generator, generator.id());
                self.ensure_node(id, None);
                vec![id]
            }
            Value::Fiber(fiber) => {
                let id = GcEntityId::new(GcEntityKind::Fiber, fiber.id());
                self.ensure_node(id, None);
                if self.scanning.insert(id) {
                    let edges = self.scan_value(&fiber.callable());
                    self.extend_edges(id, edges);
                    self.scanning.remove(&id);
                }
                vec![id]
            }
            Value::Reference(cell) => {
                let id = GcEntityId::new(GcEntityKind::Reference, cell.gc_debug_id());
                self.ensure_node(id, Some(cell.gc_refcount_estimate()));
                if self.scanning.insert(id) {
                    let value = cell.get();
                    let edges = self.scan_value(&value);
                    self.extend_edges(id, edges);
                    self.scanning.remove(&id);
                }
                vec![id]
            }
            Value::Callable(callable) => match callable.as_ref() {
                CallableValue::Closure(payload) => {
                    self.next_closure_id = self.next_closure_id.saturating_add(1);
                    let id = GcEntityId::new(GcEntityKind::Closure, self.next_closure_id);
                    self.ensure_node(id, None);
                    let edges = payload
                        .bound_this
                        .as_ref()
                        .map(|object| self.scan_value(&Value::Object(object.clone())))
                        .unwrap_or_default()
                        .into_iter()
                        .chain(payload.captures.iter().flat_map(|capture| {
                            capture
                                .value()
                                .map_or_else(Vec::new, |value| self.scan_value(value))
                                .into_iter()
                                .chain(
                                    capture
                                        .reference()
                                        .map(|cell| self.scan_value(&Value::Reference(cell)))
                                        .unwrap_or_default(),
                                )
                        }))
                        .collect::<Vec<_>>();
                    self.extend_edges(id, edges);
                    vec![id]
                }
                CallableValue::BoundMethod {
                    target: CallableMethodTarget::Object(object),
                    ..
                } => self.scan_value(&Value::Object(object.clone())),
                CallableValue::UserFunction { .. }
                | CallableValue::InternalBuiltin { .. }
                | CallableValue::BoundMethod {
                    target: CallableMethodTarget::Class(_),
                    ..
                }
                | CallableValue::MethodPlaceholder { .. }
                | CallableValue::UnresolvedDynamic { .. } => Vec::new(),
            },
            Value::Null
            | Value::Bool(_)
            | Value::Int(_)
            | Value::Float(_)
            | Value::String(_)
            | Value::Resource(_)
            | Value::Uninitialized => Vec::new(),
        }
    }

    fn ensure_node(&mut self, id: GcEntityId, refcount_estimate: Option<usize>) {
        self.nodes
            .entry(id)
            .or_insert_with(|| GcNode::new(id, refcount_estimate));
    }

    fn extend_edges(&mut self, from: GcEntityId, edges: Vec<GcEntityId>) {
        let Some(node) = self.nodes.get_mut(&from) else {
            return;
        };
        for edge in edges {
            if !node.edges.contains(&edge) {
                node.edges.push(edge);
            }
        }
        node.edges.sort();
    }

    fn cycle_candidates(&self) -> Vec<GcCycleCandidate> {
        self.nodes
            .keys()
            .copied()
            .filter(|id| self.reaches(*id, *id, &mut BTreeSet::new()))
            .map(|root| GcCycleCandidate { root })
            .collect()
    }

    fn reaches(
        &self,
        current: GcEntityId,
        target: GcEntityId,
        seen: &mut BTreeSet<GcEntityId>,
    ) -> bool {
        let Some(node) = self.nodes.get(&current) else {
            return false;
        };
        for edge in &node.edges {
            if *edge == target {
                return true;
            }
            if seen.insert(*edge) && self.reaches(*edge, target, seen) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{GcEntityId, GcEntityKind, GcRoot, GcRootKind, GcTrackedHeap, scan_roots};
    use crate::{
        ClassEntry, ClassFlags, ClosureCaptureValue, ObjectRef, PhpArray, ReferenceCell, Slot,
        Value,
    };

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
    fn gc_scans_roots_and_refcount_metadata_without_panics() {
        let class = empty_class("Box");
        let object = ObjectRef::new(&class);
        let mut array = PhpArray::new();
        array.append(Value::Object(object.clone()));
        let root = GcRoot::value(GcRootKind::Global, "globals.a", Value::Array(array.clone()));

        let snapshot = scan_roots([root]);
        let array_id = GcEntityId::new(GcEntityKind::Array, array.gc_debug_id());
        let object_id = GcEntityId::new(GcEntityKind::Object, object.id());

        assert!(snapshot.contains(array_id));
        assert!(snapshot.contains(object_id));
        assert_eq!(snapshot.nodes[&array_id].edges, vec![object_id]);
        assert_eq!(snapshot.nodes[&array_id].roots, vec!["globals.a"]);
        assert!(snapshot.nodes[&object_id].refcount_estimate.is_some());
    }

    #[test]
    fn gc_detects_object_self_cycle_candidate() {
        let class = empty_class("Node");
        let object = ObjectRef::new(&class);
        object.set_property("self", Value::Object(object.clone()));

        let snapshot = scan_roots([GcRoot::value(
            GcRootKind::FrameLocal,
            "frame0.local0",
            Value::Object(object.clone()),
        )]);
        let object_id = GcEntityId::new(GcEntityKind::Object, object.id());

        assert!(
            snapshot
                .cycle_candidates
                .iter()
                .any(|candidate| candidate.root == object_id)
        );
    }

    #[test]
    fn gc_scans_reference_and_closure_capture_edges() {
        let class = empty_class("Captured");
        let object = ObjectRef::new(&class);
        let cell = ReferenceCell::new(Value::Object(object.clone()));
        let closure = Value::closure(crate::ClosurePayload::new(
            7,
            vec![ClosureCaptureValue::by_reference(
                "x".to_owned(),
                cell.clone(),
            )],
        ));

        let snapshot = scan_roots([
            GcRoot::slot(
                GcRootKind::FrameLocal,
                "frame0.local0",
                &Slot::Reference(cell.clone()),
            ),
            GcRoot::value(GcRootKind::Temporary, "tmp0", closure),
        ]);
        let reference_id = GcEntityId::new(GcEntityKind::Reference, cell.gc_debug_id());
        let object_id = GcEntityId::new(GcEntityKind::Object, object.id());

        assert!(snapshot.contains(reference_id));
        assert_eq!(snapshot.nodes[&reference_id].edges, vec![object_id]);
        assert!(snapshot.nodes.values().any(
            |node| node.id.kind == GcEntityKind::Closure && node.edges.contains(&reference_id)
        ));
    }

    #[test]
    fn gc_collects_unrooted_object_self_cycle_test_hook() {
        let class = empty_class("Node");
        let object = ObjectRef::new(&class);
        object.set_property("self", Value::Object(object.clone()));
        let weak = object.weak_handle();
        let object_id = GcEntityId::new(GcEntityKind::Object, object.id());

        let mut heap = GcTrackedHeap::new();
        heap.track_value(&Value::Object(object));
        let result = heap.collect_cycles([]);

        assert!(result.collected.iter().any(|entity| entity.id == object_id));
        assert!(!weak.is_alive());
    }

    #[test]
    fn gc_keeps_rooted_object_cycle_test_hook() {
        let class = empty_class("RootedNode");
        let object = ObjectRef::new(&class);
        object.set_property("self", Value::Object(object.clone()));
        let weak = object.weak_handle();
        let object_id = GcEntityId::new(GcEntityKind::Object, object.id());

        let mut heap = GcTrackedHeap::new();
        heap.track_value(&Value::Object(object.clone()));
        let result = heap.collect_cycles([GcRoot::value(
            GcRootKind::FrameLocal,
            "frame0.local0",
            Value::Object(object.clone()),
        )]);

        assert!(!result.collected.iter().any(|entity| entity.id == object_id));
        assert!(weak.is_alive());
        assert_eq!(
            object.get_property("self"),
            Some(Value::Object(object.clone()))
        );
    }

    #[test]
    fn gc_collects_unrooted_reference_array_cycle_test_hook() {
        let cell = ReferenceCell::new(Value::Null);
        let mut array = PhpArray::new();
        array.append(Value::Reference(cell.clone()));
        cell.set(Value::Array(array));
        let weak = cell.weak_handle();
        let reference_id = GcEntityId::new(GcEntityKind::Reference, cell.gc_debug_id());

        let mut heap = GcTrackedHeap::new();
        heap.track_value(&Value::Reference(cell));
        let result = heap.collect_cycles([]);

        assert!(
            result
                .collected
                .iter()
                .any(|entity| entity.id == reference_id)
        );
        assert!(!weak.is_alive());
    }

    #[test]
    fn gc_keeps_rooted_reference_array_cycle_test_hook() {
        let cell = ReferenceCell::new(Value::Null);
        let mut array = PhpArray::new();
        array.append(Value::Reference(cell.clone()));
        cell.set(Value::Array(array));
        let weak = cell.weak_handle();
        let reference_id = GcEntityId::new(GcEntityKind::Reference, cell.gc_debug_id());

        let mut heap = GcTrackedHeap::new();
        heap.track_value(&Value::Reference(cell.clone()));
        let result = heap.collect_cycles([GcRoot::value(
            GcRootKind::FrameLocal,
            "frame0.local0",
            Value::Reference(cell.clone()),
        )]);

        assert!(
            !result
                .collected
                .iter()
                .any(|entity| entity.id == reference_id)
        );
        assert!(weak.is_alive());
        assert!(matches!(cell.get(), Value::Array(_)));
    }
}
