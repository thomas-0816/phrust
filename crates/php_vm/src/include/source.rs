//! Opened include-source validation and portable file identity.

use super::diagnostics::{include_error, include_metadata_error, include_read_error};
use crate::error::VmError;
use std::fs::{self, File};
use std::io::Read as _;
use std::path::{Path, PathBuf};
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
pub struct ValidatedIncludeSource {
    pub(super) loaded: LoadedInclude,
    pub(super) identity: OpenedSourceIdentity,
    pub(super) bytes_hashed: u64,
}

impl ValidatedIncludeSource {
    /// Returns the exact source bytes and canonical path validated by the loader.
    #[must_use]
    pub const fn loaded(&self) -> &LoadedInclude {
        &self.loaded
    }

    /// Consumes the validation envelope and returns its source payload.
    #[must_use]
    pub fn into_loaded(self) -> LoadedInclude {
        self.loaded
    }

    /// Consumes the source and preserves only cache-validation metadata.
    #[must_use]
    pub fn into_dependency(self) -> IncludeDependency {
        IncludeDependency {
            canonical_path: self.loaded.canonical_path,
            source_identity: self.identity,
        }
    }
}

/// Opaque identity metadata for one source consumed during compilation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct IncludeDependency {
    pub(super) canonical_path: PathBuf,
    pub(super) source_identity: OpenedSourceIdentity,
}

/// Identity of the exact bytes read from one stable opened file generation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct OpenedSourceIdentity {
    pub(super) generation: IncludePathFileFingerprint,
    pub(super) content_hash: u64,
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
    pub(super) fn has_reliable_generation(&self) -> bool {
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

pub(super) fn read_validated_file(path: &Path) -> Result<ValidatedIncludeSource, VmError> {
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
        return Ok(ValidatedIncludeSource {
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

pub(super) fn php_source_from_bytes(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(error) => error.into_bytes().into_iter().map(char::from).collect(),
    }
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
