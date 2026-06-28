//! Local include/require loader for the runtime VM MVP.

use crate::compiled_unit::CompiledUnit;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::UNIX_EPOCH;

/// Result of loading one include target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedInclude {
    /// Canonical path used for once tracking and source maps.
    pub canonical_path: PathBuf,
    /// PHP source text.
    pub source: String,
}

/// Metadata fingerprint used to validate cached include-path resolutions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncludePathFileFingerprint {
    pub len: u64,
    pub modified_unix_nanos: Option<u128>,
    pub readonly: bool,
}

/// Result of resolving one include target without loading its contents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedIncludePath {
    /// Canonical path used for once tracking and source maps.
    pub canonical_path: PathBuf,
    /// File metadata fingerprint used to invalidate stale path resolutions.
    pub fingerprint: IncludePathFileFingerprint,
}

/// Shared process-local include cache for resolution and compiled include units.
#[derive(Debug)]
pub struct IncludeCache {
    resolution_shards: Vec<Mutex<HashMap<IncludeResolutionKey, ResolvedIncludePath>>>,
    compile_shards: Vec<Mutex<HashMap<CompiledIncludeKey, Arc<CompiledUnit>>>>,
    compile_locks: Vec<IncludeCompileLockShard>,
    stats: IncludeCacheCounters,
}

impl IncludeCache {
    /// Creates a cache with at least one shard.
    #[must_use]
    pub fn new(shards: usize) -> Self {
        let shard_count = shards.max(1);
        Self {
            resolution_shards: (0..shard_count)
                .map(|_| Mutex::new(HashMap::new()))
                .collect(),
            compile_shards: (0..shard_count)
                .map(|_| Mutex::new(HashMap::new()))
                .collect(),
            compile_locks: (0..shard_count)
                .map(|_| IncludeCompileLockShard::default())
                .collect(),
            stats: IncludeCacheCounters::default(),
        }
    }

    /// Resolves an include path through a shared process-local cache.
    pub fn resolve_with_include_path(
        &self,
        loader: &IncludeLoader,
        including_file: Option<&Path>,
        path: &str,
        include_path: &[PathBuf],
        cwd: Option<&Path>,
    ) -> Result<ResolvedIncludePath, String> {
        let key = IncludeResolutionKey::new(loader, including_file, path, include_path, cwd);
        let shard_index = self.resolution_shard_index(&key);
        {
            let mut shard = self.resolution_shards[shard_index]
                .lock()
                .expect("include resolution cache shard mutex poisoned");
            if let Some(resolved) = shard.get(&key).cloned() {
                match include_path_file_fingerprint(&resolved.canonical_path) {
                    Ok(current) if current == resolved.fingerprint => {
                        self.stats.resolution_hits.fetch_add(1, Ordering::Relaxed);
                        return Ok(resolved);
                    }
                    Ok(_) | Err(_) => {
                        shard.remove(&key);
                        self.stats
                            .stale_invalidations
                            .fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
        self.stats.resolution_misses.fetch_add(1, Ordering::Relaxed);
        let resolved = loader.resolve_with_include_path(including_file, path, include_path, cwd)?;
        let mut shard = self.resolution_shards[shard_index]
            .lock()
            .expect("include resolution cache shard mutex poisoned");
        shard.entry(key).or_insert_with(|| resolved.clone());
        Ok(resolved)
    }

    /// Returns a compiled include unit for a resolved path, compiling on miss.
    pub fn get_or_compile_include(
        &self,
        loader: &IncludeLoader,
        resolved: &ResolvedIncludePath,
    ) -> Result<Arc<CompiledUnit>, String> {
        loop {
            let key = CompiledIncludeKey::new(resolved);
            let shard_index = self.compile_shard_index(&key);
            {
                let mut shard = self.compile_shards[shard_index]
                    .lock()
                    .expect("compiled include cache shard mutex poisoned");
                let stale = remove_stale_compiled_include_entries(&mut shard, &key);
                if stale > 0 {
                    self.stats
                        .stale_invalidations
                        .fetch_add(stale as u64, Ordering::Relaxed);
                }
                if let Some(compiled) = shard.get(&key) {
                    self.stats.compile_hits.fetch_add(1, Ordering::Relaxed);
                    return Ok(Arc::clone(compiled));
                }
            }

            let Some(_permit) = self.try_begin_compile(&resolved.canonical_path) else {
                self.wait_for_compile(&resolved.canonical_path);
                continue;
            };

            {
                let shard = self.compile_shards[shard_index]
                    .lock()
                    .expect("compiled include cache shard mutex poisoned");
                if let Some(compiled) = shard.get(&key) {
                    self.stats.compile_hits.fetch_add(1, Ordering::Relaxed);
                    return Ok(Arc::clone(compiled));
                }
            }

            self.stats.compile_misses.fetch_add(1, Ordering::Relaxed);
            let compiled = match compile_include(loader, resolved) {
                Ok(compiled) => {
                    let compiled = Arc::new(compiled);
                    let mut shard = self.compile_shards[shard_index]
                        .lock()
                        .expect("compiled include cache shard mutex poisoned");
                    Ok(Arc::clone(shard.entry(key).or_insert(compiled)))
                }
                Err(message) => {
                    self.stats.compile_errors.fetch_add(1, Ordering::Relaxed);
                    Err(message)
                }
            }?;
            return Ok(compiled);
        }
    }

    /// Clears cached include resolutions and compiled include units.
    pub fn clear(&self) {
        for shard in &self.resolution_shards {
            shard
                .lock()
                .expect("include resolution cache shard mutex poisoned")
                .clear();
        }
        for shard in &self.compile_shards {
            shard
                .lock()
                .expect("compiled include cache shard mutex poisoned")
                .clear();
        }
    }

    /// Returns current cache counters.
    #[must_use]
    pub fn cache_stats(&self) -> IncludeCacheStats {
        IncludeCacheStats {
            resolution_hits: self.stats.resolution_hits.load(Ordering::Relaxed),
            resolution_misses: self.stats.resolution_misses.load(Ordering::Relaxed),
            compile_hits: self.stats.compile_hits.load(Ordering::Relaxed),
            compile_misses: self.stats.compile_misses.load(Ordering::Relaxed),
            stale_invalidations: self.stats.stale_invalidations.load(Ordering::Relaxed),
            compile_errors: self.stats.compile_errors.load(Ordering::Relaxed),
        }
    }

    fn resolution_shard_index(&self, key: &IncludeResolutionKey) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.resolution_shards.len()
    }

    fn compile_shard_index(&self, key: &CompiledIncludeKey) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.compile_shards.len()
    }

    fn compile_lock_shard_index(&self, path: &Path) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        path.hash(&mut hasher);
        (hasher.finish() as usize) % self.compile_locks.len()
    }

    fn try_begin_compile(&self, path: &Path) -> Option<IncludeCompilePermit<'_>> {
        let shard = &self.compile_locks[self.compile_lock_shard_index(path)];
        let mut in_progress = shard
            .in_progress
            .lock()
            .expect("include compile lock shard mutex poisoned");
        if !in_progress.insert(path.to_path_buf()) {
            return None;
        }
        Some(IncludeCompilePermit {
            shard,
            path: path.to_path_buf(),
        })
    }

