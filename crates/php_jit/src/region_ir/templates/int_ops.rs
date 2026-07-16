//! Integer runtime templates.

use super::{
    RuntimeTemplate, RuntimeTemplateKind, TemplateGuard, TemplateParam, TemplateValueClass,
};
use crate::region_ir::{RegionValueType, SnapshotEntry, VmSlotId};

/// Integer templates.
#[must_use]
pub fn templates() -> Vec<RuntimeTemplate> {
    vec![int_add_checked(), int_compare()]
}

fn int_add_checked() -> RuntimeTemplate {
    RuntimeTemplate {
        name: "int_add_checked",
        kind: RuntimeTemplateKind::IntAddChecked,
        params: vec![
            TemplateParam::new("left", TemplateValueClass::I64, Some(VmSlotId::new(0))),
            TemplateParam::new("right", TemplateValueClass::I64, Some(VmSlotId::new(1))),
        ],
        guards: vec![
            TemplateGuard::new("inputs_are_i64", "both operands are exact i64", true),
            TemplateGuard::new("checked_no_overflow", "i64 add must not overflow", true),
        ],
        required_vm_slots: vec![VmSlotId::new(0), VmSlotId::new(1)],
        reference_cow_restrictions: vec!["no references", "no COW separation"],
        possible_side_exits: vec!["type_mismatch", "integer_overflow"],
        snapshot_requirements: snapshot_pair(),
        slow_path_helper: Some("runtime_binary_add"),
        unsupported_php_semantic_cases: vec![
            "float fallback",
            "numeric string conversion",
            "array/object/resource operands",
            "diagnostic-emitting conversion",
        ],
    }
}

fn int_compare() -> RuntimeTemplate {
    RuntimeTemplate {
        name: "int_compare",
        kind: RuntimeTemplateKind::IntCompare,
        params: vec![
            TemplateParam::new("left", TemplateValueClass::I64, Some(VmSlotId::new(0))),
            TemplateParam::new("right", TemplateValueClass::I64, Some(VmSlotId::new(1))),
        ],
        guards: vec![TemplateGuard::new(
            "inputs_are_i64",
            "both operands are exact i64",
            true,
        )],
        required_vm_slots: vec![VmSlotId::new(0), VmSlotId::new(1)],
        reference_cow_restrictions: vec!["no references"],
        possible_side_exits: vec!["type_mismatch"],
        snapshot_requirements: snapshot_pair(),
        slow_path_helper: Some("runtime_compare"),
        unsupported_php_semantic_cases: vec![
            "loose comparison conversions",
            "string/object comparison",
            "array comparison",
        ],
    }
}

fn snapshot_pair() -> Vec<SnapshotEntry> {
    vec![
        SnapshotEntry {
            slot: VmSlotId::new(0),
            value_type: RegionValueType::I64,
        },
        SnapshotEntry {
            slot: VmSlotId::new(1),
            value_type: RegionValueType::I64,
        },
    ]
}
