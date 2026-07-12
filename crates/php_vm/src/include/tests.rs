use super::resolution_cache::NEGATIVE_INCLUDE_CACHE_SHARD_CAPACITY;
use super::*;
use crate::compiled_unit::CompiledUnit;
use crate::test_include_compiler::{
    TestIncludeCompiler, TestOptimizationLevel as OptimizationLevel,
};
use php_ir::instruction::{BinaryOp, InstructionKind};
use std::fs::{self, FileTimes, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn include_cache_instances_have_stable_distinct_ids() {
    let first = IncludeCache::new(1);
    let first_id = first.instance_id();
    let second = IncludeCache::new(1);

    assert_eq!(first.instance_id(), first_id);
    assert_ne!(first_id, second.instance_id());
    assert_ne!(first_id.get(), 0);
}

#[test]
fn include_module_ownership_is_one_way() {
    let include_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/include");
    let modules = [
        ("diagnostics", &[][..]),
        ("cache_freshness", &[][..]),
        ("compile_coordinator", &["diagnostics"][..]),
        ("source", &["diagnostics"][..]),
        ("resolver", &["diagnostics", "source"][..]),
        ("compiler", &["resolver", "source"][..]),
        ("metrics", &["resolver", "source"][..]),
        ("metadata", &["metrics", "source"][..]),
        (
            "resolution_cache",
            &[
                "cache_freshness",
                "diagnostics",
                "metadata",
                "metrics",
                "resolver",
                "source",
            ][..],
        ),
        (
            "compiled_cache",
            &[
                "cache_freshness",
                "compile_coordinator",
                "compiler",
                "diagnostics",
                "metadata",
                "metrics",
                "resolver",
                "source",
            ][..],
        ),
        (
            "cache",
            &[
                "compiled_cache",
                "compiler",
                "metadata",
                "metrics",
                "resolution_cache",
                "resolver",
            ][..],
        ),
    ];

    for (index, (module, allowed_dependencies)) in modules.iter().enumerate() {
        let path = include_dir.join(format!("{module}.rs"));
        let source = fs::read_to_string(&path).expect("read include ownership source");
        assert!(
            !source.contains("use php_semantics") && !source.contains("use php_optimizer"),
            "{module} must not own frontend, lowering, or optimizer work"
        );
        assert!(
            !source.lines().any(|line| {
                let line = line.trim_start();
                line.starts_with("pub ")
                    && (line.contains("Mutex<")
                        || line.contains("RwLock<")
                        || line.contains("Condvar"))
            }),
            "{module} exposes lock implementation details"
        );

        let dependencies = source
            .lines()
            .filter_map(|line| line.trim().strip_prefix("use super::"))
            .map(|path| {
                path.split([':', '{', ';'])
                    .next()
                    .expect("module dependency")
            })
            .collect::<Vec<_>>();
        for dependency in dependencies {
            assert!(
                allowed_dependencies.contains(&dependency),
                "{module} has undeclared include-module dependency on {dependency}"
            );
            let dependency_index = modules
                .iter()
                .position(|(candidate, _)| *candidate == dependency)
                .expect("declared dependency is an include module");
            assert!(
                dependency_index < index,
                "{module} -> {dependency} violates one-way include ownership"
            );
        }
    }

    let facade = fs::read_to_string(include_dir.join("mod.rs")).expect("read include facade");
    assert!(
        facade.lines().count() <= 80,
        "include module facade must remain small"
    );
}

#[test]
fn include_path_fingerprint_identity_participates_in_equality() {
    let base = IncludePathFileFingerprint {
        len: 17,
        modified_unix_nanos: Some(10),
        changed_unix_nanos: Some(11),
        readonly: false,
        inode: Some(1),
        device: Some(2),
    };
    // An atomic replace can preserve len/mtime/readonly yet change the
    // inode; the resolution must then be treated as stale.
    let replaced = IncludePathFileFingerprint {
        inode: Some(9),
        ..base.clone()
    };
    assert_ne!(
        base, replaced,
        "inode must participate in fingerprint identity"
    );
    let moved = IncludePathFileFingerprint {
        device: Some(99),
        ..base.clone()
    };
    assert_ne!(
        base, moved,
        "device must participate in fingerprint identity"
    );
    assert_eq!(base, base.clone(), "identical identity is a cache hit");
}

#[cfg(unix)]
#[test]
fn include_path_fingerprint_captures_unix_identity() {
    let path = std::env::temp_dir().join(format!(
        "phrust_p2_identity_{}_{}.php",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::write(&path, b"<?php\n").unwrap();
    let fingerprint = include_path_file_fingerprint(&path);
    let _ = std::fs::remove_file(&path);
    let fingerprint = fingerprint.expect("fingerprint for a readable temp file");
    assert!(fingerprint.inode.is_some(), "unix exposes inode");
    assert!(fingerprint.device.is_some(), "unix exposes device");
    assert!(
        fingerprint.changed_unix_nanos.is_some(),
        "unix exposes ctime"
    );
    assert!(fingerprint.has_reliable_generation());
}

#[test]
fn missing_platform_identity_blocks_metadata_only_reuse() {
    let fingerprint = IncludePathFileFingerprint {
        len: 17,
        modified_unix_nanos: Some(10),
        changed_unix_nanos: None,
        readonly: false,
        inode: None,
        device: None,
    };

    assert!(!fingerprint.has_reliable_generation());
}

#[test]
fn include_loader_accepts_legacy_single_byte_support_files() {
    let fixture = IncludeCacheFixture::new("legacy-bytes");
    let path = fixture.root.join("legacy.inc");
    fs::write(&path, b"<?php\n// caf\xe9\n$value = 1;\n").expect("write legacy byte source");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let resolved = loader
        .resolve_with_include_path(None, "legacy.inc", &[], Some(&fixture.root))
        .expect("resolve include");

    let loaded = loader
        .load_resolved(resolved.canonical_path)
        .expect("load include");

    assert!(loaded.source.contains("café"), "{}", loaded.source);
    assert!(loaded.source.contains("$value = 1;"), "{}", loaded.source);
}

#[test]
fn include_cache_records_resolution_hits_and_misses() {
    let fixture = IncludeCacheFixture::new("resolution");
    fixture.write("lib.php", "<?php echo 'lib';\n");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);

    let first = cache
        .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
        .expect("first resolve");
    let second = cache
        .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
        .expect("second resolve");

    assert_eq!(first, second);
    assert_eq!(cache.cache_stats().resolution_misses, 1);
    assert_eq!(cache.cache_stats().resolution_hits, 1);
    // The revalidated hit also observed a stable parent-directory version.
    assert_eq!(cache.cache_stats().directory_version_hits, 1);
    assert_eq!(cache.cache_stats().directory_version_misses, 0);
    assert!(
        first.directory_version.is_some(),
        "resolutions capture the parent directory version"
    );
}

#[test]
fn negative_include_cache_replays_identical_diagnostics_and_invalidates_on_create() {
    let fixture = IncludeCacheFixture::new("negative-cache");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);

    let first = cache
        .resolve_with_include_path(&loader, None, "missing.php", &[], Some(&fixture.root))
        .expect_err("missing include fails");
    assert_eq!(first.code(), "E_PHP_VM_INCLUDE_MISSING");
    assert_eq!(cache.cache_stats().negative_cache_installs, 1);

    // Unchanged directories: the cached failure is replayed byte-for-byte
    // without re-probing candidates.
    let second = cache
        .resolve_with_include_path(&loader, None, "missing.php", &[], Some(&fixture.root))
        .expect_err("still missing");
    assert_eq!(first, second, "cached diagnostics are identical");
    assert_eq!(cache.cache_stats().negative_cache_hits, 1);

    // Creating the file changes the candidate directory's version, which
    // invalidates the entry and resolves for real.
    fixture.write("missing.php", "<?php echo 'now present';\n");
    let resolved = cache
        .resolve_with_include_path(&loader, None, "missing.php", &[], Some(&fixture.root))
        .expect("file now resolves");
    assert!(resolved.canonical_path.ends_with("missing.php"));
    assert_eq!(cache.cache_stats().negative_cache_invalidations, 1);
    assert_eq!(cache.cache_stats().negative_cache_hits, 1, "no stale hit");
}

#[test]
fn negative_include_cache_blocks_unversionable_candidates() {
    let fixture = IncludeCacheFixture::new("negative-blocked");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);

    // The candidate's parent directory does not exist, so a deeper chain
    // could appear without changing any observed version — not cacheable.
    let error = cache
        .resolve_with_include_path(
            &loader,
            None,
            "absent-dir/lib.php",
            &[],
            Some(&fixture.root),
        )
        .expect_err("missing include fails");
    assert_eq!(error.code(), "E_PHP_VM_INCLUDE_MISSING");
    assert_eq!(cache.cache_stats().negative_cache_installs, 0);
    assert_eq!(cache.cache_stats().negative_cache_blocked_unversioned, 1);

    // Every retry re-resolves; nothing was cached.
    let _ = cache
        .resolve_with_include_path(
            &loader,
            None,
            "absent-dir/lib.php",
            &[],
            Some(&fixture.root),
        )
        .expect_err("still missing");
    assert_eq!(cache.cache_stats().negative_cache_hits, 0);

    // A directory chain appearing later resolves normally.
    fixture.write("absent-dir/lib.php", "<?php\n");
    cache
        .resolve_with_include_path(
            &loader,
            None,
            "absent-dir/lib.php",
            &[],
            Some(&fixture.root),
        )
        .expect("file now resolves");
}

