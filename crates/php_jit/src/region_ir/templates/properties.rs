//! Property runtime templates.

use super::{
    RuntimeTemplate, RuntimeTemplateKind, TemplateGuard, TemplateParam, TemplateValueClass,
};
use crate::region_ir::{RegionValueType, SnapshotEntry, VmSlotId};

/// Property templates.
#[must_use]
pub fn templates() -> Vec<RuntimeTemplate> {
    vec![property_slot_fetch_guarded()]
}

fn property_slot_fetch_guarded() -> RuntimeTemplate {
    RuntimeTemplate {
        name: "property_slot_fetch_guarded",
        kind: RuntimeTemplateKind::PropertySlotFetchGuarded,
        params: vec![
            TemplateParam::new(
                "receiver",
                TemplateValueClass::Object,
                Some(VmSlotId::new(0)),
            ),
            TemplateParam::new("slot", TemplateValueClass::Mixed, Some(VmSlotId::new(1))),
        ],
        guards: vec![
            TemplateGuard::new(
                "receiver_class_matches",
                "receiver class/layout id matches",
                true,
            ),
            TemplateGuard::new(
                "no_magic_property_hooks",
                "magic __get and property hooks are unreachable",
                true,
            ),
        ],
        required_vm_slots: vec![VmSlotId::new(0), VmSlotId::new(1)],
        reference_cow_restrictions: vec![
            "property value must not require reference binding",
            "readonly/uninitialized checks stay in fallback",
        ],
        possible_side_exits: vec![
            "class_guard_miss",
            "magic_property_or_hook",
            "visibility_or_uninitialized",
        ],
        snapshot_requirements: vec![
            SnapshotEntry {
                slot: VmSlotId::new(0),
                value_type: RegionValueType::ObjectHandle,
            },
            SnapshotEntry {
                slot: VmSlotId::new(1),
                value_type: RegionValueType::MixedValue,
            },
        ],
        slow_path_helper: Some("runtime_fetch_property"),
        unsupported_php_semantic_cases: vec![
            "__get",
            "property hooks",
            "dynamic properties",
            "visibility checks",
            "uninitialized typed property errors",
        ],
    }
}
