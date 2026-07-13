//! Positive and guarded-negative include resolution cache.
//!
//! This component knows paths, loader policy, directory guards, and metrics.
//! It deliberately has no compiler or compiled-artifact imports.

use super::cache_freshness::{RevalidationClock, ValidationStamp};
use super::diagnostics::include_cache_lock_error;
use super::metadata::IncludeMetadataState;
use super::metrics::IncludeCacheCounters;
use super::resolver::{
    IncludeLoader, NegativeIncludeEntry, NegativeProbeTrace, ResolvedIncludePath,
    negative_include_cache_enabled,
};
use super::source::{include_path_file_fingerprint, resolution_path_targets};
use crate::error::VmError;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Per-shard capacity bound for negative include-path entries. Autoloader
/// probing can generate unbounded distinct missing paths (class names can be
/// user-influenced), so growth must be capped; overflow skips installation
/// and counts `negative_cache_blocked_capacity` instead.
pub(super) const NEGATIVE_INCLUDE_CACHE_SHARD_CAPACITY: usize = 1024;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct IncludeResolutionKey {
    including_file_directory: Option<PathBuf>,
    path: String,
    include_path: Vec<PathBuf>,
    cwd: Option<PathBuf>,
    allowed_roots: Vec<PathBuf>,
}

impl IncludeResolutionKey {
    fn new(
        loader: &IncludeLoader,
        including_file: Option<&Path>,
        path: &str,
        include_path: &[PathBuf],
        cwd: Option<&Path>,
    ) -> Self {
        Self {
            including_file_directory: including_file.and_then(Path::parent).map(Path::to_path_buf),
            path: path.to_owned(),
            include_path: include_path.to_vec(),
            cwd: cwd.map(Path::to_path_buf),
            allowed_roots: loader.allowed_roots().to_vec(),
        }
    }
}

#[derive(Debug)]
pub(super) struct CachedIncludeResolution {
    resolved: ResolvedIncludePath,
    validated_at: ValidationStamp,
}

impl CachedIncludeResolution {
    fn new(resolved: ResolvedIncludePath, revalidation: &RevalidationClock) -> Self {
        Self {
            resolved,
            validated_at: revalidation.stamp(),
        }
    }
}

#[derive(Debug)]
pub(super) struct ResolutionCache {
    pub(super) shards: Vec<RwLock<HashMap<IncludeResolutionKey, Arc<CachedIncludeResolution>>>>,
    negative_shards: Vec<RwLock<HashMap<IncludeResolutionKey, NegativeIncludeEntry>>>,
    stats: Arc<IncludeCacheCounters>,
    metadata: Arc<IncludeMetadataState>,
    revalidation: RevalidationClock,
}