#[cfg(unix)]
#[test]
fn negative_include_cache_does_not_cache_permission_failures() {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = IncludeCacheFixture::new("negative-eacces");
    fixture.write("locked/lib.php", "<?php\n");
    let locked = fixture.root.join("locked");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);

    // Remove search permission so canonicalize fails with EACCES, not
    // NotFound. Skip if running as root (permission bits are ignored).
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");
    let blocked =
        cache.resolve_with_include_path(&loader, None, "locked/lib.php", &[], Some(&fixture.root));
    let permission_denied = blocked.is_err()
        && fs::metadata(locked.join("lib.php"))
            .err()
            .is_some_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("chmod restore");
    if !permission_denied {
        return; // running as root or platform ignores the mode bits
    }
    // A transient permission failure must not be cached: fixing perms
    // (which changes ctime, not the guarded dir mtime) resolves normally.
    assert_eq!(cache.cache_stats().negative_cache_installs, 0);
    cache
        .resolve_with_include_path(&loader, None, "locked/lib.php", &[], Some(&fixture.root))
        .expect("include resolves once permission is restored");
}

#[test]
fn negative_include_cache_clears_and_bounds_capacity() {
    let fixture = IncludeCacheFixture::new("negative-capacity");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);

    let _ = cache
        .resolve_with_include_path(&loader, None, "gone.php", &[], Some(&fixture.root))
        .expect_err("missing include fails");
    assert_eq!(cache.cache_stats().negative_cache_installs, 1);
    cache.clear().expect("clear");
    let _ = cache
        .resolve_with_include_path(&loader, None, "gone.php", &[], Some(&fixture.root))
        .expect_err("still missing after clear");
    assert_eq!(
        cache.cache_stats().negative_cache_hits,
        0,
        "clear() drops negative entries"
    );
    assert_eq!(cache.cache_stats().negative_cache_installs, 2);

    for index in 0..NEGATIVE_INCLUDE_CACHE_SHARD_CAPACITY {
        let _ = cache
            .resolve_with_include_path(
                &loader,
                None,
                &format!("gone-{index}.php"),
                &[],
                Some(&fixture.root),
            )
            .expect_err("missing include fails");
    }
    assert!(cache.cache_stats().negative_cache_blocked_capacity > 0);
}

#[test]
fn directory_version_observes_directories_only_and_is_stable() {
    let fixture = IncludeCacheFixture::new("dir-version");
    fixture.write("lib.php", "<?php echo 'lib';\n");
    let first = include_directory_version(&fixture.root).expect("directory version");
    let second = include_directory_version(&fixture.root).expect("directory version");
    assert_eq!(first, second, "unchanged directory has a stable version");
    assert_eq!(
        include_directory_version(&fixture.root.join("lib.php")),
        None,
        "files are not directories"
    );
    assert_eq!(
        include_directory_version(&fixture.root.join("missing")),
        None,
        "missing directories are unvalidated, never a match"
    );
}

