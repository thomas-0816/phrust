use super::tests::{method_target, positional_shape, property_assign_target, property_target};
use super::{
    AutoloadClassLookupCacheKey, AutoloadClassLookupCacheTarget, AutoloadClassLookupEpochs,
    AutoloadClassLookupKind, ClassConstantStaticPropertyCacheKind,
    ClassConstantStaticPropertyCacheTarget, FunctionCallCacheTarget, IncludePathCacheKey,
    IncludePathCacheTarget, InlineCacheKind, InlineCacheTable, InvalidationEpoch,
    PropertyAssignCacheTarget, PropertyFetchCacheTarget,
};
use crate::bytecode::DenseCacheSlot;
use crate::include::IncludePathFileFingerprint;
use php_ir::ids::FunctionId;
use php_runtime::PhpString;
use std::path::PathBuf;

#[test]
fn dense_slot_binding_reaches_all_payloads_without_coordinate_map_access() {
    let kinds = [
        InlineCacheKind::FunctionCall,
        InlineCacheKind::MethodCall,
        InlineCacheKind::PropertyFetch,
        InlineCacheKind::PropertyAssign,
        InlineCacheKind::ClassConstantStaticProperty,
        InlineCacheKind::IncludePath,
        InlineCacheKind::AutoloadClassLookup,
    ];
    let descriptors: Vec<_> = kinds
        .into_iter()
        .enumerate()
        .map(|(instruction, kind)| DenseCacheSlot {
            kind,
            function: 0,
            block: 1,
            instruction: instruction as u32,
        })
        .collect();
    let mut table = InlineCacheTable::default();
    let (ids, observations) = table.bind_dense_slots(17, &descriptors);
    assert_eq!(ids.len(), kinds.len());
    assert_eq!(observations.len(), kinds.len());
    assert!(observations.iter().all(|(_, event)| event.slot_allocated));

    table.site_ids.clear();
    let (reused, observations) = table.bind_dense_slots(17, &descriptors);
    assert_eq!(reused.as_ref(), ids.as_ref());
    assert!(observations.is_empty());

    let name = PhpString::intern(b"dense_target");
    table.install_function_call_by_id(
        ids[0],
        &name,
        InvalidationEpoch::new(3),
        positional_shape(0),
        None,
        FunctionCallCacheTarget::CurrentUnit {
            function: FunctionId::new(4),
        },
    );
    let (target, observation) = table.lookup_function_call_by_id(
        ids[0],
        &name,
        InvalidationEpoch::new(3),
        &positional_shape(0),
        None,
    );
    assert!(observation.hit);
    assert_eq!(
        target,
        Some(FunctionCallCacheTarget::CurrentUnit {
            function: FunctionId::new(4)
        })
    );

    table.install_method_call_by_id(
        ids[1],
        "run",
        "denseclass",
        None,
        InvalidationEpoch::new(3),
        method_target(
            "denseclass",
            1,
            "DenseClass",
            FunctionId::new(5),
            InvalidationEpoch::new(3),
        ),
    );
    let (target, observation) = table.lookup_method_call_by_id(
        ids[1],
        "run",
        "denseclass",
        None,
        InvalidationEpoch::new(3),
    );
    assert!(target.is_some());
    assert!(observation.hit);

    table.install_property_fetch_by_id(
        ids[2],
        "value",
        "denseclass",
        None,
        InvalidationEpoch::new(3),
        PropertyFetchCacheTarget::CurrentUnit {
            target: property_target("denseclass", "DenseClass", 1),
        },
    );
    let (target, observation) = table.lookup_property_fetch_by_id(
        ids[2],
        "value",
        "denseclass",
        None,
        InvalidationEpoch::new(3),
    );
    assert!(target.is_some());
    assert!(observation.hit);

    table.install_property_assign_by_id(
        ids[3],
        "value",
        "denseclass",
        None,
        InvalidationEpoch::new(3),
        PropertyAssignCacheTarget::CurrentUnit {
            target: property_assign_target("denseclass", "DenseClass", 1),
        },
    );
    let (target, observation) = table.lookup_property_assign_by_id(
        ids[3],
        "value",
        "denseclass",
        None,
        InvalidationEpoch::new(3),
    );
    assert!(target.is_some());
    assert!(observation.hit);

    table.install_class_constant_static_property_by_id(
        ids[4],
        ClassConstantStaticPropertyCacheKind::ClassConstant,
        "denseclass",
        "VALUE",
        None,
        InvalidationEpoch::new(3),
        ClassConstantStaticPropertyCacheTarget::CurrentUnit {
            kind: ClassConstantStaticPropertyCacheKind::ClassConstant,
            resolved_class: "denseclass".to_owned(),
            declaring_class: "DenseClass".to_owned(),
            member: "VALUE".to_owned(),
        },
    );
    let (target, observation) = table.lookup_class_constant_static_property_by_id(
        ids[4],
        ClassConstantStaticPropertyCacheKind::ClassConstant,
        "denseclass",
        "VALUE",
        None,
        InvalidationEpoch::new(3),
    );
    assert!(target.is_some());
    assert!(observation.hit);

    let include_request = IncludePathCacheKey {
        path: "dense.php".to_owned(),
        include_path: vec![PathBuf::from("src")],
        cwd: PathBuf::from("/repo"),
        calling_file_directory: Some(PathBuf::from("/repo/app")),
    };
    let include_target = IncludePathCacheTarget {
        canonical_path: PathBuf::from("/repo/src/dense.php"),
        resolution_path: Some(PathBuf::from("/repo/src/dense.php")),
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
    table.install_include_path_by_id(
        ids[5],
        include_request.clone(),
        InvalidationEpoch::new(3),
        include_target.clone(),
    );
    let (target, observation) =
        table.lookup_include_path_by_id(ids[5], &include_request, InvalidationEpoch::new(3));
    assert_eq!(target, Some(include_target));
    assert!(!observation.miss);
    assert!(table.record_include_path_hit_by_id(ids[5]).hit);

    let autoload_request = AutoloadClassLookupCacheKey {
        kind: AutoloadClassLookupKind::ClassLike,
        normalized_name: "denseclass".to_owned(),
        autoload_enabled: true,
        autoload_stack_depth: 0,
        include_path_config: ".".to_owned(),
        composer_map_fingerprint: None,
    };
    let autoload_epochs = AutoloadClassLookupEpochs {
        autoload_stack_epoch: 1,
        class_table_epoch: 2,
        include_config_epoch: 3,
    };
    table.install_autoload_class_lookup_by_id(
        ids[6],
        autoload_request.clone(),
        autoload_epochs,
        AutoloadClassLookupCacheTarget::Positive {
            display_name: "DenseClass".to_owned(),
        },
    );
    let (target, observation) =
        table.lookup_autoload_class_lookup_by_id(ids[6], &autoload_request, autoload_epochs);
    assert!(target.is_some());
    assert!(observation.hit);
}
