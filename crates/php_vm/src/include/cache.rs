//! Public include-cache facade composing independent cache components.

use super::compiled_cache::CompiledIncludeCache;
use super::compiler::IncludeCompiler;
use super::metadata::{
    ComposerFingerprintTransition, DeploymentRootFingerprint, IncludeMetadataState,
};
use super::metrics::{IncludeCacheCounters, IncludeCacheStats};
use super::resolution_cache::ResolutionCache;
use super::resolver::{IncludeLoader, ResolvedIncludePath};
use crate::compiled_unit::CompiledUnit;
use crate::error::VmError;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;

static NEXT_CACHE_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

/// Process-unique identity for one include-cache instance.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CacheInstanceId(u64);

impl CacheInstanceId {
    fn next() -> Self {
        let id = NEXT_CACHE_INSTANCE_ID.fetch_add(1, Ordering::Relaxed);
        if id == 0 {
            std::process::abort();
        }
        Self(id)
    }

    /// Returns the process-local numeric identity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Server-parity default for serving cached includes without filesystem
/// probes (opcache.revalidate_freq).
pub const SERVER_INCLUDE_REVALIDATION_INTERVAL: Duration = Duration::from_secs(2);

/// Reads the include revalidation window override, when set. The CLI default
/// validates every hit, while server embedders use the deployment window.
pub fn include_revalidation_interval_from_env() -> Option<Duration> {
    std::env::var("PHRUST_INCLUDE_REVALIDATE_MS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(Duration::from_millis)
}

/// Shared process-local include cache.
///
/// Resolution and compiled-artifact storage are separate components. This
/// facade owns only cross-component orchestration and the stable public API.
#[derive(Debug)]
pub struct IncludeCache {
    instance_id: CacheInstanceId,
    pub(super) resolution: ResolutionCache,
    pub(super) compiled: CompiledIncludeCache,
    stats: Arc<IncludeCacheCounters>,
    metadata: Arc<IncludeMetadataState>,
}

impl IncludeCache {
    /// Creates a cache with at least one shard.
    #[must_use]
    pub fn new(shards: usize) -> Self {
        Self::new_with_revalidation_interval(
            shards,
            include_revalidation_interval_from_env().unwrap_or(Duration::ZERO),
        )
    }

    /// Creates a cache with an explicit revalidation window. Zero validates
    /// every hit against the filesystem.
    #[must_use]
    pub fn new_with_revalidation_interval(shards: usize, revalidation_interval: Duration) -> Self {
        let shard_count = shards.max(1);
        let stats = Arc::new(IncludeCacheCounters::default());
        let metadata = Arc::new(IncludeMetadataState::new(Arc::clone(&stats)));
        Self {
            instance_id: CacheInstanceId::next(),
            resolution: ResolutionCache::new(
                shard_count,
                Arc::clone(&stats),
                Arc::clone(&metadata),
                revalidation_interval,
            ),
            compiled: CompiledIncludeCache::new(
                shard_count,
                Arc::clone(&stats),
                Arc::clone(&metadata),
                revalidation_interval,
            ),
            stats,
            metadata,
        }
    }

    /// Returns the stable logical identity assigned when this cache was built.
    #[must_use]
    pub const fn instance_id(&self) -> CacheInstanceId {
        self.instance_id
    }

    pub fn set_deployment_root_fingerprint(&self, fingerprint: Option<DeploymentRootFingerprint>) {
        self.metadata.set_deployment_root_fingerprint(fingerprint);
    }

    pub fn revalidate_deployment_root(&self) {
        self.metadata.revalidate_deployment_root();
    }

    pub fn note_composer_fingerprint(
        &self,
        current: Option<&str>,
    ) -> ComposerFingerprintTransition {
        self.metadata.note_composer_fingerprint(current)
    }

    pub fn resolve_with_include_path(
        &self,
        loader: &IncludeLoader,
        including_file: Option<&Path>,
        path: &str,
        include_path: &[PathBuf],
        cwd: Option<&Path>,
    ) -> Result<ResolvedIncludePath, VmError> {
        self.resolution
            .resolve_with_include_path(loader, including_file, path, include_path, cwd)
    }

    pub fn get_or_compile_include(
        &self,
        loader: &IncludeLoader,
        resolved: &ResolvedIncludePath,
        compiler: &dyn IncludeCompiler,
    ) -> Result<Arc<CompiledUnit>, VmError> {
        self.compiled
            .get_or_compile_include(loader, resolved, compiler)
    }

    /// Clears cached include resolutions and compiled include units.
    pub fn clear(&self) -> Result<(), VmError> {
        self.resolution.clear()?;
        self.compiled.clear()
    }

    /// Returns current cache counters.
    #[must_use]
    pub fn cache_stats(&self) -> IncludeCacheStats {
        self.stats.snapshot()
    }
}

impl Default for IncludeCache {
    fn default() -> Self {
        Self::new(default_include_cache_shards())
    }
}

fn default_include_cache_shards() -> usize {
    std::thread::available_parallelism().map_or(16, |count| count.get().clamp(1, 64))
}