#[test]
fn fnv1a_64_is_stable_across_processes() {
    // Standard FNV-1a test vectors; a persistent cache may key on these.
    assert_eq!(fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a_64(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv1a_64(b"foobar"), 0x85944171f73967e8);
}

#[test]
fn composer_map_fingerprint_detects_maps_and_walks_ancestors() {
    let fixture = IncludeCacheFixture::new("composer-map");
    assert_eq!(
        composer_autoload_map_fingerprint(&fixture.root),
        None,
        "no vendor/composer directory means unknown"
    );

    fixture.write(
        "vendor/composer/autoload_classmap.php",
        "<?php return [];\n",
    );
    let from_root = composer_autoload_map_fingerprint(&fixture.root).expect("map detected at root");
    assert!(from_root.starts_with("composer-map-v1:"), "{from_root}");

    // A front controller under public/ finds the same project root map.
    fixture.write("public/index.php", "<?php\n");
    let from_public = composer_autoload_map_fingerprint(&fixture.root.join("public"))
        .expect("map detected from public/");
    assert_eq!(from_root, from_public);

    // Rewriting a map file changes the fingerprint.
    fixture.write(
        "vendor/composer/autoload_classmap.php",
        "<?php return ['App\\\\A' => 'src/A.php'];\n",
    );
    let after_rewrite =
        composer_autoload_map_fingerprint(&fixture.root).expect("map still detected");
    assert_ne!(from_root, after_rewrite);
}

#[test]
fn composer_fingerprint_transitions_attribute_staleness() {
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    assert_eq!(
        cache.note_composer_fingerprint(Some("composer-map-v1:aa")),
        ComposerFingerprintTransition::Unchanged,
        "first observation is not stale"
    );
    assert_eq!(
        cache.note_composer_fingerprint(Some("composer-map-v1:aa")),
        ComposerFingerprintTransition::Unchanged
    );
    assert_eq!(
        cache.note_composer_fingerprint(Some("composer-map-v1:bb")),
        ComposerFingerprintTransition::Changed
    );
    assert_eq!(
        cache.note_composer_fingerprint(None),
        ComposerFingerprintTransition::Changed,
        "a map disappearing is a change"
    );
    assert_eq!(cache.cache_stats().composer_fingerprint_stale, 2);
}

#[test]
fn deployment_root_fingerprint_counts_present_missing_and_stale() {
    let fixture = IncludeCacheFixture::new("deployment-root");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);

    cache.set_deployment_root_fingerprint(DeploymentRootFingerprint::observe(
        &fixture.root.join("missing"),
        DeploymentRootMode::DevMutable,
    ));
    assert_eq!(cache.cache_stats().deployment_fingerprint_missing, 1);

    let observed =
        DeploymentRootFingerprint::observe(&fixture.root, DeploymentRootMode::ImmutableDeclared)
            .expect("observable root");
    assert_eq!(observed.mode, DeploymentRootMode::ImmutableDeclared);
    cache.set_deployment_root_fingerprint(Some(observed.clone()));
    assert_eq!(cache.cache_stats().deployment_fingerprint_present, 1);
    cache.revalidate_deployment_root();
    assert_eq!(
        cache.cache_stats().deployment_fingerprint_stale,
        0,
        "unchanged root is not stale"
    );

    // A stored version that no longer matches attributes staleness. Use a
    // synthetic mismatch so the test does not depend on filesystem mtime
    // granularity.
    cache.set_deployment_root_fingerprint(Some(DeploymentRootFingerprint {
        directory_version: Some(IncludeDirectoryVersion {
            modified_unix_nanos: Some(1),
            inode: Some(1),
            device: Some(1),
        }),
        ..observed
    }));
    cache.revalidate_deployment_root();
    assert_eq!(cache.cache_stats().deployment_fingerprint_stale, 1);
}

