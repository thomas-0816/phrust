//! Local include/require loader for the runtime VM MVP.

use crate::compiled_unit::CompiledUnit;
use crate::error::VmError;
use php_diagnostics::{
    DiagnosticEnvelope, DiagnosticLayer, DiagnosticPhase, DiagnosticSeverity, DiagnosticSuggestion,
};
use php_optimizer::{OptimizationLevel, PassContext, PassPipeline};
use php_runtime::{FilesystemCapabilities, phar};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::Read as _;
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

#[derive(Clone, Debug)]
struct ValidatedLoadedInclude {
    loaded: LoadedInclude,
    identity: OpenedSourceIdentity,
    bytes_hashed: u64,
}

/// Identity of the exact bytes read from one stable opened file generation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct OpenedSourceIdentity {
    generation: IncludePathFileFingerprint,
    content_hash: u64,
}

/// Metadata fingerprint used to validate cached include-path resolutions.
///
/// `inode`/`device` capture filesystem identity where the platform exposes it
/// (Unix), so an atomic replace or symlink swap that preserves `len`+`mtime`
/// still invalidates the cached resolution. They are `None` on platforms that
/// do not expose identity; because the whole struct is compared by equality,
/// a missing identity only ever matches another missing identity — it never
/// widens reuse (fail-closed).
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct IncludePathFileFingerprint {
    pub len: u64,
    pub modified_unix_nanos: Option<u128>,
    /// Metadata-change time where the platform exposes it. Unlike mtime, this
    /// changes when callers rewrite bytes and restore the visible mtime.
    pub changed_unix_nanos: Option<i128>,
    pub readonly: bool,
    pub inode: Option<u64>,
    pub device: Option<u64>,
}

impl IncludePathFileFingerprint {
    fn has_reliable_generation(&self) -> bool {
        self.inode.is_some() && self.device.is_some() && self.changed_unix_nanos.is_some()
    }
}

/// Portable directory version for the include/autoload graph.
///
/// Captures the directory's modification time and filesystem identity where
/// the platform exposes them. Compared by equality: a `None` field only ever
/// matches another `None`, so missing platform data narrows reuse instead of
/// widening it (fail-closed). Directory versions are metadata and counters
/// only today — negative include-path caching stays disabled until a
/// validated policy consumes them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IncludeDirectoryVersion {
    pub modified_unix_nanos: Option<u128>,
    pub inode: Option<u64>,
    pub device: Option<u64>,
}

/// Observes a directory's current version. `None` means the directory could
/// not be inspected; callers must treat that as "unvalidated", never as a
/// match.
#[must_use]
pub fn include_directory_version(dir: &Path) -> Option<IncludeDirectoryVersion> {
    let metadata = fs::metadata(dir).ok()?;
    if !metadata.is_dir() {
        return None;
    }
    let modified_unix_nanos = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos());
    let (inode, device) = file_identity(&metadata);
    Some(IncludeDirectoryVersion {
        modified_unix_nanos,
        inode,
        device,
    })
}

/// Result of resolving one include target without loading its contents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedIncludePath {
    /// Canonical path used for once tracking and source maps.
    pub canonical_path: PathBuf,
    /// Candidate path whose canonical target produced `canonical_path`.
    /// Re-canonicalizing this path detects final-component and ancestor
    /// symlink swaps before a cached resolution is reused. Phar entries do
    /// not have a local candidate path.
    pub resolution_path: Option<PathBuf>,
    /// File metadata fingerprint used to invalidate stale path resolutions.
    pub fingerprint: IncludePathFileFingerprint,
    /// Version of the canonical path's parent directory at resolve time.
    /// Metadata only: revalidation compares it for the directory-version
    /// counters without changing whether the resolution is accepted. `None`
    /// (phar entries, uninspectable directories) always counts as a miss.
    pub directory_version: Option<IncludeDirectoryVersion>,
}

/// Per-shard capacity bound for negative include-path entries. Autoloader
/// probing can generate unbounded distinct missing paths (class names can be
/// user-influenced), so growth must be capped; overflow skips installation
/// and counts `negative_cache_blocked_capacity` instead.
const NEGATIVE_INCLUDE_CACHE_SHARD_CAPACITY: usize = 1024;

/// Process-global enable for directory-version-validated negative
/// include-path caching, read once. Default **on**: a cached miss is only
/// served while the directory version of every probed candidate's parent is
/// byte-identical to install time, so a file appearing anywhere the original
/// probe looked invalidates the entry (a directory's mtime/identity changes
/// when entries are created or removed). Set `PHRUST_NEGATIVE_INCLUDE_CACHE`
/// to a falsey value (`0`, `off`, `false`, `no`, or empty) to disable.
#[must_use]
pub fn negative_include_cache_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| match std::env::var("PHRUST_NEGATIVE_INCLUDE_CACHE") {
        Ok(value) => !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no" | ""
        ),
        Err(_) => true,
    })
}

/// Directory-version guards for a candidate parent, captured by the loader
/// immediately BEFORE probing that candidate so a file created concurrently
/// with (or after) the probe changes the version and invalidates the guard.
/// `None` means the miss is not cacheable — an unversionable/relative parent,
/// a non-`NotFound` (transient) failure, or a symlink candidate whose target
/// could appear in a directory these guards do not cover.
pub(crate) struct NegativeProbeTrace {
    guards: Option<Vec<NegativeProbeGuard>>,
}

impl NegativeProbeTrace {
    fn uncacheable() -> Self {
        Self { guards: None }
    }
}

/// One probed missing candidate and the parent-directory version observed
/// before the probe. The candidate path itself is rechecked on replay so files
/// that appear on filesystems with coarse directory-mtime granularity still
/// invalidate the cached miss.
#[derive(Clone, Debug)]
struct NegativeProbeGuard {
    candidate: PathBuf,
    directory: PathBuf,
    directory_version: IncludeDirectoryVersion,
}

/// One cached missing-include resolution: the deterministic error to replay
/// plus the directory-version guards that must all still match for the entry
/// to be served.
#[derive(Clone, Debug)]
struct NegativeIncludeEntry {
    error: VmError,
    guards: Vec<NegativeProbeGuard>,
}

impl NegativeIncludeEntry {
    fn is_still_valid(&self) -> bool {
        self.guards.iter().all(|guard| {
            fs::symlink_metadata(&guard.candidate).is_err()
                && include_directory_version(&guard.directory)
                    .is_some_and(|current| current == guard.directory_version)
        })
    }
}

/// Shared process-local include cache for resolution and compiled include units.
#[derive(Debug)]
pub struct IncludeCache {
    resolution_shards: Vec<Mutex<HashMap<IncludeResolutionKey, ResolvedIncludePath>>>,
    negative_shards: Vec<Mutex<HashMap<IncludeResolutionKey, NegativeIncludeEntry>>>,
    compile_shards: Vec<Mutex<HashMap<CompiledIncludeKey, Arc<CompiledUnit>>>>,
    compile_lookup_shards: Vec<Mutex<HashMap<CompiledIncludeLookupKey, CompiledIncludeKey>>>,
    compile_locks: Vec<IncludeCompileLockShard>,
    stats: IncludeCacheCounters,
    /// Deployment-root fingerprint installed by production-mode server runs.
    /// Metadata only: revalidation feeds the `deployment_fingerprint_*`
    /// counters without changing any cache decision.
    deployment_root: Mutex<Option<DeploymentRootFingerprint>>,
    /// Last Composer map fingerprint observed by a request, for cross-request
    /// staleness attribution. `Some(None)` records "observed, no map found".
    composer_last_fingerprint: Mutex<Option<Option<String>>>,
}

