//! Local include/require loader for the runtime VM MVP.

use crate::compiled_unit::CompiledUnit;
use crate::error::VmError;
use php_diagnostics::{
    DiagnosticEnvelope, DiagnosticLayer, DiagnosticPhase, DiagnosticSeverity, DiagnosticSuggestion,
};
use php_optimizer::{OptimizationLevel, PassContext, PassPipeline};
use php_runtime::{FilesystemCapabilities, phar};
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
    ) -> Result<ResolvedIncludePath, VmError> {
        let key = IncludeResolutionKey::new(loader, including_file, path, include_path, cwd);
        let shard_index = self.resolution_shard_index(&key);
        {
            let mut shard = self.resolution_shards[shard_index]
                .lock()
                .map_err(|_| include_cache_lock_error("resolution", "lookup"))?;
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
            .map_err(|_| include_cache_lock_error("resolution", "insert"))?;
        shard.entry(key).or_insert_with(|| resolved.clone());
        Ok(resolved)
    }

    /// Returns a compiled include unit for a resolved path, compiling on miss.
    pub fn get_or_compile_include(
        &self,
        loader: &IncludeLoader,
        resolved: &ResolvedIncludePath,
        optimization_level: OptimizationLevel,
    ) -> Result<Arc<CompiledUnit>, VmError> {
        loop {
            let key = CompiledIncludeKey::new(resolved, optimization_level);
            let shard_index = self.compile_shard_index(&key);
            {
                let mut shard = self.compile_shards[shard_index]
                    .lock()
                    .map_err(|_| include_cache_lock_error("compiled", "lookup"))?;
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

            let Some(_permit) = self.try_begin_compile(&resolved.canonical_path)? else {
                self.wait_for_compile(&resolved.canonical_path)?;
                continue;
            };

            {
                let shard = self.compile_shards[shard_index]
                    .lock()
                    .map_err(|_| include_cache_lock_error("compiled", "lookup-after-wait"))?;
                if let Some(compiled) = shard.get(&key) {
                    self.stats.compile_hits.fetch_add(1, Ordering::Relaxed);
                    return Ok(Arc::clone(compiled));
                }
            }

            self.stats.compile_misses.fetch_add(1, Ordering::Relaxed);
            let compiled = match compile_include(loader, resolved, optimization_level) {
                Ok(compiled) => {
                    let compiled = Arc::new(compiled);
                    let mut shard = self.compile_shards[shard_index]
                        .lock()
                        .map_err(|_| include_cache_lock_error("compiled", "insert"))?;
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
    pub fn clear(&self) -> Result<(), VmError> {
        for shard in &self.resolution_shards {
            shard
                .lock()
                .map_err(|_| include_cache_lock_error("resolution", "clear"))?
                .clear();
        }
        for shard in &self.compile_shards {
            shard
                .lock()
                .map_err(|_| include_cache_lock_error("compiled", "clear"))?
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
        Ok(Self { allowed_roots })
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
        if self.allowed_roots.is_empty() {
            return Err(include_error(
                "E_PHP_VM_INCLUDE_DISABLED",
                "include loader has no allowed roots",
            ));
        }
        if phar::is_phar_uri(path) {
            return self.resolve_phar_include(path, cwd);
        }
        if path.contains("://") {
            return Err(include_error(
                "E_PHP_VM_INCLUDE_UNSUPPORTED_SCHEME",
                format!("stream include `{path}` is not supported"),
            )
            .with_context("path", path));
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
        let mut last_error = None;
        let mut canonical = None;
        for candidate in candidates {
            match fs::canonicalize(&candidate) {
                Ok(path) => {
                    canonical = Some(path);
                    break;
                }
                Err(error) => {
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
        let canonical = canonical.ok_or_else(|| {
            last_error.unwrap_or_else(|| {
                include_error("E_PHP_VM_INCLUDE_MISSING", format!("{path}: not found"))
                    .with_context("path", path)
            })
        })?;
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
        let fingerprint = include_path_file_fingerprint(&canonical)?;
        Ok(ResolvedIncludePath {
            canonical_path: canonical,
            fingerprint,
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
        let source = fs::read_to_string(&canonical).map_err(|error| {
            include_error(
                "E_PHP_VM_INCLUDE_READ",
                format!("{}: {error}", canonical.display()),
            )
            .with_context("canonical_path", canonical.display())
        })?;
        Ok(LoadedInclude {
            canonical_path: canonical,
            source,
        })
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
            fingerprint,
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
    let metadata = fs::metadata(path).map_err(|error| {
        include_error(
            "E_PHP_VM_INCLUDE_METADATA",
            format!("{}: {error}", path.display()),
        )
        .with_context("path", path.display())
    })?;
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
    len: u64,
    modified_unix_nanos: Option<u128>,
    readonly: bool,
    compiler_version: &'static str,
    debug_assertions: bool,
    optimization_level: &'static str,
}

impl CompiledIncludeKey {
    fn new(resolved: &ResolvedIncludePath, optimization_level: OptimizationLevel) -> Self {
        Self {
            canonical_path: resolved.canonical_path.clone(),
            len: resolved.fingerprint.len,
            modified_unix_nanos: resolved.fingerprint.modified_unix_nanos,
            readonly: resolved.fingerprint.readonly,
            compiler_version: env!("CARGO_PKG_VERSION"),
            debug_assertions: cfg!(debug_assertions),
            optimization_level: optimization_level.as_str(),
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
    optimization_level: OptimizationLevel,
) -> Result<CompiledUnit, VmError> {
    let loaded = loader.load_resolved(resolved.canonical_path.clone())?;
    compile_loaded_include(loaded, optimization_level)
}

fn compile_loaded_include(
    loaded: LoadedInclude,
    optimization_level: OptimizationLevel,
) -> Result<CompiledUnit, VmError> {
    let frontend = php_semantics::analyze_source(&loaded.source);
    if frontend.has_errors() {
        return Err(include_error(
            "E_PHP_VM_INCLUDE_COMPILE_ERROR",
            format!(
                "{} failed frontend analysis",
                loaded.canonical_path.display()
            ),
        )
        .with_context("path", loaded.canonical_path.display())
        .with_context("stage", "frontend"));
    }
    let mut lowering = php_ir::lower_frontend_result(
        &frontend,
        php_ir::LoweringOptions {
            source_path: loaded.canonical_path.to_string_lossy().into_owned(),
            source_text: Some(loaded.source),
            ..php_ir::LoweringOptions::default()
        },
    );
    if !lowering.diagnostics.is_empty() || lowering.verification.is_err() {
        return Err(include_error(
            "E_PHP_VM_INCLUDE_COMPILE_ERROR",
            format!("{} failed IR lowering", loaded.canonical_path.display()),
        )
        .with_context("path", loaded.canonical_path.display())
        .with_context("stage", "ir_lowering"));
    }
    if optimization_level.runs_pipeline() {
        PassPipeline::performance()
            .run(&mut lowering.unit, &PassContext::new(optimization_level))
            .map_err(|error| {
                include_error(
                    "E_PHP_VM_INCLUDE_COMPILE_ERROR",
                    format!(
                        "{} optimizer failed: {error}",
                        loaded.canonical_path.display()
                    ),
                )
                .with_context("path", loaded.canonical_path.display())
                .with_context("stage", "optimizer")
            })?;
    }
    Ok(CompiledUnit::new(lowering.unit))
}

fn default_include_cache_shards() -> usize {
    std::thread::available_parallelism().map_or(16, |count| count.get().clamp(1, 64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use php_ir::instruction::{BinaryOp, InstructionKind};
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
            .get_or_compile_include(&loader, &first_resolved, OptimizationLevel::O0)
            .expect("first compile");
        fixture.write("lib.php", "<?php echo 'two';\n");
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
            fs::write(self.root.join(name), source).expect("write include cache fixture file");
        }
    }

    impl Drop for IncludeCacheFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
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
