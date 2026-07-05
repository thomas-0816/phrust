use php_vm::api::QuickeningSiteSnapshot;
use std::{collections::BTreeMap, sync::Mutex};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct PersistentMetadataStats {
    pub(crate) feedback_templates: u64,
}

#[derive(Debug, Default)]
pub(crate) struct PersistentMetadataStore {
    quickening_templates: Mutex<Vec<QuickeningSiteSnapshot>>,
}

impl PersistentMetadataStore {
    pub(crate) fn quickening_templates(&self) -> Vec<QuickeningSiteSnapshot> {
        self.quickening_templates
            .lock()
            .map(|templates| templates.clone())
            .unwrap_or_default()
    }

    pub(crate) fn absorb_quickening_feedback(
        &self,
        feedback: Vec<QuickeningSiteSnapshot>,
    ) -> usize {
        if feedback.is_empty() {
            return 0;
        }
        let Ok(mut templates) = self.quickening_templates.lock() else {
            return 0;
        };
        let accepted = feedback.len();
        let merged = templates
            .iter()
            .chain(feedback.iter())
            .map(|snapshot| (snapshot.site, *snapshot))
            .collect::<BTreeMap<_, _>>();
        *templates = merged.values().copied().collect();
        accepted
    }

    pub(crate) fn stats(&self) -> PersistentMetadataStats {
        let feedback_templates = self
            .quickening_templates
            .lock()
            .map(|templates| templates.len() as u64)
            .unwrap_or_default();
        PersistentMetadataStats { feedback_templates }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use php_vm::experimental::{QuickeningSiteKey, QuickeningSpecialization, QuickeningState};

    #[test]
    fn quickening_feedback_templates_are_deduplicated_by_site() {
        let store = PersistentMetadataStore::default();
        let first = QuickeningSiteSnapshot {
            site: QuickeningSiteKey::Dense {
                unit: 1,
                function: 2,
                instruction: 3,
            },
            state: QuickeningState::Specialized,
            specialization: Some(QuickeningSpecialization::AddIntInt),
            guard_failures: 0,
        };
        let replacement = QuickeningSiteSnapshot {
            guard_failures: 4,
            ..first
        };
        let second = QuickeningSiteSnapshot {
            site: QuickeningSiteKey::Ir {
                function: 5,
                block: 6,
                instruction: 7,
            },
            state: QuickeningState::Blacklisted,
            specialization: None,
            guard_failures: 2,
        };

        assert_eq!(store.absorb_quickening_feedback(vec![first, second]), 2);
        assert_eq!(store.absorb_quickening_feedback(vec![replacement]), 1);

        let templates = store.quickening_templates();
        assert_eq!(templates.len(), 2);
        assert!(templates.contains(&replacement));
        assert!(templates.contains(&second));
        assert_eq!(store.stats().feedback_templates, 2);
    }
}
