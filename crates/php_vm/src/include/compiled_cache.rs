//! Compiled include artifact cache and stampede coordination.

use super::cache_freshness::{RevalidationClock, ValidationStamp};
use super::compile_coordinator::IncludeCompileCoordinator;
use super::compiler::{IncludeCompiler, IncludeCompilerFingerprint};
use super::diagnostics::include_cache_lock_error;
use super::metadata::{DeploymentRootMode, IncludeMetadataState};
use super::metrics::IncludeCacheCounters;
use super::resolver::{IncludeLoader, ResolvedIncludePath};
use super::source::{
    IncludeDependency, IncludePathFileFingerprint, OpenedSourceIdentity, ValidatedIncludeSource,
    include_path_file_fingerprint, read_validated_file, resolution_path_targets,
};
use crate::compiled_unit::CompiledUnit;
use crate::error::VmError;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};
use std::time::Duration;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct CompiledIncludeKey {
    canonical_path: PathBuf,
    source_identity: OpenedSourceIdentity,
    local_dependencies: Vec<IncludeDependency>,
    compiler: IncludeCompilerFingerprint,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CompiledIncludeLookupKey {
    canonical_path: PathBuf,
    compiler: IncludeCompilerFingerprint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IncludeValidationMode {
    Content,
    IdentityOnly,
}

#[derive(Clone, Copy, Debug)]
enum PrimarySourceValidation<'a> {
    Content(&'a OpenedSourceIdentity),
    IdentityOnly(&'a IncludePathFileFingerprint),
}

impl CompiledIncludeKey {
    fn new(
        canonical_path: PathBuf,
        source_identity: OpenedSourceIdentity,
        local_dependencies: Vec<IncludeDependency>,
        compiler: IncludeCompilerFingerprint,
    ) -> Self {
        Self {
            canonical_path,
            source_identity,
            local_dependencies,
            compiler,
        }
    }
}

impl CompiledIncludeLookupKey {
    fn new(canonical_path: PathBuf, compiler: IncludeCompilerFingerprint) -> Self {
        Self {
            canonical_path,
            compiler,
        }
    }

    fn from_compiled_key(key: &CompiledIncludeKey) -> Self {
        Self {
            canonical_path: key.canonical_path.clone(),
            compiler: key.compiler.clone(),
        }
    }
}

#[derive(Debug)]
pub(super) struct CachedCompiledInclude {
    compiled: Arc<CompiledUnit>,
    validated_at: ValidationStamp,
}

impl CachedCompiledInclude {
    fn new(compiled: Arc<CompiledUnit>, revalidation: &RevalidationClock) -> Self {
        Self {
            compiled,
            validated_at: revalidation.stamp(),
        }
    }
}

#[derive(Debug)]
pub(super) struct CompiledIncludeCache {
    pub(super) shards: Vec<RwLock<HashMap<CompiledIncludeKey, Arc<CachedCompiledInclude>>>>,
    lookup_shards: Vec<RwLock<HashMap<CompiledIncludeLookupKey, CompiledIncludeKey>>>,
    pub(super) compile_coordinator: IncludeCompileCoordinator,
    stats: Arc<IncludeCacheCounters>,
    metadata: Arc<IncludeMetadataState>,
    revalidation: RevalidationClock,
}

impl CompiledIncludeCache {
    pub(super) fn new(
        shard_count: usize,
        stats: Arc<IncludeCacheCounters>,
        metadata: Arc<IncludeMetadataState>,
        revalidation_interval: Duration,
    ) -> Self {
        Self {
            shards: (0..shard_count)
                .map(|_| RwLock::new(HashMap::new()))
                .collect(),
            lookup_shards: (0..shard_count)
                .map(|_| RwLock::new(HashMap::new()))
                .collect(),
            compile_coordinator: IncludeCompileCoordinator::new(shard_count),
            stats,
            metadata,
            revalidation: RevalidationClock::new(revalidation_interval),
        }
    }

    /// Returns a compiled include unit for a resolved path, compiling on miss.
    pub(super) fn get_or_compile_include(
        &self,
        loader: &IncludeLoader,
        resolved: &ResolvedIncludePath,
        compiler: &dyn IncludeCompiler,
    ) -> Result<Arc<CompiledUnit>, VmError> {
        let compiler_fingerprint = compiler.fingerprint(loader);
        loop {
            if let Some(compiled) =
                self.lookup_fresh_compiled_include(resolved, &compiler_fingerprint)?
            {
                return Ok(compiled);
            }
            let validation_mode = if self.immutable_identity_only_allowed(resolved) {
                IncludeValidationMode::IdentityOnly
            } else {
                IncludeValidationMode::Content
            };
            let validated = match validation_mode {
                IncludeValidationMode::Content => {
                    Some(self.load_and_record_validation(loader, resolved)?)
                }
                IncludeValidationMode::IdentityOnly => None,
            };
            let validation = validated.as_ref().map_or(
                PrimarySourceValidation::IdentityOnly(&resolved.fingerprint),
                |loaded| PrimarySourceValidation::Content(&loaded.identity),
            );
            if let Some(compiled) = self.lookup_compiled_include(
                resolved,
                &compiler_fingerprint,
                validation,
                validation_mode,
            )? {
                return Ok(compiled);
            }

            let Some(_permit) = self
                .compile_coordinator
                .try_begin(&resolved.canonical_path)?
            else {
                self.compile_coordinator.wait(&resolved.canonical_path)?;
                continue;
            };

            if let Some(compiled) = self.lookup_compiled_include(
                resolved,
                &compiler_fingerprint,
                validation,
                validation_mode,
            )? {
                return Ok(compiled);
            }

            let validated = match validated {
                Some(validated) => validated,
                None => self.load_and_record_validation(loader, resolved)?,
            };
            self.stats.compile_misses.fetch_add(1, Ordering::Relaxed);
            let source_identity = validated.identity.clone();
            let canonical_path = validated.loaded.canonical_path.clone();
            let compiled = match compiler.compile_include(validated, loader) {
                Ok(compilation) => {
                    let local_dependencies = compilation.dependencies;
                    self.record_compiled_dependency_reads(&local_dependencies);
                    let key = CompiledIncludeKey::new(
                        canonical_path,
                        source_identity,
                        local_dependencies,
                        compiler_fingerprint.clone(),
                    );
                    let shard_index = self.compile_shard_index(&key);
                    let compiled = Arc::new(compilation.unit);
                    let compiled = {
                        let mut shard = self.shards[shard_index]
                            .write()
                            .map_err(|_| include_cache_lock_error("compiled", "insert"))?;
                        Arc::clone(
                            &shard
                                .entry(key.clone())
                                .or_insert_with(|| {
                                    Arc::new(CachedCompiledInclude::new(
                                        compiled,
                                        &self.revalidation,
                                    ))
                                })
                                .compiled,
                        )
                    };
                    self.insert_compiled_lookup_index(&key)?;
                    Ok(compiled)
                }
                Err(message) => {
                    self.stats.compile_errors.fetch_add(1, Ordering::Relaxed);
                    Err(message)
                }
            }?;
            return Ok(compiled);
        }
    }

    /// Serves a hit whose revalidation window is still open, without touching
    /// the filesystem. This runs before any validation input is prepared:
    /// preparing those inputs is what costs per-call fs work (a full source
    /// read in content mode, a resolution-path canonicalize in immutable
    /// identity mode), and a freshly-validated slot never consults them.
    fn lookup_fresh_compiled_include(
        &self,
        resolved: &ResolvedIncludePath,
        compiler: &IncludeCompilerFingerprint,
    ) -> Result<Option<Arc<CompiledUnit>>, VmError> {
        if !self.revalidation.enabled() {
            return Ok(None);
        }
        let lookup_key =
            CompiledIncludeLookupKey::new(resolved.canonical_path.clone(), compiler.clone());
        let shard_index = self.compile_shard_index_for_path(&lookup_key.canonical_path);
        let Some(full_key) = ({
            let lookup_shard = self.lookup_shards[shard_index]
                .read()
                .map_err(|_| include_cache_lock_error("compiled-index", "lookup"))?;
            lookup_shard.get(&lookup_key).cloned()
        }) else {
            return Ok(None);
        };
        let slot = {
            let shard = self.shards[shard_index]
                .read()
                .map_err(|_| include_cache_lock_error("compiled", "lookup"))?;
            shard.get(&full_key).map(Arc::clone)
        };
        let Some(slot) = slot else {
            return Ok(None);
        };
        if self.revalidation.is_fresh(&slot.validated_at) {
            self.stats.compile_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(Some(Arc::clone(&slot.compiled)));
        }
        Ok(None)
    }

    fn lookup_compiled_include(
        &self,
        resolved: &ResolvedIncludePath,
        compiler: &IncludeCompilerFingerprint,
        validation: PrimarySourceValidation<'_>,
        validation_mode: IncludeValidationMode,
    ) -> Result<Option<Arc<CompiledUnit>>, VmError> {
        let lookup_key =
            CompiledIncludeLookupKey::new(resolved.canonical_path.clone(), compiler.clone());
        let shard_index = self.compile_shard_index_for_path(&lookup_key.canonical_path);
        let Some(full_key) = ({
            let lookup_shard = self.lookup_shards[shard_index]
                .read()
                .map_err(|_| include_cache_lock_error("compiled-index", "lookup"))?;
            lookup_shard.get(&lookup_key).cloned()
        }) else {
            return Ok(None);
        };
        let hit = {
            let shard = self.shards[shard_index]
                .read()
                .map_err(|_| include_cache_lock_error("compiled", "lookup"))?;
            shard
                .get_key_value(&full_key)
                .map(|(key, slot)| (key.clone(), Arc::clone(slot)))
        };
        let Some((key, slot)) = hit else {
            return Ok(None);
        };
        if self.revalidation.enabled() && self.revalidation.is_fresh(&slot.validated_at) {
            self.stats.compile_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(Some(Arc::clone(&slot.compiled)));
        }
        if !self.primary_source_is_fresh(&key, validation) {
            self.invalidate_compiled_include(&key, false)?;
            return Ok(None);
        }
        match self.dependencies_are_fresh(&key, validation_mode) {
            Ok(true) => {
                if validation_mode == IncludeValidationMode::IdentityOnly {
                    self.stats
                        .identity_only_hits
                        .fetch_add(1, Ordering::Relaxed);
                }
                self.stats.compile_hits.fetch_add(1, Ordering::Relaxed);
                self.revalidation.touch(&slot.validated_at);
                Ok(Some(Arc::clone(&slot.compiled)))
            }
            Ok(false) | Err(_) => {
                self.invalidate_compiled_include(&key, true)?;
                Ok(None)
            }
        }
    }

    fn primary_source_is_fresh(
        &self,
        key: &CompiledIncludeKey,
        validation: PrimarySourceValidation<'_>,
    ) -> bool {
        match validation {
            PrimarySourceValidation::Content(identity) => {
                if key.source_identity.content_hash != identity.content_hash {
                    self.stats
                        .content_mismatches
                        .fetch_add(1, Ordering::Relaxed);
                }
                key.source_identity == *identity
            }
            PrimarySourceValidation::IdentityOnly(generation) => {
                generation.has_reliable_generation()
                    && key.source_identity.generation == *generation
            }
        }
    }

    fn invalidate_compiled_include(
        &self,
        key: &CompiledIncludeKey,
        dependency: bool,
    ) -> Result<(), VmError> {
        let shard_index = self.compile_shard_index(key);
        let mut shard = self.shards[shard_index]
            .write()
            .map_err(|_| include_cache_lock_error("compiled", "invalidate"))?;
        if shard.remove(key).is_some() {
            drop(shard);
            self.remove_compiled_lookup_index(key)?;
            self.stats
                .stale_invalidations
                .fetch_add(1, Ordering::Relaxed);
            if dependency {
                self.stats
                    .stale_dependency_invalidations
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        Ok(())
    }

    fn insert_compiled_lookup_index(&self, key: &CompiledIncludeKey) -> Result<(), VmError> {
        let shard_index = self.compile_shard_index(key);
        let lookup_key = CompiledIncludeLookupKey::from_compiled_key(key);
        let mut shard = self.lookup_shards[shard_index]
            .write()
            .map_err(|_| include_cache_lock_error("compiled-index", "insert"))?;
        shard.insert(lookup_key, key.clone());
        Ok(())
    }

    fn remove_compiled_lookup_index(&self, key: &CompiledIncludeKey) -> Result<(), VmError> {
        let shard_index = self.compile_shard_index(key);
        let lookup_key = CompiledIncludeLookupKey::from_compiled_key(key);
        let mut shard = self.lookup_shards[shard_index]
            .write()
            .map_err(|_| include_cache_lock_error("compiled-index", "remove"))?;
        shard.remove(&lookup_key);
        Ok(())
    }

    fn dependencies_are_fresh(
        &self,
        key: &CompiledIncludeKey,
        validation_mode: IncludeValidationMode,
    ) -> Result<bool, VmError> {
        if self.metadata.trusts_immutable_path(&key.canonical_path)
            && key.local_dependencies.iter().all(|dependency| {
                self.metadata
                    .trusts_immutable_path(&dependency.canonical_path)
            })
        {
            self.stats
                .immutable_release_hits
                .fetch_add(1, Ordering::Relaxed);
            return Ok(true);
        }
        for dependency in &key.local_dependencies {
            self.stats
                .dependency_metadata_validations
                .fetch_add(1, Ordering::Relaxed);
            match validation_mode {
                IncludeValidationMode::IdentityOnly => {
                    if !dependency
                        .source_identity
                        .generation
                        .has_reliable_generation()
                    {
                        self.stats
                            .conservative_misses
                            .fetch_add(1, Ordering::Relaxed);
                        return Ok(false);
                    }
                    let current = include_path_file_fingerprint(&dependency.canonical_path)?;
                    if current != dependency.source_identity.generation {
                        return Ok(false);
                    }
                }
                IncludeValidationMode::Content => {
                    let current = match read_validated_file(&dependency.canonical_path) {
                        Ok(current) => current,
                        Err(_) => {
                            self.stats
                                .conservative_misses
                                .fetch_add(1, Ordering::Relaxed);
                            return Ok(false);
                        }
                    };
                    self.record_content_validation(&current);
                    if current.identity.content_hash != dependency.source_identity.content_hash {
                        self.stats
                            .content_mismatches
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    if current.identity != dependency.source_identity {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }

    fn immutable_identity_only_allowed(&self, resolved: &ResolvedIncludePath) -> bool {
        let fingerprint = match self.metadata.deployment_root_fingerprint() {
            Ok(fingerprint) => fingerprint,
            Err(()) => {
                self.stats
                    .conservative_misses
                    .fetch_add(1, Ordering::Relaxed);
                return false;
            }
        };
        let Some(fingerprint) = fingerprint else {
            return false;
        };
        if fingerprint.mode != DeploymentRootMode::ImmutableDeclared
            || !resolved
                .canonical_path
                .starts_with(&fingerprint.canonical_root)
        {
            return false;
        }

        // Immutable mode is an operator promise that files beneath the release
        // root remain valid until the cache is explicitly cleared. Still
        // re-canonicalize the original candidate so a symlink or release-root
        // swap cannot keep an artifact compiled for the previous target.
        if !resolution_path_targets(
            resolved.resolution_path.as_deref(),
            &resolved.canonical_path,
        ) {
            self.stats
                .conservative_misses
                .fetch_add(1, Ordering::Relaxed);
            return false;
        }
        true
    }

    fn load_and_record_validation(
        &self,
        loader: &IncludeLoader,
        resolved: &ResolvedIncludePath,
    ) -> Result<ValidatedIncludeSource, VmError> {
        let loaded = loader.load_validated_resolved(resolved).inspect_err(|_| {
            self.stats
                .conservative_misses
                .fetch_add(1, Ordering::Relaxed);
        })?;
        self.record_content_validation(&loaded);
        Ok(loaded)
    }

    fn record_content_validation(&self, loaded: &ValidatedIncludeSource) {
        self.stats.source_reads.fetch_add(1, Ordering::Relaxed);
        self.stats
            .source_bytes_hashed
            .fetch_add(loaded.bytes_hashed, Ordering::Relaxed);
        self.stats
            .content_validations
            .fetch_add(1, Ordering::Relaxed);
    }

    fn record_compiled_dependency_reads(&self, dependencies: &[IncludeDependency]) {
        let count = dependencies.len() as u64;
        let bytes = dependencies
            .iter()
            .map(|dependency| dependency.source_identity.generation.len)
            .sum();
        self.stats.source_reads.fetch_add(count, Ordering::Relaxed);
        self.stats
            .source_bytes_hashed
            .fetch_add(bytes, Ordering::Relaxed);
        self.stats
            .content_validations
            .fetch_add(count, Ordering::Relaxed);
    }

    fn compile_shard_index(&self, key: &CompiledIncludeKey) -> usize {
        self.compile_shard_index_for_path(&key.canonical_path)
    }

    fn compile_shard_index_for_path(&self, path: &Path) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        path.hash(&mut hasher);
        (hasher.finish() as usize) % self.shards.len()
    }

    pub(super) fn clear(&self) -> Result<(), VmError> {
        for shard in &self.shards {
            shard
                .write()
                .map_err(|_| include_cache_lock_error("compiled", "clear"))?
                .clear();
        }
        for shard in &self.lookup_shards {
            shard
                .write()
                .map_err(|_| include_cache_lock_error("compiled-index", "clear"))?
                .clear();
        }
        Ok(())
    }
}
