use crate::executor::{CompiledPhpScript, PhpExecutor};
use crate::input::{PhpCompileInput, PhpExecutionError};
use php_optimizer::OptimizationLevel;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant, UNIX_EPOCH};

/// Sharded process-local cache for immutable compiled scripts.
#[derive(Debug)]
pub struct CompiledScriptCache {
    enabled: bool,
    max_entries_per_shard: usize,
    check_interval: Duration,
    shards: Vec<Mutex<HashMap<CompiledScriptCacheKey, CompiledScriptCacheEntry>>>,
    compile_locks: Vec<CompileLockShard>,
    stats: CompiledScriptCacheCounters,
    access_clock: AtomicU64,
}

impl CompiledScriptCache {
    /// Creates an enabled cache with at least one shard.
    #[must_use]
    pub fn new(shards: usize) -> Self {
        Self::new_with_limits(shards, default_cache_max_entries(), Duration::ZERO)
    }

    /// Creates an enabled cache with a process-wide approximate entry limit.
    #[must_use]
    pub fn new_with_limits(shards: usize, max_entries: usize, check_interval: Duration) -> Self {
        let shard_count = shards.max(1);
        let max_entries_per_shard =
            max_entries.max(1).saturating_add(shard_count - 1) / shard_count;
        Self {
            enabled: true,
            max_entries_per_shard: max_entries_per_shard.max(1),
            check_interval,
            shards: (0..shard_count)
                .map(|_| Mutex::new(HashMap::new()))
                .collect(),
            compile_locks: (0..shard_count)
                .map(|_| CompileLockShard::default())
                .collect(),
            stats: CompiledScriptCacheCounters::default(),
            access_clock: AtomicU64::new(0),
        }
    }

