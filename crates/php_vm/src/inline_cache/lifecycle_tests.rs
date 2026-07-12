use super::*;
use crate::include::IncludePathFileFingerprint;
use php_ir::ids::{BlockId, FunctionId, InstrId};
use php_runtime::api::PhpString;
use std::path::PathBuf;
use std::sync::Arc;

fn positional_shape(arity: u32) -> FunctionCallShape {
    FunctionCallShape {
        arity,
        named_arguments: Vec::new(),
        by_ref_arguments: CallReferenceMask::default(),
    }
}

#[test]
fn typed_slot_layout_is_smaller_than_legacy_option_layout() {
    #[allow(dead_code)]
    struct LegacyInlineCacheSlotLayout {
        id: InlineCacheId,
        seeded: bool,
        kind: InlineCacheKind,
        state: InlineCacheState,
        unit_key: u64,
        function: FunctionId,
        block: BlockId,
        instruction: InstrId,
        epoch: InvalidationEpoch,
        stats: InlineCacheStats,
        function_call_name: Option<PhpString>,
        function_call_shape: Option<FunctionCallShape>,
        function_call_builtin_metadata: Option<FunctionCallBuiltinMetadata>,
        function_call_target: Option<FunctionCallCacheTarget>,
        function_call_entries: Vec<FunctionCallPolymorphicEntry>,
        method_call_name: Option<String>,
        method_call_receiver_class: Option<String>,
        method_call_scope: Option<String>,
        method_call_target: Option<MethodCallCacheTarget>,
        method_call_entries: Vec<MethodCallPolymorphicEntry>,
        property_fetch_name: Option<String>,
        property_fetch_receiver_class: Option<String>,
        property_fetch_scope: Option<String>,
        property_fetch_target: Option<PropertyFetchCacheTarget>,
        property_fetch_entries: Vec<PropertyFetchPolymorphicEntry>,
        property_assign_name: Option<String>,
        property_assign_receiver_class: Option<String>,
        property_assign_scope: Option<String>,
        property_assign_target: Option<PropertyAssignCacheTarget>,
        property_assign_entries: Vec<PropertyAssignPolymorphicEntry>,
        class_static_kind: Option<ClassConstantStaticPropertyCacheKind>,
        class_static_resolved_class: Option<String>,
        class_static_member: Option<String>,
        class_static_scope: Option<String>,
        class_static_target: Option<ClassConstantStaticPropertyCacheTarget>,
        include_path_key: Option<IncludePathCacheKey>,
        include_path_target: Option<IncludePathCacheTarget>,
        autoload_key: Option<AutoloadClassLookupCacheKey>,
        autoload_epochs: Option<AutoloadClassLookupEpochs>,
        autoload_target: Option<AutoloadClassLookupCacheTarget>,
    }

    let legacy = std::mem::size_of::<LegacyInlineCacheSlotLayout>();
    let typed = std::mem::size_of::<InlineCacheSlot>();
    eprintln!(
        "inline_cache_size_bytes header={} payload={} slot={typed} legacy={legacy}",
        std::mem::size_of::<InlineCacheHeader>(),
        std::mem::size_of::<InlineCachePayload>(),
    );
    assert!(typed < legacy, "typed={typed} legacy={legacy}");
    assert!(typed * 2 < legacy, "typed={typed} legacy={legacy}");
}

#[test]
fn dense_id_rejects_another_payload_family() {
    let mut table = InlineCacheTable::default();
    let (id, _) = table.bind_slot(
        1,
        FunctionId::new(0),
        BlockId::new(0),
        InstrId::new(0),
        InlineCacheKind::MethodCall,
    );
    let (target, observation) = table.lookup_function_call_by_id(
        id,
        &PhpString::intern(b"wrong_family"),
        InvalidationEpoch::new(0),
        &positional_shape(0),
        None,
    );
    assert!(target.is_none());
    assert!(observation.miss);
    assert_eq!(table.slots[id.index()].kind(), InlineCacheKind::MethodCall);
}

#[test]
fn call_reference_mask_keeps_common_positional_shape_inline() {
    let common = CallReferenceMask::from_flags(std::iter::repeat_n(false, 12));
    assert!(!common.any());
    assert_eq!(common.inline, 0);
    assert!(common.overflow.is_empty());

    let mut flags = vec![false; 130];
    flags[1] = true;
    flags[129] = true;
    let complex = CallReferenceMask::from_flags(flags);
    assert!(complex.any());
    assert_eq!(complex.inline, 1 << 1);
    assert_eq!(complex.overflow.len(), 2);
    assert_eq!(complex.overflow[1], 1 << 1);
}

#[test]
fn inline_cache_table_allocates_one_stable_slot_per_instruction_kind() {
    let function = FunctionId::new(0);
    let block = BlockId::new(1);
    let instruction = InstrId::new(2);
    let mut table = InlineCacheTable::default();

    let first = table.observe_slot(
        17,
        function,
        block,
        instruction,
        InlineCacheKind::FunctionCall,
    );
    let second = table.observe_slot(
        17,
        function,
        block,
        instruction,
        InlineCacheKind::FunctionCall,
    );
    let third = table.observe_slot(17, function, block, instruction, InlineCacheKind::DimFetch);

    assert!(first.candidate);
    assert!(first.slot_allocated);
    assert!(second.candidate);
    assert!(!second.slot_allocated);
    assert!(third.slot_allocated);
    assert_eq!(table.slot_count(), 2);
}

