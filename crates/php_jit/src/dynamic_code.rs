//! Compile-once coordination for native include/eval and runtime declarations.
//!
//! The coordinator owns publication, not frontend compilation. Its callback is
//! deliberately invoked without holding the map or slot lock, so a compiler may
//! recursively compile a different include/eval key without deadlocking.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::ThreadId;

/// Exact identity of one dynamic source compilation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DynamicCodeCacheKey {
    pub source_identity: String,
    pub source_hash: u64,
    pub dependency_identity: String,
    pub semantic_config_hash: u64,
    pub runtime_abi_hash: u64,
    pub target_cpu: String,
}

impl DynamicCodeCacheKey {
    /// Deterministic restart-cache key. Absolute entry addresses are excluded.
    #[must_use]
    pub fn restart_cache_key(&self) -> String {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for bytes in [
            self.source_identity.as_bytes(),
            &self.source_hash.to_le_bytes(),
            self.dependency_identity.as_bytes(),
            &self.semantic_config_hash.to_le_bytes(),
            &self.runtime_abi_hash.to_le_bytes(),
            self.target_cpu.as_bytes(),
        ] {
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            hash ^= 0xff;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        format!("dynamic-{hash:016x}")
    }
}

/// One published native function/declaration entry in a dynamic unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicNativeEntry {
    pub function_id: u32,
    pub generation: u64,
    /// Process-local address. Restart-cache serialization must omit this field.
    pub address: usize,
}

/// Fully compiled dynamic unit. Publication occurs atomically only after every
/// entry is available, preventing any first instruction from running early.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicNativeArtifact {
    pub unit_identity: String,
    pub entry: DynamicNativeEntry,
    pub declarations: Vec<DynamicNativeEntry>,
    pub restart_cache_key: String,
}

/// Where a native dynamic artifact was obtained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicCodeCacheDisposition {
    Compiled,
    ProcessCache,
    RestartCache,
    WaitedForOwner,
}

/// Stable dynamic-compilation failure. It is cached for the exact source key,
/// so concurrent callers observe the same PHP-compatible compile result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicCodeCompileError {
    Compile(String),
    RecursiveSameKey(String),
    Poisoned(&'static str),
}

impl fmt::Display for DynamicCodeCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compile(message) => formatter.write_str(message),
            Self::RecursiveSameKey(key) => {
                write!(formatter, "recursive dynamic compilation for {key}")
            }
            Self::Poisoned(lock) => write!(formatter, "dynamic compilation {lock} lock poisoned"),
        }
    }
}

impl std::error::Error for DynamicCodeCompileError {}

#[derive(Debug)]
enum SlotState {
    Empty,
    Compiling(ThreadId),
    Ready(Arc<DynamicNativeArtifact>),
    Failed(DynamicCodeCompileError),
}

#[derive(Debug)]
struct CompileSlot {
    state: Mutex<SlotState>,
    ready: Condvar,
}

impl CompileSlot {
    fn new() -> Self {
        Self {
            state: Mutex::new(SlotState::Empty),
            ready: Condvar::new(),
        }
    }
}

/// Native dynamic-source process cache and stampede coordinator.
#[derive(Debug)]
pub struct DynamicCodeCompileOnce {
    slots: Mutex<HashMap<DynamicCodeCacheKey, Arc<CompileSlot>>>,
    /// Validated relocatable artifacts loaded by the restart cache. Prompt 12
    /// owns persistence/validation; this layer owns exact-key participation.
    restart_artifacts: Mutex<HashMap<DynamicCodeCacheKey, Arc<DynamicNativeArtifact>>>,
    process_id: u32,
}

impl Default for DynamicCodeCompileOnce {
    fn default() -> Self {
        Self::new()
    }
}