    fn wait_for_compile(&self, path: &Path) {
        let shard = &self.compile_locks[self.compile_lock_shard_index(path)];
        let mut in_progress = shard
            .in_progress
            .lock()
            .expect("include compile lock shard mutex poisoned");
        while in_progress.contains(path) {
            in_progress = shard
                .condvar
                .wait(in_progress)
                .expect("include compile lock shard mutex poisoned");
        }
    }
}

impl Default for IncludeCache {
    fn default() -> Self {
        Self::new(default_include_cache_shards())
    }
}

/// Snapshot of shared include-cache counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IncludeCacheStats {
    pub resolution_hits: u64,
    pub resolution_misses: u64,
    pub compile_hits: u64,
    pub compile_misses: u64,
    pub stale_invalidations: u64,
    pub compile_errors: u64,
}

#[derive(Debug, Default)]
struct IncludeCacheCounters {
    resolution_hits: AtomicU64,
    resolution_misses: AtomicU64,
    compile_hits: AtomicU64,
    compile_misses: AtomicU64,
    stale_invalidations: AtomicU64,
    compile_errors: AtomicU64,
}

#[derive(Debug, Default)]
struct IncludeCompileLockShard {
    in_progress: Mutex<HashSet<PathBuf>>,
    condvar: Condvar,
}

struct IncludeCompilePermit<'a> {
    shard: &'a IncludeCompileLockShard,
    path: PathBuf,
}

impl Drop for IncludeCompilePermit<'_> {
    fn drop(&mut self) {
        let mut in_progress = self
            .shard
            .in_progress
            .lock()
            .expect("include compile lock shard mutex poisoned");
        in_progress.remove(&self.path);
        self.shard.condvar.notify_all();
    }
}

/// Root-constrained local include loader.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncludeLoader {
    allowed_roots: Vec<PathBuf>,
}

