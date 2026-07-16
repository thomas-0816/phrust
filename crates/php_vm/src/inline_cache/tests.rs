use super::{
    AutoloadClassLookupCacheKey, AutoloadClassLookupCacheTarget, AutoloadClassLookupEpochs,
    AutoloadClassLookupKind, CallReferenceMask, ClassConstantStaticPropertyCacheKind,
    ClassConstantStaticPropertyCacheTarget, ClassRelationCache, ClassRelationCacheKey,
    ClassRelationCacheLookup, ClassRelationCacheTarget, ClassRelationEpochs, ClassRelationKind,
    FunctionCallBuiltinKind, FunctionCallBuiltinMetadata, FunctionCallCacheTarget,
    FunctionCallShape, FunctionCallSiteSnapshot, IncludePathCacheKey, IncludePathCacheTarget,
    InlineCacheKind, InlineCachePayload, InlineCacheState, InlineCacheTable, InvalidationEpoch,
    MethodCallCacheTarget, MethodCallGuardMetadata, MethodCallResolvedTarget,
    MethodCallRouteIdentity, MethodCallShape, PropertyFetchCacheTarget,
    PropertyFetchLayoutMetadata, PropertyFetchResolvedTarget,
};
use crate::include::IncludePathFileFingerprint;
use php_ir::ids::{BlockId, ClassId, FunctionId, InstrId};
use php_runtime::api::PhpString;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

pub(super) fn positional_shape(arity: u32) -> FunctionCallShape {
    FunctionCallShape {
        arity,
        named_arguments: Vec::new(),
        by_ref_arguments: CallReferenceMask::default(),
    }
}

pub(super) fn method_target(
    _receiver_class: &str,
    receiver_class_id: u32,
    declaring_class: &str,
    function: FunctionId,
    epoch: InvalidationEpoch,
) -> MethodCallCacheTarget {
    MethodCallCacheTarget::CurrentUnit {
        target: Rc::new(MethodCallResolvedTarget {
            declaring_class: declaring_class.to_owned(),
            function,
            route: None,
            guard: MethodCallGuardMetadata {
                receiver_class_id: ClassId::new(receiver_class_id),
                class_layout_epoch: epoch.raw(),
                method_table_epoch: epoch.raw(),
                method_slot_index: Some(0),
                method_is_final: false,
                method_is_private: false,
                method_is_static: false,
                receiver_has_override: false,
                argument_shape: MethodCallShape {
                    arity: 0,
                    named_arguments: Vec::new(),
                    by_ref_arguments: CallReferenceMask::default(),
                },
                by_ref_compatible: true,
                has_magic_call: false,
            },
        }),
    }
}

fn builtin_metadata(name: &str) -> FunctionCallBuiltinMetadata {
    FunctionCallBuiltinMetadata {
        implementation_id: format!("internal_registry:{name}"),
        version: 1,
    }
}

fn property_layout(class_id: u32) -> PropertyFetchLayoutMetadata {
    PropertyFetchLayoutMetadata {
        class_id,
        layout_version: 6,
        property_slot_index: Some(0),
        visibility_context: None,
        typed_property_initialized: true,
        has_property_hooks: false,
        has_magic_get: false,
        dynamic_property_fallback: false,
    }
}

pub(super) fn property_target(
    receiver_class: &str,
    declaring_class: &str,
    class_id: u32,
) -> Arc<PropertyFetchResolvedTarget> {
    Arc::new(PropertyFetchResolvedTarget {
        receiver_class: receiver_class.to_owned(),
        declaring_class: declaring_class.to_owned(),
        property: "value".to_owned(),
        storage_name: "value".to_owned(),
        layout: property_layout(class_id),
        object_layout_epoch: 0,
        declared_slot: None,
    })
}

pub(super) fn property_assign_target(
    receiver_class: &str,
    declaring_class: &str,
    class_id: u32,
) -> Arc<super::PropertyAssignResolvedTarget> {
    Arc::new(super::PropertyAssignResolvedTarget {
        receiver_class: receiver_class.to_owned(),
        declaring_class: declaring_class.to_owned(),
        property: "value".to_owned(),
        storage_name: "value".to_owned(),
        layout: super::PropertyAssignLayoutMetadata {
            class_id,
            layout_version: 6,
            property_slot_index: Some(0),
            visibility_context: None,
            typed_property: false,
            readonly_or_init_only: false,
            reference_slot: false,
            has_property_hooks: false,
            has_magic_set: false,
            dynamic_property_fallback: false,
        },
        object_layout_epoch: 0,
        declared_slot: Some(0),
        slot_write_eligible: true,
    })
}

#[test]
fn function_call_cache_hits_same_name_and_epoch() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();

    table.observe_slot(
        7,
        function,
        block,
        instruction,
        InlineCacheKind::FunctionCall,
    );
    table.install_function_call(
        7,
        function,
        block,
        instruction,
        &PhpString::intern(b"strlen"),
        InvalidationEpoch::new(3),
        positional_shape(1),
        Some(builtin_metadata("strlen")),
        FunctionCallCacheTarget::Builtin {
            kind: FunctionCallBuiltinKind::InternalRegistry,
            name: Arc::from("strlen"),
        },
    );
    let (target, event) = table.lookup_function_call(
        7,
        function,
        block,
        instruction,
        &PhpString::intern(b"strlen"),
        InvalidationEpoch::new(3),
        &positional_shape(1),
        Some(&builtin_metadata("strlen")),
    );

    assert!(target.is_some());
    assert!(event.hit);
    assert!(!event.miss);
}

#[test]
fn function_call_cache_invalidates_on_epoch_change() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();

    table.observe_slot(
        7,
        function,
        block,
        instruction,
        InlineCacheKind::FunctionCall,
    );
    table.install_function_call(
        7,
        function,
        block,
        instruction,
        &PhpString::intern(b"perf_fn"),
        InvalidationEpoch::new(1),
        positional_shape(0),
        None,
        FunctionCallCacheTarget::CurrentUnit {
            unit_identity: 0,
            function,
        },
    );
    let (target, event) = table.lookup_function_call(
        7,
        function,
        block,
        instruction,
        &PhpString::intern(b"perf_fn"),
        InvalidationEpoch::new(2),
        &positional_shape(0),
        None,
    );

    assert!(target.is_none());
    assert!(event.invalidation);
    assert!(event.miss);
}