impl DynamicCodeCompileOnce {
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
            restart_artifacts: Mutex::new(HashMap::new()),
            process_id: std::process::id(),
        }
    }

    /// Installs an already validated restart-cache artifact for exact-key use.
    pub fn install_restart_artifact(
        &self,
        key: DynamicCodeCacheKey,
        artifact: DynamicNativeArtifact,
    ) -> Result<(), DynamicCodeCompileError> {
        self.restart_artifacts
            .lock()
            .map_err(|_| DynamicCodeCompileError::Poisoned("restart-cache"))?
            .insert(key, Arc::new(artifact));
        Ok(())
    }

    /// Returns or compiles a complete native artifact exactly once.
    ///
    /// The compile callback runs outside all coordinator locks. A concurrent
    /// caller waits for atomic publication; same-thread recursion for the exact
    /// key fails explicitly instead of deadlocking.
    pub fn get_or_compile(
        &self,
        key: DynamicCodeCacheKey,
        compile: impl FnOnce() -> Result<DynamicNativeArtifact, DynamicCodeCompileError>,
    ) -> Result<(Arc<DynamicNativeArtifact>, DynamicCodeCacheDisposition), DynamicCodeCompileError>
    {
        if self.process_id != std::process::id() {
            return Err(DynamicCodeCompileError::Compile(
                "dynamic compiler must be reinitialized after fork".to_owned(),
            ));
        }
        let slot = self
            .slots
            .lock()
            .map_err(|_| DynamicCodeCompileError::Poisoned("map"))?
            .entry(key.clone())
            .or_insert_with(|| Arc::new(CompileSlot::new()))
            .clone();
        let current = std::thread::current().id();
        let mut waited = false;
        loop {
            let mut state = slot
                .state
                .lock()
                .map_err(|_| DynamicCodeCompileError::Poisoned("slot"))?;
            match &*state {
                SlotState::Ready(artifact) => {
                    return Ok((
                        Arc::clone(artifact),
                        if waited {
                            DynamicCodeCacheDisposition::WaitedForOwner
                        } else {
                            DynamicCodeCacheDisposition::ProcessCache
                        },
                    ));
                }
                SlotState::Failed(error) => return Err(error.clone()),
                SlotState::Compiling(owner) if *owner == current => {
                    return Err(DynamicCodeCompileError::RecursiveSameKey(
                        key.restart_cache_key(),
                    ));
                }
                SlotState::Compiling(_) => {
                    waited = true;
                    drop(
                        slot.ready
                            .wait(state)
                            .map_err(|_| DynamicCodeCompileError::Poisoned("wait"))?,
                    );
                }
                SlotState::Empty => {
                    if let Some(artifact) = self
                        .restart_artifacts
                        .lock()
                        .map_err(|_| DynamicCodeCompileError::Poisoned("restart-cache"))?
                        .get(&key)
                        .cloned()
                    {
                        *state = SlotState::Ready(Arc::clone(&artifact));
                        slot.ready.notify_all();
                        return Ok((artifact, DynamicCodeCacheDisposition::RestartCache));
                    }
                    *state = SlotState::Compiling(current);
                    drop(state);
                    let compiled = compile();
                    let mut state = slot
                        .state
                        .lock()
                        .map_err(|_| DynamicCodeCompileError::Poisoned("slot"))?;
                    match compiled {
                        Ok(artifact) => {
                            let artifact = Arc::new(artifact);
                            *state = SlotState::Ready(Arc::clone(&artifact));
                            slot.ready.notify_all();
                            return Ok((artifact, DynamicCodeCacheDisposition::Compiled));
                        }
                        Err(error) => {
                            *state = SlotState::Failed(error.clone());
                            slot.ready.notify_all();
                            return Err(error);
                        }
                    }
                }
            }
        }
    }

    /// Replaces inherited synchronization after `fork`. Call in the child
    /// before worker threads or dynamic compilation are started.
    pub fn reinitialize_after_fork(&mut self) {
        *self = Self::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};

    fn key(name: &str, hash: u64) -> DynamicCodeCacheKey {
        DynamicCodeCacheKey {
            source_identity: name.to_owned(),
            source_hash: hash,
            dependency_identity: "deps-v1".to_owned(),
            semantic_config_hash: 17,
            runtime_abi_hash: crate::JIT_RUNTIME_ABI_HASH,
            target_cpu: "test-cpu".to_owned(),
        }
    }

    fn artifact(key: &DynamicCodeCacheKey, address: usize) -> DynamicNativeArtifact {
        DynamicNativeArtifact {
            unit_identity: key.source_identity.clone(),
            entry: DynamicNativeEntry {
                function_id: 0,
                generation: 1,
                address,
            },
            declarations: vec![DynamicNativeEntry {
                function_id: 1,
                generation: 1,
                address: address + 1,
            }],
            restart_cache_key: key.restart_cache_key(),
        }
    }

    #[test]
    fn concurrent_dynamic_compile_waits_and_publishes_once() {
        let coordinator = Arc::new(DynamicCodeCompileOnce::new());
        let count = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(8));
        let source_key = key("include.php", 1);
        let threads = (0..8)
            .map(|_| {
                let coordinator = Arc::clone(&coordinator);
                let count = Arc::clone(&count);
                let barrier = Arc::clone(&barrier);
                let source_key = source_key.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    coordinator
                        .get_or_compile(source_key.clone(), || {
                            count.fetch_add(1, Ordering::SeqCst);
                            Ok(artifact(&source_key, 0x1000))
                        })
                        .map(|(artifact, _)| artifact.entry.address)
                })
            })
            .collect::<Vec<_>>();
        for thread in threads {
            assert_eq!(thread.join().expect("thread must finish"), Ok(0x1000));
        }
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn nested_dynamic_compile_uses_independent_key_without_deadlock() {
        let coordinator = DynamicCodeCompileOnce::new();
        let outer = key("outer.php", 1);
        let inner = key("inner.php", 2);
        let (compiled, _) = coordinator
            .get_or_compile(outer.clone(), || {
                let (nested, _) =
                    coordinator.get_or_compile(inner.clone(), || Ok(artifact(&inner, 0x2000)))?;
                assert_eq!(nested.entry.address, 0x2000);
                Ok(artifact(&outer, 0x1000))
            })
            .expect("nested compilation should publish");
        assert_eq!(compiled.entry.address, 0x1000);
    }

    #[test]
    fn exact_source_key_controls_process_and_restart_cache_reuse() {
        let coordinator = DynamicCodeCompileOnce::new();
        let original = key("eval:request:1", 7);
        coordinator
            .install_restart_artifact(original.clone(), artifact(&original, 0x3000))
            .expect("restart artifact should install");
        let (_, disposition) = coordinator
            .get_or_compile(original.clone(), || {
                panic!("restart cache should satisfy key")
            })
            .expect("restart artifact should resolve");
        assert_eq!(disposition, DynamicCodeCacheDisposition::RestartCache);

        let changed = key("eval:request:1", 8);
        let (_, disposition) = coordinator
            .get_or_compile(changed.clone(), || Ok(artifact(&changed, 0x4000)))
            .expect("changed source should compile");
        assert_eq!(disposition, DynamicCodeCacheDisposition::Compiled);
    }

    #[test]
    fn dynamic_compile_errors_are_explicit_and_cached() {
        let coordinator = DynamicCodeCompileOnce::new();
        let source_key = key("eval:parse-error", 9);
        let error = coordinator
            .get_or_compile(source_key.clone(), || {
                Err(DynamicCodeCompileError::Compile(
                    "PHP Parse error: unexpected token".to_owned(),
                ))
            })
            .expect_err("parse error should remain explicit");
        assert!(error.to_string().contains("PHP Parse error"));
        let second = coordinator.get_or_compile(source_key, || panic!("error must be cached"));
        assert!(matches!(second, Err(DynamicCodeCompileError::Compile(_))));
    }

    #[test]
    fn after_fork_reinitialization_discards_inherited_synchronization() {
        let mut coordinator = DynamicCodeCompileOnce::new();
        let source_key = key("before-fork.php", 10);
        coordinator
            .get_or_compile(source_key.clone(), || Ok(artifact(&source_key, 0x5000)))
            .expect("parent artifact should compile");
        coordinator.reinitialize_after_fork();
        let (_, disposition) = coordinator
            .get_or_compile(source_key.clone(), || Ok(artifact(&source_key, 0x6000)))
            .expect("child coordinator should compile independently");
        assert_eq!(disposition, DynamicCodeCacheDisposition::Compiled);
    }
}
