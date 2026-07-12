//! Include resolution, allowed-root policy, and stream adapters.

use super::diagnostics::{include_error, include_error_suggestion};
use super::source::{
    IncludeDirectoryVersion, IncludePathFileFingerprint, LoadedInclude, OpenedSourceIdentity,
    ValidatedIncludeSource, fnv1a_64, include_directory_version, include_path_file_fingerprint,
    php_source_from_bytes, read_validated_file,
};
use crate::error::VmError;
use php_diagnostics::{
    DiagnosticEnvelope, DiagnosticLayer, DiagnosticPhase, DiagnosticSeverity, DiagnosticSuggestion,
};
use php_runtime::api::{FilesystemCapabilities, normalize_class_name, phar};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

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
    pub(super) guards: Option<Vec<NegativeProbeGuard>>,
}

impl NegativeProbeTrace {
    pub(super) fn uncacheable() -> Self {
        Self { guards: None }
    }
}

/// One probed missing candidate and the parent-directory version observed
/// before the probe. The candidate path itself is rechecked on replay so files
/// that appear on filesystems with coarse directory-mtime granularity still
/// invalidate the cached miss.
#[derive(Clone, Debug)]
pub(super) struct NegativeProbeGuard {
    pub(super) candidate: PathBuf,
    pub(super) directory: PathBuf,
    pub(super) directory_version: IncludeDirectoryVersion,
}

/// One cached missing-include resolution: the deterministic error to replay
/// plus the directory-version guards that must all still match for the entry
/// to be served.
#[derive(Clone, Debug)]
pub(super) struct NegativeIncludeEntry {
    pub(super) error: VmError,
    pub(super) guards: Vec<NegativeProbeGuard>,
}

impl NegativeIncludeEntry {
    pub(super) fn is_still_valid(&self) -> bool {
        self.guards.iter().all(|guard| {
            fs::symlink_metadata(&guard.candidate).is_err()
                && include_directory_version(&guard.directory)
                    .is_some_and(|current| current == guard.directory_version)
        })
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
        self.compilation_dependencies
            .insert(normalize_class_name(declaration.as_ref()), path.into());
        self
    }

    fn compilation_dependency(&self, declaration: &str) -> Option<&Path> {
        self.compilation_dependencies
            .get(&normalize_class_name(declaration))
            .map(PathBuf::as_path)
    }

    /// Fingerprints explicit declaration-to-file mappings for compiler caches.
    #[must_use]
    pub fn compilation_dependency_fingerprint(&self) -> u64 {
        let mut serialized = Vec::new();
        for (declaration, path) in &self.compilation_dependencies {
            serialized.extend_from_slice(declaration.as_bytes());
            serialized.push(0);
            serialized.extend_from_slice(path.to_string_lossy().as_bytes());
            serialized.push(b'\n');
        }
        fnv1a_64(&serialized)
    }

    /// Loads the explicitly mapped source for a normalized declaration name.
    ///
    /// Resolution and root enforcement remain loader responsibilities. The
    /// compiler validates that the source actually provides the declaration.
    pub fn load_compilation_dependency(
        &self,
        declaration: &str,
    ) -> Result<Option<ValidatedIncludeSource>, VmError> {
        let Some(path) = self.compilation_dependency(declaration) else {
            return Ok(None);
        };
        let path = path.to_string_lossy();
        let resolved = self.resolve_with_include_path(
            None,
            &path,
            &[],
            self.allowed_roots.first().map(PathBuf::as_path),
        )?;
        self.load_validated_resolved(&resolved).map(Some)
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

    pub fn load_validated_resolved(
        &self,
        resolved: &ResolvedIncludePath,
    ) -> Result<ValidatedIncludeSource, VmError> {
        let canonical_text = resolved.canonical_path.to_string_lossy();
        if phar::is_phar_uri(&canonical_text) {
            let loaded = self.load_phar_include(&canonical_text)?;
            let bytes_hashed = loaded.source.len() as u64;
            return Ok(ValidatedIncludeSource {
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