    /// Creates a cache facade that always compiles and never stores entries.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            max_entries_per_shard: 1,
            check_interval: Duration::ZERO,
            shards: vec![Mutex::new(HashMap::new())],
            compile_locks: vec![CompileLockShard::default()],
            stats: CompiledScriptCacheCounters::default(),
            access_clock: AtomicU64::new(0),
        }
    }

    /// Returns a cached script or compiles and stores a fresh artifact.
    pub fn get_or_compile_script(
        &self,
        executor: &PhpExecutor,
        input: PhpScriptCacheInput,
    ) -> Result<CompiledScriptCacheLookup, PhpExecutionError> {
        self.get_or_compile_script_with(executor, input, Self::compile_uncached)
    }

    fn get_or_compile_script_with<F>(
        &self,
        executor: &PhpExecutor,
        input: PhpScriptCacheInput,
        compile: F,
    ) -> Result<CompiledScriptCacheLookup, PhpExecutionError>
    where
        F: Fn(
            &Self,
            &PhpExecutor,
            PhpScriptCacheInput,
            String,
        ) -> Result<CompiledPhpScript, PhpExecutionError>,
    {
        if !self.enabled {
            self.stats.misses.fetch_add(1, Ordering::Relaxed);
            let source = read_script_source(&input.path)?;
            return compile(self, executor, input, source).map(|compiled| {
                CompiledScriptCacheLookup {
                    compiled: Arc::new(compiled),
                    hit: false,
                }
            });
        }

        let canonical_path = input.path.canonicalize().map_err(|error| {
            PhpExecutionError::Engine(format!(
                "{}: canonicalize failed: {error}",
                input.path.display()
            ))
        })?;

        loop {
            if let Some(compiled) = self.lookup_fresh_by_path(&canonical_path) {
                return Ok(CompiledScriptCacheLookup {
                    compiled,
                    hit: true,
                });
            }

            let source = read_script_source(&input.path)?;
            let metadata = read_script_metadata(&input.path)?;
            let key = CompiledScriptCacheKey::new_with_canonical_path(
                canonical_path.clone(),
                &input,
                &source,
                &metadata,
            );
            if let Some(compiled) = self.lookup_exact(&key) {
                return Ok(CompiledScriptCacheLookup {
                    compiled,
                    hit: true,
                });
            }

            let Some(_permit) = self.try_begin_compile(&canonical_path) else {
                self.wait_for_compile(&canonical_path);
                continue;
            };

            if let Some(compiled) = self.lookup_exact(&key) {
                return Ok(CompiledScriptCacheLookup {
                    compiled,
                    hit: true,
                });
            }

            self.stats.misses.fetch_add(1, Ordering::Relaxed);
            match compile(self, executor, input, source) {
                Ok(compiled) => {
                    let compiled = Arc::new(compiled);
                    self.insert_compiled(key, Arc::clone(&compiled));
                    return Ok(CompiledScriptCacheLookup {
                        compiled,
                        hit: false,
                    });
                }
                Err(error) => {
                    self.stats.compile_errors.fetch_add(1, Ordering::Relaxed);
                    return Err(error);
                }
            }
        }
    }

    fn lookup_fresh_by_path(&self, canonical_path: &Path) -> Option<Arc<CompiledPhpScript>> {
        if self.check_interval.is_zero() {
            return None;
        }
        let shard_index = self.shard_index_for_path(canonical_path);
        let mut shard = self.shards[shard_index]
            .lock()
            .expect("compiled script cache shard mutex poisoned");
        let now = Instant::now();
        let (_, entry) = shard.iter_mut().find(|(key, entry)| {
            key.path == canonical_path && now.duration_since(entry.checked_at) < self.check_interval
        })?;
        self.stats.hits.fetch_add(1, Ordering::Relaxed);
        entry.access_tick = self.next_access_tick();
        Some(Arc::clone(&entry.compiled))
    }

    fn lookup_exact(&self, key: &CompiledScriptCacheKey) -> Option<Arc<CompiledPhpScript>> {
        let shard_index = self.shard_index(key);
        let mut shard = self.shards[shard_index]
            .lock()
            .expect("compiled script cache shard mutex poisoned");
        let stale = remove_stale_path_entries(&mut shard, key);
        if stale > 0 {
            self.stats
                .stale_invalidations
                .fetch_add(stale as u64, Ordering::Relaxed);
            self.stats
                .entries
                .fetch_sub(stale as u64, Ordering::Relaxed);
        }
        let entry = shard.get_mut(key)?;
        self.stats.hits.fetch_add(1, Ordering::Relaxed);
        entry.access_tick = self.next_access_tick();
        entry.checked_at = Instant::now();
        Some(Arc::clone(&entry.compiled))
    }

    fn insert_compiled(&self, key: CompiledScriptCacheKey, compiled: Arc<CompiledPhpScript>) {
        let shard_index = self.shard_index(&key);
        let mut shard = self.shards[shard_index]
            .lock()
            .expect("compiled script cache shard mutex poisoned");
        if let Some(entry) = shard.get_mut(&key) {
            entry.access_tick = self.next_access_tick();
            entry.checked_at = Instant::now();
            return;
        }
        while shard.len() >= self.max_entries_per_shard {
            let Some(evict_key) = shard
                .iter()
                .min_by_key(|(_, entry)| entry.access_tick)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            shard.remove(&evict_key);
            self.stats.evictions.fetch_add(1, Ordering::Relaxed);
            self.stats.entries.fetch_sub(1, Ordering::Relaxed);
        }
        shard.insert(
            key,
            CompiledScriptCacheEntry {
                compiled,
                access_tick: self.next_access_tick(),
                checked_at: Instant::now(),
            },
        );
        self.stats.entries.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns current cache counters.
    #[must_use]
    pub fn cache_stats(&self) -> CompiledScriptCacheStats {
        let entries_by_shard = self
            .shards
            .iter()
            .map(|shard| {
                shard
                    .lock()
                    .expect("compiled script cache shard mutex poisoned")
                    .len()
            })
            .collect();
        CompiledScriptCacheStats {
            hits: self.stats.hits.load(Ordering::Relaxed),
            misses: self.stats.misses.load(Ordering::Relaxed),
            stale_invalidations: self.stats.stale_invalidations.load(Ordering::Relaxed),
            compile_errors: self.stats.compile_errors.load(Ordering::Relaxed),
            evictions: self.stats.evictions.load(Ordering::Relaxed),
            compile_in_progress: self.stats.compile_in_progress.load(Ordering::Relaxed),
            entries: self.stats.entries.load(Ordering::Relaxed),
            entries_by_shard,
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

    fn try_begin_compile(&self, path: &Path) -> Option<CompilePermit<'_>> {
        let shard = &self.compile_locks[self.shard_index_for_path(path)];
        let mut in_progress = shard
            .in_progress
            .lock()
            .expect("compiled script compile lock mutex poisoned");
        if !in_progress.insert(path.to_path_buf()) {
            return None;
        }
        self.stats
            .compile_in_progress
            .fetch_add(1, Ordering::Relaxed);
        Some(CompilePermit {
            shard,
            stats: &self.stats,
            path: path.to_path_buf(),
        })
    }

    fn wait_for_compile(&self, path: &Path) {
        let shard = &self.compile_locks[self.shard_index_for_path(path)];
        let mut in_progress = shard
            .in_progress
            .lock()
            .expect("compiled script compile lock mutex poisoned");
        while in_progress.contains(path) {
            in_progress = shard
                .condvar
                .wait(in_progress)
                .expect("compiled script compile lock mutex poisoned");
        }
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
        self.shard_index_for_path(&key.path)
    }

    fn shard_index_for_path(&self, path: &Path) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        path.hash(&mut hasher);
        (hasher.finish() as usize) % self.shards.len()
    }

    fn next_access_tick(&self) -> u64 {
        self.access_clock
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1)
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
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompiledScriptCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub stale_invalidations: u64,
    pub compile_errors: u64,
    pub evictions: u64,
    pub compile_in_progress: u64,
    pub entries: u64,
    pub entries_by_shard: Vec<usize>,
}