impl IncludeLoader {
    /// Creates a loader with canonicalized allowed roots.
    pub fn new(roots: impl IntoIterator<Item = PathBuf>) -> Result<Self, String> {
        let mut allowed_roots = Vec::new();
        for root in roots {
            let canonical = fs::canonicalize(&root)
                .map_err(|error| format!("E_PHP_VM_INCLUDE_ROOT: {}: {error}", root.display()))?;
            if !allowed_roots.contains(&canonical) {
                allowed_roots.push(canonical);
            }
        }
        Ok(Self { allowed_roots })
    }

    /// Creates a loader that permits files under `root`.
    pub fn for_root(root: impl Into<PathBuf>) -> Result<Self, String> {
        Self::new([root.into()])
    }

    /// Returns configured roots.
    #[must_use]
    pub fn allowed_roots(&self) -> &[PathBuf] {
        &self.allowed_roots
    }

    /// Loads a file after resolving it against the including file directory and
    /// checking that the canonical path remains within an allowed root.
    pub fn load(&self, including_file: Option<&Path>, path: &str) -> Result<LoadedInclude, String> {
        self.load_with_include_path(including_file, path, &[], None)
    }

    /// Loads a file using PHP-style include_path candidates for relative paths,
    /// then applies the same allowed-root check as `load`.
    pub fn load_with_include_path(
        &self,
        including_file: Option<&Path>,
        path: &str,
        include_path: &[PathBuf],
        cwd: Option<&Path>,
    ) -> Result<LoadedInclude, String> {
        let resolved = self.resolve_with_include_path(including_file, path, include_path, cwd)?;
        self.load_resolved(resolved.canonical_path)
    }

    /// Resolves a file using PHP-style include_path candidates without reading
    /// or executing file contents.
    pub fn resolve_with_include_path(
        &self,
        including_file: Option<&Path>,
        path: &str,
        include_path: &[PathBuf],
        cwd: Option<&Path>,
    ) -> Result<ResolvedIncludePath, String> {
        if self.allowed_roots.is_empty() {
            return Err(
                "E_PHP_VM_INCLUDE_DISABLED: include loader has no allowed roots".to_owned(),
            );
        }
        if path.contains("://") {
            return Err(format!(
                "E_PHP_VM_INCLUDE_UNSUPPORTED_SCHEME: stream include `{path}` is not supported"
            ));
        }
        let raw = Path::new(path);
        let mut candidates = Vec::new();
        if raw.is_absolute() {
            candidates.push(raw.to_path_buf());
        } else {
            let base = including_file.and_then(Path::parent);
            for entry in include_path {
                candidates.push(resolve_include_path_entry(base, cwd, entry).join(raw));
            }
            if let Some(parent) = base {
                candidates.push(parent.join(raw));
            }
            if let Some(cwd) = cwd {
                candidates.push(cwd.join(raw));
            }
            candidates.push(raw.to_path_buf());
        }
        let mut last_error = None;
        let mut canonical = None;
        for candidate in candidates {
            match fs::canonicalize(&candidate) {
                Ok(path) => {
                    canonical = Some(path);
                    break;
                }
                Err(error) => {
                    last_error = Some(format!(
                        "E_PHP_VM_INCLUDE_MISSING: {}: {error}",
                        candidate.display()
                    ));
                }
            }
        }
        let canonical = canonical.ok_or_else(|| {
            last_error.unwrap_or_else(|| format!("E_PHP_VM_INCLUDE_MISSING: {path}: not found"))
        })?;
        if !self
            .allowed_roots
            .iter()
            .any(|root| canonical.starts_with(root))
        {
            return Err(format!(
                "E_PHP_VM_INCLUDE_OUTSIDE_ROOT: {} is outside allowed include roots",
                canonical.display()
            ));
        }
        let fingerprint = include_path_file_fingerprint(&canonical)?;
        Ok(ResolvedIncludePath {
            canonical_path: canonical,
            fingerprint,
        })
    }

    /// Loads a previously resolved canonical include path, rechecking that the
    /// path remains inside an allowed root.
    pub fn load_resolved(&self, canonical: PathBuf) -> Result<LoadedInclude, String> {
        if !self
            .allowed_roots
            .iter()
            .any(|root| canonical.starts_with(root))
        {
            return Err(format!(
                "E_PHP_VM_INCLUDE_OUTSIDE_ROOT: {} is outside allowed include roots",
                canonical.display()
            ));
        }
        let source = fs::read_to_string(&canonical)
            .map_err(|error| format!("E_PHP_VM_INCLUDE_READ: {}: {error}", canonical.display()))?;
        Ok(LoadedInclude {
            canonical_path: canonical,
            source,
        })
    }
}