impl ResolutionCache {
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
            negative_shards: (0..shard_count)
                .map(|_| RwLock::new(HashMap::new()))
                .collect(),
            stats,
            metadata,
            revalidation: RevalidationClock::new(revalidation_interval),
        }
    }

    /// Resolves an include path through a shared process-local cache.
    pub(super) fn resolve_with_include_path(
        &self,
        loader: &IncludeLoader,
        including_file: Option<&Path>,
        path: &str,
        include_path: &[PathBuf],
        cwd: Option<&Path>,
    ) -> Result<ResolvedIncludePath, VmError> {
        let key = IncludeResolutionKey::new(loader, including_file, path, include_path, cwd);
        let shard_index = self.shard_index(&key);
        if let Some(cached) = {
            let shard = self.shards[shard_index]
                .read()
                .map_err(|_| include_cache_lock_error("resolution", "lookup"))?;
            shard.get(&key).cloned()
        } {
            if self.revalidation.enabled() && self.revalidation.is_fresh(&cached.validated_at) {
                self.stats.resolution_hits.fetch_add(1, Ordering::Relaxed);
                return Ok(cached.resolved.clone());
            }
            let resolved = &cached.resolved;
            let target_is_current = resolution_path_targets(
                resolved.resolution_path.as_deref(),
                &resolved.canonical_path,
            );
            if target_is_current
                && self
                    .metadata
                    .trusts_immutable_path(&resolved.canonical_path)
            {
                self.stats.resolution_hits.fetch_add(1, Ordering::Relaxed);
                self.stats
                    .immutable_release_hits
                    .fetch_add(1, Ordering::Relaxed);
                // Re-arm the freshness window: without this stamp an
                // immutable-trusted entry never counts as freshly validated,
                // so every hit after the window re-canonicalizes its
                // resolution path — one fs op per include per request. With
                // it, the symlink/root-swap probe keeps its per-window
                // cadence instead of running per hit.
                self.revalidation.touch(&cached.validated_at);
                return Ok(resolved.clone());
            }
            match include_path_file_fingerprint(&resolved.canonical_path) {
                Ok(current) if target_is_current && current == resolved.fingerprint => {
                    self.stats.resolution_hits.fetch_add(1, Ordering::Relaxed);
                    self.stats.observe_directory_version(resolved);
                    self.revalidation.touch(&cached.validated_at);
                    return Ok(resolved.clone());
                }
                Ok(_) | Err(_) => {
                    let mut shard = self.shards[shard_index]
                        .write()
                        .map_err(|_| include_cache_lock_error("resolution", "invalidate"))?;
                    shard.remove(&key);
                    self.stats
                        .stale_invalidations
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        if let Some(error) = self.lookup_negative_include(&key) {
            return Err(error);
        }
        self.stats.resolution_misses.fetch_add(1, Ordering::Relaxed);
        let resolved = match loader.resolve_with_include_path_traced(
            including_file,
            path,
            include_path,
            cwd,
        ) {
            Ok(resolved) => resolved,
            Err((error, trace)) => {
                self.maybe_install_negative_include(key, &error, trace);
                return Err(error);
            }
        };
        let mut shard = self.shards[shard_index]
            .write()
            .map_err(|_| include_cache_lock_error("resolution", "insert"))?;
        shard.entry(key).or_insert_with(|| {
            Arc::new(CachedIncludeResolution::new(
                resolved.clone(),
                &self.revalidation,
            ))
        });
        Ok(resolved)
    }

    /// Serves a cached missing-include failure only while every guard
    /// directory version still matches. Any changed or unobservable guard
    /// drops the entry and falls back to full resolution.
    fn lookup_negative_include(&self, key: &IncludeResolutionKey) -> Option<VmError> {
        if !negative_include_cache_enabled() {
            return None;
        }
        let shard_index = self.negative_shard_index(key);
        // Clone the entry out, then validate its directory-version guards
        // (a stat per guard) WITHOUT holding the shard lock, so concurrent
        // threads hashing to this shard do not convoy on filesystem I/O.
        // A poisoned shard is advisory-degraded to a cache miss, never a hard
        // include failure.
        let entry = {
            let shard = self.negative_shards[shard_index].read().ok()?;
            shard.get(key)?.clone()
        };
        if entry.is_still_valid() {
            self.stats
                .negative_cache_hits
                .fetch_add(1, Ordering::Relaxed);
            return Some(entry.error);
        }
        if let Ok(mut shard) = self.negative_shards[shard_index].write() {
            shard.remove(key);
        }
        self.stats
            .negative_cache_invalidations
            .fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Installs a directory-version-guarded negative entry for a missing
    /// include path. Fail-closed: only a genuine `E_PHP_VM_INCLUDE_MISSING`
    /// whose loader trace captured pre-probe versions for every (absolute,
    /// non-symlink, ENOENT) candidate parent is cacheable, and each shard is
    /// capacity-bounded.
    fn maybe_install_negative_include(
        &self,
        key: IncludeResolutionKey,
        error: &VmError,
        trace: NegativeProbeTrace,
    ) {
        if !negative_include_cache_enabled() || error.code() != "E_PHP_VM_INCLUDE_MISSING" {
            return;
        }
        let Some(guards) = trace.guards.filter(|guards| !guards.is_empty()) else {
            self.stats
                .negative_cache_blocked_unversioned
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        let entry = NegativeIncludeEntry {
            error: error.clone(),
            guards,
        };
        let shard_index = self.negative_shard_index(&key);
        let Ok(mut shard) = self.negative_shards[shard_index].write() else {
            return;
        };
        if shard.len() >= NEGATIVE_INCLUDE_CACHE_SHARD_CAPACITY && !shard.contains_key(&key) {
            self.stats
                .negative_cache_blocked_capacity
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        shard.insert(key, entry);
        self.stats
            .negative_cache_installs
            .fetch_add(1, Ordering::Relaxed);
    }

    fn negative_shard_index(&self, key: &IncludeResolutionKey) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.negative_shards.len()
    }

    fn shard_index(&self, key: &IncludeResolutionKey) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.shards.len()
    }

    pub(super) fn clear(&self) -> Result<(), VmError> {
        for shard in &self.shards {
            shard
                .write()
                .map_err(|_| include_cache_lock_error("resolution", "clear"))?
                .clear();
        }
        for shard in &self.negative_shards {
            shard
                .write()
                .map_err(|_| include_cache_lock_error("negative", "clear"))?
                .clear();
        }
        Ok(())
    }
}