#[derive(Debug, Default)]
struct CompiledScriptCacheCounters {
    hits: AtomicU64,
    misses: AtomicU64,
    stale_invalidations: AtomicU64,
    compile_errors: AtomicU64,
    evictions: AtomicU64,
    compile_in_progress: AtomicU64,
    entries: AtomicU64,
}

#[derive(Debug)]
struct CompiledScriptCacheEntry {
    compiled: Arc<CompiledPhpScript>,
    access_tick: u64,
    checked_at: Instant,
}

#[derive(Debug, Default)]
struct CompileLockShard {
    in_progress: Mutex<HashSet<PathBuf>>,
    condvar: Condvar,
}

struct CompilePermit<'a> {
    shard: &'a CompileLockShard,
    stats: &'a CompiledScriptCacheCounters,
    path: PathBuf,
}

impl Drop for CompilePermit<'_> {
    fn drop(&mut self) {
        let mut in_progress = self
            .shard
            .in_progress
            .lock()
            .expect("compiled script compile lock mutex poisoned");
        in_progress.remove(&self.path);
        self.stats
            .compile_in_progress
            .fetch_sub(1, Ordering::Relaxed);
        self.shard.condvar.notify_all();
    }
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
    fn new_with_canonical_path(
        path: PathBuf,
        input: &PhpScriptCacheInput,
        source: &str,
        metadata: &fs::Metadata,
    ) -> Self {
        let modified_nanos = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_nanos());
        Self {
            path,
            len: metadata.len(),
            modified_nanos,
            source_hash: stable_source_hash(source),
            optimization_level: input.optimization_level.as_str(),
            executor_version: env!("CARGO_PKG_VERSION"),
            debug_assertions: cfg!(debug_assertions),
        }
    }
}