pub fn include_path_file_fingerprint(path: &Path) -> Result<IncludePathFileFingerprint, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("E_PHP_VM_INCLUDE_METADATA: {}: {error}", path.display()))?;
    let modified_unix_nanos = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos());
    Ok(IncludePathFileFingerprint {
        len: metadata.len(),
        modified_unix_nanos,
        readonly: metadata.permissions().readonly(),
    })
}

fn resolve_include_path_entry(base: Option<&Path>, cwd: Option<&Path>, entry: &Path) -> PathBuf {
    if entry.is_absolute() {
        return entry.to_path_buf();
    }
    if entry == Path::new(".") {
        if let Some(base) = base {
            return base.to_path_buf();
        }
        if let Some(cwd) = cwd {
            return cwd.to_path_buf();
        }
    }
    if let Some(cwd) = cwd {
        return cwd.join(entry);
    }
    if let Some(base) = base {
        return base.join(entry);
    }
    entry.to_path_buf()
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct IncludeResolutionKey {
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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CompiledIncludeKey {
    canonical_path: PathBuf,
    len: u64,
    modified_unix_nanos: Option<u128>,
    readonly: bool,
    compiler_version: &'static str,
    debug_assertions: bool,
}

impl CompiledIncludeKey {
    fn new(resolved: &ResolvedIncludePath) -> Self {
        Self {
            canonical_path: resolved.canonical_path.clone(),
            len: resolved.fingerprint.len,
            modified_unix_nanos: resolved.fingerprint.modified_unix_nanos,
            readonly: resolved.fingerprint.readonly,
            compiler_version: env!("CARGO_PKG_VERSION"),
            debug_assertions: cfg!(debug_assertions),
        }
    }
}

fn remove_stale_compiled_include_entries(
    shard: &mut HashMap<CompiledIncludeKey, Arc<CompiledUnit>>,
    key: &CompiledIncludeKey,
) -> usize {
    let before = shard.len();
    shard.retain(|existing, _| existing.canonical_path != key.canonical_path || existing == key);
    before.saturating_sub(shard.len())
}

fn compile_include(
    loader: &IncludeLoader,
    resolved: &ResolvedIncludePath,
) -> Result<CompiledUnit, String> {
    let loaded = loader.load_resolved(resolved.canonical_path.clone())?;
    let frontend = php_semantics::analyze_source(&loaded.source);
    if frontend.has_errors() {
        return Err(format!(
            "E_PHP_VM_INCLUDE_COMPILE_ERROR: {} failed frontend analysis",
            loaded.canonical_path.display()
        ));
    }
    let lowering = php_ir::lower_frontend_result(
        &frontend,
        php_ir::LoweringOptions {
            source_path: loaded.canonical_path.to_string_lossy().into_owned(),
            source_text: Some(loaded.source),
            ..php_ir::LoweringOptions::default()
        },
    );
    if !lowering.diagnostics.is_empty() || lowering.verification.is_err() {
        return Err(format!(
            "E_PHP_VM_INCLUDE_COMPILE_ERROR: {} failed IR lowering",
            loaded.canonical_path.display()
        ));
    }
    Ok(CompiledUnit::new(lowering.unit))
}

fn default_include_cache_shards() -> usize {
    std::thread::available_parallelism().map_or(16, |count| count.get().clamp(1, 64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn include_cache_records_resolution_hits_and_misses() {
        let fixture = IncludeCacheFixture::new("resolution");
        fixture.write("lib.php", "<?php echo 'lib';\n");
        let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
        let cache = IncludeCache::new(1);

        let first = cache
            .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
            .expect("first resolve");
        let second = cache
            .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
            .expect("second resolve");

        assert_eq!(first, second);
        assert_eq!(cache.cache_stats().resolution_misses, 1);
        assert_eq!(cache.cache_stats().resolution_hits, 1);
    }

    #[test]
    fn include_cache_invalidates_compiled_include_after_file_edit() {
        let fixture = IncludeCacheFixture::new("compiled-stale");
        fixture.write("lib.php", "<?php echo 'one';\n");
        let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
        let cache = IncludeCache::new(1);

        let first_resolved = cache
            .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
            .expect("first resolve");
        let first = cache
            .get_or_compile_include(&loader, &first_resolved)
            .expect("first compile");
        fixture.write("lib.php", "<?php echo 'two';\n");
        let second_resolved = cache
            .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
            .expect("second resolve");
        let second = cache
            .get_or_compile_include(&loader, &second_resolved)
            .expect("second compile");

        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(cache.cache_stats().compile_misses, 2);
        assert!(cache.cache_stats().stale_invalidations >= 1);
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
            fs::write(self.root.join(name), source).expect("write include cache fixture file");
        }
    }

    impl Drop for IncludeCacheFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