#[test]
fn function_call_cache_guards_call_shape_and_builtin_metadata() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();
    let shape = FunctionCallShape {
        arity: 2,
        named_arguments: vec!["left".to_owned(), "right".to_owned()],
        by_ref_arguments: CallReferenceMask::default(),
    };

    table.observe_slot(
        7,
        function,
        block,
        instruction,
        InlineCacheKind::FunctionCall,
    );
    table.install_function_call(
        7,
        function,
        block,
        instruction,
        &PhpString::intern(b"strlen"),
        InvalidationEpoch::new(1),
        shape.clone(),
        Some(builtin_metadata("strlen")),
        FunctionCallCacheTarget::Builtin {
            kind: FunctionCallBuiltinKind::InternalRegistry,
            name: Arc::from("strlen"),
        },
    );

    let wrong_shape = positional_shape(2);
    let (target, event) = table.lookup_function_call(
        7,
        function,
        block,
        instruction,
        &PhpString::intern(b"strlen"),
        InvalidationEpoch::new(1),
        &wrong_shape,
        Some(&builtin_metadata("strlen")),
    );
    assert!(target.is_none());
    assert!(event.guard_failure);

    table.install_function_call(
        7,
        function,
        block,
        instruction,
        &PhpString::intern(b"strlen"),
        InvalidationEpoch::new(1),
        shape.clone(),
        Some(builtin_metadata("strlen")),
        FunctionCallCacheTarget::Builtin {
            kind: FunctionCallBuiltinKind::InternalRegistry,
            name: Arc::from("strlen"),
        },
    );
    let wrong_metadata = FunctionCallBuiltinMetadata {
        implementation_id: "InternalRegistry:strlen".to_owned(),
        version: 2,
    };
    let (target, event) = table.lookup_function_call(
        7,
        function,
        block,
        instruction,
        &PhpString::intern(b"strlen"),
        InvalidationEpoch::new(1),
        &shape,
        Some(&wrong_metadata),
    );
    assert!(target.is_none());
    assert!(event.guard_failure);
}

#[test]
fn function_call_cache_type_changes_reach_capped_megamorphic_state() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();

    table.observe_slot(
        7,
        function,
        block,
        instruction,
        InlineCacheKind::FunctionCall,
    );
    table.install_function_call(
        7,
        function,
        block,
        instruction,
        &PhpString::intern(b"perf_fn_a"),
        InvalidationEpoch::new(1),
        positional_shape(0),
        None,
        FunctionCallCacheTarget::CurrentUnit {
            unit_identity: 0,
            function,
        },
    );

    let mut saw_megamorphic = false;
    for name in ["perf_fn_b", "perf_fn_c", "perf_fn_d", "perf_fn_e"] {
        let (target, event) = table.lookup_function_call(
            7,
            function,
            block,
            instruction,
            &PhpString::intern(name.as_bytes()),
            InvalidationEpoch::new(1),
            &positional_shape(0),
            None,
        );
        assert!(target.is_none());
        assert!(event.resolver_required);
        saw_megamorphic |= event.megamorphic;
        assert!(!event.disabled);
        table.install_function_call(
            7,
            function,
            block,
            instruction,
            &PhpString::intern(name.as_bytes()),
            InvalidationEpoch::new(1),
            positional_shape(0),
            None,
            FunctionCallCacheTarget::CurrentUnit {
                unit_identity: 0,
                function,
            },
        );
    }

    let slot = table.slots.first().expect("slot");
    assert!(saw_megamorphic);
    assert_eq!(slot.state, InlineCacheState::Megamorphic);
    assert_eq!(slot.stats.guard_failures, 0);
    assert_eq!(slot.stats.misses, 4);
    assert_eq!(slot.stats.megamorphic_transitions, 1);
    assert_eq!(slot.stats.disabled_transitions, 0);
    assert_eq!(
        slot.payload(),
        &InlineCachePayload::Empty(InlineCacheKind::FunctionCall)
    );
}

#[test]
fn function_call_cache_hits_polymorphic_entries_before_cap() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();

    table.observe_slot(
        7,
        function,
        block,
        instruction,
        InlineCacheKind::FunctionCall,
    );
    for (index, name) in ["perf_fn_a", "perf_fn_b"].iter().enumerate() {
        table.install_function_call(
            7,
            function,
            block,
            instruction,
            &PhpString::intern(name.as_bytes()),
            InvalidationEpoch::new(1),
            positional_shape(0),
            None,
            FunctionCallCacheTarget::CurrentUnit {
                unit_identity: 0,
                function: FunctionId::new(index as u32),
            },
        );
    }

    let (target, event) = table.lookup_function_call(
        7,
        function,
        block,
        instruction,
        &PhpString::intern(b"perf_fn_b"),
        InvalidationEpoch::new(1),
        &positional_shape(0),
        None,
    );

    assert_eq!(
        target,
        Some(FunctionCallCacheTarget::CurrentUnit {
            unit_identity: 0,
            function: FunctionId::new(1)
        })
    );
    assert!(event.hit);
    assert!(event.polymorphic);
    assert!(!event.guard_failure);
}

#[test]
fn method_call_cache_hits_same_receiver_scope_and_epoch() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();

    table.observe_slot(9, function, block, instruction, InlineCacheKind::MethodCall);
    table.install_method_call(
        9,
        function,
        block,
        instruction,
        "value",
        "performancemethod",
        Some("performancecaller"),
        InvalidationEpoch::new(4),
        method_target(
            "performancemethod",
            7,
            "PerfMethod",
            function,
            InvalidationEpoch::new(4),
        ),
    );
    let (target, event) = table.lookup_method_call(
        9,
        function,
        block,
        instruction,
        "value",
        "performancemethod",
        Some("performancecaller"),
        InvalidationEpoch::new(4),
    );

    assert!(target.is_some());
    assert_eq!(event.kind, Some(InlineCacheKind::MethodCall));
    assert!(event.hit);
    assert!(!event.miss);
}