fn remove_stale_path_entries(
    shard: &mut HashMap<CompiledScriptCacheKey, CompiledScriptCacheEntry>,
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

fn read_script_source(path: &Path) -> Result<String, PhpExecutionError> {
    fs::read_to_string(path)
        .map_err(|error| PhpExecutionError::Engine(format!("{}: {error}", path.display())))
}

fn read_script_metadata(path: &Path) -> Result<fs::Metadata, PhpExecutionError> {
    fs::metadata(path)
        .map_err(|error| PhpExecutionError::Engine(format!("{}: {error}", path.display())))
}

fn default_cache_shards() -> usize {
    std::thread::available_parallelism().map_or(16, |count| count.get().clamp(1, 64))
}

fn default_cache_max_entries() -> usize {
    4096
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{PhpExecutionOutput, PhpRequestExecutionInput};
    use php_runtime::api::RuntimeContext;
    use std::sync::{Barrier, atomic::Ordering};

    #[test]
    fn compiled_script_cache_hits_after_first_compile() {
        let fixture = CacheFixture::new("cache-hit");
        fixture.write("<?php echo \"hi\\n\";");
        let executor = PhpExecutor::new();
        let cache = CompiledScriptCache::new(2);

        let first = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("first compile");
        let second = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("second lookup");

        assert!(!first.hit);
        assert!(second.hit);
        let stats = cache.cache_stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.stale_invalidations, 0);
        assert_eq!(stats.compile_errors, 0);
        assert_eq!(stats.evictions, 0);
        assert_eq!(stats.compile_in_progress, 0);
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.entries_by_shard.iter().sum::<usize>(), 1);
    }

    #[test]
    fn bounded_compiled_script_cache_evicts_when_limit_is_exceeded() {
        let fixture = CacheFixture::new("cache-evict");
        let executor = PhpExecutor::new();
        let cache = CompiledScriptCache::new_with_limits(1, 1, Duration::ZERO);

        fixture.write_named("one.php", "<?php echo \"one\";");
        fixture.write_named("two.php", "<?php echo \"two\";");
        let first = cache
            .get_or_compile_script(&executor, fixture.input_named("one.php"))
            .expect("first compile");
        let second = cache
            .get_or_compile_script(&executor, fixture.input_named("two.php"))
            .expect("second compile");

        assert!(!first.hit);
        assert!(!second.hit);
        let stats = cache.cache_stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.entries_by_shard, vec![1]);
        assert_eq!(stats.evictions, 1);
    }

    #[test]
    fn compiled_script_cache_clear_removes_entries_without_resetting_metrics() {
        let fixture = CacheFixture::new("cache-clear");
        fixture.write("<?php echo \"clear\";");
        let executor = PhpExecutor::new();
        let cache = CompiledScriptCache::new(1);

        cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("compile before clear");
        cache.clear();

        let stats = cache.cache_stats();
        assert_eq!(stats.entries, 0);
        assert_eq!(stats.entries_by_shard, vec![0]);
        assert_eq!(stats.misses, 1);
    }

    #[test]
    fn compiled_script_cache_avoids_same_script_compile_stampede() {
        let fixture = CacheFixture::new("cache-stampede");
        fixture.write("<?php echo \"hot\";");
        let cache = Arc::new(CompiledScriptCache::new_with_limits(1, 16, Duration::ZERO));
        let compile_count = Arc::new(AtomicU64::new(0));
        let barrier = Arc::new(Barrier::new(4));
        let mut threads = Vec::new();

        for _ in 0..4 {
            let cache = Arc::clone(&cache);
            let compile_count = Arc::clone(&compile_count);
            let barrier = Arc::clone(&barrier);
            let input = fixture.input();
            threads.push(std::thread::spawn(move || {
                let executor = PhpExecutor::new();
                barrier.wait();
                cache
                    .get_or_compile_script_with(
                        &executor,
                        input,
                        |cache, executor, input, source| {
                            compile_count.fetch_add(1, Ordering::SeqCst);
                            std::thread::sleep(Duration::from_millis(25));
                            cache.compile_uncached(executor, input, source)
                        },
                    )
                    .expect("stampede compile")
            }));
        }

        let lookups: Vec<_> = threads
            .into_iter()
            .map(|thread| thread.join().expect("stampede thread"))
            .collect();

        assert_eq!(compile_count.load(Ordering::SeqCst), 1);
        assert_eq!(lookups.iter().filter(|lookup| !lookup.hit).count(), 1);
        assert_eq!(lookups.iter().filter(|lookup| lookup.hit).count(), 3);
        assert_eq!(cache.cache_stats().compile_in_progress, 0);
    }

    #[test]
    fn compiled_script_cache_invalidates_modified_script() {
        let fixture = CacheFixture::new("cache-stale");
        fixture.write("<?php echo \"one\";");
        let executor = PhpExecutor::new();
        let cache = CompiledScriptCache::new(1);

        let first = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("first compile");
        fixture.write("<?php echo \"two\";");
        let second = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("second compile");

        assert!(!first.hit);
        assert!(!second.hit);
        assert_eq!(cache.cache_stats().stale_invalidations, 1);
        assert_eq!(cache.cache_stats().entries, 1);
        let output = execute_cached_for_test(&executor, &second.compiled);
        assert_eq!(output.stdout, b"two");
    }

    #[test]
    fn compiled_script_cache_compile_error_does_not_poison_later_success() {
        let fixture = CacheFixture::new("cache-compile-error");
        fixture.write("<?php function {");
        let executor = PhpExecutor::new();
        let cache = CompiledScriptCache::new(1);

        assert!(matches!(
            cache.get_or_compile_script(&executor, fixture.input()),
            Err(PhpExecutionError::Compile(_))
        ));
        fixture.write("<?php echo \"ok\";");
        let lookup = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("successful compile after error");

        assert!(!lookup.hit);
        assert_eq!(cache.cache_stats().compile_errors, 1);
        assert_eq!(cache.cache_stats().entries, 1);
        let output = execute_cached_for_test(&executor, &lookup.compiled);
        assert_eq!(output.stdout, b"ok");
    }

    #[test]
    fn disabled_compiled_script_cache_always_compiles() {
        let fixture = CacheFixture::new("cache-disabled");
        fixture.write("<?php echo \"hi\";");
        let executor = PhpExecutor::new();
        let cache = CompiledScriptCache::disabled();

        let first = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("first compile");
        let second = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("second compile");

        assert!(!first.hit);
        assert!(!second.hit);
        assert_eq!(cache.cache_stats().hits, 0);
        assert_eq!(cache.cache_stats().misses, 2);
        assert_eq!(cache.cache_stats().entries, 0);
    }

    struct CacheFixture {
        path: PathBuf,
        root: PathBuf,
    }

    impl CacheFixture {
        fn new(name: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "phrust-executor-{name}-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir(&root).expect("create cache fixture root");
            let path = root.join("index.php");
            Self { path, root }
        }

        fn write(&self, source: &str) {
            std::fs::write(&self.path, source).expect("write cache fixture");
        }

        fn write_named(&self, name: &str, source: &str) {
            std::fs::write(self.root.join(name), source).expect("write named cache fixture");
        }

        fn input(&self) -> PhpScriptCacheInput {
            self.input_for_path(self.path.clone())
        }

        fn input_named(&self, name: &str) -> PhpScriptCacheInput {
            self.input_for_path(self.root.join(name))
        }

        fn input_for_path(&self, path: PathBuf) -> PhpScriptCacheInput {
            PhpScriptCacheInput {
                source_path: path.to_string_lossy().into_owned(),
                path,
                optimization_level: OptimizationLevel::O0,
            }
        }
    }

    impl Drop for CacheFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn execute_cached_for_test(
        executor: &PhpExecutor,
        compiled: &CompiledPhpScript,
    ) -> PhpExecutionOutput {
        executor.execute_compiled(
            compiled,
            PhpRequestExecutionInput {
                real_path: None,
                cwd: std::env::current_dir().expect("current directory"),
                include_roots: Vec::new(),
                runtime_context: RuntimeContext::controlled_cli("index.php", Vec::new()),
                collect_counters: false,
            },
        )
    }
}