#[test]
fn inline_cache_slot_state_starts_cold() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();

    table.observe_slot(
        1,
        function,
        block,
        instruction,
        InlineCacheKind::PropertyFetch,
    );
    assert_eq!(table.slots.len(), 1);
    let slot = &table.slots[0];

    assert_eq!(slot.id.raw(), 0);
    assert_eq!(slot.state, InlineCacheState::Cold);
    assert_eq!(slot.epoch.raw(), 0);
    assert_eq!(slot.stats.hits, 0);
}

#[test]
fn class_static_cache_transitions_from_cold_through_megamorphic() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();
    table.observe_slot(
        13,
        function,
        block,
        instruction,
        InlineCacheKind::ClassConstantStaticProperty,
    );
    assert_eq!(table.slots[0].state, InlineCacheState::Cold);

    for index in 0..=POLYMORPHIC_INLINE_CACHE_LIMIT {
        let class = format!("performance{index}");
        table.install_class_constant_static_property(
            13,
            function,
            block,
            instruction,
            ClassConstantStaticPropertyCacheKind::ClassConstant,
            &class,
            "VALUE",
            None,
            InvalidationEpoch::new(8),
            ClassConstantStaticPropertyCacheTarget::CurrentUnit {
                kind: ClassConstantStaticPropertyCacheKind::ClassConstant,
                resolved_class: class.clone(),
                declaring_class: class.clone(),
                member: "VALUE".to_owned(),
            },
        );
        if index == 0 {
            assert_eq!(table.slots[0].state, InlineCacheState::Monomorphic);
        } else if index < POLYMORPHIC_INLINE_CACHE_LIMIT {
            assert_eq!(table.slots[0].state, InlineCacheState::Polymorphic);
        }
    }

    assert_eq!(table.slots[0].state, InlineCacheState::Megamorphic);
    assert_eq!(
        table.slots[0].payload(),
        &InlineCachePayload::Empty(InlineCacheKind::ClassConstantStaticProperty)
    );
}

#[test]
fn autoload_lookup_cache_transitions_from_cold_through_megamorphic() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let epochs = AutoloadClassLookupEpochs {
        autoload_stack_epoch: 1,
        class_table_epoch: 2,
        include_config_epoch: 3,
    };
    let mut table = InlineCacheTable::default();
    table.observe_slot(
        17,
        function,
        block,
        instruction,
        InlineCacheKind::AutoloadClassLookup,
    );
    assert_eq!(table.slots[0].state, InlineCacheState::Cold);

    for index in 0..=POLYMORPHIC_INLINE_CACHE_LIMIT {
        table.install_autoload_class_lookup(
            17,
            function,
            block,
            instruction,
            AutoloadClassLookupCacheKey {
                kind: AutoloadClassLookupKind::Class,
                normalized_name: format!("performance\\cache\\thing{index}"),
                autoload_enabled: true,
                autoload_stack_depth: 0,
                include_path_config: "vendor".to_owned(),
                composer_map_fingerprint: Some(Arc::from("classmap:1")),
            },
            epochs,
            AutoloadClassLookupCacheTarget::Negative,
        );
        if index == 0 {
            assert_eq!(table.slots[0].state, InlineCacheState::Monomorphic);
        } else if index < POLYMORPHIC_INLINE_CACHE_LIMIT {
            assert_eq!(table.slots[0].state, InlineCacheState::Polymorphic);
        }
    }

    assert_eq!(table.slots[0].state, InlineCacheState::Megamorphic);
    assert_eq!(
        table.slots[0].payload(),
        &InlineCachePayload::Empty(InlineCacheKind::AutoloadClassLookup)
    );
}

#[test]
fn include_path_cache_transitions_from_cold_through_megamorphic() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();
    table.observe_slot(
        15,
        function,
        block,
        instruction,
        InlineCacheKind::IncludePath,
    );
    assert_eq!(table.slots[0].state, InlineCacheState::Cold);

    for index in 0..=POLYMORPHIC_INLINE_CACHE_LIMIT {
        let path = format!("lib{index}.php");
        table.install_include_path(
            15,
            function,
            block,
            instruction,
            IncludePathCacheKey {
                path: path.clone(),
                include_path: vec![PathBuf::from("src")],
                cwd: PathBuf::from("/repo"),
                calling_file_directory: Some(PathBuf::from("/repo/app")),
            },
            InvalidationEpoch::new(2),
            IncludePathCacheTarget {
                canonical_path: PathBuf::from(format!("/repo/src/{path}")),
                resolution_path: Some(PathBuf::from(format!("/repo/src/{path}"))),
                fingerprint: IncludePathFileFingerprint {
                    len: index as u64,
                    modified_unix_nanos: Some(10),
                    changed_unix_nanos: None,
                    readonly: false,
                    inode: None,
                    device: None,
                },
                directory_version: None,
            },
        );
        if index == 0 {
            assert_eq!(table.slots[0].state, InlineCacheState::Monomorphic);
        } else if index < POLYMORPHIC_INLINE_CACHE_LIMIT {
            assert_eq!(table.slots[0].state, InlineCacheState::Polymorphic);
        }
    }

    assert_eq!(table.slots[0].state, InlineCacheState::Megamorphic);
    assert_eq!(
        table.slots[0].payload(),
        &InlineCachePayload::Empty(InlineCacheKind::IncludePath)
    );
}