#[test]
fn method_call_cache_guard_fails_on_receiver_change() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();

    table.observe_slot(9, function, block, instruction, InlineCacheKind::MethodCall);
    table.install_method_call(
        9,
        function,
        block,
        instruction,
        "value",
        "performancemethoda",
        None,
        InvalidationEpoch::new(4),
        method_target(
            "performancemethoda",
            7,
            "PerfMethodA",
            function,
            InvalidationEpoch::new(4),
        ),
    );
    let (target, event) = table.lookup_method_call(
        9,
        function,
        block,
        instruction,
        "value",
        "performancemethodb",
        None,
        InvalidationEpoch::new(4),
    );

    assert!(target.is_none());
    assert_eq!(event.kind, Some(InlineCacheKind::MethodCall));
    assert!(event.guard_failure);
    assert!(event.miss);
}

#[test]
fn method_call_cache_invalidates_on_epoch_change() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();

    table.observe_slot(9, function, block, instruction, InlineCacheKind::MethodCall);
    table.install_method_call(
        9,
        function,
        block,
        instruction,
        "value",
        "performancemethod",
        None,
        InvalidationEpoch::new(4),
        method_target(
            "performancemethod",
            7,
            "PerfMethod",
            function,
            InvalidationEpoch::new(4),
        ),
    );
    let (target, event) = table.lookup_method_call(
        9,
        function,
        block,
        instruction,
        "value",
        "performancemethod",
        None,
        InvalidationEpoch::new(5),
    );

    assert!(target.is_none());
    assert_eq!(event.kind, Some(InlineCacheKind::MethodCall));
    assert!(event.invalidation);
    assert!(event.miss);
}

#[test]
fn method_call_cache_records_polymorphic_receiver_targets() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();

    table.observe_slot(9, function, block, instruction, InlineCacheKind::MethodCall);
    for (receiver, function_id) in [
        ("performancemethoda", FunctionId::new(1)),
        ("performancemethodb", FunctionId::new(2)),
    ] {
        table.install_method_call(
            9,
            function,
            block,
            instruction,
            "value",
            receiver,
            None,
            InvalidationEpoch::new(4),
            method_target(
                receiver,
                function_id.raw(),
                receiver,
                function_id,
                InvalidationEpoch::new(4),
            ),
        );
    }

    let (target, event) = table.lookup_method_call(
        9,
        function,
        block,
        instruction,
        "value",
        "performancemethodb",
        None,
        InvalidationEpoch::new(4),
    );

    assert!(target.is_some());
    assert!(event.hit);
    assert!(event.polymorphic);
    assert!(!event.monomorphic);
    let slot = table.slots.first().expect("slot");
    assert_eq!(slot.state, InlineCacheState::Polymorphic);
    assert_eq!(slot.method_call_entries().len(), 2);
}

#[test]
fn method_call_cache_overflow_reaches_megamorphic_state() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();

    table.observe_slot(9, function, block, instruction, InlineCacheKind::MethodCall);
    for receiver in [
        "performancemethoda",
        "performancemethodb",
        "performancemethodc",
        "performancemethodd",
        "performancemethode",
    ] {
        table.install_method_call(
            9,
            function,
            block,
            instruction,
            "value",
            receiver,
            None,
            InvalidationEpoch::new(4),
            method_target(receiver, 7, receiver, function, InvalidationEpoch::new(4)),
        );
    }

    let (target, event) = table.lookup_method_call(
        9,
        function,
        block,
        instruction,
        "value",
        "performancemethoda",
        None,
        InvalidationEpoch::new(4),
    );

    assert!(target.is_none());
    assert!(event.megamorphic);
    assert!(event.resolver_required);
    let slot = table.slots.first().expect("slot");
    assert_eq!(slot.state, InlineCacheState::Megamorphic);
    assert!(slot.method_call_entries().is_empty());
}

#[test]
fn property_fetch_cache_hits_same_receiver_scope_and_epoch() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();

    table.observe_slot(
        11,
        function,
        block,
        instruction,
        InlineCacheKind::PropertyFetch,
    );
    table.install_property_fetch(
        11,
        function,
        block,
        instruction,
        "value",
        "performancebox",
        None,
        InvalidationEpoch::new(6),
        PropertyFetchCacheTarget::CurrentUnit {
            target: property_target("performancebox", "PerfBox", 11),
        },
    );
    let (target, event) = table.lookup_property_fetch(
        11,
        function,
        block,
        instruction,
        "value",
        "performancebox",
        Some("different_scope_allowed_for_public"),
        InvalidationEpoch::new(6),
    );

    assert!(target.is_some());
    assert_eq!(event.kind, Some(InlineCacheKind::PropertyFetch));
    assert!(event.hit);
    assert!(!event.miss);
}

#[test]
fn property_fetch_cache_guard_fails_on_receiver_change() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();

    table.observe_slot(
        11,
        function,
        block,
        instruction,
        InlineCacheKind::PropertyFetch,
    );
    table.install_property_fetch(
        11,
        function,
        block,
        instruction,
        "value",
        "performanceboxa",
        None,
        InvalidationEpoch::new(6),
        PropertyFetchCacheTarget::CurrentUnit {
            target: property_target("performanceboxa", "PerfBoxA", 12),
        },
    );
    let (target, event) = table.lookup_property_fetch(
        11,
        function,
        block,
        instruction,
        "value",
        "performanceboxb",
        None,
        InvalidationEpoch::new(6),
    );

    assert!(target.is_none());
    assert_eq!(event.kind, Some(InlineCacheKind::PropertyFetch));
    assert!(event.guard_failure);
    assert!(event.miss);
}

#[test]
fn property_fetch_cache_invalidates_on_epoch_change() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();

    table.observe_slot(
        11,
        function,
        block,
        instruction,
        InlineCacheKind::PropertyFetch,
    );
    table.install_property_fetch(
        11,
        function,
        block,
        instruction,
        "value",
        "performancebox",
        None,
        InvalidationEpoch::new(6),
        PropertyFetchCacheTarget::CurrentUnit {
            target: property_target("performancebox", "PerfBox", 13),
        },
    );
    let (target, event) = table.lookup_property_fetch(
        11,
        function,
        block,
        instruction,
        "value",
        "performancebox",
        None,
        InvalidationEpoch::new(7),
    );

    assert!(target.is_none());
    assert_eq!(event.kind, Some(InlineCacheKind::PropertyFetch));
    assert!(event.invalidation);
    assert!(event.miss);
}

