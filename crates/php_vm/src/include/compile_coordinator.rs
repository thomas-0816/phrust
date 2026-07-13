//! Per-path stampede coordination for compiled includes.

use super::diagnostics::include_cache_lock_error;
use crate::error::VmError;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};

#[derive(Debug, Default)]
pub(super) struct IncludeCompileLockShard {
    pub(super) in_progress: Mutex<HashSet<PathBuf>>,
    condvar: Condvar,
}

pub(super) struct IncludeCompilePermit<'a> {
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

#[derive(Debug)]
pub(super) struct IncludeCompileCoordinator {
    pub(super) shards: Vec<IncludeCompileLockShard>,
}

impl IncludeCompileCoordinator {
    pub(super) fn new(shard_count: usize) -> Self {
        Self {
            shards: (0..shard_count)
                .map(|_| IncludeCompileLockShard::default())
                .collect(),
        }
    }

    pub(super) fn try_begin(
        &self,
        path: &Path,
    ) -> Result<Option<IncludeCompilePermit<'_>>, VmError> {
        let shard = &self.shards[self.shard_index(path)];
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

    pub(super) fn wait(&self, path: &Path) -> Result<(), VmError> {
        let shard = &self.shards[self.shard_index(path)];
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

    fn shard_index(&self, path: &Path) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        path.hash(&mut hasher);
        (hasher.finish() as usize) % self.shards.len()
    }
}
