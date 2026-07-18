//! Bounded worker-owned cache above Region IR construction.
//!
//! The lower Cranelift code manager deduplicates machine-code emission, but its
//! key is reached only after authoritative Region IR has been rebuilt. Server
//! workers retain these immutable compile records so warm requests can reuse
//! the published handles without repeating that frontend work. Every entry
//! owns exactly one requested PHP function; foreign aliases are rejected.

use php_ir::FunctionId;
use php_jit::JitUnitCompileRecord;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{
    Arc, Condvar, Mutex, MutexGuard, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard,
};
use std::time::Instant;

const DEFAULT_NATIVE_COMPILE_CACHE_ENTRIES: usize = 4_096;
const DEFAULT_LOADED_NATIVE_UNIT_ENTRIES: usize = 4_096;
const DEFAULT_NATIVE_COMPILE_PARALLELISM: usize = 1;
const DEFAULT_NATIVE_COMPILE_QUEUE_LIMIT: usize = 64;

#[derive(Debug, Default)]
struct CompileLimitState {
    active: usize,
    foreground_queued: usize,
    background_queued: usize,
}

#[derive(Debug)]
struct ProcessCompileLimiter {
    maximum_parallel: usize,
    maximum_queue: usize,
    state: Mutex<CompileLimitState>,
    ready: Condvar,
}

impl ProcessCompileLimiter {
    fn from_environment() -> Self {
        let configured = |name: &str, fallback: usize| {
            std::env::var(name)
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(fallback)
        };
        Self {
            maximum_parallel: configured(
                "PHRUST_NATIVE_COMPILE_PARALLELISM",
                DEFAULT_NATIVE_COMPILE_PARALLELISM,
            ),
            maximum_queue: configured(
                "PHRUST_NATIVE_COMPILE_QUEUE_LIMIT",
                DEFAULT_NATIVE_COMPILE_QUEUE_LIMIT,
            ),
            state: Mutex::new(CompileLimitState::default()),
            ready: Condvar::new(),
        }
    }

    fn acquire(&self, background: bool) -> Result<ProcessCompilePermit<'_>, String> {
        let mut state = lock_unpoisoned(&self.state);
        let must_wait = |state: &CompileLimitState| {
            state.active >= self.maximum_parallel || (background && state.foreground_queued > 0)
        };
        if must_wait(&state) {
            let queued = state
                .foreground_queued
                .saturating_add(state.background_queued);
            if queued >= self.maximum_queue {
                return Err(format!(
                    "E_NATIVE_COMPILE_QUEUE_FULL: active={} queued={} limit={}",
                    state.active, queued, self.maximum_queue
                ));
            }
            if background {
                state.background_queued = state.background_queued.saturating_add(1);
            } else {
                state.foreground_queued = state.foreground_queued.saturating_add(1);
            }
            while must_wait(&state) {
                state = self
                    .ready
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            if background {
                state.background_queued = state.background_queued.saturating_sub(1);
            } else {
                state.foreground_queued = state.foreground_queued.saturating_sub(1);
            }
        }
        state.active = state.active.saturating_add(1);
        Ok(ProcessCompilePermit { limiter: self })
    }
}

struct ProcessCompilePermit<'a> {
    limiter: &'a ProcessCompileLimiter,
}

impl Drop for ProcessCompilePermit<'_> {
    fn drop(&mut self) {
        let mut state = lock_unpoisoned(&self.limiter.state);
        state.active = state.active.saturating_sub(1);
        self.limiter.ready.notify_all();
    }
}

fn process_compile_limiter() -> &'static ProcessCompileLimiter {
    static LIMITER: OnceLock<ProcessCompileLimiter> = OnceLock::new();
    LIMITER.get_or_init(ProcessCompileLimiter::from_environment)
}

/// Immutable cached artifact publication shared by every request in a process.
#[derive(Debug)]
pub(super) struct LoadedNativeUnit {
    _artifact: Arc<php_jit::NativeLoadedArtifact>,
    native_entries: Arc<BTreeMap<FunctionId, php_jit::JitFunctionHandle>>,
}

