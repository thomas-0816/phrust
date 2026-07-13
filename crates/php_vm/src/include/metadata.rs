//! Composer/autoload and deployment-root metadata.

use super::metrics::IncludeCacheCounters;
use super::source::{
    IncludeDirectoryVersion, fnv1a_64, include_directory_version, include_path_file_fingerprint,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

/// Cross-request transition of the observed Composer map fingerprint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerFingerprintTransition {
    /// First observation in this process, or unchanged since the last one.
    Unchanged,
    /// The fingerprint differs from the previous request's observation — the
    /// deployment's autoload maps changed while the process was running.
    Changed,
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
    /// swapped release directory). Cached paths and compiled artifacts beneath
    /// the observed root are trusted until explicit cache clear or restart.
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
/// mutability mode. Immutable mode is an explicit source-validation policy;
/// development mode remains the default and validates source on cache hits.
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

#[derive(Debug)]
pub(super) struct IncludeMetadataState {
    stats: Arc<IncludeCacheCounters>,
    deployment_root: Mutex<Option<DeploymentRootFingerprint>>,
    composer_last_fingerprint: Mutex<Option<Option<String>>>,
}

impl IncludeMetadataState {
    pub(super) fn new(stats: Arc<IncludeCacheCounters>) -> Self {
        Self {
            stats,
            deployment_root: Mutex::new(None),
            composer_last_fingerprint: Mutex::new(None),
        }
    }

    /// Installs the deployment-root fingerprint for this process. Counts
    /// `deployment_fingerprint_present` when the root was observable and
    /// `deployment_fingerprint_missing` otherwise; a `None` fingerprint keeps
    /// the slot empty so later revalidations keep counting `missing`.
    pub(super) fn set_deployment_root_fingerprint(
        &self,
        fingerprint: Option<DeploymentRootFingerprint>,
    ) {
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
    pub(super) fn revalidate_deployment_root(&self) {
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
    pub(super) fn note_composer_fingerprint(
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

    pub(super) fn trusts_immutable_path(&self, path: &Path) -> bool {
        let Ok(root) = self.deployment_root.lock() else {
            return false;
        };
        root.as_ref().is_some_and(|root| {
            root.mode == DeploymentRootMode::ImmutableDeclared
                && path.starts_with(&root.canonical_root)
        })
    }

    pub(super) fn deployment_root_fingerprint(
        &self,
    ) -> Result<Option<DeploymentRootFingerprint>, ()> {
        self.deployment_root
            .lock()
            .map(|fingerprint| fingerprint.clone())
            .map_err(|_| ())
    }
}