#[test]
fn property_fetch_cache_records_polymorphic_receiver_targets() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();

    table.observe_slot(
        11,
        function,
        block,
        instruction,
        InlineCacheKind::PropertyFetch,
    );
    for receiver in ["performanceboxa", "performanceboxb"] {
        table.install_property_fetch(
            11,
            function,
            block,
            instruction,
            "value",
            receiver,
            None,
            InvalidationEpoch::new(6),
            PropertyFetchCacheTarget::CurrentUnit {
                target: property_target(receiver, receiver, 14),
            },
        );
    }

    let (target, event) = table.lookup_property_fetch(
        11,
        function,
        block,
        instruction,
        "value",
        "performanceboxb",
        Some("public_scope"),
        InvalidationEpoch::new(6),
    );

    assert!(target.is_some());
    assert!(event.hit);
    assert!(event.polymorphic);
    assert!(!event.monomorphic);
    let slot = table.slots.first().expect("slot");
    assert_eq!(slot.state, InlineCacheState::Polymorphic);
    assert_eq!(slot.property_fetch_entries().len(), 2);
}

#[test]
fn property_fetch_cache_overflow_reaches_megamorphic_state() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();

    table.observe_slot(
        11,
        function,
        block,
        instruction,
        InlineCacheKind::PropertyFetch,
    );
    for receiver in [
        "performanceboxa",
        "performanceboxb",
        "performanceboxc",
        "performanceboxd",
        "performanceboxe",
    ] {
        table.install_property_fetch(
            11,
            function,
            block,
            instruction,
            "value",
            receiver,
            None,
            InvalidationEpoch::new(6),
            PropertyFetchCacheTarget::CurrentUnit {
                target: property_target(receiver, receiver, 15),
            },
        );
    }

    let (target, event) = table.lookup_property_fetch(
        11,
        function,
        block,
        instruction,
        "value",
        "performanceboxa",
        None,
        InvalidationEpoch::new(6),
    );

    assert!(target.is_none());
    assert!(event.megamorphic);
    assert!(event.resolver_required);
    let slot = table.slots.first().expect("slot");
    assert_eq!(slot.state, InlineCacheState::Megamorphic);
    assert!(slot.property_fetch_entries().is_empty());
}

#[test]
fn property_assign_cache_transitions_from_cold_through_megamorphic() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();

    table.observe_slot(
        12,
        function,
        block,
        instruction,
        InlineCacheKind::PropertyAssign,
    );
    assert_eq!(table.slots[0].state, InlineCacheState::Cold);

    for (index, receiver) in [
        "performanceboxa",
        "performanceboxb",
        "performanceboxc",
        "performanceboxd",
    ]
    .into_iter()
    .enumerate()
    {
        table.install_property_assign(
            12,
            function,
            block,
            instruction,
            "value",
            receiver,
            None,
            InvalidationEpoch::new(6),
            super::PropertyAssignCacheTarget::CurrentUnit {
                target: property_assign_target(receiver, receiver, index as u32),
            },
        );
        assert_eq!(
            table.slots[0].state,
            if index == 0 {
                InlineCacheState::Monomorphic
            } else {
                InlineCacheState::Polymorphic
            }
        );
    }

    table.install_property_assign(
        12,
        function,
        block,
        instruction,
        "value",
        "performanceboxe",
        None,
        InvalidationEpoch::new(6),
        super::PropertyAssignCacheTarget::CurrentUnit {
            target: property_assign_target("performanceboxe", "performanceboxe", 5),
        },
    );
    let (target, event) = table.lookup_property_assign(
        12,
        function,
        block,
        instruction,
        "value",
        "performanceboxa",
        None,
        InvalidationEpoch::new(6),
    );
    assert!(target.is_none());
    assert!(event.megamorphic);
    assert!(event.resolver_required);
    assert_eq!(table.slots[0].state, InlineCacheState::Megamorphic);
    assert!(table.slots[0].property_assign_entries().is_empty());
}

#[test]
fn property_assign_cache_invalidates_and_reinstalls() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();

    table.observe_slot(
        13,
        function,
        block,
        instruction,
        InlineCacheKind::PropertyAssign,
    );
    table.install_property_assign(
        13,
        function,
        block,
        instruction,
        "value",
        "performancebox",
        None,
        InvalidationEpoch::new(6),
        super::PropertyAssignCacheTarget::CurrentUnit {
            target: property_assign_target("performancebox", "PerfBox", 6),
        },
    );
    let (target, event) = table.lookup_property_assign(
        13,
        function,
        block,
        instruction,
        "value",
        "performancebox",
        None,
        InvalidationEpoch::new(7),
    );
    assert!(target.is_none());
    assert!(event.invalidation);
    assert_eq!(table.slots[0].state, InlineCacheState::Cold);
    assert!(table.slots[0].property_assign_entries().is_empty());

    table.install_property_assign(
        13,
        function,
        block,
        instruction,
        "value",
        "performancebox",
        None,
        InvalidationEpoch::new(7),
        super::PropertyAssignCacheTarget::CurrentUnit {
            target: property_assign_target("performancebox", "PerfBox", 7),
        },
    );
    let (target, event) = table.lookup_property_assign(
        13,
        function,
        block,
        instruction,
        "value",
        "performancebox",
        None,
        InvalidationEpoch::new(7),
    );
    assert!(target.is_some());
    assert!(event.hit);
    assert_eq!(table.slots[0].state, InlineCacheState::Monomorphic);
}

