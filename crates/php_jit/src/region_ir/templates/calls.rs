//! Call and builtin runtime templates.

use super::{
    RuntimeTemplate, RuntimeTemplateKind, TemplateGuard, TemplateParam, TemplateValueClass,
};
use crate::region_ir::{RegionValueType, SnapshotEntry, VmSlotId};

/// Call/builtin templates.
#[must_use]
pub fn templates() -> Vec<RuntimeTemplate> {
    vec![
        known_builtin_strlen_exact(),
        known_builtin_count_packed_exact(),
    ]
}

fn known_builtin_strlen_exact() -> RuntimeTemplate {
    RuntimeTemplate {
        name: "known_builtin_strlen_exact",
        kind: RuntimeTemplateKind::KnownBuiltinStrlenExact,
        params: vec![TemplateParam::new(
            "value",
            TemplateValueClass::ExactString,
            Some(VmSlotId::new(0)),
        )],
        guards: vec![TemplateGuard::new(
            "arg_is_exact_string",
            "argument is an exact string and cannot call __toString",
            true,
        )],
        required_vm_slots: vec![VmSlotId::new(0)],
        reference_cow_restrictions: vec!["string handle is immutable"],
        possible_side_exits: vec!["type_mismatch", "object_to_string"],
        snapshot_requirements: vec![SnapshotEntry {
            slot: VmSlotId::new(0),
            value_type: RegionValueType::StringHandle,
        }],
        slow_path_helper: Some("known_strlen_helper"),
        unsupported_php_semantic_cases: vec![
            "object __toString",
            "array/resource conversion diagnostics",
            "mbstring overloads are not modeled here",
        ],
    }
}

fn known_builtin_count_packed_exact() -> RuntimeTemplate {
    RuntimeTemplate {
        name: "known_builtin_count_packed_exact",
        kind: RuntimeTemplateKind::KnownBuiltinCountPackedExact,
        params: vec![TemplateParam::new(
            "value",
            TemplateValueClass::PackedArray,
            Some(VmSlotId::new(0)),
        )],
        guards: vec![TemplateGuard::new(
            "arg_is_packed_array",
            "argument is a packed array with known length",
            true,
        )],
        required_vm_slots: vec![VmSlotId::new(0)],
        reference_cow_restrictions: vec!["no reference-sensitive count path"],
        possible_side_exits: vec!["type_mismatch", "object_countable"],
        snapshot_requirements: vec![SnapshotEntry {
            slot: VmSlotId::new(0),
            value_type: RegionValueType::ArrayHandle,
        }],
        slow_path_helper: Some("known_count_helper"),
        unsupported_php_semantic_cases: vec![
            "Countable object",
            "non-array TypeError",
            "recursive count mode",
        ],
    }
}