/// Cross-request transition of the observed Composer map fingerprint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerFingerprintTransition {
    /// First observation in this process, or unchanged since the last one.
    Unchanged,
    /// The fingerprint differs from the previous request's observation — the
    /// deployment's autoload maps changed while the process was running.
    Changed,
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
            negative_shards: (0..shard_count)
                .map(|_| Mutex::new(HashMap::new()))
                .collect(),
            compile_shards: (0..shard_count)
                .map(|_| Mutex::new(HashMap::new()))
                .collect(),
            compile_lookup_shards: (0..shard_count)
                .map(|_| Mutex::new(HashMap::new()))
                .collect(),
            compile_locks: (0..shard_count)
                .map(|_| IncludeCompileLockShard::default())
                .collect(),
            stats: IncludeCacheCounters::default(),
            deployment_root: Mutex::new(None),
            composer_last_fingerprint: Mutex::new(None),
        }
    }

    /// Installs the deployment-root fingerprint for this process. Counts
    /// `deployment_fingerprint_present` when the root was observable and
    /// `deployment_fingerprint_missing` otherwise; a `None` fingerprint keeps
    /// the slot empty so later revalidations keep counting `missing`.
    pub fn set_deployment_root_fingerprint(&self, fingerprint: Option<DeploymentRootFingerprint>) {
        match &fingerprint {
            Some(_) => {
                self.stats
                    .deployment_fingerprint_present
                    .fetch_add(1, Ordering::Relaxed);
            }
            None => {
                self.stats
                    .deployment_fingerprint_missing
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        if let Ok(mut slot) = self.deployment_root.lock() {
            *slot = fingerprint;
        }
    }

    /// Re-observes the deployment root's directory version and counts
    /// `deployment_fingerprint_stale` when it no longer matches the installed
    /// fingerprint. Metadata only — no cache entries are invalidated here.
    pub fn revalidate_deployment_root(&self) {
        let Ok(slot) = self.deployment_root.lock() else {
            return;
        };
        let Some(fingerprint) = slot.as_ref() else {
            return;
        };
        let current = include_directory_version(&fingerprint.canonical_root);
        let matches = match (&fingerprint.directory_version, &current) {
            (Some(stored), Some(current)) => stored == current,
            _ => false,
        };
        if !matches {
            self.stats
                .deployment_fingerprint_stale
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Records the Composer map fingerprint a request observed and reports
    /// whether it changed since the previous request in this process.
    pub fn note_composer_fingerprint(
        &self,
        current: Option<&str>,
    ) -> ComposerFingerprintTransition {
        let Ok(mut last) = self.composer_last_fingerprint.lock() else {
            return ComposerFingerprintTransition::Unchanged;
        };
        let transition = match last.as_ref() {
            Some(previous) if previous.as_deref() != current => {
                self.stats
                    .composer_fingerprint_stale
                    .fetch_add(1, Ordering::Relaxed);
                ComposerFingerprintTransition::Changed
            }
            _ => ComposerFingerprintTransition::Unchanged,
        };
        *last = Some(current.map(str::to_owned));
        transition
    }

    /// Resolves an include path through a shared process-local cache.
    pub fn resolve_with_include_path(
        &self,
        loader: &IncludeLoader,
        including_file: Option<&Path>,
        path: &str,
        include_path: &[PathBuf],
        cwd: Option<&Path>,
    ) -> Result<ResolvedIncludePath, VmError> {
        let key = IncludeResolutionKey::new(loader, including_file, path, include_path, cwd);
        let shard_index = self.resolution_shard_index(&key);
        if let Some(resolved) = {
            let shard = self.resolution_shards[shard_index]
                .lock()
                .map_err(|_| include_cache_lock_error("resolution", "lookup"))?;
            shard.get(&key).cloned()
        } {
            let target_is_current = resolution_path_targets(
                resolved.resolution_path.as_deref(),
                &resolved.canonical_path,
            );
            match include_path_file_fingerprint(&resolved.canonical_path) {
                Ok(current) if target_is_current && current == resolved.fingerprint => {
                    self.stats.resolution_hits.fetch_add(1, Ordering::Relaxed);
                    self.observe_directory_version(&resolved);
                    return Ok(resolved);
                }
                Ok(_) | Err(_) => {
                    let mut shard = self.resolution_shards[shard_index]
                        .lock()
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
        let mut shard = self.resolution_shards[shard_index]
            .lock()
            .map_err(|_| include_cache_lock_error("resolution", "insert"))?;
        shard.entry(key).or_insert_with(|| resolved.clone());
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
            let shard = self.negative_shards[shard_index].lock().ok()?;
            shard.get(key)?.clone()
        };
        if entry.is_still_valid() {
            self.stats
                .negative_cache_hits
                .fetch_add(1, Ordering::Relaxed);
            return Some(entry.error);
        }
        if let Ok(mut shard) = self.negative_shards[shard_index].lock() {
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
        let Ok(mut shard) = self.negative_shards[shard_index].lock() else {
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

    /// Returns a compiled include unit for a resolved path, compiling on miss.
    pub fn get_or_compile_include(
        &self,
        loader: &IncludeLoader,
        resolved: &ResolvedIncludePath,
        optimization_level: OptimizationLevel,
    ) -> Result<Arc<CompiledUnit>, VmError> {
        let compiler = IncludeCompilerIdentity::current(
            optimization_level,
            loader.compilation_dependency_fingerprint(),
        );
        loop {
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
            if let Some(compiled) =
                self.lookup_compiled_include(resolved, &compiler, validation, validation_mode)?
            {
                return Ok(compiled);
            }

            let Some(_permit) = self.try_begin_compile(&resolved.canonical_path)? else {
                self.wait_for_compile(&resolved.canonical_path)?;
                continue;
            };

            if let Some(compiled) =
                self.lookup_compiled_include(resolved, &compiler, validation, validation_mode)?
            {
                return Ok(compiled);
            }

            let validated = match validated {
                Some(validated) => validated,
                None => self.load_and_record_validation(loader, resolved)?,
            };
            self.stats.compile_misses.fetch_add(1, Ordering::Relaxed);
            let source_identity = validated.identity;
            let canonical_path = validated.loaded.canonical_path.clone();
            let mut resolver = LoaderCompilationResolver::new(loader);
            let compiled = match compile_loaded_include_with_dependencies(
                validated.loaded,
                optimization_level,
                &mut resolver,
            ) {
                Ok((compiled, local_dependencies)) => {
                    self.record_compiled_dependency_reads(&local_dependencies);
                    let key = CompiledIncludeKey::new(
                        canonical_path,
                        source_identity,
                        local_dependencies,
                        compiler.clone(),
                    );
                    let shard_index = self.compile_shard_index(&key);
                    let compiled = Arc::new(compiled);
                    let compiled = {
                        let mut shard = self.compile_shards[shard_index]
                            .lock()
                            .map_err(|_| include_cache_lock_error("compiled", "insert"))?;
                        Arc::clone(shard.entry(key.clone()).or_insert(compiled))
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

    fn lookup_compiled_include(
        &self,
        resolved: &ResolvedIncludePath,
        compiler: &IncludeCompilerIdentity,
        validation: PrimarySourceValidation<'_>,
        validation_mode: IncludeValidationMode,
    ) -> Result<Option<Arc<CompiledUnit>>, VmError> {
        let lookup_key =
            CompiledIncludeLookupKey::new(resolved.canonical_path.clone(), compiler.clone());
        let shard_index = self.compile_shard_index_for_path(&lookup_key.canonical_path);
        let Some(full_key) = ({
            let lookup_shard = self.compile_lookup_shards[shard_index]
                .lock()
                .map_err(|_| include_cache_lock_error("compiled-index", "lookup"))?;
            lookup_shard.get(&lookup_key).cloned()
        }) else {
            return Ok(None);
        };
        let hit = {
            let shard = self.compile_shards[shard_index]
                .lock()
                .map_err(|_| include_cache_lock_error("compiled", "lookup"))?;
            shard
                .get_key_value(&full_key)
                .map(|(key, compiled)| (key.clone(), Arc::clone(compiled)))
        };
        let Some((key, compiled)) = hit else {
            return Ok(None);
        };
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
                Ok(Some(compiled))
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
        let mut shard = self.compile_shards[shard_index]
            .lock()
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
        let mut shard = self.compile_lookup_shards[shard_index]
            .lock()
            .map_err(|_| include_cache_lock_error("compiled-index", "insert"))?;
        shard.insert(lookup_key, key.clone());
        Ok(())
    }

    fn remove_compiled_lookup_index(&self, key: &CompiledIncludeKey) -> Result<(), VmError> {
        let shard_index = self.compile_shard_index(key);
        let lookup_key = CompiledIncludeLookupKey::from_compiled_key(key);
        let mut shard = self.compile_lookup_shards[shard_index]
            .lock()
            .map_err(|_| include_cache_lock_error("compiled-index", "remove"))?;
        shard.remove(&lookup_key);
        Ok(())
    }

    /// Compares the stored parent-directory version against the current one
    /// and records the directory-version counters. Metadata only: this never
    /// affects whether the resolution hit is accepted — it measures how often
    /// a future directory-version-validated negative cache would have been
    /// consistent.
    fn observe_directory_version(&self, resolved: &ResolvedIncludePath) {
        let current = resolved
            .canonical_path
            .parent()
            .and_then(include_directory_version);
        let matches = match (&resolved.directory_version, &current) {
            (Some(stored), Some(current)) => stored == current,
            _ => false,
        };
        if matches {
            self.stats
                .directory_version_hits
                .fetch_add(1, Ordering::Relaxed);
        } else {
            self.stats
                .directory_version_misses
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    fn dependencies_are_fresh(
        &self,
        key: &CompiledIncludeKey,
        validation_mode: IncludeValidationMode,
    ) -> Result<bool, VmError> {
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
        let fingerprint = match self.deployment_root.lock() {
            Ok(slot) => slot.clone(),
            Err(_) => {
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

        let deployment_is_current = match (
            fingerprint.directory_version,
            include_directory_version(&fingerprint.canonical_root),
        ) {
            (Some(stored), Some(current)) => stored == current,
            _ => false,
        };
        let parent_is_current = match (
            resolved.directory_version,
            resolved
                .canonical_path
                .parent()
                .and_then(include_directory_version),
        ) {
            (Some(stored), Some(current)) => stored == current,
            _ => false,
        };
        let source_is_current = resolved.fingerprint.has_reliable_generation()
            && include_path_file_fingerprint(&resolved.canonical_path)
                .is_ok_and(|current| current == resolved.fingerprint);
        if deployment_is_current && parent_is_current && source_is_current {
            true
        } else {
            self.stats
                .conservative_misses
                .fetch_add(1, Ordering::Relaxed);
            false
        }
    }

    fn load_and_record_validation(
        &self,
        loader: &IncludeLoader,
        resolved: &ResolvedIncludePath,
    ) -> Result<ValidatedLoadedInclude, VmError> {
        let loaded = loader.load_validated_resolved(resolved).inspect_err(|_| {
            self.stats
                .conservative_misses
                .fetch_add(1, Ordering::Relaxed);
        })?;
        self.record_content_validation(&loaded);
        Ok(loaded)
    }

    fn record_content_validation(&self, loaded: &ValidatedLoadedInclude) {
        self.stats.source_reads.fetch_add(1, Ordering::Relaxed);
        self.stats
            .source_bytes_hashed
            .fetch_add(loaded.bytes_hashed, Ordering::Relaxed);
        self.stats
            .content_validations
            .fetch_add(1, Ordering::Relaxed);
    }

    fn record_compiled_dependency_reads(&self, dependencies: &[CompiledIncludeDependencyKey]) {
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

    /// Clears cached include resolutions and compiled include units.
    pub fn clear(&self) -> Result<(), VmError> {
        for shard in &self.resolution_shards {
            shard
                .lock()
                .map_err(|_| include_cache_lock_error("resolution", "clear"))?
                .clear();
        }
        for shard in &self.negative_shards {
            shard
                .lock()
                .map_err(|_| include_cache_lock_error("negative", "clear"))?
                .clear();
        }
        for shard in &self.compile_shards {
            shard
                .lock()
                .map_err(|_| include_cache_lock_error("compiled", "clear"))?
                .clear();
        }
        for shard in &self.compile_lookup_shards {
            shard
                .lock()
                .map_err(|_| include_cache_lock_error("compiled-index", "clear"))?
                .clear();
        }
        Ok(())
    }

    /// Returns current cache counters.
    #[must_use]
    pub fn cache_stats(&self) -> IncludeCacheStats {
        IncludeCacheStats {
            resolution_hits: self.stats.resolution_hits.load(Ordering::Relaxed),
            resolution_misses: self.stats.resolution_misses.load(Ordering::Relaxed),
            compile_hits: self.stats.compile_hits.load(Ordering::Relaxed),
            compile_misses: self.stats.compile_misses.load(Ordering::Relaxed),
            source_reads: self.stats.source_reads.load(Ordering::Relaxed),
            source_bytes_hashed: self.stats.source_bytes_hashed.load(Ordering::Relaxed),
            content_validations: self.stats.content_validations.load(Ordering::Relaxed),
            identity_only_hits: self.stats.identity_only_hits.load(Ordering::Relaxed),
            content_mismatches: self.stats.content_mismatches.load(Ordering::Relaxed),
            conservative_misses: self.stats.conservative_misses.load(Ordering::Relaxed),
            dependency_metadata_validations: self
                .stats
                .dependency_metadata_validations
                .load(Ordering::Relaxed),
            stale_invalidations: self.stats.stale_invalidations.load(Ordering::Relaxed),
            stale_dependency_invalidations: self
                .stats
                .stale_dependency_invalidations
                .load(Ordering::Relaxed),
            compile_errors: self.stats.compile_errors.load(Ordering::Relaxed),
            directory_version_hits: self.stats.directory_version_hits.load(Ordering::Relaxed),
            directory_version_misses: self.stats.directory_version_misses.load(Ordering::Relaxed),
            composer_fingerprint_stale: self
                .stats
                .composer_fingerprint_stale
                .load(Ordering::Relaxed),
            deployment_fingerprint_present: self
                .stats
                .deployment_fingerprint_present
                .load(Ordering::Relaxed),
            deployment_fingerprint_missing: self
                .stats
                .deployment_fingerprint_missing
                .load(Ordering::Relaxed),
            deployment_fingerprint_stale: self
                .stats
                .deployment_fingerprint_stale
                .load(Ordering::Relaxed),
            negative_cache_hits: self.stats.negative_cache_hits.load(Ordering::Relaxed),
            negative_cache_installs: self.stats.negative_cache_installs.load(Ordering::Relaxed),
            negative_cache_invalidations: self
                .stats
                .negative_cache_invalidations
                .load(Ordering::Relaxed),
            negative_cache_blocked_unversioned: self
                .stats
                .negative_cache_blocked_unversioned
                .load(Ordering::Relaxed),
            negative_cache_blocked_capacity: self
                .stats
                .negative_cache_blocked_capacity
                .load(Ordering::Relaxed),
        }
    }

    fn resolution_shard_index(&self, key: &IncludeResolutionKey) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.resolution_shards.len()
    }

    fn compile_shard_index(&self, key: &CompiledIncludeKey) -> usize {
        self.compile_shard_index_for_path(&key.canonical_path)
    }

    fn compile_shard_index_for_path(&self, path: &Path) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        path.hash(&mut hasher);
        (hasher.finish() as usize) % self.compile_shards.len()
    }

    fn compile_lock_shard_index(&self, path: &Path) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        path.hash(&mut hasher);
        (hasher.finish() as usize) % self.compile_locks.len()
    }

    fn try_begin_compile(&self, path: &Path) -> Result<Option<IncludeCompilePermit<'_>>, VmError> {
        let shard = &self.compile_locks[self.compile_lock_shard_index(path)];
        let mut in_progress = shard
            .in_progress
            .lock()
            .map_err(|_| include_cache_lock_error("compile-lock", "begin"))?;
        if !in_progress.insert(path.to_path_buf()) {
            return Ok(None);
        }
        Ok(Some(IncludeCompilePermit {
            shard,
            path: path.to_path_buf(),
        }))
    }

    fn wait_for_compile(&self, path: &Path) -> Result<(), VmError> {
        let shard = &self.compile_locks[self.compile_lock_shard_index(path)];
        let mut in_progress = shard
            .in_progress
            .lock()
            .map_err(|_| include_cache_lock_error("compile-lock", "wait"))?;
        while in_progress.contains(path) {
            in_progress = shard
                .condvar
                .wait(in_progress)
                .map_err(|_| include_cache_lock_error("compile-lock", "wait"))?;
        }
        Ok(())
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
    pub source_reads: u64,
    pub source_bytes_hashed: u64,
    pub content_validations: u64,
    pub identity_only_hits: u64,
    pub content_mismatches: u64,
    pub conservative_misses: u64,
    pub dependency_metadata_validations: u64,
    pub stale_invalidations: u64,
    pub stale_dependency_invalidations: u64,
    pub compile_errors: u64,
    pub directory_version_hits: u64,
    pub directory_version_misses: u64,
    pub composer_fingerprint_stale: u64,
    pub deployment_fingerprint_present: u64,
    pub deployment_fingerprint_missing: u64,
    pub deployment_fingerprint_stale: u64,
    pub negative_cache_hits: u64,
    pub negative_cache_installs: u64,
    pub negative_cache_invalidations: u64,
    pub negative_cache_blocked_unversioned: u64,
    pub negative_cache_blocked_capacity: u64,
}

#[derive(Debug, Default)]
struct IncludeCacheCounters {
    resolution_hits: AtomicU64,
    resolution_misses: AtomicU64,
    compile_hits: AtomicU64,
    compile_misses: AtomicU64,
    source_reads: AtomicU64,
    source_bytes_hashed: AtomicU64,
    content_validations: AtomicU64,
    identity_only_hits: AtomicU64,
    content_mismatches: AtomicU64,
    conservative_misses: AtomicU64,
    dependency_metadata_validations: AtomicU64,
    stale_invalidations: AtomicU64,
    stale_dependency_invalidations: AtomicU64,
    compile_errors: AtomicU64,
    directory_version_hits: AtomicU64,
    directory_version_misses: AtomicU64,
    composer_fingerprint_stale: AtomicU64,
    deployment_fingerprint_present: AtomicU64,
    deployment_fingerprint_missing: AtomicU64,
    deployment_fingerprint_stale: AtomicU64,
    negative_cache_hits: AtomicU64,
    negative_cache_installs: AtomicU64,
    negative_cache_invalidations: AtomicU64,
    negative_cache_blocked_unversioned: AtomicU64,
    negative_cache_blocked_capacity: AtomicU64,
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
        if let Ok(mut in_progress) = self.shard.in_progress.lock() {
            in_progress.remove(&self.path);
        }
        self.shard.condvar.notify_all();
    }
}

/// Root-constrained local include loader.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncludeLoader {
    allowed_roots: Vec<PathBuf>,
    compilation_dependencies: BTreeMap<String, PathBuf>,
}

impl IncludeLoader {
    /// Creates a loader with canonicalized allowed roots.
    pub fn new(roots: impl IntoIterator<Item = PathBuf>) -> Result<Self, VmError> {
        let mut allowed_roots = Vec::new();
        for root in roots {
            let canonical = fs::canonicalize(&root).map_err(|error| {
                include_error(
                    "E_PHP_VM_INCLUDE_ROOT",
                    format!("{}: {error}", root.display()),
                )
                .with_context("root", root.display())
            })?;
            if !allowed_roots.contains(&canonical) {
                allowed_roots.push(canonical);
            }
        }
        Ok(Self {
            allowed_roots,
            compilation_dependencies: BTreeMap::new(),
        })
    }

    /// Creates a loader that permits files under `root`.
    pub fn for_root(root: impl Into<PathBuf>) -> Result<Self, VmError> {
        Self::new([root.into()])
    }

    /// Returns configured roots.
    #[must_use]
    pub fn allowed_roots(&self) -> &[PathBuf] {
        &self.allowed_roots
    }

    /// Adds an explicit declaration-to-file mapping for multi-file lowering.
    ///
    /// The executor or autoload metadata provider owns these mappings. The
    /// compiler never searches source text or directory trees to guess where
    /// a declaration lives. Relative paths are resolved from the first
    /// configured root and all targets remain subject to the normal root
    /// policy when loaded.
    #[must_use]
    pub fn with_compilation_dependency(
        mut self,
        declaration: impl AsRef<str>,
        path: impl Into<PathBuf>,
    ) -> Self {
        self.compilation_dependencies.insert(
            php_ir::module::normalize_class_name(declaration.as_ref()),
            path.into(),
        );
        self
    }

    fn compilation_dependency(&self, declaration: &str) -> Option<&Path> {
        self.compilation_dependencies
            .get(&php_ir::module::normalize_class_name(declaration))
            .map(PathBuf::as_path)
    }

    fn compilation_dependency_fingerprint(&self) -> u64 {
        let mut serialized = Vec::new();
        for (declaration, path) in &self.compilation_dependencies {
            serialized.extend_from_slice(declaration.as_bytes());
            serialized.push(0);
            serialized.extend_from_slice(path.to_string_lossy().as_bytes());
            serialized.push(b'\n');
        }
        fnv1a_64(&serialized)
    }

    /// Converts an include/require error string to the shared diagnostic envelope.
    #[must_use]
    pub fn include_failure_diagnostic(
        &self,
        error: &VmError,
        path: &str,
        including_file: Option<&Path>,
        include_path: &[PathBuf],
        cwd: Option<&Path>,
        cache_used: bool,
    ) -> DiagnosticEnvelope {
        let code = error.code();
        let mut context = error.context().clone();
        context.insert("path".to_string(), path.to_string());
        context.insert("cache_used".to_string(), cache_used.to_string());
        if let Some(including_file) = including_file {
            context.insert(
                "including_file".to_string(),
                including_file.display().to_string(),
            );
        }
        if let Some(cwd) = cwd {
            context.insert("cwd".to_string(), cwd.display().to_string());
        }
        if !include_path.is_empty() {
            context.insert(
                "include_path".to_string(),
                include_path
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(":"),
            );
        }
        if !self.allowed_roots.is_empty() {
            context.insert(
                "allowed_roots".to_string(),
                self.allowed_roots
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(":"),
            );
        }

        let mut envelope = DiagnosticEnvelope::new(
            code,
            DiagnosticLayer::vm(),
            DiagnosticPhase::new(error.phase()),
            DiagnosticSeverity::FatalError,
            error.render_message(),
        )
        .with_context(context);
        envelope.suggestion = Some(DiagnosticSuggestion::new(include_error_suggestion(code)));
        envelope.php_visible = true;
        envelope
    }

    /// Loads a file after resolving it against the including file directory and
    /// checking that the canonical path remains within an allowed root.
    pub fn load(
        &self,
        including_file: Option<&Path>,
        path: &str,
    ) -> Result<LoadedInclude, VmError> {
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
    ) -> Result<LoadedInclude, VmError> {
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
    ) -> Result<ResolvedIncludePath, VmError> {
        self.resolve_with_include_path_traced(including_file, path, include_path, cwd)
            .map_err(|(error, _)| error)
    }

    /// Like [`Self::resolve_with_include_path`] but also reports, when
    /// resolution fails as a genuine missing path, the directory-version
    /// guards needed to install a negative-cache entry — captured immediately
    /// BEFORE probing each candidate so a file created concurrently with the
    /// probe invalidates the guard. Non-local failures (disabled loader,
    /// stream schemes, phar, non-`NotFound` errors, symlink/relative
    /// candidates) yield an uncacheable trace.
    // The error variant is cold (resolution failures) and immediately
    // consumed by the negative-cache installer; boxing would only add an
    // allocation on the diagnostic path.
    #[allow(clippy::result_large_err)]
    pub(crate) fn resolve_with_include_path_traced(
        &self,
        including_file: Option<&Path>,
        path: &str,
        include_path: &[PathBuf],
        cwd: Option<&Path>,
    ) -> Result<ResolvedIncludePath, (VmError, NegativeProbeTrace)> {
        if self.allowed_roots.is_empty() {
            return Err((
                include_error(
                    "E_PHP_VM_INCLUDE_DISABLED",
                    "include loader has no allowed roots",
                ),
                NegativeProbeTrace::uncacheable(),
            ));
        }
        if phar::is_phar_uri(path) {
            return self
                .resolve_phar_include(path, cwd)
                .map_err(|error| (error, NegativeProbeTrace::uncacheable()));
        }
        if path.contains("://") {
            return Err((
                include_error(
                    "E_PHP_VM_INCLUDE_UNSUPPORTED_SCHEME",
                    format!("stream include `{path}` is not supported"),
                )
                .with_context("path", path),
                NegativeProbeTrace::uncacheable(),
            ));
        }
        let raw = Path::new(path);
        let mut candidates = Vec::new();
        if raw.is_absolute() {
            push_include_candidate(&mut candidates, raw.to_path_buf());
        } else if path_has_explicit_relative_prefix(raw) {
            if let Some(cwd) = cwd {
                push_include_candidate(&mut candidates, cwd.join(raw));
            } else {
                push_include_candidate(&mut candidates, raw.to_path_buf());
            }
        } else {
            let base = including_file.and_then(Path::parent);
            for entry in include_path {
                push_include_candidate(
                    &mut candidates,
                    resolve_include_path_entry(cwd, entry).join(raw),
                );
            }
            if let Some(cwd) = cwd {
                push_include_candidate(&mut candidates, cwd.join(raw));
            }
            if let Some(parent) = base {
                push_include_candidate(&mut candidates, parent.join(raw));
            }
            push_include_candidate(&mut candidates, raw.to_path_buf());
        }
        // Capture negative-cache guards only when the cache would use them.
        // Each guard is the version of a candidate's parent directory read
        // *before* that candidate is probed, so a file created concurrently
        // with (or after) the probe changes the version and invalidates the
        // guard. `cacheable` stays true only while every failed candidate is a
        // non-symlink path that failed with NotFound and whose parent is
        // versionable; a non-NotFound (transient permission/IO) failure or a
        // dangling symlink (whose target may appear in an unguarded directory)
        // makes the miss unsafe to cache. Relative candidates are anchored at
        // the process working directory, matching how `fs::canonicalize`
        // resolves them (residual limit: a mid-process `chdir` is not captured
        // in the resolution key — the server and CLI never chdir while serving).
        let capture_guards = negative_include_cache_enabled();
        let mut process_cwd: Option<Option<PathBuf>> = None;
        let mut guards: Vec<NegativeProbeGuard> = Vec::new();
        let mut cacheable = capture_guards;
        let mut last_error = None;
        let mut resolved_candidate = None;
        for candidate in &candidates {
            let mut guard_candidate = None;
            if cacheable {
                let absolute = if candidate.is_absolute() {
                    Some(candidate.clone())
                } else {
                    let cwd = process_cwd
                        .get_or_insert_with(|| std::env::current_dir().ok())
                        .clone();
                    cwd.map(|cwd| cwd.join(candidate))
                };
                match absolute.and_then(|path| {
                    let parent = path.parent()?.to_path_buf();
                    Some((path, parent))
                }) {
                    Some((path, parent)) => match include_directory_version(&parent) {
                        Some(version) => {
                            guard_candidate = Some(NegativeProbeGuard {
                                candidate: path,
                                directory: parent,
                                directory_version: version,
                            });
                        }
                        None => cacheable = false,
                    },
                    None => cacheable = false,
                }
            }
            match fs::canonicalize(candidate) {
                Ok(path) => {
                    let candidate = if candidate.is_absolute() {
                        candidate.clone()
                    } else {
                        process_cwd
                            .get_or_insert_with(|| std::env::current_dir().ok())
                            .as_ref()
                            .map_or_else(|| candidate.clone(), |cwd| cwd.join(candidate))
                    };
                    resolved_candidate = Some((path, candidate));
                    break;
                }
                Err(error) => {
                    if cacheable
                        && (error.kind() != std::io::ErrorKind::NotFound
                            || fs::symlink_metadata(candidate).is_ok())
                    {
                        cacheable = false;
                    }
                    if cacheable {
                        if let Some(guard) = guard_candidate {
                            guards.push(guard);
                        } else {
                            cacheable = false;
                        }
                    }
                    last_error = Some(
                        include_error(
                            "E_PHP_VM_INCLUDE_MISSING",
                            format!("{}: {error}", candidate.display()),
                        )
                        .with_context("candidate", candidate.display()),
                    );
                }
            }
        }
        let Some((canonical, resolution_path)) = resolved_candidate else {
            let error = last_error.unwrap_or_else(|| {
                include_error("E_PHP_VM_INCLUDE_MISSING", format!("{path}: not found"))
                    .with_context("path", path)
            });
            let trace = NegativeProbeTrace {
                guards: cacheable.then_some(guards),
            };
            return Err((error, trace));
        };
        if !self
            .allowed_roots
            .iter()
            .any(|root| canonical.starts_with(root))
        {
            return Err((
                include_error(
                    "E_PHP_VM_INCLUDE_OUTSIDE_ROOT",
                    format!("{} is outside allowed include roots", canonical.display()),
                )
                .with_context("canonical_path", canonical.display()),
                NegativeProbeTrace::uncacheable(),
            ));
        }
        let fingerprint = include_path_file_fingerprint(&canonical)
            .map_err(|error| (error, NegativeProbeTrace::uncacheable()))?;
        let directory_version = canonical.parent().and_then(include_directory_version);
        Ok(ResolvedIncludePath {
            canonical_path: canonical,
            resolution_path: Some(resolution_path),
            fingerprint,
            directory_version,
        })
    }

    /// Loads a previously resolved canonical include path, rechecking that the
    /// path remains inside an allowed root.
    pub fn load_resolved(&self, canonical: PathBuf) -> Result<LoadedInclude, VmError> {
        let canonical_text = canonical.to_string_lossy();
        if phar::is_phar_uri(&canonical_text) {
            return self.load_phar_include(&canonical_text);
        }
        if !self
            .allowed_roots
            .iter()
            .any(|root| canonical.starts_with(root))
        {
            return Err(include_error(
                "E_PHP_VM_INCLUDE_OUTSIDE_ROOT",
                format!("{} is outside allowed include roots", canonical.display()),
            )
            .with_context("canonical_path", canonical.display()));
        }
        let source = fs::read(&canonical).map_err(|error| {
            include_error(
                "E_PHP_VM_INCLUDE_READ",
                format!("{}: {error}", canonical.display()),
            )
            .with_context("canonical_path", canonical.display())
        })?;
        let source = php_source_from_bytes(source);
        Ok(LoadedInclude {
            canonical_path: canonical,
            source,
        })
    }

    fn load_validated_resolved(
        &self,
        resolved: &ResolvedIncludePath,
    ) -> Result<ValidatedLoadedInclude, VmError> {
        let canonical_text = resolved.canonical_path.to_string_lossy();
        if phar::is_phar_uri(&canonical_text) {
            let loaded = self.load_phar_include(&canonical_text)?;
            let bytes_hashed = loaded.source.len() as u64;
            return Ok(ValidatedLoadedInclude {
                identity: OpenedSourceIdentity {
                    generation: resolved.fingerprint.clone(),
                    content_hash: fnv1a_64(loaded.source.as_bytes()),
                },
                loaded,
                bytes_hashed,
            });
        }
        if !self
            .allowed_roots
            .iter()
            .any(|root| resolved.canonical_path.starts_with(root))
        {
            return Err(include_error(
                "E_PHP_VM_INCLUDE_OUTSIDE_ROOT",
                format!(
                    "{} is outside allowed include roots",
                    resolved.canonical_path.display()
                ),
            )
            .with_context("canonical_path", resolved.canonical_path.display()));
        }
        read_validated_file(&resolved.canonical_path)
    }

    fn resolve_phar_include(
        &self,
        path: &str,
        cwd: Option<&Path>,
    ) -> Result<ResolvedIncludePath, VmError> {
        let cwd = cwd
            .or_else(|| self.allowed_roots.first().map(PathBuf::as_path))
            .unwrap_or_else(|| Path::new("."));
        let capabilities =
            FilesystemCapabilities::none().with_allowed_roots(self.allowed_roots.clone());
        let parsed = phar::parse_uri(path, cwd, &capabilities).map_err(|error| {
            include_error("E_PHP_VM_INCLUDE_PHAR", error.to_string()).with_context("path", path)
        })?;
        let canonical_path = PathBuf::from(format!(
            "phar://{}/{}",
            parsed.archive_path.display(),
            parsed.entry_path
        ));
        let fingerprint = include_path_file_fingerprint(&parsed.archive_path)?;
        Ok(ResolvedIncludePath {
            canonical_path,
            resolution_path: None,
            fingerprint,
            // Phar entries have no meaningful parent-directory version; `None`
            // always counts as a directory-version miss (conservative).
            directory_version: None,
        })
    }

    fn load_phar_include(&self, path: &str) -> Result<LoadedInclude, VmError> {
        let capabilities =
            FilesystemCapabilities::none().with_allowed_roots(self.allowed_roots.clone());
        let bytes = phar::read_uri(path, Path::new("."), &capabilities).map_err(|error| {
            include_error("E_PHP_VM_INCLUDE_READ", error.to_string()).with_context("path", path)
        })?;
        let source = String::from_utf8(bytes).map_err(|error| {
            include_error(
                "E_PHP_VM_INCLUDE_READ",
                format!("phar entry `{path}` is not valid UTF-8: {error}"),
            )
            .with_context("path", path)
        })?;
        Ok(LoadedInclude {
            canonical_path: PathBuf::from(path),
            source,
        })
    }
}

fn read_validated_file(path: &Path) -> Result<ValidatedLoadedInclude, VmError> {
    const MAX_STABLE_READ_ATTEMPTS: usize = 3;

    for _ in 0..MAX_STABLE_READ_ATTEMPTS {
        let mut file = File::open(path).map_err(|error| include_read_error(path, error))?;
        let before = file
            .metadata()
            .map(|metadata| include_file_fingerprint(&metadata))
            .map_err(|error| include_metadata_error(path, error))?;
        let mut bytes = Vec::with_capacity(before.len.try_into().unwrap_or(0));
        file.read_to_end(&mut bytes)
            .map_err(|error| include_read_error(path, error))?;
        let after = file
            .metadata()
            .map(|metadata| include_file_fingerprint(&metadata))
            .map_err(|error| include_metadata_error(path, error))?;
        if before != after || after.len != bytes.len() as u64 {
            continue;
        }
        let bytes_hashed = bytes.len() as u64;
        let content_hash = fnv1a_64(&bytes);
        return Ok(ValidatedLoadedInclude {
            loaded: LoadedInclude {
                canonical_path: path.to_path_buf(),
                source: php_source_from_bytes(bytes),
            },
            identity: OpenedSourceIdentity {
                generation: after,
                content_hash,
            },
            bytes_hashed,
        });
    }
    Err(include_error(
        "E_PHP_VM_INCLUDE_CHANGED_DURING_READ",
        format!("{} changed while it was being read", path.display()),
    )
    .with_context("canonical_path", path.display())
    .with_context("attempts", MAX_STABLE_READ_ATTEMPTS))
}

fn include_read_error(path: &Path, error: std::io::Error) -> VmError {
    include_error(
        "E_PHP_VM_INCLUDE_READ",
        format!("{}: {error}", path.display()),
    )
    .with_context("canonical_path", path.display())
}

fn include_metadata_error(path: &Path, error: std::io::Error) -> VmError {
    include_error(
        "E_PHP_VM_INCLUDE_METADATA",
        format!("{}: {error}", path.display()),
    )
    .with_context("path", path.display())
}

fn include_error_suggestion(code: &str) -> &'static str {
    match code {
        "E_PHP_VM_INCLUDE_DISABLED" => {
            "configure an allowed include root before executing include or require"
        }
        "E_PHP_VM_INCLUDE_UNSUPPORTED_SCHEME" => {
            "use a local path or phar URI supported by the include loader"
        }
        "E_PHP_VM_INCLUDE_MISSING" => {
            "check the requested path, current working directory, and include_path entries"
        }
        "E_PHP_VM_INCLUDE_OUTSIDE_ROOT" => {
            "add the canonical parent directory to the allowed include roots"
        }
        "E_PHP_VM_INCLUDE_COMPILE_ERROR" => {
            "inspect the included file compile diagnostic and source span"
        }
        _ => "inspect the include path and loader configuration",
    }
}

fn include_error(code: &'static str, message: impl Into<String>) -> VmError {
    VmError::fatal(code, "include", message)
}

fn php_source_from_bytes(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(error) => error.into_bytes().into_iter().map(char::from).collect(),
    }
}

fn include_cache_lock_error(cache: &'static str, operation: &'static str) -> VmError {
    VmError::internal(
        "E_PHP_VM_INCLUDE_CACHE_POISONED",
        "include",
        format!("{cache} include cache lock poisoned during {operation}"),
    )
    .with_context("cache", cache)
    .with_context("operation", operation)
}

pub fn include_path_file_fingerprint(path: &Path) -> Result<IncludePathFileFingerprint, VmError> {
    let metadata = fs::metadata(path).map_err(|error| include_metadata_error(path, error))?;
    Ok(include_file_fingerprint(&metadata))
}

pub(crate) fn resolution_path_targets(
    resolution_path: Option<&Path>,
    canonical_path: &Path,
) -> bool {
    resolution_path.is_none_or(|path| {
        fs::canonicalize(path).is_ok_and(|canonical| canonical == canonical_path)
    })
}

fn include_file_fingerprint(metadata: &fs::Metadata) -> IncludePathFileFingerprint {
    let modified_unix_nanos = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos());
    let (inode, device) = file_identity(metadata);
    IncludePathFileFingerprint {
        len: metadata.len(),
        modified_unix_nanos,
        changed_unix_nanos: file_changed_unix_nanos(metadata),
        readonly: metadata.permissions().readonly(),
        inode,
        device,
    }
}

/// Stable, dependency-free 64-bit FNV-1a hash for engine-owned content
/// identity. `DefaultHasher` is explicitly unstable across releases and
/// processes, so it must never leak into anything a future persistent cache
/// could key on.
#[must_use]
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Well-known Composer autoload map files, fingerprinted as engine metadata.
/// This never changes runtime behavior: the fingerprint feeds cache keys and
/// counters only, and a missing map yields `None` (unknown), which blocks any
/// persistent reuse keyed on it.
const COMPOSER_MAP_FILES: &[&str] = &[
    "autoload_classmap.php",
    "autoload_files.php",
    "autoload_psr4.php",
    "autoload_real.php",
    "autoload_static.php",
];

/// Fingerprints a detected Composer `vendor/composer` autoload map near
/// `anchor_dir` (the entry script's directory), walking at most four ancestor
/// levels so front controllers under `public/`/`web/` still find the project
/// root. Returns `None` when no map directory is detected.
#[must_use]
pub fn composer_autoload_map_fingerprint(anchor_dir: &Path) -> Option<String> {
    let mut dir = Some(anchor_dir);
    for _ in 0..=4 {
        let candidate = dir?;
        let composer_dir = candidate.join("vendor").join("composer");
        if composer_dir.is_dir() {
            return Some(render_composer_map_fingerprint(&composer_dir));
        }
        dir = candidate.parent();
    }
    None
}

fn render_composer_map_fingerprint(composer_dir: &Path) -> String {
    // The hashed text must be a defined serialization, not incidental Debug
    // formatting (which Rust does not guarantee stable across toolchains) —
    // fnv1a_64 was chosen precisely so a future persistent cache can key on
    // this fingerprint. Render each optional field with an explicit spelling.
    fn field<T: std::fmt::Display>(value: Option<T>) -> String {
        value.map_or_else(|| "none".to_owned(), |value| value.to_string())
    }
    let mut rendered = format!("{}\n", composer_dir.display());
    for name in COMPOSER_MAP_FILES {
        match include_path_file_fingerprint(&composer_dir.join(name)) {
            Ok(fingerprint) => {
                rendered.push_str(&format!(
                    "{name}|{}|{}|{}|{}|{}|{}\n",
                    fingerprint.len,
                    field(fingerprint.modified_unix_nanos),
                    field(fingerprint.changed_unix_nanos),
                    u8::from(fingerprint.readonly),
                    field(fingerprint.inode),
                    field(fingerprint.device),
                ));
            }
            Err(_) => rendered.push_str(&format!("{name}|absent\n")),
        }
    }
    format!("composer-map-v1:{:016x}", fnv1a_64(rendered.as_bytes()))
}

/// Declared mutability of a deployment root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeploymentRootMode {
    /// Development default: the root may mutate at any time, so persistent
    /// reuse keyed on the root stays blocked.
    DevMutable,
    /// Operator-declared immutable deployment root (for example an atomically
    /// swapped release directory). Declaration is a config input, not a
    /// filesystem probe — the engine still revalidates the directory version.
    ImmutableDeclared,
}

impl DeploymentRootMode {
    /// Stable config/report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DevMutable => "dev",
            Self::ImmutableDeclared => "immutable",
        }
    }
}

/// Deployment-root fingerprint for production-mode server runs: the canonical
/// root, its directory version at startup, and the operator-declared
/// mutability mode. Metadata and counters only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeploymentRootFingerprint {
    pub canonical_root: PathBuf,
    pub directory_version: Option<IncludeDirectoryVersion>,
    pub mode: DeploymentRootMode,
}

impl DeploymentRootFingerprint {
    /// Observes a deployment root. `None` when the root cannot be
    /// canonicalized, which callers count as `deployment_fingerprint_missing`.
    #[must_use]
    pub fn observe(root: &Path, mode: DeploymentRootMode) -> Option<Self> {
        let canonical_root = fs::canonicalize(root).ok()?;
        let directory_version = include_directory_version(&canonical_root);
        Some(Self {
            canonical_root,
            directory_version,
            mode,
        })
    }
}

/// Filesystem identity `(inode, device)` when the platform exposes it. Unix
/// reports both; other platforms report `(None, None)`, which keeps caching
/// conservative rather than optimistic.
#[cfg(unix)]
fn file_identity(metadata: &fs::Metadata) -> (Option<u64>, Option<u64>) {
    use std::os::unix::fs::MetadataExt as _;
    (Some(metadata.ino()), Some(metadata.dev()))
}

#[cfg(unix)]
fn file_changed_unix_nanos(metadata: &fs::Metadata) -> Option<i128> {
    use std::os::unix::fs::MetadataExt as _;
    Some(i128::from(metadata.ctime()) * 1_000_000_000 + i128::from(metadata.ctime_nsec()))
}

#[cfg(not(unix))]
fn file_identity(_metadata: &fs::Metadata) -> (Option<u64>, Option<u64>) {
    (None, None)
}

#[cfg(not(unix))]
fn file_changed_unix_nanos(_metadata: &fs::Metadata) -> Option<i128> {
    None
}

fn path_has_explicit_relative_prefix(path: &Path) -> bool {
    matches!(
        path.components().next(),
        Some(std::path::Component::CurDir | std::path::Component::ParentDir)
    )
}

fn push_include_candidate(candidates: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !candidates.contains(&candidate) {
        candidates.push(candidate);
    }
}

fn resolve_include_path_entry(cwd: Option<&Path>, entry: &Path) -> PathBuf {
    if entry.is_absolute() {
        return entry.to_path_buf();
    }
    if entry == Path::new(".")
        && let Some(cwd) = cwd
    {
        return cwd.to_path_buf();
    }
    if let Some(cwd) = cwd {
        return cwd.join(entry);
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
    source_identity: OpenedSourceIdentity,
    local_dependencies: Vec<CompiledIncludeDependencyKey>,
    compiler: IncludeCompilerIdentity,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct IncludeCompilerIdentity {
    compiler_version: &'static str,
    debug_assertions: bool,
    optimization_level: &'static str,
    compilation_dependency_fingerprint: u64,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CompiledIncludeLookupKey {
    canonical_path: PathBuf,
    compiler: IncludeCompilerIdentity,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CompiledIncludeDependencyKey {
    canonical_path: PathBuf,
    source_identity: OpenedSourceIdentity,
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
        local_dependencies: Vec<CompiledIncludeDependencyKey>,
        compiler: IncludeCompilerIdentity,
    ) -> Self {
        Self {
            canonical_path,
            source_identity,
            local_dependencies,
            compiler,
        }
    }
}

impl IncludeCompilerIdentity {
    fn current(
        optimization_level: OptimizationLevel,
        compilation_dependency_fingerprint: u64,
    ) -> Self {
        Self {
            compiler_version: env!("CARGO_PKG_VERSION"),
            debug_assertions: cfg!(debug_assertions),
            optimization_level: optimization_level.as_str(),
            compilation_dependency_fingerprint,
        }
    }
}

impl CompiledIncludeLookupKey {
    fn new(canonical_path: PathBuf, compiler: IncludeCompilerIdentity) -> Self {
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

pub(crate) fn compile_loaded_include(
    loaded: LoadedInclude,
    optimization_level: OptimizationLevel,
) -> Result<CompiledUnit, VmError> {
    let mut resolver = NoCompilationResolver;
    compile_loaded_include_with_dependencies(loaded, optimization_level, &mut resolver)
        .map(|(compiled, _)| compiled)
}

pub(crate) fn compile_loaded_include_with_loader(
    loaded: LoadedInclude,
    optimization_level: OptimizationLevel,
    loader: &IncludeLoader,
) -> Result<CompiledUnit, VmError> {
    let mut resolver = LoaderCompilationResolver::new(loader);
    compile_loaded_include_with_dependencies(loaded, optimization_level, &mut resolver)
        .map(|(compiled, _)| compiled)
}

trait CompilationResolver {
    fn resolve_trait(
        &mut self,
        request: &php_ir::UnresolvedTraitRequest,
    ) -> Result<Option<ValidatedLoadedInclude>, VmError>;
}

struct NoCompilationResolver;

impl CompilationResolver for NoCompilationResolver {
    fn resolve_trait(
        &mut self,
        _request: &php_ir::UnresolvedTraitRequest,
    ) -> Result<Option<ValidatedLoadedInclude>, VmError> {
        Ok(None)
    }
}

struct LoaderCompilationResolver<'a> {
    loader: &'a IncludeLoader,
}

impl<'a> LoaderCompilationResolver<'a> {
    const fn new(loader: &'a IncludeLoader) -> Self {
        Self { loader }
    }
}

impl CompilationResolver for LoaderCompilationResolver<'_> {
    fn resolve_trait(
        &mut self,
        request: &php_ir::UnresolvedTraitRequest,
    ) -> Result<Option<ValidatedLoadedInclude>, VmError> {
        let Some(path) = self.loader.compilation_dependency(&request.normalized_name) else {
            return Ok(None);
        };
        let path = path.to_string_lossy();
        let resolved = self.loader.resolve_with_include_path(
            None,
            &path,
            &[],
            self.loader.allowed_roots().first().map(PathBuf::as_path),
        )?;
        let loaded = self.loader.load_validated_resolved(&resolved)?;
        let probe = php_ir::CompilationSession::new(
            loaded.loaded.canonical_path.to_string_lossy().into_owned(),
            loaded.loaded.source.clone(),
        );
        if !probe
            .declared_trait_names(probe.entry())
            .iter()
            .any(|name| name == &request.normalized_name)
        {
            return Err(include_error(
                "E_PHP_VM_INCLUDE_DEPENDENCY_MISMATCH",
                format!(
                    "mapped file {} does not declare trait `{}`",
                    loaded.loaded.canonical_path.display(),
                    request.normalized_name
                ),
            )
            .with_context("declaration", &request.normalized_name)
            .with_context("canonical_path", loaded.loaded.canonical_path.display()));
        }
        Ok(Some(loaded))
    }
}

fn compile_loaded_include_with_dependencies(
    loaded: LoadedInclude,
    optimization_level: OptimizationLevel,
    resolver: &mut dyn CompilationResolver,
) -> Result<(CompiledUnit, Vec<CompiledIncludeDependencyKey>), VmError> {
    let entry_path = loaded.canonical_path.clone();
    let mut session = php_ir::CompilationSession::new(
        loaded.canonical_path.to_string_lossy().into_owned(),
        loaded.source,
    );
    let mut local_dependencies = Vec::new();
    let mut providers = HashMap::<String, php_ir::CompilationFileId>::new();
    for name in session.declared_trait_names(session.entry()) {
        providers.insert(name, session.entry());
    }
    let mut next_file = 0;
    while next_file < session.files().len() {
        let file_id = session.files()[next_file].id();
        if session.files()[next_file].frontend().has_errors() {
            return Err(include_error(
                "E_PHP_VM_INCLUDE_COMPILE_ERROR",
                format!(
                    "{} failed frontend analysis",
                    session.files()[next_file].path()
                ),
            )
            .with_context("path", session.files()[next_file].path())
            .with_context("stage", "frontend"));
        }
        let requests = session.unresolved_trait_requests(file_id);
        for request in requests {
            if let Some(provider) = providers.get(&request.normalized_name).copied() {
                let provider_source = &session.files()[provider.index()];
                session.add_dependency(
                    file_id,
                    &request.normalized_name,
                    provider_source.path().to_owned(),
                    provider_source.source().to_owned(),
                );
                continue;
            }
            let Some(validated) = resolver.resolve_trait(&request)? else {
                continue;
            };
            let path = validated.loaded.canonical_path.clone();
            let dependency = session.add_dependency(
                file_id,
                &request.normalized_name,
                path.to_string_lossy().into_owned(),
                validated.loaded.source,
            );
            for declared in session.declared_trait_names(dependency) {
                if let Some(previous) = providers.insert(declared.clone(), dependency)
                    && previous != dependency
                {
                    return Err(include_error(
                        "E_PHP_VM_INCLUDE_DUPLICATE_DECLARATION",
                        format!("duplicate trait declaration `{declared}`"),
                    )
                    .with_context("declaration", declared));
                }
            }
            local_dependencies.push(CompiledIncludeDependencyKey {
                canonical_path: path,
                source_identity: validated.identity,
            });
        }
        next_file += 1;
    }

    if let Some(cycle) = session.dependency_cycle() {
        let paths = cycle
            .edges
            .iter()
            .map(|edge| session.files()[edge.requester.index()].path())
            .chain(
                cycle
                    .edges
                    .last()
                    .map(|edge| session.files()[edge.dependency.index()].path()),
            )
            .collect::<Vec<_>>();
        let declarations = cycle
            .edges
            .iter()
            .map(|edge| edge.declaration.as_str())
            .collect::<Vec<_>>();
        return Err(include_error(
            "E_PHP_VM_INCLUDE_DEPENDENCY_CYCLE",
            format!("declaration dependency cycle: {}", paths.join(" -> ")),
        )
        .with_context("paths", paths.join(":"))
        .with_context("declarations", declarations.join(":")));
    }

    let mut lowering =
        php_ir::lower_compilation_session(&session, php_ir::LoweringOptions::default());

    if !lowering.diagnostics.is_empty() || lowering.verification.is_err() {
        let detail = ir_lowering_failure_detail(&lowering);
        return Err(include_error(
            "E_PHP_VM_INCLUDE_COMPILE_ERROR",
            format!(
                "{} failed IR lowering: {detail}",
                session.files()[session.entry().index()].path()
            ),
        )
        .with_context("path", session.files()[session.entry().index()].path())
        .with_context("stage", "ir_lowering")
        .with_context("detail", detail)
        .with_context(
            "local_trait_files",
            session
                .files()
                .iter()
                .filter(|file| file.id() != session.entry())
                .map(|file| file.path().to_owned())
                .collect::<Vec<_>>()
                .join(":"),
        ));
    }
    if optimization_level.runs_pipeline() {
        PassPipeline::performance()
            .run(&mut lowering.unit, &PassContext::new(optimization_level))
            .map_err(|error| {
                include_error(
                    "E_PHP_VM_INCLUDE_COMPILE_ERROR",
                    format!("{} optimizer failed: {error}", entry_path.display()),
                )
                .with_context("path", entry_path.display())
                .with_context("stage", "optimizer")
            })?;
    }
    Ok((CompiledUnit::new(lowering.unit), local_dependencies))
}

fn ir_lowering_failure_detail(lowering: &php_ir::LoweringResult) -> String {
    if let Some(diagnostic) = lowering.diagnostics.first() {
        return format!("{}: {}", diagnostic.id, diagnostic.message);
    }
    if let Err(error) = &lowering.verification {
        return format!("IR verification failed: {error:?}");
    }
    "unknown IR lowering failure".to_string()
}

#[cfg(test)]
fn missing_local_trait_names(lowering: &php_ir::LoweringResult) -> Vec<String> {
    let mut traits = Vec::new();
    for diagnostic in &lowering.diagnostics {
        let Some(missing_trait) = diagnostic.missing_trait() else {
            continue;
        };
        if !traits
            .iter()
            .any(|existing| existing == &missing_trait.normalized_name)
        {
            traits.push(missing_trait.normalized_name.clone());
        }
    }
    traits
}

fn default_include_cache_shards() -> usize {
    std::thread::available_parallelism().map_or(16, |count| count.get().clamp(1, 64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use php_ir::instruction::{BinaryOp, InstructionKind};
    use std::fs::{FileTimes, OpenOptions};
    use std::sync::Barrier;
    use std::time::{SystemTime, UNIX_EPOCH};

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
        let cache = IncludeCache::new(1);

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
        let cache = IncludeCache::new(1);

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
        let cache = IncludeCache::new(1);

        // Remove search permission so canonicalize fails with EACCES, not
        // NotFound. Skip if running as root (permission bits are ignored).
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");
        let blocked = cache.resolve_with_include_path(
            &loader,
            None,
            "locked/lib.php",
            &[],
            Some(&fixture.root),
        );
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
        let cache = IncludeCache::new(1);

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
        let from_root =
            composer_autoload_map_fingerprint(&fixture.root).expect("map detected at root");
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
        let cache = IncludeCache::new(1);
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
        let cache = IncludeCache::new(1);

        cache.set_deployment_root_fingerprint(DeploymentRootFingerprint::observe(
            &fixture.root.join("missing"),
            DeploymentRootMode::DevMutable,
        ));
        assert_eq!(cache.cache_stats().deployment_fingerprint_missing, 1);

        let observed = DeploymentRootFingerprint::observe(
            &fixture.root,
            DeploymentRootMode::ImmutableDeclared,
        )
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
        let cache = IncludeCache::new(1);

        let first_resolved = cache
            .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
            .expect("first resolve");
        let first = cache
            .get_or_compile_include(&loader, &first_resolved, OptimizationLevel::O0)
            .expect("first compile");
        fixture.write("lib.php", "<?php echo 'two'; echo '!';\n");
        let second_resolved = cache
            .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
            .expect("second resolve");
        let second = cache
            .get_or_compile_include(&loader, &second_resolved, OptimizationLevel::O0)
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
        let cache = IncludeCache::new(1);
        let resolved = cache
            .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
            .expect("resolve include");
        let first = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
            .expect("first compile");

        replace_preserving_metadata(
            &path,
            "<?php class CachedPrimary { public $other = null; }\n",
        );
        let resolved_after_replace = cache
            .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
            .expect("resolve replacement");
        let second = cache
            .get_or_compile_include(&loader, &resolved_after_replace, OptimizationLevel::O0)
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
        let cache = IncludeCache::new(1);
        let resolved = cache
            .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
            .expect("resolve include");
        let first = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
            .expect("first compile");

        rewrite_preserving_metadata(
            &path,
            "<?php class CachedMutable { public $other = null; }\n",
        );
        let second = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
            .expect("compile rewritten source");

        assert!(!Arc::ptr_eq(&first, &second));
        assert!(class_has_property(&second, "cachedmutable", "other"));
        assert!(!class_has_property(&second, "cachedmutable", "first"));
    }

    #[test]
    fn include_cache_keys_compiled_units_by_optimization_level() {
        let fixture = IncludeCacheFixture::new("compiled-optimization");
        fixture.write("lib.php", "<?php echo 1 + 2;\n");
        let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
        let cache = IncludeCache::new(1);
        let resolved = loader
            .resolve_with_include_path(None, "lib.php", &[], Some(&fixture.root))
            .expect("resolve include");

        let baseline = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
            .expect("baseline include compile");
        let optimized = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O2)
            .expect("optimized include compile");
        let stats = cache.cache_stats();

        assert_eq!(stats.compile_misses, 2);
        assert_eq!(stats.compile_hits, 0);
        assert!(binary_add_count(&baseline) > 0);
        assert_eq!(binary_add_count(&optimized), 0);
    }

    #[test]
    fn compiler_identity_includes_version_profile_and_optimization() {
        let baseline = IncludeCompilerIdentity::current(OptimizationLevel::O0, 0);
        let optimized = IncludeCompilerIdentity::current(OptimizationLevel::O2, 0);
        let different_version = IncludeCompilerIdentity {
            compiler_version: "different",
            ..baseline.clone()
        };
        let different_profile = IncludeCompilerIdentity {
            debug_assertions: !baseline.debug_assertions,
            ..baseline.clone()
        };
        let different_dependencies = IncludeCompilerIdentity {
            compilation_dependency_fingerprint: 1,
            ..baseline.clone()
        };

        assert_ne!(baseline, optimized);
        assert_ne!(baseline, different_version);
        assert_ne!(baseline, different_profile);
        assert_ne!(baseline, different_dependencies);
    }

    #[test]
    fn missing_trait_resolution_uses_typed_payload_not_rendered_message() {
        let source = concat!(
            "<?php namespace Demo; ",
            "use Vendor\\Package\\Odd_used_by_Trait as LocalAlias; ",
            "class Owner { use LocalAlias; }"
        );
        let frontend = php_semantics::analyze_source(source);
        let mut lowering = php_ir::lower_frontend_result(
            &frontend,
            php_ir::LoweringOptions {
                source_path: "typed-trait.php".to_string(),
                source_text: Some(source.to_string()),
                ..php_ir::LoweringOptions::default()
            },
        );

        let expected = vec!["vendor\\package\\odd_used_by_trait".to_string()];
        assert_eq!(missing_local_trait_names(&lowering), expected);
        lowering
            .diagnostics
            .iter_mut()
            .find(|diagnostic| diagnostic.missing_trait().is_some())
            .expect("missing-trait diagnostic")
            .message = "rendering can change without changing compiler control flow".to_string();
        assert_eq!(missing_local_trait_names(&lowering), expected);
    }

    #[test]
    fn mutable_compiled_include_cache_validates_content_on_hit() {
        let fixture = IncludeCacheFixture::new("compiled-hit-content-validation");
        fixture.write("lib.php", "<?php echo 'cached';\n");
        let loader = IncludeLoader::for_root(&fixture.root).expect("loader");
        let cache = IncludeCache::new(1);
        let resolved = cache
            .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
            .expect("resolve include");

        let first = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
            .expect("first compile");
        let source_reads_after_first = cache.cache_stats().source_reads;
        let second = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
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
        let cache = IncludeCache::new(1);
        cache.set_deployment_root_fingerprint(DeploymentRootFingerprint::observe(
            &fixture.root,
            DeploymentRootMode::ImmutableDeclared,
        ));
        let resolved = cache
            .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
            .expect("resolve include");

        let first = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
            .expect("first compile");
        let second = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
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
        let cache = Arc::new(IncludeCache::new(4));
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
                        .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
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
        let cache = IncludeCache::new(1);
        let resolved = loader
            .resolve_with_include_path(
                None,
                "src/Providers/ProviderRegistry.php",
                &[],
                Some(&fixture.root),
            )
            .expect("resolve include");

        let compiled = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
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
        let cache = IncludeCache::new(1);
        let resolved = loader
            .resolve_with_include_path(None, "src/Registry.php", &[], Some(&fixture.root))
            .expect("resolve include");

        let error = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
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
        let cache = IncludeCache::new(1);
        let resolved = first_loader
            .resolve_with_include_path(None, "src/Registry.php", &[], Some(&fixture.root))
            .expect("resolve include");

        let first = cache
            .get_or_compile_include(&first_loader, &resolved, OptimizationLevel::O0)
            .expect("compile first mapping");
        let second = cache
            .get_or_compile_include(&second_loader, &resolved, OptimizationLevel::O0)
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
        let cache = IncludeCache::new(1);
        let resolved = loader
            .resolve_with_include_path(None, "src/Registry.php", &[], Some(&fixture.root))
            .expect("resolve include");

        let compiled = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
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
        let cache = IncludeCache::new(1);
        let resolved = loader
            .resolve_with_include_path(None, "src/Registry.php", &[], Some(&fixture.root))
            .expect("resolve include");

        let error = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
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
        let cache = IncludeCache::new(1);
        let resolved = loader
            .resolve_with_include_path(None, "src/Registry.php", &[], Some(&fixture.root))
            .expect("resolve include");

        let error = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
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
        let cache = IncludeCache::new(1);
        let resolved = loader
            .resolve_with_include_path(None, "src/A.php", &[], Some(&fixture.root))
            .expect("resolve include");

        let first = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
            .expect_err("dependency cycle must fail");
        let second = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
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
        let cache = IncludeCache::new(1);
        let resolved = loader
            .resolve_with_include_path(
                None,
                "src/Providers/ProviderRegistry.php",
                &[],
                Some(&fixture.root),
            )
            .expect("resolve include");
        let first = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
            .expect("first compile");

        std::thread::sleep(std::time::Duration::from_millis(2));
        fixture.write(
            "src/Providers/Http/Traits/WithHttpTransporterTrait.php",
            "<?php\nnamespace Demo\\Providers\\Http\\Traits;\ntrait WithHttpTransporterTrait { private $second = null; }\n",
        );
        let second = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
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
        let cache = IncludeCache::new(1);
        let resolved = loader
            .resolve_with_include_path(
                None,
                "src/Providers/ProviderRegistry.php",
                &[],
                Some(&fixture.root),
            )
            .expect("resolve include");
        let first = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
            .expect("first compile");

        replace_preserving_metadata(
            &trait_path,
            "<?php\nnamespace Demo\\Providers\\Http\\Traits;\ntrait WithHttpTransporterTrait { private $other = null; }\n",
        );
        let second = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
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
        let cache = IncludeCache::new(1);
        let first_resolved = cache
            .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
            .expect("resolve first target");
        let first = cache
            .get_or_compile_include(&loader, &first_resolved, OptimizationLevel::O0)
            .expect("compile first target");

        fs::remove_file(&link).expect("remove first symlink");
        symlink(fixture.root.join("other.php"), &link).expect("create replacement symlink");
        let second_resolved = cache
            .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
            .expect("resolve replacement target");
        let second = cache
            .get_or_compile_include(&loader, &second_resolved, OptimizationLevel::O0)
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
        fs::write(fixture.root.join("cwd/cwd-only.php"), "<?php echo 'cwd';\n")
            .expect("cwd include");
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
        let cache = IncludeCache::new(1);
        poison_mutex(&cache.resolution_shards[0]);

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
        let cache = IncludeCache::new(1);
        let resolved = loader
            .resolve_with_include_path(None, "lib.php", &[], Some(&fixture.root))
            .expect("resolve include");
        poison_mutex(&cache.compile_shards[0]);

        let error = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
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
        let cache = IncludeCache::new(1);
        let resolved = loader
            .resolve_with_include_path(None, "lib.php", &[], Some(&fixture.root))
            .expect("resolve include");
        poison_mutex(&cache.compile_locks[0].in_progress);

        let error = cache
            .get_or_compile_include(&loader, &resolved, OptimizationLevel::O0)
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
}
