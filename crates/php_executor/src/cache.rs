use crate::executor::{CompiledPhpScript, PhpExecutor};
use crate::input::{PhpCompileInput, PhpExecutionError};
use php_optimizer::OptimizationLevel;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::UNIX_EPOCH;

/// Sharded process-local cache for immutable compiled scripts.
#[derive(Debug)]
pub struct CompiledScriptCache {
    enabled: bool,
    shards: Vec<Mutex<HashMap<CompiledScriptCacheKey, Arc<CompiledPhpScript>>>>,
    stats: CompiledScriptCacheCounters,
}

impl CompiledScriptCache {
    /// Creates an enabled cache with at least one shard.
    #[must_use]
    pub fn new(shards: usize) -> Self {
        let shard_count = shards.max(1);
        Self {
            enabled: true,
            shards: (0..shard_count)
                .map(|_| Mutex::new(HashMap::new()))
                .collect(),
            stats: CompiledScriptCacheCounters::default(),
        }
    }

    /// Creates a cache facade that always compiles and never stores entries.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            shards: vec![Mutex::new(HashMap::new())],
            stats: CompiledScriptCacheCounters::default(),
        }
    }

    /// Returns a cached script or compiles and stores a fresh artifact.
    pub fn get_or_compile_script(
        &self,
        executor: &PhpExecutor,
        input: PhpScriptCacheInput,
    ) -> Result<CompiledScriptCacheLookup, PhpExecutionError> {
        let source = fs::read_to_string(&input.path).map_err(|error| {
            PhpExecutionError::Engine(format!("{}: {error}", input.path.display()))
        })?;
        let metadata = fs::metadata(&input.path).map_err(|error| {
            PhpExecutionError::Engine(format!("{}: {error}", input.path.display()))
        })?;
        let key = CompiledScriptCacheKey::new(&input, &source, &metadata)?;
        if !self.enabled {
            self.stats.misses.fetch_add(1, Ordering::Relaxed);
            return self
                .compile_uncached(executor, input, source)
                .map(|compiled| CompiledScriptCacheLookup {
                    compiled: Arc::new(compiled),
                    hit: false,
                });
        }

        let shard_index = self.shard_index(&key);
        let mut shard = self.shards[shard_index]
            .lock()
            .expect("compiled script cache shard mutex poisoned");
        let stale = remove_stale_path_entries(&mut shard, &key);
        if stale > 0 {
            self.stats
                .stale_invalidations
                .fetch_add(stale as u64, Ordering::Relaxed);
            self.stats
                .entries
                .fetch_sub(stale as u64, Ordering::Relaxed);
        }
        if let Some(compiled) = shard.get(&key) {
            self.stats.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(CompiledScriptCacheLookup {
                compiled: Arc::clone(compiled),
                hit: true,
            });
        }
        self.stats.misses.fetch_add(1, Ordering::Relaxed);
        match self.compile_uncached(executor, input, source) {
            Ok(compiled) => {
                let compiled = Arc::new(compiled);
                shard.insert(key, Arc::clone(&compiled));
                self.stats.entries.fetch_add(1, Ordering::Relaxed);
                Ok(CompiledScriptCacheLookup {
                    compiled,
                    hit: false,
                })
            }
            Err(error) => {
                self.stats.compile_errors.fetch_add(1, Ordering::Relaxed);
                Err(error)
            }
        }
    }

    /// Returns current cache counters.
    #[must_use]
    pub fn cache_stats(&self) -> CompiledScriptCacheStats {
        CompiledScriptCacheStats {
            hits: self.stats.hits.load(Ordering::Relaxed),
            misses: self.stats.misses.load(Ordering::Relaxed),
            stale_invalidations: self.stats.stale_invalidations.load(Ordering::Relaxed),
            compile_errors: self.stats.compile_errors.load(Ordering::Relaxed),
            entries: self.stats.entries.load(Ordering::Relaxed),
        }
    }

    /// Clears all cached entries and resets the approximate entry count.
    pub fn clear(&self) {
        for shard in &self.shards {
            let mut shard = shard
                .lock()
                .expect("compiled script cache shard mutex poisoned");
            shard.clear();
        }
        self.stats.entries.store(0, Ordering::Relaxed);
    }

    fn compile_uncached(
        &self,
        executor: &PhpExecutor,
        input: PhpScriptCacheInput,
        source: String,
    ) -> Result<CompiledPhpScript, PhpExecutionError> {
        executor.compile_source(PhpCompileInput {
            source,
            source_path: input.source_path,
            optimization_level: Some(input.optimization_level),
        })
    }

    fn shard_index(&self, key: &CompiledScriptCacheKey) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.path.hash(&mut hasher);
        (hasher.finish() as usize) % self.shards.len()
    }
}

impl Default for CompiledScriptCache {
    fn default() -> Self {
        Self::new(default_cache_shards())
    }
}

/// File-backed script compilation input for the process-local cache.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhpScriptCacheInput {
    pub path: PathBuf,
    pub source_path: String,
    pub optimization_level: OptimizationLevel,
}

/// Cache lookup result.
#[derive(Clone, Debug)]
pub struct CompiledScriptCacheLookup {
    pub compiled: Arc<CompiledPhpScript>,
    pub hit: bool,
}

/// Snapshot of cache counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompiledScriptCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub stale_invalidations: u64,
    pub compile_errors: u64,
    pub entries: u64,
}

#[derive(Debug, Default)]
struct CompiledScriptCacheCounters {
    hits: AtomicU64,
    misses: AtomicU64,
    stale_invalidations: AtomicU64,
    compile_errors: AtomicU64,
    entries: AtomicU64,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CompiledScriptCacheKey {
    path: PathBuf,
    len: u64,
    modified_nanos: u128,
    source_hash: u64,
    optimization_level: &'static str,
    executor_version: &'static str,
    debug_assertions: bool,
}

impl CompiledScriptCacheKey {
    fn new(
        input: &PhpScriptCacheInput,
        source: &str,
        metadata: &fs::Metadata,
    ) -> Result<Self, PhpExecutionError> {
        let path = input.path.canonicalize().map_err(|error| {
            PhpExecutionError::Engine(format!(
                "{}: canonicalize failed: {error}",
                input.path.display()
            ))
        })?;
        let modified_nanos = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_nanos());
        Ok(Self {
            path,
            len: metadata.len(),
            modified_nanos,
            source_hash: stable_source_hash(source),
            optimization_level: input.optimization_level.as_str(),
            executor_version: env!("CARGO_PKG_VERSION"),
            debug_assertions: cfg!(debug_assertions),
        })
    }
}

fn remove_stale_path_entries(
    shard: &mut HashMap<CompiledScriptCacheKey, Arc<CompiledPhpScript>>,
    key: &CompiledScriptCacheKey,
) -> usize {
    let before = shard.len();
    shard.retain(|existing, _| existing.path != key.path || existing == key);
    before.saturating_sub(shard.len())
}

fn stable_source_hash(source: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

fn default_cache_shards() -> usize {
    std::thread::available_parallelism().map_or(16, |count| count.get().clamp(1, 64))
}