#[test]
fn include_cache_invalidates_compiled_include_after_file_edit() {
    let fixture = IncludeCacheFixture::new("compiled-stale");
    fixture.write("lib.php", "<?php echo 'one';\n");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);

    let first_resolved = cache
        .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
        .expect("first resolve");
    let first = cache
        .get_or_compile_include(
            &loader,
            &first_resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("first compile");
    fixture.write("lib.php", "<?php echo 'two'; echo '!';\n");
    let second_resolved = cache
        .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
        .expect("second resolve");
    let second = cache
        .get_or_compile_include(
            &loader,
            &second_resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("second compile");

    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(cache.cache_stats().compile_misses, 2);
    assert!(cache.cache_stats().stale_invalidations >= 1);
}

#[cfg(unix)]
#[test]
fn include_cache_rejects_same_metadata_atomic_replacement() {
    let fixture = IncludeCacheFixture::new("compiled-atomic-replace");
    let path = fixture.root.join("lib.php");
    fixture.write(
        "lib.php",
        "<?php class CachedPrimary { public $first = null; }\n",
    );
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    let resolved = cache
        .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
        .expect("resolve include");
    let first = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("first compile");

    replace_preserving_metadata(
        &path,
        "<?php class CachedPrimary { public $other = null; }\n",
    );
    let resolved_after_replace = cache
        .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
        .expect("resolve replacement");
    let second = cache
        .get_or_compile_include(
            &loader,
            &resolved_after_replace,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("compile replacement");

    assert!(!Arc::ptr_eq(&first, &second));
    assert!(class_has_property(&second, "cachedprimary", "other"));
    assert!(!class_has_property(&second, "cachedprimary", "first"));
}

#[test]
fn mutable_include_cache_rejects_same_metadata_in_place_rewrite() {
    let fixture = IncludeCacheFixture::new("compiled-in-place-rewrite");
    let path = fixture.root.join("lib.php");
    fixture.write(
        "lib.php",
        "<?php class CachedMutable { public $first = null; }\n",
    );
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    let resolved = cache
        .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
        .expect("resolve include");
    let first = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("first compile");

    rewrite_preserving_metadata(
        &path,
        "<?php class CachedMutable { public $other = null; }\n",
    );
    let second = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("compile rewritten source");

    assert!(!Arc::ptr_eq(&first, &second));
    assert!(class_has_property(&second, "cachedmutable", "other"));
    assert!(!class_has_property(&second, "cachedmutable", "first"));
}

#[test]
fn immutable_release_trusts_cache_until_explicit_clear() {
    let fixture = IncludeCacheFixture::new("compiled-immutable");
    fixture.write("lib.php", "<?php echo 'one';\n");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    cache.set_deployment_root_fingerprint(DeploymentRootFingerprint::observe(
        &fixture.root,
        DeploymentRootMode::ImmutableDeclared,
    ));

    let first_resolved = cache
        .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
        .expect("first resolve");
    let first = cache
        .get_or_compile_include(
            &loader,
            &first_resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("first compile");
    fixture.write("lib.php", "<?php echo 'changed and longer';\n");
    let trusted_resolved = cache
        .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
        .expect("trusted resolve");
    let trusted = cache
        .get_or_compile_include(
            &loader,
            &trusted_resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("trusted compile lookup");
    assert!(Arc::ptr_eq(&first, &trusted));
    assert!(cache.cache_stats().immutable_release_hits >= 2);

    cache.clear().expect("clear immutable cache");
    let fresh_resolved = cache
        .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
        .expect("fresh resolve after clear");
    let fresh = cache
        .get_or_compile_include(
            &loader,
            &fresh_resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("fresh compile after clear");
    assert!(!Arc::ptr_eq(&first, &fresh));
}

#[test]
fn include_cache_keys_compiled_units_by_optimization_level() {
    let fixture = IncludeCacheFixture::new("compiled-optimization");
    fixture.write("lib.php", "<?php echo 1 + 2;\n");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    let resolved = loader
        .resolve_with_include_path(None, "lib.php", &[], Some(&fixture.root))
        .expect("resolve include");

    let baseline = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("baseline include compile");
    let optimized = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O2),
        )
        .expect("optimized include compile");
    let stats = cache.cache_stats();

    assert_eq!(stats.compile_misses, 2);
    assert_eq!(stats.compile_hits, 0);
    assert!(binary_add_count(&baseline) > 0);
    assert_eq!(binary_add_count(&optimized), 0);
}

#[test]
fn compiler_fingerprint_is_opaque_and_includes_dependencies() {
    let fixture = IncludeCacheFixture::new("compiler-fingerprint");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let mapped_loader = loader
        .clone()
        .with_compilation_dependency("Demo\\Shared", "shared.php");
    let baseline = TestIncludeCompiler::new(OptimizationLevel::O0).fingerprint(&loader);
    let optimized = TestIncludeCompiler::new(OptimizationLevel::O2).fingerprint(&loader);
    let different_dependencies =
        TestIncludeCompiler::new(OptimizationLevel::O0).fingerprint(&mapped_loader);

    assert_ne!(baseline, optimized);
    assert_ne!(baseline, different_dependencies);
    assert_ne!(
        baseline,
        IncludeCompilerFingerprint::new("different compiler")
    );
}

#[test]
fn mutable_compiled_include_cache_validates_content_on_hit() {
    let fixture = IncludeCacheFixture::new("compiled-hit-content-validation");
    fixture.write("lib.php", "<?php echo 'cached';\n");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    let resolved = cache
        .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
        .expect("resolve include");

    let first = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("first compile");
    let source_reads_after_first = cache.cache_stats().source_reads;
    let second = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("second lookup");
    let stats = cache.cache_stats();

    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(source_reads_after_first, 1);
    assert_eq!(stats.source_reads, 2);
    assert_eq!(stats.content_validations, 2);
    assert!(stats.source_bytes_hashed > 0);
    assert_eq!(stats.identity_only_hits, 0);
    assert_eq!(stats.compile_misses, 1);
    assert_eq!(stats.compile_hits, 1);
}

#[test]
fn immutable_compiled_include_cache_uses_guarded_identity_hit() {
    let fixture = IncludeCacheFixture::new("compiled-hit-immutable-identity");
    fixture.write("lib.php", "<?php echo 'cached';\n");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    cache.set_deployment_root_fingerprint(DeploymentRootFingerprint::observe(
        &fixture.root,
        DeploymentRootMode::ImmutableDeclared,
    ));
    let resolved = cache
        .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
        .expect("resolve include");

    let first = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("first compile");
    let second = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("second lookup");
    let stats = cache.cache_stats();

    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(stats.source_reads, 1);
    assert_eq!(stats.content_validations, 1);
    assert_eq!(stats.identity_only_hits, 1);
    assert_eq!(stats.compile_misses, 1);
    assert_eq!(stats.compile_hits, 1);
}

#[test]
fn concurrent_include_miss_compiles_once() {
    const THREADS: usize = 8;

    let fixture = IncludeCacheFixture::new("compiled-stampede");
    fixture.write("lib.php", "<?php class StampedeTarget {}\n");
    let loader = Arc::new(IncludeLoader::for_root(&fixture.root).expect("loader"));
    let cache = Arc::new(IncludeCache::new_with_revalidation_interval(
        4,
        Duration::ZERO,
    ));
    let resolved = Arc::new(
        cache
            .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
            .expect("resolve include"),
    );
    let barrier = Arc::new(Barrier::new(THREADS));
    let handles = (0..THREADS)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let cache = Arc::clone(&cache);
            let loader = Arc::clone(&loader);
            let resolved = Arc::clone(&resolved);
            std::thread::spawn(move || {
                barrier.wait();
                cache
                    .get_or_compile_include(
                        &loader,
                        &resolved,
                        &TestIncludeCompiler::new(OptimizationLevel::O0),
                    )
                    .expect("concurrent compile")
            })
        })
        .collect::<Vec<_>>();
    let compiled = handles
        .into_iter()
        .map(|handle| handle.join().expect("compile thread"))
        .collect::<Vec<_>>();

    assert!(
        compiled.iter().all(|unit| Arc::ptr_eq(&compiled[0], unit)),
        "every waiter receives the one installed compiled unit"
    );
    let stats = cache.cache_stats();
    assert_eq!(stats.compile_misses, 1);
    assert_eq!(stats.compile_hits, (THREADS - 1) as u64);
    assert!(stats.source_reads >= THREADS as u64);
    assert!(stats.source_reads <= (THREADS * 2 - 1) as u64);
    assert_eq!(stats.content_validations, stats.source_reads);
}

#[test]
fn compiled_include_resolves_explicit_trait_dependency() {
    let fixture = IncludeCacheFixture::new("local-psr-trait");
    fixture.write(
            "src/Providers/ProviderRegistry.php",
            "<?php\nnamespace Demo\\Providers;\nuse Demo\\Providers\\Http\\Traits\\WithHttpTransporterTrait;\nclass ProviderRegistry {\n    use WithHttpTransporterTrait { setHttpTransporter as setHttpTransporterOriginal; }\n}\n",
        );
    fixture.write(
            "src/Providers/Http/Traits/WithHttpTransporterTrait.php",
            "<?php\nnamespace Demo\\Providers\\Http\\Traits;\ntrait WithHttpTransporterTrait {\n    private $httpTransporter = null;\n    public function setHttpTransporter($value): void { $this->httpTransporter = $value; }\n}\n",
        );
    let loader = IncludeLoader::for_root(&fixture.root)
        .expect("loader")
        .with_compilation_dependency(
            "Demo\\Providers\\Http\\Traits\\WithHttpTransporterTrait",
            "src/Providers/Http/Traits/WithHttpTransporterTrait.php",
        );
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    let resolved = loader
        .resolve_with_include_path(
            None,
            "src/Providers/ProviderRegistry.php",
            &[],
            Some(&fixture.root),
        )
        .expect("resolve include");

    let compiled = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("compile provider registry");
    let class = compiled
        .unit()
        .classes
        .iter()
        .find(|class| class.name == "demo\\providers\\providerregistry")
        .expect("provider registry class");

    assert!(
        class
            .properties
            .iter()
            .any(|property| property.name == "httpTransporter")
    );
    assert!(
        class
            .methods
            .iter()
            .any(|method| method.name == "sethttptransporteroriginal")
    );
    let method = class
        .methods
        .iter()
        .find(|method| method.name == "sethttptransporteroriginal")
        .expect("aliased method");
    assert_eq!(
        compiled.unit().functions[method.function.index()]
            .span
            .file
            .index(),
        1,
        "dependency method diagnostics retain the dependency file"
    );
}

#[test]
fn explicit_trait_dependency_must_declare_the_requested_trait() {
    let fixture = IncludeCacheFixture::new("mapped-trait-mismatch");
    fixture.write(
        "src/Registry.php",
        "<?php namespace Demo; use Shared\\ExpectedTrait; class Registry { use ExpectedTrait; }",
    );
    fixture.write(
        "src/WrongTrait.php",
        "<?php namespace Shared; trait WrongTrait {}",
    );
    let loader = IncludeLoader::for_root(&fixture.root)
        .expect("loader")
        .with_compilation_dependency("Shared\\ExpectedTrait", "src/WrongTrait.php");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    let resolved = loader
        .resolve_with_include_path(None, "src/Registry.php", &[], Some(&fixture.root))
        .expect("resolve include");

    let error = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect_err("a mismatched declaration mapping must fail closed");

    assert_eq!(error.code(), "E_PHP_VM_INCLUDE_DEPENDENCY_MISMATCH");
    assert_eq!(
        error.context().get("declaration").map(String::as_str),
        Some("shared\\expectedtrait")
    );
}

#[test]
fn compilation_dependency_mapping_participates_in_cache_identity() {
    let fixture = IncludeCacheFixture::new("mapped-trait-cache-identity");
    fixture.write(
        "src/Registry.php",
        "<?php namespace Demo; use Shared\\SelectedTrait; class Registry { use SelectedTrait; }",
    );
    fixture.write(
        "src/FirstTrait.php",
        "<?php namespace Shared; trait SelectedTrait { private $first = null; }",
    );
    fixture.write(
        "src/SecondTrait.php",
        "<?php namespace Shared; trait SelectedTrait { private $second = null; }",
    );
    let first_loader = IncludeLoader::for_root(&fixture.root)
        .expect("first loader")
        .with_compilation_dependency("Shared\\SelectedTrait", "src/FirstTrait.php");
    let second_loader = IncludeLoader::for_root(&fixture.root)
        .expect("second loader")
        .with_compilation_dependency("Shared\\SelectedTrait", "src/SecondTrait.php");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    let resolved = first_loader
        .resolve_with_include_path(None, "src/Registry.php", &[], Some(&fixture.root))
        .expect("resolve include");

    let first = cache
        .get_or_compile_include(
            &first_loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("compile first mapping");
    let second = cache
        .get_or_compile_include(
            &second_loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("compile second mapping");

    assert!(!Arc::ptr_eq(&first, &second));
    assert!(class_has_property(&first, "demo\\registry", "first"));
    assert!(class_has_property(&second, "demo\\registry", "second"));
    assert_eq!(cache.cache_stats().compile_misses, 2);
}

#[test]
fn compiled_include_resolves_nested_trait_dependencies() {
    let fixture = IncludeCacheFixture::new("nested-trait-session");
    fixture.write(
            "src/Registry.php",
            "<?php\nnamespace Demo;\nuse Demo\\Traits\\OuterTrait;\nclass Registry { use OuterTrait; }\n",
        );
    fixture.write(
            "src/Traits/OuterTrait.php",
            "<?php\nnamespace Demo\\Traits;\nuse Demo\\Traits\\InnerTrait;\ntrait OuterTrait { use InnerTrait; public function outerMethod(): void {} }\n",
        );
    fixture.write(
            "src/Traits/InnerTrait.php",
            "<?php\nnamespace Demo\\Traits;\ntrait InnerTrait { private $inner = null; public function innerMethod(): void {} }\n",
        );
    let loader = IncludeLoader::for_root(&fixture.root)
        .expect("loader")
        .with_compilation_dependency("Demo\\Traits\\OuterTrait", "src/Traits/OuterTrait.php")
        .with_compilation_dependency("Demo\\Traits\\InnerTrait", "src/Traits/InnerTrait.php");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    let resolved = loader
        .resolve_with_include_path(None, "src/Registry.php", &[], Some(&fixture.root))
        .expect("resolve include");

    let compiled = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("compile nested traits");
    let class = compiled
        .unit()
        .classes
        .iter()
        .find(|class| class.name == "demo\\registry")
        .expect("registry class");
    assert!(
        class
            .methods
            .iter()
            .any(|method| method.name == "innermethod")
    );
    assert!(
        class
            .methods
            .iter()
            .any(|method| method.name == "outermethod")
    );
    assert!(
        class
            .properties
            .iter()
            .any(|property| property.name == "inner")
    );
    assert_eq!(compiled.unit().files.len(), 3);
}

#[test]
fn compiled_include_reports_missing_trait_without_retrying() {
    let fixture = IncludeCacheFixture::new("missing-trait-session");
    fixture.write(
        "src/Registry.php",
        "<?php namespace Demo; use Missing\\AbsentTrait; class Registry { use AbsentTrait; }",
    );
    fixture.write(
        "src/AbsentTrait.php",
        "<?php namespace Missing; trait AbsentTrait {}",
    );
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    let resolved = loader
        .resolve_with_include_path(None, "src/Registry.php", &[], Some(&fixture.root))
        .expect("resolve include");

    let error = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect_err("an unmapped sibling trait must not be discovered by scanning");
    assert_eq!(error.code(), "E_PHP_VM_INCLUDE_COMPILE_ERROR");
    assert!(error.render_message().contains("E_PHP_IR_TRAIT_NOT_FOUND"));
}

#[test]
fn compiled_include_rejects_duplicate_resolved_traits_deterministically() {
    let fixture = IncludeCacheFixture::new("duplicate-trait-session");
    fixture.write(
            "src/Registry.php",
            "<?php namespace Demo; use Shared\\FirstTrait; use Shared\\SecondTrait; class Registry { use FirstTrait; use SecondTrait; }",
        );
    fixture.write(
        "a/FirstTrait.php",
        "<?php namespace Shared; trait FirstTrait {} trait DuplicateTrait {}",
    );
    fixture.write(
        "b/SecondTrait.php",
        "<?php namespace Shared; trait SecondTrait {} trait DuplicateTrait {}",
    );
    let loader = IncludeLoader::for_root(&fixture.root)
        .expect("loader")
        .with_compilation_dependency("Shared\\FirstTrait", "a/FirstTrait.php")
        .with_compilation_dependency("Shared\\SecondTrait", "b/SecondTrait.php");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    let resolved = loader
        .resolve_with_include_path(None, "src/Registry.php", &[], Some(&fixture.root))
        .expect("resolve include");

    let error = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect_err("duplicate trait must fail");
    assert_eq!(error.code(), "E_PHP_VM_INCLUDE_DUPLICATE_DECLARATION");
    assert!(error.render_message().contains("shared\\duplicatetrait"));
}

#[test]
fn compiled_include_rejects_dependency_cycles_deterministically() {
    let fixture = IncludeCacheFixture::new("trait-cycle-session");
    fixture.write(
        "src/A.php",
        "<?php namespace Demo; use Demo\\B; trait A { use B; }",
    );
    fixture.write(
        "src/B.php",
        "<?php namespace Demo; use Demo\\A; trait B { use A; }",
    );
    let loader = IncludeLoader::for_root(&fixture.root)
        .expect("loader")
        .with_compilation_dependency("Demo\\B", "src/B.php");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    let resolved = loader
        .resolve_with_include_path(None, "src/A.php", &[], Some(&fixture.root))
        .expect("resolve include");

    let first = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect_err("dependency cycle must fail");
    let second = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect_err("dependency cycle must remain deterministic");
    assert_eq!(first.code(), "E_PHP_VM_INCLUDE_DEPENDENCY_CYCLE");
    assert_eq!(first, second);
    assert!(first.render_message().contains("A.php"));
    assert!(first.render_message().contains("B.php"));
}

#[test]
fn compiled_include_cache_invalidates_after_explicit_trait_edit() {
    let fixture = IncludeCacheFixture::new("local-psr-trait-stale");
    fixture.write(
            "src/Providers/ProviderRegistry.php",
            "<?php\nnamespace Demo\\Providers;\nuse Demo\\Providers\\Http\\Traits\\WithHttpTransporterTrait;\nclass ProviderRegistry { use WithHttpTransporterTrait; }\n",
        );
    fixture.write(
            "src/Providers/Http/Traits/WithHttpTransporterTrait.php",
            "<?php\nnamespace Demo\\Providers\\Http\\Traits;\ntrait WithHttpTransporterTrait { private $first = null; }\n",
        );
    let loader = IncludeLoader::for_root(&fixture.root)
        .expect("loader")
        .with_compilation_dependency(
            "Demo\\Providers\\Http\\Traits\\WithHttpTransporterTrait",
            "src/Providers/Http/Traits/WithHttpTransporterTrait.php",
        );
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    let resolved = loader
        .resolve_with_include_path(
            None,
            "src/Providers/ProviderRegistry.php",
            &[],
            Some(&fixture.root),
        )
        .expect("resolve include");
    let first = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("first compile");

    std::thread::sleep(std::time::Duration::from_millis(2));
    fixture.write(
            "src/Providers/Http/Traits/WithHttpTransporterTrait.php",
            "<?php\nnamespace Demo\\Providers\\Http\\Traits;\ntrait WithHttpTransporterTrait { private $second = null; }\n",
        );
    let second = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("second compile");

    assert!(!Arc::ptr_eq(&first, &second));
    let stats = cache.cache_stats();
    assert_eq!(
        stats.source_reads, 5,
        "primary and trait bytes are counted for validation and recompilation"
    );
    assert!(stats.dependency_metadata_validations > 0);
    assert!(stats.stale_dependency_invalidations > 0);
    let class = second
        .unit()
        .classes
        .iter()
        .find(|class| class.name == "demo\\providers\\providerregistry")
        .expect("provider registry class");
    assert!(
        class
            .properties
            .iter()
            .any(|property| property.name == "second")
    );
}

#[cfg(unix)]
#[test]
fn compiled_include_cache_rejects_same_metadata_trait_replacement() {
    let fixture = IncludeCacheFixture::new("local-psr-trait-atomic-replace");
    fixture.write(
            "src/Providers/ProviderRegistry.php",
            "<?php\nnamespace Demo\\Providers;\nuse Demo\\Providers\\Http\\Traits\\WithHttpTransporterTrait;\nclass ProviderRegistry { use WithHttpTransporterTrait; }\n",
        );
    let trait_path = fixture
        .root
        .join("src/Providers/Http/Traits/WithHttpTransporterTrait.php");
    fixture.write(
            "src/Providers/Http/Traits/WithHttpTransporterTrait.php",
            "<?php\nnamespace Demo\\Providers\\Http\\Traits;\ntrait WithHttpTransporterTrait { private $first = null; }\n",
        );
    let loader = IncludeLoader::for_root(&fixture.root)
        .expect("loader")
        .with_compilation_dependency(
            "Demo\\Providers\\Http\\Traits\\WithHttpTransporterTrait",
            "src/Providers/Http/Traits/WithHttpTransporterTrait.php",
        );
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    let resolved = loader
        .resolve_with_include_path(
            None,
            "src/Providers/ProviderRegistry.php",
            &[],
            Some(&fixture.root),
        )
        .expect("resolve include");
    let first = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("first compile");

    replace_preserving_metadata(
        &trait_path,
        "<?php\nnamespace Demo\\Providers\\Http\\Traits;\ntrait WithHttpTransporterTrait { private $other = null; }\n",
    );
    let second = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("compile replacement dependency");

    assert!(!Arc::ptr_eq(&first, &second));
    assert!(class_has_property(
        &second,
        "demo\\providers\\providerregistry",
        "other"
    ));
    assert!(!class_has_property(
        &second,
        "demo\\providers\\providerregistry",
        "first"
    ));
}

#[cfg(unix)]
#[test]
fn include_cache_rejects_symlink_target_swap() {
    use std::os::unix::fs::symlink;

    let fixture = IncludeCacheFixture::new("compiled-symlink-swap");
    fixture.write(
        "first.php",
        "<?php class CachedSymlink { public $first = null; }\n",
    );
    fixture.write(
        "other.php",
        "<?php class CachedSymlink { public $other = null; }\n",
    );
    let link = fixture.root.join("lib.php");
    symlink(fixture.root.join("first.php"), &link).expect("create first symlink");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    let first_resolved = cache
        .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
        .expect("resolve first target");
    let first = cache
        .get_or_compile_include(
            &loader,
            &first_resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("compile first target");

    fs::remove_file(&link).expect("remove first symlink");
    symlink(fixture.root.join("other.php"), &link).expect("create replacement symlink");
    let second_resolved = cache
        .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
        .expect("resolve replacement target");
    let second = cache
        .get_or_compile_include(
            &loader,
            &second_resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect("compile replacement target");

    assert!(!Arc::ptr_eq(&first, &second));
    assert!(class_has_property(&second, "cachedsymlink", "other"));
}

#[test]
fn include_path_dot_entry_resolves_to_runtime_cwd() {
    let fixture = IncludeCacheFixture::new("include-path-dot");
    let script_dir = fixture.root.join("script");
    let cwd = fixture.root.join("cwd");
    fs::create_dir_all(&script_dir).expect("create script dir");
    fs::create_dir_all(&cwd).expect("create cwd");
    fs::write(script_dir.join("dep.php"), "<?php echo 'script';\n").expect("write script dep");
    fs::write(cwd.join("dep.php"), "<?php echo 'cwd';\n").expect("write cwd dep");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");

    let resolved = loader
        .resolve_with_include_path(
            Some(&script_dir.join("index.php")),
            "dep.php",
            &[PathBuf::from(".")],
            Some(&cwd),
        )
        .expect("resolve include");

    assert_eq!(
        resolved.canonical_path,
        cwd.join("dep.php")
            .canonicalize()
            .expect("canonical cwd dep")
    );
}

#[test]
fn explicit_relative_include_ignores_include_path() {
    let fixture = IncludeCacheFixture::new("explicit-relative");
    let script_dir = fixture.root.join("script");
    let include_path = fixture.root.join("include-path");
    let cwd = fixture.root.join("cwd");
    fs::create_dir_all(&script_dir).expect("create script dir");
    fs::create_dir_all(&include_path).expect("create include_path dir");
    fs::create_dir_all(&cwd).expect("create cwd");
    fs::write(script_dir.join("dep.php"), "<?php echo 'script';\n").expect("write script dep");
    fs::write(include_path.join("dep.php"), "<?php echo 'include-path';\n")
        .expect("write include_path dep");
    fs::write(cwd.join("dep.php"), "<?php echo 'cwd';\n").expect("write cwd dep");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");

    let resolved = loader
        .resolve_with_include_path(
            Some(&script_dir.join("index.php")),
            "./dep.php",
            &[include_path],
            Some(&cwd),
        )
        .expect("resolve include");

    assert_eq!(
        resolved.canonical_path,
        cwd.join("dep.php")
            .canonicalize()
            .expect("canonical cwd dep")
    );
}

#[test]
fn bare_relative_fallback_uses_cwd_before_including_file_directory() {
    let fixture = IncludeCacheFixture::new("bare-fallback");
    let script_dir = fixture.root.join("script");
    let cwd = fixture.root.join("cwd");
    fs::create_dir_all(&script_dir).expect("create script dir");
    fs::create_dir_all(&cwd).expect("create cwd");
    fs::write(script_dir.join("dep.php"), "<?php echo 'script';\n").expect("write script dep");
    fs::write(cwd.join("dep.php"), "<?php echo 'cwd';\n").expect("write cwd dep");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");

    let resolved = loader
        .resolve_with_include_path(
            Some(&script_dir.join("nested").join("index.php")),
            "dep.php",
            &[],
            Some(&cwd),
        )
        .expect("resolve include");

    assert_eq!(
        resolved.canonical_path,
        cwd.join("dep.php")
            .canonicalize()
            .expect("canonical cwd dep")
    );
}

#[test]
fn include_loader_rejects_paths_outside_allowed_roots() {
    let fixture = IncludeCacheFixture::new("outside-root");
    let outside_root = fixture.root.with_file_name(format!(
        "{}-outside",
        fixture
            .root
            .file_name()
            .expect("fixture root name")
            .to_string_lossy()
    ));
    let outside_file = outside_root.join("dep.php");
    fs::create_dir_all(&outside_root).expect("create outside root");
    fs::write(&outside_file, "<?php echo 'outside';\n").expect("write outside file");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");

    let error = loader
        .resolve_with_include_path(
            None,
            &outside_file.to_string_lossy(),
            &[],
            Some(&fixture.root),
        )
        .expect_err("outside-root include should fail");

    assert_eq!(error.code(), "E_PHP_VM_INCLUDE_OUTSIDE_ROOT");
    let _ = fs::remove_dir_all(outside_root);
}

#[test]
fn include_failure_has_shared_envelope_context() {
    let fixture = IncludeCacheFixture::new("include-diagnostic");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let error = loader
        .resolve_with_include_path(None, "missing.php", &[], Some(&fixture.root))
        .expect_err("missing include");

    let envelope = loader.include_failure_diagnostic(
        &error,
        "missing.php",
        None,
        &[],
        Some(&fixture.root),
        true,
    );
    let json: serde_json::Value =
        serde_json::from_str(&envelope.compact_json().expect("json")).expect("parse json");

    assert_eq!(json["code"], "E_PHP_VM_INCLUDE_MISSING");
    assert_eq!(json["layer"], "vm");
    assert_eq!(json["phase"], "include");
    assert_eq!(json["context"]["path"], "missing.php");
    assert_eq!(json["context"]["cache_used"], "true");
    assert!(
        json["context"]["allowed_roots"]
            .as_str()
            .unwrap()
            .contains("include-diagnostic")
    );
    assert_eq!(json["php_visible"], true);
}

#[test]
fn include_loader_resolution_order_and_allowed_roots_are_explicit() {
    let fixture = IncludeCacheFixture::new("resolution-order");
    fs::create_dir_all(fixture.root.join("caller")).expect("caller dir");
    fs::create_dir_all(fixture.root.join("lib")).expect("lib dir");
    fs::create_dir_all(fixture.root.join("cwd")).expect("cwd dir");
    fs::write(
        fixture.root.join("caller/shared.php"),
        "<?php echo 'caller';\n",
    )
    .expect("caller include");
    fs::write(
        fixture.root.join("lib/shared.php"),
        "<?php echo 'include-path';\n",
    )
    .expect("include-path include");
    fs::write(fixture.root.join("cwd/cwd-only.php"), "<?php echo 'cwd';\n").expect("cwd include");
    fixture.write("absolute.php", "<?php echo 'absolute';\n");
    let outside = std::env::temp_dir().join(format!(
        "phrust-include-outside-{}-{}.php",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::write(&outside, "<?php echo 'outside';\n").expect("outside include");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let including_file = fixture.root.join("caller/index.php");
    let cwd = fixture.root.join("cwd");

    let include_path_first = loader
        .resolve_with_include_path(
            Some(&including_file),
            "shared.php",
            &[fixture.root.join("lib")],
            Some(&cwd),
        )
        .expect("include_path resolution");
    let including_file_dir = loader
        .resolve_with_include_path(
            Some(&including_file),
            "shared.php",
            &[PathBuf::from(".")],
            Some(&cwd),
        )
        .expect("including-file resolution");
    let cwd_fallback = loader
        .resolve_with_include_path(Some(&including_file), "cwd-only.php", &[], Some(&cwd))
        .expect("cwd fallback resolution");
    let absolute = loader
        .resolve_with_include_path(
            Some(&including_file),
            &fixture.root.join("absolute.php").to_string_lossy(),
            &[],
            Some(&cwd),
        )
        .expect("absolute resolution");
    let outside_root = loader
        .resolve_with_include_path(
            Some(&including_file),
            &outside.to_string_lossy(),
            &[],
            Some(&cwd),
        )
        .expect_err("outside root rejected");
    let _ = fs::remove_file(&outside);

    assert_eq!(
        include_path_first.canonical_path,
        fs::canonicalize(fixture.root.join("lib/shared.php")).expect("canonical lib")
    );
    assert_eq!(
        including_file_dir.canonical_path,
        fs::canonicalize(fixture.root.join("caller/shared.php")).expect("canonical caller")
    );
    assert_eq!(
        cwd_fallback.canonical_path,
        fs::canonicalize(fixture.root.join("cwd/cwd-only.php")).expect("canonical cwd")
    );
    assert_eq!(
        absolute.canonical_path,
        fs::canonicalize(fixture.root.join("absolute.php")).expect("canonical absolute")
    );
    assert_eq!(outside_root.code(), "E_PHP_VM_INCLUDE_OUTSIDE_ROOT");
}

#[test]
fn include_loader_reads_phar_entries_under_allowed_roots() {
    let fixture = IncludeCacheFixture::new("phar");
    let archive = fixture.root.join("fixture.phar");
    fs::write(&archive, fixture_phar()).expect("write phar fixture");
    let archive = archive.canonicalize().expect("canonical archive");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let uri = format!("phar://{}/lib/hello.php", archive.to_string_lossy());

    let resolved = loader
        .resolve_with_include_path(None, &uri, &[], Some(&fixture.root))
        .expect("resolve phar include");
    assert!(
        resolved
            .canonical_path
            .to_string_lossy()
            .starts_with("phar://")
    );
    let loaded = loader
        .load_resolved(resolved.canonical_path)
        .expect("load phar include");

    assert_eq!(
        loaded.source,
        "<?php echo 'from-phar|';\nreturn 'include-ok';\n"
    );
}

#[test]
fn poisoned_resolution_cache_returns_typed_error() {
    let fixture = IncludeCacheFixture::new("poison-resolution");
    fixture.write("lib.php", "<?php echo 'lib';\n");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    poison_rwlock(&cache.resolution.shards[0]);

    let error = cache
        .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
        .expect_err("poisoned resolution lock should return an error");

    assert_eq!(error.code(), "E_PHP_VM_INCLUDE_CACHE_POISONED");
    assert_eq!(
        error.context().get("cache").map(String::as_str),
        Some("resolution")
    );
}

#[test]
fn poisoned_compiled_cache_returns_typed_error() {
    let fixture = IncludeCacheFixture::new("poison-compiled");
    fixture.write("lib.php", "<?php echo 'lib';\n");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    let resolved = loader
        .resolve_with_include_path(None, "lib.php", &[], Some(&fixture.root))
        .expect("resolve include");
    poison_rwlock(&cache.compiled.shards[0]);

    let error = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect_err("poisoned compile lock should return an error");

    assert_eq!(error.code(), "E_PHP_VM_INCLUDE_CACHE_POISONED");
    assert_eq!(
        error.context().get("cache").map(String::as_str),
        Some("compiled")
    );
}

#[test]
fn poisoned_compile_lock_returns_typed_error() {
    let fixture = IncludeCacheFixture::new("poison-compile-lock");
    fixture.write("lib.php", "<?php echo 'lib';\n");
    let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
    let cache = IncludeCache::new_with_revalidation_interval(1, Duration::ZERO);
    let resolved = loader
        .resolve_with_include_path(None, "lib.php", &[], Some(&fixture.root))
        .expect("resolve include");
    poison_mutex(&cache.compiled.compile_coordinator.shards[0].in_progress);

    let error = cache
        .get_or_compile_include(
            &loader,
            &resolved,
            &TestIncludeCompiler::new(OptimizationLevel::O0),
        )
        .expect_err("poisoned compile coordination lock should return an error");

    assert_eq!(error.code(), "E_PHP_VM_INCLUDE_CACHE_POISONED");
    assert_eq!(
        error.context().get("cache").map(String::as_str),
        Some("compile-lock")
    );
}

fn fixture_phar() -> Vec<u8> {
    hex_decode(
        "3c3f706870205f5f48414c545f434f4d50494c455228293b203f3e0a6b000000020000001101000000000c000000666978747572652e70686172000000000d0000006c69622f68656c6c6f2e7068702e000000800092652e00000000000000000000000000000008000000646174612e7478740700000080009265070000000000000000000000000000003c3f706870206563686f202766726f6d2d706861727c273b0a72657475726e2027696e636c7564652d6f6b273b0a7061796c6f6164",
    )
}

fn hex_decode(input: &str) -> Vec<u8> {
    input
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_value(pair[0]);
            let low = hex_value(pair[1]);
            high << 4 | low
        })
        .collect()
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => panic!("invalid hex byte"),
    }
}

struct IncludeCacheFixture {
    root: PathBuf,
}

impl IncludeCacheFixture {
    fn new(name: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "phrust-include-cache-{}-{name}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create include cache fixture");
        Self { root }
    }

    fn write(&self, name: &str, source: &str) {
        let path = self.root.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create include cache fixture directory");
        }
        fs::write(path, source).expect("write include cache fixture file");
    }
}

impl Drop for IncludeCacheFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn rewrite_preserving_metadata(path: &Path, replacement: &str) {
    let before = fs::metadata(path).expect("metadata before rewrite");
    assert_eq!(
        before.len(),
        replacement.len() as u64,
        "same-length fixture"
    );
    fs::write(path, replacement).expect("rewrite fixture");
    fs::set_permissions(path, before.permissions()).expect("restore permissions");
    restore_file_times(path, &before);
    let after = fs::metadata(path).expect("metadata after rewrite");
    assert_eq!(after.len(), before.len());
    assert_eq!(after.modified().ok(), before.modified().ok());
    assert_eq!(
        after.permissions().readonly(),
        before.permissions().readonly()
    );
}

#[cfg(unix)]
fn replace_preserving_metadata(path: &Path, replacement: &str) {
    let before = fs::metadata(path).expect("metadata before replacement");
    assert_eq!(
        before.len(),
        replacement.len() as u64,
        "same-length fixture"
    );
    let replacement_path = path.with_extension("replacement.php");
    fs::write(&replacement_path, replacement).expect("write replacement fixture");
    fs::set_permissions(&replacement_path, before.permissions()).expect("restore permissions");
    restore_file_times(&replacement_path, &before);
    fs::rename(&replacement_path, path).expect("atomically replace fixture");
    let after = fs::metadata(path).expect("metadata after replacement");
    assert_eq!(after.len(), before.len());
    assert_eq!(after.modified().ok(), before.modified().ok());
    assert_eq!(
        after.permissions().readonly(),
        before.permissions().readonly()
    );
}

fn restore_file_times(path: &Path, metadata: &fs::Metadata) {
    let mut times = FileTimes::new();
    if let Ok(modified) = metadata.modified() {
        times = times.set_modified(modified);
    }
    if let Ok(accessed) = metadata.accessed() {
        times = times.set_accessed(accessed);
    }
    OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open fixture to restore times")
        .set_times(times)
        .expect("restore fixture times");
}

fn class_has_property(compiled: &CompiledUnit, class_name: &str, property_name: &str) -> bool {
    compiled
        .unit()
        .classes
        .iter()
        .find(|class| class.name == class_name)
        .is_some_and(|class| {
            class
                .properties
                .iter()
                .any(|property| property.name == property_name)
        })
}

fn binary_add_count(compiled: &CompiledUnit) -> usize {
    compiled
        .unit()
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .filter(|instruction| {
            matches!(
                instruction.kind,
                InstructionKind::Binary {
                    op: BinaryOp::Add,
                    ..
                }
            )
        })
        .count()
}

fn poison_mutex<T>(mutex: &Mutex<T>) {
    let _ = std::panic::catch_unwind(|| {
        let _guard = mutex.lock().expect("lock before poisoning");
        panic!("poison include-cache mutex for deterministic error test");
    });
}

fn poison_rwlock<T>(lock: &RwLock<T>) {
    let _ = std::panic::catch_unwind(|| {
        let _guard = lock.write().expect("lock before poisoning");
        panic!("poison include-cache rwlock for deterministic error test");
    });
}
