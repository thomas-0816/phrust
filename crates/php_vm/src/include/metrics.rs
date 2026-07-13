//! Typed include-cache counters and immutable reporting snapshots.

use super::resolver::ResolvedIncludePath;
use super::source::include_directory_version;
use std::sync::atomic::{AtomicU64, Ordering};

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
    pub immutable_release_hits: u64,
    pub negative_cache_hits: u64,
    pub negative_cache_installs: u64,
    pub negative_cache_invalidations: u64,
    pub negative_cache_blocked_unversioned: u64,
    pub negative_cache_blocked_capacity: u64,
}

#[derive(Debug, Default)]
pub(super) struct IncludeCacheCounters {
    pub(super) resolution_hits: AtomicU64,
    pub(super) resolution_misses: AtomicU64,
    pub(super) compile_hits: AtomicU64,
    pub(super) compile_misses: AtomicU64,
    pub(super) source_reads: AtomicU64,
    pub(super) source_bytes_hashed: AtomicU64,
    pub(super) content_validations: AtomicU64,
    pub(super) identity_only_hits: AtomicU64,
    pub(super) content_mismatches: AtomicU64,
    pub(super) conservative_misses: AtomicU64,
    pub(super) dependency_metadata_validations: AtomicU64,
    pub(super) stale_invalidations: AtomicU64,
    pub(super) stale_dependency_invalidations: AtomicU64,
    pub(super) compile_errors: AtomicU64,
    pub(super) directory_version_hits: AtomicU64,
    pub(super) directory_version_misses: AtomicU64,
    pub(super) composer_fingerprint_stale: AtomicU64,
    pub(super) deployment_fingerprint_present: AtomicU64,
    pub(super) deployment_fingerprint_missing: AtomicU64,
    pub(super) deployment_fingerprint_stale: AtomicU64,
    pub(super) immutable_release_hits: AtomicU64,
    pub(super) negative_cache_hits: AtomicU64,
    pub(super) negative_cache_installs: AtomicU64,
    pub(super) negative_cache_invalidations: AtomicU64,
    pub(super) negative_cache_blocked_unversioned: AtomicU64,
    pub(super) negative_cache_blocked_capacity: AtomicU64,
}

impl IncludeCacheCounters {
    /// Returns current cache counters.
    #[must_use]
    pub(super) fn snapshot(&self) -> IncludeCacheStats {
        IncludeCacheStats {
            resolution_hits: self.resolution_hits.load(Ordering::Relaxed),
            resolution_misses: self.resolution_misses.load(Ordering::Relaxed),
            compile_hits: self.compile_hits.load(Ordering::Relaxed),
            compile_misses: self.compile_misses.load(Ordering::Relaxed),
            source_reads: self.source_reads.load(Ordering::Relaxed),
            source_bytes_hashed: self.source_bytes_hashed.load(Ordering::Relaxed),
            content_validations: self.content_validations.load(Ordering::Relaxed),
            identity_only_hits: self.identity_only_hits.load(Ordering::Relaxed),
            content_mismatches: self.content_mismatches.load(Ordering::Relaxed),
            conservative_misses: self.conservative_misses.load(Ordering::Relaxed),
            dependency_metadata_validations: self
                .dependency_metadata_validations
                .load(Ordering::Relaxed),
            stale_invalidations: self.stale_invalidations.load(Ordering::Relaxed),
            stale_dependency_invalidations: self
                .stale_dependency_invalidations
                .load(Ordering::Relaxed),
            compile_errors: self.compile_errors.load(Ordering::Relaxed),
            directory_version_hits: self.directory_version_hits.load(Ordering::Relaxed),
            directory_version_misses: self.directory_version_misses.load(Ordering::Relaxed),
            composer_fingerprint_stale: self.composer_fingerprint_stale.load(Ordering::Relaxed),
            deployment_fingerprint_present: self
                .deployment_fingerprint_present
                .load(Ordering::Relaxed),
            deployment_fingerprint_missing: self
                .deployment_fingerprint_missing
                .load(Ordering::Relaxed),
            deployment_fingerprint_stale: self.deployment_fingerprint_stale.load(Ordering::Relaxed),
            immutable_release_hits: self.immutable_release_hits.load(Ordering::Relaxed),
            negative_cache_hits: self.negative_cache_hits.load(Ordering::Relaxed),
            negative_cache_installs: self.negative_cache_installs.load(Ordering::Relaxed),
            negative_cache_invalidations: self.negative_cache_invalidations.load(Ordering::Relaxed),
            negative_cache_blocked_unversioned: self
                .negative_cache_blocked_unversioned
                .load(Ordering::Relaxed),
            negative_cache_blocked_capacity: self
                .negative_cache_blocked_capacity
                .load(Ordering::Relaxed),
        }
    }

    /// Compares the stored parent-directory version against the current one
    /// and records the directory-version counters. Metadata only: this never
    /// affects whether the resolution hit is accepted — it measures how often
    /// a future directory-version-validated negative cache would have been
    /// consistent.
    pub(super) fn observe_directory_version(&self, resolved: &ResolvedIncludePath) {
        let current = resolved
            .canonical_path
            .parent()
            .and_then(include_directory_version);
        let matches = match (&resolved.directory_version, &current) {
            (Some(stored), Some(current)) => stored == current,
            _ => false,
        };
        if matches {
            self.directory_version_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.directory_version_misses
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}