#[test]
fn warmed_dispatch_hits_share_resolved_target_allocations() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);

    let mut function_table = InlineCacheTable::default();
    function_table.observe_slot(
        19,
        function,
        block,
        instruction,
        InlineCacheKind::FunctionCall,
    );
    let installed_builtin_name: Arc<str> = Arc::from("strlen");
    function_table.install_function_call(
        19,
        function,
        block,
        instruction,
        &PhpString::intern(b"strlen"),
        InvalidationEpoch::new(1),
        positional_shape(1),
        Some(builtin_metadata("strlen")),
        FunctionCallCacheTarget::Builtin {
            kind: FunctionCallBuiltinKind::InternalRegistry,
            name: Arc::clone(&installed_builtin_name),
        },
    );
    let (function_hit, function_event) = function_table.lookup_function_call(
        19,
        function,
        block,
        instruction,
        &PhpString::intern(b"strlen"),
        InvalidationEpoch::new(1),
        &positional_shape(1),
        Some(&builtin_metadata("strlen")),
    );
    let Some(FunctionCallCacheTarget::Builtin { name, .. }) = function_hit else {
        panic!("builtin function cache hit");
    };
    assert!(function_event.hit);
    assert!(Arc::ptr_eq(&installed_builtin_name, &name));

    let mut method_table = InlineCacheTable::default();
    method_table.observe_slot(
        20,
        function,
        block,
        instruction,
        InlineCacheKind::MethodCall,
    );
    let installed_method = method_target(
        "performancebox",
        1,
        "PerfBox",
        FunctionId::new(1),
        InvalidationEpoch::new(1),
    );
    method_table.install_method_call(
        20,
        function,
        block,
        instruction,
        "value",
        "performancebox",
        None,
        InvalidationEpoch::new(1),
        installed_method.clone(),
    );
    let (method_hit, method_event) = method_table.lookup_method_call(
        20,
        function,
        block,
        instruction,
        "value",
        "performancebox",
        None,
        InvalidationEpoch::new(1),
    );
    let method_hit = method_hit.expect("method cache hit");
    assert!(method_event.hit);
    assert!(std::ptr::eq(
        installed_method.resolved_target(),
        method_hit.resolved_target()
    ));
    let installed_method_name =
        Arc::clone(&method_table.slots[0].method_call_entries()[0].lowered_method);
    let cached_method_name = &method_table.slots[0].method_call_entries()[0].lowered_method;
    assert!(Arc::ptr_eq(&installed_method_name, cached_method_name));

    let mut fetch_table = InlineCacheTable::default();
    fetch_table.observe_slot(
        21,
        function,
        block,
        instruction,
        InlineCacheKind::PropertyFetch,
    );
    let installed_fetch = PropertyFetchCacheTarget::CurrentUnit {
        target: property_target("performancebox", "PerfBox", 2),
    };
    fetch_table.install_property_fetch(
        21,
        function,
        block,
        instruction,
        "value",
        "performancebox",
        None,
        InvalidationEpoch::new(2),
        installed_fetch.clone(),
    );
    let (fetch_hit, fetch_event) = fetch_table.lookup_property_fetch(
        21,
        function,
        block,
        instruction,
        "value",
        "performancebox",
        None,
        InvalidationEpoch::new(2),
    );
    let fetch_hit = fetch_hit.expect("property-fetch cache hit");
    assert!(fetch_event.hit);
    assert!(std::ptr::eq(
        installed_fetch.resolved_target(),
        fetch_hit.resolved_target()
    ));

    let mut assign_table = InlineCacheTable::default();
    assign_table.observe_slot(
        22,
        function,
        block,
        instruction,
        InlineCacheKind::PropertyAssign,
    );
    let installed_assign = super::PropertyAssignCacheTarget::CurrentUnit {
        target: property_assign_target("performancebox", "PerfBox", 3),
    };
    assign_table.install_property_assign(
        22,
        function,
        block,
        instruction,
        "value",
        "performancebox",
        None,
        InvalidationEpoch::new(3),
        installed_assign.clone(),
    );
    let (assign_hit, assign_event) = assign_table.lookup_property_assign(
        22,
        function,
        block,
        instruction,
        "value",
        "performancebox",
        None,
        InvalidationEpoch::new(3),
    );
    let assign_hit = assign_hit.expect("property-assign cache hit");
    assert!(assign_event.hit);
    assert!(std::ptr::eq(
        installed_assign.resolved_target(),
        assign_hit.resolved_target()
    ));
}

#[test]
fn dispatch_identity_is_stable_and_unit_scoped() {
    let function = FunctionId::new(4);
    let current = FunctionCallCacheTarget::CurrentUnit {
        unit_identity: 11,
        function,
    };
    assert_eq!(current.clone(), current);
    assert_ne!(
        current,
        FunctionCallCacheTarget::CurrentUnit {
            unit_identity: 12,
            function,
        }
    );

    let route = MethodCallRouteIdentity {
        owner_unit_identity: 11,
        declaring_class_id: ClassId::new(2),
        function,
        method_slot_index: 3,
    };
    assert_eq!(route, route);
    assert_ne!(
        route,
        MethodCallRouteIdentity {
            owner_unit_identity: 12,
            ..route
        }
    );
    assert_ne!(
        route,
        MethodCallRouteIdentity {
            method_slot_index: 4,
            ..route
        }
    );
}

#[test]
fn class_static_cache_hits_same_class_member_scope_and_epoch() {
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
    table.install_class_constant_static_property(
        13,
        function,
        block,
        instruction,
        ClassConstantStaticPropertyCacheKind::ClassConstant,
        "performanceclass",
        "VALUE",
        None,
        InvalidationEpoch::new(8),
        ClassConstantStaticPropertyCacheTarget::CurrentUnit {
            kind: ClassConstantStaticPropertyCacheKind::ClassConstant,
            resolved_class: "performanceclass".to_owned(),
            declaring_class: "PerfClass".to_owned(),
            member: "VALUE".to_owned(),
        },
    );
    let (target, event) = table.lookup_class_constant_static_property(
        13,
        function,
        block,
        instruction,
        ClassConstantStaticPropertyCacheKind::ClassConstant,
        "performanceclass",
        "VALUE",
        Some("public_scope_ignored"),
        InvalidationEpoch::new(8),
    );

    assert!(target.is_some());
    assert_eq!(
        event.kind,
        Some(InlineCacheKind::ClassConstantStaticProperty)
    );
    assert!(event.hit);
    assert!(!event.miss);
}

