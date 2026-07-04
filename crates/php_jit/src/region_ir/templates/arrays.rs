//! Array runtime templates.

use super::{
    RuntimeTemplate, RuntimeTemplateKind, TemplateGuard, TemplateParam, TemplateValueClass,
};
use crate::region_ir::{RegionValueType, SnapshotEntry, VmSlotId};

/// Array templates.
#[must_use]
pub fn templates() -> Vec<RuntimeTemplate> {
    vec![
        packed_array_fetch_readonly(),
        packed_foreach_int_sum_metadata_only(),
        isset_array_key_interned_exact(),
        record_array_lookup_symbol_guard(),
    ]
}

fn record_array_lookup_symbol_guard() -> RuntimeTemplate {
    RuntimeTemplate {
        name: "record_array_lookup_symbol_guard",
        kind: RuntimeTemplateKind::RecordArrayLookupSymbolGuard,
        params: vec![
            TemplateParam::new(
                "array",
                TemplateValueClass::PackedArray,
                Some(VmSlotId::new(0)),
            ),
            TemplateParam::new(
                "key",
                TemplateValueClass::ExactString,
                Some(VmSlotId::new(1)),
            ),
        ],
        guards: vec![
            TemplateGuard::new("array_is_record_shaped", "array uses record storage", true),
            TemplateGuard::new(
                "key_symbol_has_slot",
                "interned key symbol resolves to a shape slot",
                true,
            ),
            TemplateGuard::new(
                "slot_not_reference",
                "resolved slot does not hold a reference cell",
                true,
            ),
        ],
        required_vm_slots: vec![VmSlotId::new(0), VmSlotId::new(1)],
        reference_cow_restrictions: vec![
            "array must use record-shaped storage",
            "slot value must not be a reference cell",
            "result is a read-only clone",
        ],
        possible_side_exits: vec!["layout_mismatch", "key_symbol_miss", "reference_cell"],
        snapshot_requirements: array_snapshot(),
        fallback_helper: Some("interpreter_fetch_dim"),
        unsupported_php_semantic_cases: vec![
            "missing-key warning/null behavior",
            "by-reference element access",
            "ArrayAccess object",
            "integer or numeric-string keys",
        ],
    }
}

fn packed_array_fetch_readonly() -> RuntimeTemplate {
    RuntimeTemplate {
        name: "packed_array_fetch_readonly",
        kind: RuntimeTemplateKind::PackedArrayFetchReadonly,
        params: vec![
            TemplateParam::new(
                "array",
                TemplateValueClass::PackedArray,
                Some(VmSlotId::new(0)),
            ),
            TemplateParam::new("index", TemplateValueClass::I64, Some(VmSlotId::new(1))),
        ],
        guards: vec![
            TemplateGuard::new("array_is_packed", "array layout is packed", true),
            TemplateGuard::new("index_in_bounds", "index is in packed bounds", true),
            TemplateGuard::new(
                "readonly_no_ref_cow",
                "fetch cannot expose references or COW",
                true,
            ),
        ],
        required_vm_slots: vec![VmSlotId::new(0), VmSlotId::new(1)],
        reference_cow_restrictions: vec![
            "array must not contain reference cells",
            "array must not require COW separation",
            "result must be read-only",
        ],
        possible_side_exits: vec![
            "layout_mismatch",
            "bounds_miss",
            "reference_cell",
            "cow_required",
        ],
        snapshot_requirements: array_snapshot(),
        fallback_helper: Some("interpreter_fetch_dim"),
        unsupported_php_semantic_cases: vec![
            "by-reference element access",
            "missing-key warning/null behavior",
            "ArrayAccess object",
            "string offset access",
        ],
    }
}

fn packed_foreach_int_sum_metadata_only() -> RuntimeTemplate {
    RuntimeTemplate {
        name: "packed_foreach_int_sum_metadata_only",
        kind: RuntimeTemplateKind::PackedForeachIntSumMetadataOnly,
        params: vec![TemplateParam::new(
            "array",
            TemplateValueClass::PackedArray,
            Some(VmSlotId::new(0)),
        )],
        guards: vec![
            TemplateGuard::new("array_is_packed", "array layout is packed", true),
            TemplateGuard::new(
                "elements_are_i64",
                "all visited elements are exact i64",
                true,
            ),
            TemplateGuard::new(
                "readonly_no_ref_cow",
                "iteration cannot expose references or COW",
                true,
            ),
        ],
        required_vm_slots: vec![VmSlotId::new(0)],
        reference_cow_restrictions: vec![
            "no by-reference foreach",
            "no reference cells",
            "no COW separation",
        ],
        possible_side_exits: vec!["layout_mismatch", "non_i64_element", "reference_cell"],
        snapshot_requirements: array_snapshot(),
        fallback_helper: Some("interpreter_foreach"),
        unsupported_php_semantic_cases: vec![
            "destructor during iteration",
            "iterator object",
            "mutation during iteration",
        ],
    }
}

fn isset_array_key_interned_exact() -> RuntimeTemplate {
    RuntimeTemplate {
        name: "isset_array_key_interned_exact",
        kind: RuntimeTemplateKind::IssetArrayKeyInternedExact,
        params: vec![
            TemplateParam::new(
                "array",
                TemplateValueClass::PackedArray,
                Some(VmSlotId::new(0)),
            ),
            TemplateParam::new(
                "key",
                TemplateValueClass::InternedKey,
                Some(VmSlotId::new(1)),
            ),
        ],
        guards: vec![
            TemplateGuard::new("key_is_interned", "array key is exact and interned", true),
            TemplateGuard::new(
                "array_no_ref_cow",
                "array lookup is not ref/COW-sensitive",
                true,
            ),
        ],
        required_vm_slots: vec![VmSlotId::new(0), VmSlotId::new(1)],
        reference_cow_restrictions: vec!["no reference cells", "no COW separation"],
        possible_side_exits: vec!["key_miss", "reference_cell", "cow_required"],
        snapshot_requirements: array_snapshot(),
        fallback_helper: Some("interpreter_isset_dim"),
        unsupported_php_semantic_cases: vec![
            "ArrayAccess object",
            "string offset isset",
            "null-vs-missing PHP isset semantics",
        ],
    }
}

fn array_snapshot() -> Vec<SnapshotEntry> {
    vec![
        SnapshotEntry {
            slot: VmSlotId::new(0),
            value_type: RegionValueType::ArrayHandle,
        },
        SnapshotEntry {
            slot: VmSlotId::new(1),
            value_type: RegionValueType::I64,
        },
    ]
}
