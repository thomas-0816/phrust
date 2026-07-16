//! Incremental request-root membership for native runtime handles.

use std::collections::HashSet;

use php_runtime::api::Value;

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
    rooted_objects: HashSet<u64>,
    rooted_arrays: HashSet<u64>,
    rooted_references: HashSet<u64>,
    dirty: bool,
    generation: u64,
    rebuilds: u64,
    last_reason: Option<RootMutationReason>,
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

    pub(super) fn replace(&mut self, membership: RootMembership) {
        self.rooted_objects = membership.objects;
        self.rooted_arrays = membership.arrays;
        self.rooted_references = membership.references;
        self.dirty = false;
        self.generation = self.generation.saturating_add(1);
        self.rebuilds = self.rebuilds.saturating_add(1);
    }

    pub(super) fn contains(&self, object_id: u64) -> bool {
        self.rooted_objects.contains(&object_id)
    }

    pub(super) fn contains_container(&self, value: &Value) -> bool {
        match value {
            Value::Object(object) => self.rooted_objects.contains(&object.id()),
            Value::Array(array) => self.rooted_arrays.contains(&array.gc_debug_id()),
            Value::Reference(reference) => {
                self.rooted_references.contains(&reference.gc_debug_id())
            }
            _ => false,
        }
    }

    pub(super) fn last_reason(&self) -> RootMutationReason {
        self.last_reason
            .unwrap_or(RootMutationReason::RootedContainer)
    }

    #[cfg(test)]
    pub(super) const fn rebuilds(&self) -> u64 {
        self.rebuilds
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
#[derive(Default)]
pub(super) struct RootMembership {
    objects: HashSet<u64>,
    arrays: HashSet<u64>,
    references: HashSet<u64>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_index_reuses_membership_until_marked_dirty() {
        let mut index = RequestRootIndex::new_dirty();
        index.replace(RootMembership {
            objects: HashSet::from([7]),
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
}