#[test]
fn class_static_cache_guard_fails_on_resolved_class_change() {
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
    table.install_class_constant_static_property(
        13,
        function,
        block,
        instruction,
        ClassConstantStaticPropertyCacheKind::StaticProperty,
        "performancea",
        "value",
        None,
        InvalidationEpoch::new(8),
        ClassConstantStaticPropertyCacheTarget::CurrentUnit {
            kind: ClassConstantStaticPropertyCacheKind::StaticProperty,
            resolved_class: "performancea".to_owned(),
            declaring_class: "PerfA".to_owned(),
            member: "value".to_owned(),
        },
    );
    let (target, event) = table.lookup_class_constant_static_property(
        13,
        function,
        block,
        instruction,
        ClassConstantStaticPropertyCacheKind::StaticProperty,
        "performanceb",
        "value",
        None,
        InvalidationEpoch::new(8),
    );

    assert!(target.is_none());
    assert_eq!(
        event.kind,
        Some(InlineCacheKind::ClassConstantStaticProperty)
    );
    assert!(event.guard_failure);
    assert!(event.miss);
}

#[test]
fn class_static_cache_invalidates_on_epoch_change() {
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
    table.install_class_constant_static_property(
        13,
        function,
        block,
        instruction,
        ClassConstantStaticPropertyCacheKind::EnumCase,
        "performanceenum",
        "Ready",
        None,
        InvalidationEpoch::new(8),
        ClassConstantStaticPropertyCacheTarget::CurrentUnit {
            kind: ClassConstantStaticPropertyCacheKind::EnumCase,
            resolved_class: "performanceenum".to_owned(),
            declaring_class: "PerfEnum".to_owned(),
            member: "Ready".to_owned(),
        },
    );
    let (target, event) = table.lookup_class_constant_static_property(
        13,
        function,
        block,
        instruction,
        ClassConstantStaticPropertyCacheKind::EnumCase,
        "performanceenum",
        "Ready",
        None,
        InvalidationEpoch::new(9),
    );

    assert!(target.is_none());
    assert_eq!(
        event.kind,
        Some(InlineCacheKind::ClassConstantStaticProperty)
    );
    assert!(event.invalidation);
    assert!(event.miss);
}

#[test]
fn autoload_class_lookup_cache_hits_same_guard_and_epochs() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();
    let request = AutoloadClassLookupCacheKey {
        kind: AutoloadClassLookupKind::Class,
        normalized_name: "performance\\cache\\thing".to_owned(),
        autoload_enabled: true,
        autoload_stack_depth: 0,
        include_path_config: "vendor".to_owned(),
        composer_map_fingerprint: Some(std::sync::Arc::from("classmap:1")),
    };
    let epochs = AutoloadClassLookupEpochs {
        autoload_stack_epoch: 1,
        class_table_epoch: 2,
        include_config_epoch: 3,
    };

    table.observe_slot(
        17,
        function,
        block,
        instruction,
        InlineCacheKind::AutoloadClassLookup,
    );
    table.install_autoload_class_lookup(
        17,
        function,
        block,
        instruction,
        request.clone(),
        epochs,
        AutoloadClassLookupCacheTarget::Positive {
            display_name: "Perf\\Cache\\Thing".to_owned(),
        },
    );
    let (target, event) =
        table.lookup_autoload_class_lookup(17, function, block, instruction, &request, epochs);

    assert_eq!(
        target,
        Some(AutoloadClassLookupCacheTarget::Positive {
            display_name: "Perf\\Cache\\Thing".to_owned(),
        })
    );
    assert_eq!(event.kind, Some(InlineCacheKind::AutoloadClassLookup));
    assert!(event.hit);
    assert!(!event.miss);
}

#[test]
fn autoload_class_lookup_cache_guard_fails_on_lookup_kind_change() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();
    let request = AutoloadClassLookupCacheKey {
        kind: AutoloadClassLookupKind::Class,
        normalized_name: "performance\\cache\\thing".to_owned(),
        autoload_enabled: false,
        autoload_stack_depth: 0,
        include_path_config: ".".to_owned(),
        composer_map_fingerprint: None,
    };
    let changed = AutoloadClassLookupCacheKey {
        kind: AutoloadClassLookupKind::Interface,
        ..request.clone()
    };
    let epochs = AutoloadClassLookupEpochs::default();

    table.observe_slot(
        17,
        function,
        block,
        instruction,
        InlineCacheKind::AutoloadClassLookup,
    );
    table.install_autoload_class_lookup(
        17,
        function,
        block,
        instruction,
        request,
        epochs,
        AutoloadClassLookupCacheTarget::Negative,
    );
    let (target, event) =
        table.lookup_autoload_class_lookup(17, function, block, instruction, &changed, epochs);

    assert!(target.is_none());
    assert_eq!(event.kind, Some(InlineCacheKind::AutoloadClassLookup));
    assert!(event.guard_failure);
    assert!(event.miss);
}

#[test]
fn autoload_class_lookup_cache_invalidates_on_class_table_epoch_change() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();
    let request = AutoloadClassLookupCacheKey {
        kind: AutoloadClassLookupKind::Class,
        normalized_name: "performance\\cache\\late".to_owned(),
        autoload_enabled: false,
        autoload_stack_depth: 0,
        include_path_config: ".".to_owned(),
        composer_map_fingerprint: None,
    };

    table.observe_slot(
        17,
        function,
        block,
        instruction,
        InlineCacheKind::AutoloadClassLookup,
    );
    table.install_autoload_class_lookup(
        17,
        function,
        block,
        instruction,
        request.clone(),
        AutoloadClassLookupEpochs {
            autoload_stack_epoch: 0,
            class_table_epoch: 1,
            include_config_epoch: 0,
        },
        AutoloadClassLookupCacheTarget::Negative,
    );
    let (target, event) = table.lookup_autoload_class_lookup(
        17,
        function,
        block,
        instruction,
        &request,
        AutoloadClassLookupEpochs {
            autoload_stack_epoch: 0,
            class_table_epoch: 2,
            include_config_epoch: 0,
        },
    );

    assert!(target.is_none());
    assert_eq!(event.kind, Some(InlineCacheKind::AutoloadClassLookup));
    assert!(event.invalidation);
    assert!(event.miss);
}

