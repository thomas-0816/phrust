//! Reference alias-state classification for conservative optimization guards.

use php_runtime::api::{Slot, Value};

/// Coarse PHP reference/aliasing state used to poison only unsafe fast paths.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum AliasState {
    #[default]
    NoReferencesObserved,
    LocalOnlyReference,
    EscapedReference,
    GlobalOrSuperglobalReference,
    PropertyOrArrayDimReference,
    UnknownAliasing,
}

impl AliasState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoReferencesObserved => "no_references_observed",
            Self::LocalOnlyReference => "local_only_reference",
            Self::EscapedReference => "escaped_reference",
            Self::GlobalOrSuperglobalReference => "global_or_superglobal_reference",
            Self::PropertyOrArrayDimReference => "property_or_array_dim_reference",
            Self::UnknownAliasing => "unknown_aliasing",
        }
    }

    #[must_use]
    pub const fn is_reference_sensitive(self) -> bool {
        !matches!(self, Self::NoReferencesObserved)
    }
}

/// Stable key used by counter/debug reports for state movement.
#[must_use]
pub fn alias_transition_key(from: AliasState, to: AliasState) -> String {
    format!("{}->{}", from.as_str(), to.as_str())
}

/// Classifies a local/property/array storage slot without dereferencing away
/// reference identity.
#[must_use]
pub fn slot_alias_state(slot: &Slot) -> AliasState {
    match slot {
        Slot::Reference(_) => AliasState::LocalOnlyReference,
        Slot::Value(value) => value_alias_state(value),
    }
}

/// Classifies value-level reference exposure. Unknown heap-bearing values fail
/// closed because this pass is a deoptimization policy, not an optimizer.
#[must_use]
pub fn value_alias_state(value: &Value) -> AliasState {
    match value {
        Value::Reference(_) => AliasState::EscapedReference,
        Value::Array(array) if array.contains_references_fast() => {
            AliasState::PropertyOrArrayDimReference
        }
        Value::Object(object) => {
            if object
                .properties_snapshot()
                .iter()
                .any(|(_, property)| matches!(property, Value::Reference(_)))
            {
                AliasState::PropertyOrArrayDimReference
            } else {
                AliasState::NoReferencesObserved
            }
        }
        Value::Fiber(_) | Value::Generator(_) | Value::Callable(_) => AliasState::UnknownAliasing,
        _ => AliasState::NoReferencesObserved,
    }
}

#[cfg(test)]
mod tests {
    use php_runtime::api::{PhpArray, ReferenceCell, Slot, Value};

    use super::{AliasState, alias_transition_key, slot_alias_state, value_alias_state};

    #[test]
    fn classifies_required_alias_states() {
        assert_eq!(
            value_alias_state(&Value::Int(1)),
            AliasState::NoReferencesObserved
        );

        let cell = ReferenceCell::new(Value::Int(1));
        assert_eq!(
            slot_alias_state(&Slot::Reference(cell.clone())),
            AliasState::LocalOnlyReference
        );
        assert_eq!(
            value_alias_state(&Value::Reference(cell.clone())),
            AliasState::EscapedReference
        );

        let mut array = PhpArray::new();
        array.insert(
            php_runtime::api::ArrayKey::Int(0),
            Value::Reference(cell.clone()),
        );
        assert_eq!(
            value_alias_state(&Value::Array(array)),
            AliasState::PropertyOrArrayDimReference
        );
    }

    #[test]
    fn transition_keys_are_stable_report_labels() {
        assert_eq!(
            alias_transition_key(
                AliasState::NoReferencesObserved,
                AliasState::LocalOnlyReference
            ),
            "no_references_observed->local_only_reference"
        );
    }
}