impl LoadedNativeUnit {
    fn new(artifact: php_jit::NativeLoadedArtifact) -> Result<Self, php_jit::NativeCacheError> {
        let artifact = Arc::new(artifact);
        let native_entries = artifact
            .image()
            .functions
            .iter()
            .map(|function| {
                let function_id = FunctionId::new(function.function_id);
                php_jit::JitFunctionHandle::from_cached_artifact(
                    Arc::clone(&artifact),
                    function_id,
                    artifact.region_metadata(function.function_id).cloned(),
                )
                .map(|handle| (function_id, handle))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        Ok(Self {
            _artifact: artifact,
            native_entries: Arc::new(native_entries),
        })
    }

    pub(super) fn native_entries(&self) -> &Arc<BTreeMap<FunctionId, php_jit::JitFunctionHandle>> {
        &self.native_entries
    }
}

#[derive(Debug, Default)]
pub(super) struct LoadedNativeUnitRegistry {
    units: RwLock<BTreeMap<String, Arc<LoadedNativeUnit>>>,
    hits: AtomicU64,
    maps: AtomicU64,
    entry_table_constructions: AtomicU64,
    mapped_executable_bytes: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct LoadedNativeUnitRegistryStats {
    pub(super) hits: u64,
    pub(super) maps: u64,
    pub(super) entry_table_constructions: u64,
    pub(super) mapped_executable_bytes: u64,
}

impl LoadedNativeUnitRegistry {
    /// Loads, validates, maps, and publishes one artifact identity at most once.
    pub(super) fn get_or_load(
        &self,
        identity: &php_jit::NativeCacheIdentity,
        load: impl FnOnce() -> Result<Option<php_jit::NativeLoadedArtifact>, php_jit::NativeCacheError>,
    ) -> Result<Option<Arc<LoadedNativeUnit>>, php_jit::NativeCacheError> {
        let key = identity.cache_key();
        if let Some(unit) = read_unpoisoned(&self.units).get(&key).cloned() {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(Some(unit));
        }
        // Serialize only the cold publication path. Recheck after acquiring
        // the writer so concurrent cold requests cannot map the same identity
        // twice; every warm request takes only a shared read lock.
        let mut units = write_unpoisoned(&self.units);
        if let Some(unit) = units.get(&key) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(Some(Arc::clone(unit)));
        }
        let Some(artifact) = load()? else {
            return Ok(None);
        };
        let unit = Arc::new(LoadedNativeUnit::new(artifact)?);
        self.maps.fetch_add(1, Ordering::Relaxed);
        self.entry_table_constructions
            .fetch_add(1, Ordering::Relaxed);
        self.mapped_executable_bytes.fetch_add(
            unit._artifact.mapped_executable_bytes() as u64,
            Ordering::Relaxed,
        );
        while units.len() >= DEFAULT_LOADED_NATIVE_UNIT_ENTRIES {
            let Some(retired) = units
                .iter()
                .find_map(|(key, unit)| (Arc::strong_count(unit) == 1).then(|| key.clone()))
            else {
                break;
            };
            units.remove(&retired);
        }
        units.insert(key, Arc::clone(&unit));
        Ok(Some(unit))
    }

    pub(super) fn stats(&self) -> LoadedNativeUnitRegistryStats {
        LoadedNativeUnitRegistryStats {
            hits: self.hits.load(Ordering::Relaxed),
            maps: self.maps.load(Ordering::Relaxed),
            entry_table_constructions: self.entry_table_constructions.load(Ordering::Relaxed),
            mapped_executable_bytes: self.mapped_executable_bytes.load(Ordering::Relaxed),
        }
    }
}

fn read_unpoisoned<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write_unpoisoned<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct NativeCompileCacheKey {
    unit_cache_identity: u64,
    function: u32,
    optimization_level: u8,
    external_signatures_hash: u64,
}

impl NativeCompileCacheKey {
    pub(super) const fn new(
        unit_cache_identity: u64,
        function: FunctionId,
        optimization_level: u8,
        external_signatures_hash: u64,
    ) -> Self {
        Self {
            unit_cache_identity,
            function: function.raw(),
            optimization_level,
            external_signatures_hash,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NativeCompileCacheDisposition {
    Hit,
    Miss,
    Wait,
}

impl NativeCompileCacheDisposition {
    pub(super) const fn compiled(self) -> bool {
        matches!(self, Self::Miss)
    }
}

/// Snapshot of the process-worker cache above Region IR lowering.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeCompileCacheStats {
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub insertions: u64,
    pub evictions: u64,
    pub compile_waits: u64,
    pub compile_failures: u64,
    pub compile_time_nanos: u64,
}

impl NativeCompileCacheStats {
    pub(super) fn saturating_delta(self, before: Self) -> Self {
        Self {
            entries: self.entries,
            hits: self.hits.saturating_sub(before.hits),
            misses: self.misses.saturating_sub(before.misses),
            insertions: self.insertions.saturating_sub(before.insertions),
            evictions: self.evictions.saturating_sub(before.evictions),
            compile_waits: self.compile_waits.saturating_sub(before.compile_waits),
            compile_failures: self
                .compile_failures
                .saturating_sub(before.compile_failures),
            compile_time_nanos: self
                .compile_time_nanos
                .saturating_sub(before.compile_time_nanos),
        }
    }
}

#[derive(Debug, Default)]
struct NativeCompileCacheMetrics {
    hits: u64,
    misses: u64,
    insertions: u64,
    evictions: u64,
    compile_waits: u64,
    compile_failures: u64,
    compile_time_nanos: u64,
}

#[derive(Debug)]
struct CompileWait {
    result: Mutex<Option<Result<Arc<[JitUnitCompileRecord]>, String>>>,
    ready: Condvar,
}

impl CompileWait {
    fn new() -> Self {
        Self {
            result: Mutex::new(None),
            ready: Condvar::new(),
        }
    }

    fn wait(&self) -> Result<Arc<[JitUnitCompileRecord]>, String> {
        let mut result = lock_unpoisoned(&self.result);
        while result.is_none() {
            result = self
                .ready
                .wait(result)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        match result.as_ref() {
            Some(result) => result.clone(),
            None => Err("native compile coordination ended without a result".to_owned()),
        }
    }

    fn publish(&self, result: Result<Arc<[JitUnitCompileRecord]>, String>) {
        *lock_unpoisoned(&self.result) = Some(result);
        self.ready.notify_all();
    }
}

#[derive(Debug, Default)]
struct NativeCompileCacheState {
    primary_entries: HashMap<NativeCompileCacheKey, Arc<[JitUnitCompileRecord]>>,
    primary_lru: VecDeque<NativeCompileCacheKey>,
    in_flight: HashMap<NativeCompileCacheKey, Arc<CompileWait>>,
    permanent_failures: HashMap<NativeCompileCacheKey, String>,
    metrics: NativeCompileCacheMetrics,
}

#[derive(Debug)]
pub(super) struct NativeCompileCache {
    capacity: usize,
    state: Mutex<NativeCompileCacheState>,
}

impl Default for NativeCompileCache {
    fn default() -> Self {
        Self::new(DEFAULT_NATIVE_COMPILE_CACHE_ENTRIES)
    }
}

impl NativeCompileCache {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            state: Mutex::new(NativeCompileCacheState::default()),
        }
    }

    pub(super) fn get_or_compile(
        &self,
        key: NativeCompileCacheKey,
        compile: impl FnOnce() -> Result<Vec<JitUnitCompileRecord>, String>,
    ) -> Result<(Arc<[JitUnitCompileRecord]>, NativeCompileCacheDisposition), String> {
        self.get_or_compile_with_priority(key, false, compile)
    }

    pub(super) fn get_or_compile_background(
        &self,
        key: NativeCompileCacheKey,
        compile: impl FnOnce() -> Result<Vec<JitUnitCompileRecord>, String>,
    ) -> Result<(Arc<[JitUnitCompileRecord]>, NativeCompileCacheDisposition), String> {
        self.get_or_compile_with_priority(key, true, compile)
    }

    fn get_or_compile_with_priority(
        &self,
        key: NativeCompileCacheKey,
        background: bool,
        compile: impl FnOnce() -> Result<Vec<JitUnitCompileRecord>, String>,
    ) -> Result<(Arc<[JitUnitCompileRecord]>, NativeCompileCacheDisposition), String> {
        let wait = {
            let mut state = lock_unpoisoned(&self.state);
            if let Some(records) = state.primary_entries.get(&key).cloned() {
                state.metrics.hits = state.metrics.hits.saturating_add(1);
                touch_lru(&mut state.primary_lru, key);
                return Ok((records, NativeCompileCacheDisposition::Hit));
            }
            if let Some(error) = state.permanent_failures.get(&key) {
                return Err(error.clone());
            }
            if let Some(wait) = state.in_flight.get(&key).cloned() {
                state.metrics.hits = state.metrics.hits.saturating_add(1);
                state.metrics.compile_waits = state.metrics.compile_waits.saturating_add(1);
                Some(wait)
            } else {
                state.metrics.misses = state.metrics.misses.saturating_add(1);
                let wait = Arc::new(CompileWait::new());
                state.in_flight.insert(key, Arc::clone(&wait));
                None
            }
        };

        if let Some(wait) = wait {
            return wait
                .wait()
                .map(|records| (records, NativeCompileCacheDisposition::Wait));
        }

        let compile_started = Instant::now();
        let result = process_compile_limiter()
            .acquire(background)
            .and_then(|_permit| compile())
            .and_then(|records| {
                if records.len() == 1 && records[0].function.raw() == key.function {
                    Ok(records)
                } else {
                    Err(format!(
                        "E_NATIVE_COMPILE_BREADTH: compile_miss({}) produced functions {:?}",
                        key.function,
                        records
                            .iter()
                            .map(|record| record.function.raw())
                            .collect::<Vec<_>>()
                    ))
                }
            })
            .map(Arc::<[JitUnitCompileRecord]>::from);
        let compile_time_nanos = compile_started
            .elapsed()
            .as_nanos()
            .min(u128::from(u64::MAX)) as u64;
        let wait = {
            let mut state = lock_unpoisoned(&self.state);
            let wait = state.in_flight.remove(&key);
            state.metrics.compile_time_nanos = state
                .metrics
                .compile_time_nanos
                .saturating_add(compile_time_nanos);
            if let Ok(records) = &result {
                insert_primary(&mut state, key, Arc::clone(records));
                state.metrics.insertions = state.metrics.insertions.saturating_add(1);
                evict_primary_entries(&mut state, self.capacity);
            } else {
                state.metrics.compile_failures = state.metrics.compile_failures.saturating_add(1);
                if let Err(error) = &result
                    && permanent_compile_failure(error)
                {
                    state.permanent_failures.insert(key, error.clone());
                }
            }
            wait
        };
        if let Some(wait) = wait {
            wait.publish(result.clone());
        }
        result.map(|records| (records, NativeCompileCacheDisposition::Miss))
    }

    pub(super) fn stats(&self) -> NativeCompileCacheStats {
        let state = lock_unpoisoned(&self.state);
        NativeCompileCacheStats {
            entries: state.primary_entries.len(),
            hits: state.metrics.hits,
            misses: state.metrics.misses,
            insertions: state.metrics.insertions,
            evictions: state.metrics.evictions,
            compile_waits: state.metrics.compile_waits,
            compile_failures: state.metrics.compile_failures,
            compile_time_nanos: state.metrics.compile_time_nanos,
        }
    }

    pub(super) fn contains(&self, key: NativeCompileCacheKey) -> bool {
        lock_unpoisoned(&self.state)
            .primary_entries
            .contains_key(&key)
    }
}

fn permanent_compile_failure(error: &str) -> bool {
    !error.starts_with("E_NATIVE_COMPILE_QUEUE_FULL:")
        && !error.contains("Cranelift code limit is exhausted")
        && !error.contains("allocation failed")
}

fn touch_lru(lru: &mut VecDeque<NativeCompileCacheKey>, key: NativeCompileCacheKey) {
    remove_lru(lru, key);
    lru.push_back(key);
}

fn remove_lru(lru: &mut VecDeque<NativeCompileCacheKey>, key: NativeCompileCacheKey) {
    if let Some(index) = lru.iter().position(|candidate| *candidate == key) {
        lru.remove(index);
    }
}

fn insert_primary(
    state: &mut NativeCompileCacheState,
    key: NativeCompileCacheKey,
    records: Arc<[JitUnitCompileRecord]>,
) {
    state.primary_entries.insert(key, records);
    touch_lru(&mut state.primary_lru, key);
}

fn evict_primary_entries(state: &mut NativeCompileCacheState, capacity: usize) {
    while state.primary_entries.len() > capacity {
        let Some(evicted) = state.primary_lru.pop_front() else {
            break;
        };
        if state.primary_entries.remove(&evicted).is_some() {
            state.metrics.evictions = state.metrics.evictions.saturating_add(1);
        }
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    fn key(unit: u64) -> NativeCompileCacheKey {
        NativeCompileCacheKey::new(unit, FunctionId::new(0), 0, 0)
    }

    fn record(function: u32) -> JitUnitCompileRecord {
        JitUnitCompileRecord {
            function: FunctionId::new(function),
            result: php_jit::JitCompileResult {
                status: php_jit::JitCompileStatus::Rejected {
                    reason: "test-only".to_owned(),
                },
                handle: None,
                diagnostics: Vec::new(),
                stats: php_jit::JitStats::default(),
            },
        }
    }

    #[test]
    fn foreground_compile_overtakes_queued_background_work() {
        let limiter = Arc::new(ProcessCompileLimiter {
            maximum_parallel: 1,
            maximum_queue: 4,
            state: Mutex::new(CompileLimitState::default()),
            ready: Condvar::new(),
        });
        let active = limiter.acquire(false).expect("initial foreground permit");
        let (order_tx, order_rx) = std::sync::mpsc::channel();

        let background_limiter = Arc::clone(&limiter);
        let background_tx = order_tx.clone();
        let background = thread::spawn(move || {
            let _permit = background_limiter.acquire(true).expect("background permit");
            background_tx.send("background").unwrap();
        });
        while lock_unpoisoned(&limiter.state).background_queued == 0 {
            thread::yield_now();
        }

        let foreground_limiter = Arc::clone(&limiter);
        let foreground = thread::spawn(move || {
            let _permit = foreground_limiter
                .acquire(false)
                .expect("foreground permit");
            order_tx.send("foreground").unwrap();
        });
        while lock_unpoisoned(&limiter.state).foreground_queued == 0 {
            thread::yield_now();
        }

        drop(active);
        assert_eq!(
            order_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            "foreground"
        );
        foreground.join().unwrap();
        assert_eq!(
            order_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            "background"
        );
        background.join().unwrap();
    }

    #[test]
    fn cache_is_bounded_and_uses_lru_eviction() {
        let cache = NativeCompileCache::new(2);
        let compile = || Ok(vec![record(0)]);
        cache.get_or_compile(key(1), compile).unwrap();
        cache.get_or_compile(key(2), compile).unwrap();
        cache.get_or_compile(key(1), compile).unwrap();
        cache.get_or_compile(key(3), compile).unwrap();

        let (_, disposition) = cache.get_or_compile(key(2), compile).unwrap();
        assert_eq!(disposition, NativeCompileCacheDisposition::Miss);
        let stats = cache.stats();
        assert_eq!(stats.entries, 2);
        assert_eq!(stats.evictions, 2);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 4);
    }

    #[test]
    fn concurrent_same_key_compiles_once() {
        let cache = Arc::new(NativeCompileCache::new(2));
        let compiles = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let handles = (0..8)
            .map(|_| {
                let cache = Arc::clone(&cache);
                let compiles = Arc::clone(&compiles);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    cache
                        .get_or_compile(key(1), || {
                            compiles.fetch_add(1, Ordering::Relaxed);
                            thread::sleep(Duration::from_millis(25));
                            Ok(vec![record(0)])
                        })
                        .unwrap()
                        .1
                })
            })
            .collect::<Vec<_>>();
        let dispositions = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(compiles.load(Ordering::Relaxed), 1);
        assert!(dispositions.contains(&NativeCompileCacheDisposition::Miss));
        assert!(dispositions.contains(&NativeCompileCacheDisposition::Wait));
        assert_eq!(
            dispositions
                .iter()
                .filter(|disposition| **disposition == NativeCompileCacheDisposition::Wait)
                .count(),
            7
        );
        assert_eq!(cache.stats().compile_waits, 7);
    }

    #[test]
    fn compile_breadth_violation_is_rejected_and_cached() {
        let cache = NativeCompileCache::new(8);
        let first = cache
            .get_or_compile(key(7), || Ok(vec![record(0), record(3)]))
            .unwrap_err();
        let second = cache
            .get_or_compile(key(7), || panic!("permanent breadth failure recompiled"))
            .unwrap_err();

        assert!(first.starts_with("E_NATIVE_COMPILE_BREADTH:"));
        assert_eq!(first, second);
        assert_eq!(cache.stats().entries, 0);
        assert_eq!(cache.stats().compile_failures, 1);
    }

    #[test]
    fn different_functions_require_independent_compile_groups() {
        let cache = NativeCompileCache::new(2);
        cache
            .get_or_compile(key(1), || Ok(vec![record(0)]))
            .unwrap();
        let function_three = NativeCompileCacheKey::new(1, FunctionId::new(3), 0, 0);
        cache
            .get_or_compile(function_three, || Ok(vec![record(3)]))
            .unwrap();

        let (_, disposition) = cache
            .get_or_compile(key(1), || panic!("requested function recompiled"))
            .unwrap();

        assert_eq!(disposition, NativeCompileCacheDisposition::Hit);
        assert_eq!(cache.stats().entries, 2);
    }
}