#[test]
fn runtime_install_over_a_seeded_slot_drops_seed_attribution() {
    let mut table = InlineCacheTable::default();
    let snapshot = FunctionCallSiteSnapshot {
        function: 0,
        block: 0,
        instruction: 0,
        lowered_name: "seeded".to_owned(),
        arity: 0,
        epoch: 1,
        target_function: 4,
    };
    assert_eq!(
        table.seed_persistent_function_callsites(7, std::slice::from_ref(&snapshot), |_| true),
        1
    );
    // A runtime install at the same site (a second distinct call) clears
    // the seed flag, so later hits are not misattributed as seeded.
    table.observe_slot(
        7,
        FunctionId::new(0),
        BlockId::new(0),
        InstrId::new(0),
        InlineCacheKind::FunctionCall,
    );
    table.install_function_call(
        7,
        FunctionId::new(0),
        BlockId::new(0),
        InstrId::new(0),
        &PhpString::intern(b"learned"),
        InvalidationEpoch::new(1),
        FunctionCallShape {
            arity: 0,
            named_arguments: Vec::new(),
            by_ref_arguments: CallReferenceMask::default(),
        },
        None,
        FunctionCallCacheTarget::CurrentUnit {
            unit_identity: 0,
            function: FunctionId::new(9),
        },
    );
    let name = PhpString::intern(b"learned");
    let shape = FunctionCallShape {
        arity: 0,
        named_arguments: Vec::new(),
        by_ref_arguments: CallReferenceMask::default(),
    };
    let (_, observation) = table.lookup_function_call(
        7,
        FunctionId::new(0),
        BlockId::new(0),
        InstrId::new(0),
        &name,
        InvalidationEpoch::new(1),
        &shape,
        None,
    );
    assert!(observation.hit);
    assert!(
        !observation.seeded,
        "a runtime-learned hit must not attribute to the seed"
    );
}

#[test]
fn seed_rejects_callsites_whose_target_no_longer_resolves() {
    let mut table = InlineCacheTable::default();
    let snapshot = FunctionCallSiteSnapshot {
        function: 0,
        block: 2,
        instruction: 7,
        lowered_name: "app\\f".to_owned(),
        arity: 0,
        epoch: 1,
        target_function: 5,
    };
    // Target resolution fails (e.g. the recorded global-f target no longer
    // matches the namespaced call name, or the id is out of range): no
    // slot is created, so a later lookup misses instead of dispatching the
    // wrong function.
    assert_eq!(
        table.seed_persistent_function_callsites(1, std::slice::from_ref(&snapshot), |_| false),
        0
    );
    let name = PhpString::intern(b"app\\f");
    let shape = FunctionCallShape {
        arity: 0,
        named_arguments: Vec::new(),
        by_ref_arguments: CallReferenceMask::default(),
    };
    let (target, _) = table.lookup_function_call(
        1,
        FunctionId::new(0),
        BlockId::new(2),
        InstrId::new(7),
        &name,
        InvalidationEpoch::new(1),
        &shape,
        None,
    );
    assert!(target.is_none(), "rejected seed must not install a target");
}

#[test]
fn seeded_function_callsites_hit_behind_the_full_guard_protocol() {
    let mut table = InlineCacheTable::default();
    let snapshot = FunctionCallSiteSnapshot {
        function: 0,
        block: 2,
        instruction: 7,
        lowered_name: "probe_tag".to_owned(),
        arity: 1,
        epoch: 3,
        target_function: 9,
    };
    // This test exercises the lookup guard protocol, not target
    // resolution, so accept the seed unconditionally.
    assert_eq!(
        table.seed_persistent_function_callsites(42, &[snapshot], |_| true),
        1
    );

    let name = PhpString::intern(b"probe_tag");
    let shape = FunctionCallShape {
        arity: 1,
        named_arguments: Vec::new(),
        by_ref_arguments: CallReferenceMask::default(),
    };
    // Matching name/shape/epoch: the seeded target dispatches and the hit
    // attributes to the seed.
    let (target, observation) = table.lookup_function_call(
        42,
        FunctionId::new(0),
        BlockId::new(2),
        InstrId::new(7),
        &name,
        InvalidationEpoch::new(3),
        &shape,
        None,
    );
    assert_eq!(
        target,
        Some(FunctionCallCacheTarget::CurrentUnit {
            unit_identity: 42,
            function: FunctionId::new(9)
        })
    );
    assert!(observation.hit);
    assert!(observation.seeded);

    // A live epoch that diverges from the recorded observation epoch
    // invalidates the seed back to generic resolution.
    let (target, observation) = table.lookup_function_call(
        42,
        FunctionId::new(0),
        BlockId::new(2),
        InstrId::new(7),
        &name,
        InvalidationEpoch::new(4),
        &shape,
        None,
    );
    assert!(target.is_none());
    assert!(observation.invalidation);
    assert!(
        observation.seeded,
        "the invalidation attributes to the seed"
    );

    // Exporting a seeded-then-invalidated slot yields nothing.
    assert!(table.export_persistent_function_callsites(42).is_empty());
}

#[test]
fn include_path_cache_hits_same_request_and_epoch_after_validation() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();
    let request = IncludePathCacheKey {
        path: "lib.php".to_owned(),
        include_path: vec![PathBuf::from("src")],
        cwd: PathBuf::from("/repo"),
        calling_file_directory: Some(PathBuf::from("/repo/app")),
    };
    let target = IncludePathCacheTarget {
        canonical_path: PathBuf::from("/repo/src/lib.php"),
        resolution_path: Some(PathBuf::from("/repo/src/lib.php")),
        fingerprint: IncludePathFileFingerprint {
            len: 17,
            modified_unix_nanos: Some(10),
            changed_unix_nanos: None,
            readonly: false,
            inode: None,
            device: None,
        },
        directory_version: None,
    };

    table.observe_slot(
        15,
        function,
        block,
        instruction,
        InlineCacheKind::IncludePath,
    );
    table.install_include_path(
        15,
        function,
        block,
        instruction,
        request.clone(),
        InvalidationEpoch::new(2),
        target.clone(),
    );
    let (cached, probe) = table.lookup_include_path(
        15,
        function,
        block,
        instruction,
        &request,
        InvalidationEpoch::new(2),
    );
    let hit = table.record_include_path_hit(15, function, block, instruction);

    assert_eq!(cached, Some(target));
    assert_eq!(probe.kind, Some(InlineCacheKind::IncludePath));
    assert!(!probe.hit);
    assert!(hit.hit);
    assert!(!hit.miss);
}

