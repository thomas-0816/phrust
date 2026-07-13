//! String runtime templates.

use super::{
    RuntimeTemplate, RuntimeTemplateKind, TemplateGuard, TemplateParam, TemplateValueClass,
};
use crate::region_ir::{RegionValueType, SnapshotEntry, VmSlotId};

/// String templates.
#[must_use]
pub fn templates() -> Vec<RuntimeTemplate> {
    vec![string_concat_exact()]
}

fn string_concat_exact() -> RuntimeTemplate {
    RuntimeTemplate {
        name: "string_concat_exact",
        kind: RuntimeTemplateKind::StringConcatExact,
        params: vec![
            TemplateParam::new(
                "left",
                TemplateValueClass::ExactString,
                Some(VmSlotId::new(0)),
            ),
            TemplateParam::new(
                "right",
                TemplateValueClass::ExactString,
                Some(VmSlotId::new(1)),
            ),
        ],
        guards: vec![
            TemplateGuard::new("inputs_are_exact_strings", "no conversion needed", true),
            TemplateGuard::new(
                "no_to_string_or_magic",
                "object conversion and magic methods are unreachable",
                true,
            ),
        ],
        required_vm_slots: vec![VmSlotId::new(0), VmSlotId::new(1)],
        reference_cow_restrictions: vec!["string handles are immutable"],
        possible_side_exits: vec!["type_mismatch", "object_to_string"],
        snapshot_requirements: vec![
            SnapshotEntry {
                slot: VmSlotId::new(0),
                value_type: RegionValueType::StringHandle,
            },
            SnapshotEntry {
                slot: VmSlotId::new(1),
                value_type: RegionValueType::StringHandle,
            },
        ],
        slow_path_helper: Some("runtime_concat"),
        unsupported_php_semantic_cases: vec![
            "object __toString",
            "array to string warning",
            "resource conversion",
            "binary string allocation path",
        ],
    }
}