#[test]
fn include_path_cache_guard_fails_on_include_path_order_change() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();
    let request = IncludePathCacheKey {
        path: "lib.php".to_owned(),
        include_path: vec![PathBuf::from("first"), PathBuf::from("second")],
        cwd: PathBuf::from("/repo"),
        calling_file_directory: Some(PathBuf::from("/repo/app")),
    };
    let changed = IncludePathCacheKey {
        include_path: vec![PathBuf::from("second"), PathBuf::from("first")],
        ..request.clone()
    };

    table.observe_slot(
        15,
        function,
        block,
        instruction,
        InlineCacheKind::IncludePath,
    );
    table.install_include_path(
        15,
        function,
        block,
        instruction,
        request,
        InvalidationEpoch::new(2),
        IncludePathCacheTarget {
            canonical_path: PathBuf::from("/repo/first/lib.php"),
            resolution_path: Some(PathBuf::from("/repo/first/lib.php")),
            fingerprint: IncludePathFileFingerprint {
                len: 17,
                modified_unix_nanos: Some(10),
                changed_unix_nanos: None,
                readonly: false,
                inode: None,
                device: None,
            },
            directory_version: None,
        },
    );
    let (cached, event) = table.lookup_include_path(
        15,
        function,
        block,
        instruction,
        &changed,
        InvalidationEpoch::new(2),
    );

    assert!(cached.is_none());
    assert_eq!(event.kind, Some(InlineCacheKind::IncludePath));
    assert!(event.guard_failure);
    assert!(event.miss);
}

#[test]
fn include_path_cache_invalidates_on_epoch_change() {
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    let mut table = InlineCacheTable::default();
    let request = IncludePathCacheKey {
        path: "lib.php".to_owned(),
        include_path: vec![PathBuf::from("src")],
        cwd: PathBuf::from("/repo"),
        calling_file_directory: Some(PathBuf::from("/repo/app")),
    };

    table.observe_slot(
        15,
        function,
        block,
        instruction,
        InlineCacheKind::IncludePath,
    );
    table.install_include_path(
        15,
        function,
        block,
        instruction,
        request.clone(),
        InvalidationEpoch::new(2),
        IncludePathCacheTarget {
            canonical_path: PathBuf::from("/repo/src/lib.php"),
            resolution_path: Some(PathBuf::from("/repo/src/lib.php")),
            fingerprint: IncludePathFileFingerprint {
                len: 17,
                modified_unix_nanos: Some(10),
                changed_unix_nanos: None,
                readonly: false,
                inode: None,
                device: None,
            },
            directory_version: None,
        },
    );
    let (cached, event) = table.lookup_include_path(
        15,
        function,
        block,
        instruction,
        &request,
        InvalidationEpoch::new(3),
    );

    assert!(cached.is_none());
    assert_eq!(event.kind, Some(InlineCacheKind::IncludePath));
    assert!(event.invalidation);
    assert!(event.miss);
}

fn class_relation_key(kind: ClassRelationKind) -> ClassRelationCacheKey {
    ClassRelationCacheKey {
        kind,
        subject: "child".to_owned(),
        target: "base".to_owned(),
        member: None,
        visibility_context: None,
        config_fingerprint: "unit:1:strict:false".to_owned(),
    }
}

#[test]
fn class_relation_cache_records_hit_miss_and_invalidation() {
    let mut cache = ClassRelationCache::default();
    let key = class_relation_key(ClassRelationKind::InstanceOf);
    let epochs = ClassRelationEpochs {
        class_table_epoch: 1,
        autoload_epoch: 2,
        include_eval_epoch: 3,
        trait_interface_map_version: 4,
        method_table_version: 5,
    };
    let target = ClassRelationCacheTarget {
        matches: true,
        method_slot: None,
        declaring_class: None,
    };

    assert_eq!(cache.lookup(&key, epochs), ClassRelationCacheLookup::Miss);
    let slot = cache.install(key.clone(), epochs, target.clone());
    assert_eq!(slot.raw(), 0);
    assert_eq!(
        cache.lookup(&key, epochs),
        ClassRelationCacheLookup::Hit(target)
    );
    assert_eq!(
        cache.lookup(
            &key,
            ClassRelationEpochs {
                class_table_epoch: 2,
                ..epochs
            },
        ),
        ClassRelationCacheLookup::Invalidated
    );
    assert!(cache.is_empty());
}

#[test]
fn class_relation_cache_exposes_required_slot_kinds() {
    let kinds = [
        ClassRelationKind::ExtendsClass,
        ClassRelationKind::ImplementsInterface,
        ClassRelationKind::TraitComposition,
        ClassRelationKind::InstanceOf,
        ClassRelationKind::MethodOverrideSlot,
        ClassRelationKind::FinalMethodOrClass,
        ClassRelationKind::VisibilityContext,
        ClassRelationKind::AbstractInterfaceMethodRelation,
    ];
    let mut cache = ClassRelationCache::default();
    let epochs = ClassRelationEpochs::default();

    for (index, kind) in kinds.iter().copied().enumerate() {
        let mut key = class_relation_key(kind);
        key.member = Some(format!("m{index}"));
        cache.install(
            key.clone(),
            epochs,
            ClassRelationCacheTarget {
                matches: index % 2 == 0,
                method_slot: u32::try_from(index).ok(),
                declaring_class: Some("child".to_owned()),
            },
        );
        assert!(matches!(
            cache.lookup(&key, epochs),
            ClassRelationCacheLookup::Hit(_)
        ));
    }

    assert_eq!(cache.len(), kinds.len());
}
