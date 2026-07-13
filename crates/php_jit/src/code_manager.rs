//! Process-wide, bounded ownership for Cranelift executable code.

use crate::JitFunctionHandle;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::default_libcall_names;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock, TryLockError};

const DEFAULT_CODE_LIMIT: usize = 64 * 1024 * 1024;
const DEFAULT_GENERATION_LIMIT: usize = 1024 * 1024;

/// Stable process-cache identity for one compiled specialization.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CraneliftCodeKey {
    /// Compiled-unit identity or IR fingerprint.
    pub compiled_unit: String,
    /// Function/region identity chosen by the frontend.
    pub region: String,
    /// Runtime ABI hash embedded in the native entry.
    pub abi_hash: u64,
    /// Effective compiler/runtime configuration hash.
    pub config_hash: u64,
    /// Layout or invalidation generation supplied by the runtime.
    pub invalidation_generation: u64,
    /// Versioned specialization spelling.
    pub specialization: String,
}

impl CraneliftCodeKey {
    /// Returns a deterministic unique Cranelift symbol for this cache key.
    #[must_use]
    pub fn symbol(&self) -> String {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for bytes in [
            self.compiled_unit.as_bytes(),
            self.region.as_bytes(),
            &self.abi_hash.to_le_bytes(),
            &self.config_hash.to_le_bytes(),
            &self.invalidation_generation.to_le_bytes(),
            self.specialization.as_bytes(),
        ] {
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            hash ^= 0xff;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        let specialization = self
            .specialization
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character
                } else {
                    '_'
                }
            })
            .collect::<String>();
        format!("phrust_cl_{specialization}_{hash:016x}")
    }
}

/// Immutable metadata shared by every clone of a published native handle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledRegionMetadata {
    /// Stable process-cache key.
    pub key: CraneliftCodeKey,
    /// Native code bytes attributed by Cranelift.
    pub code_bytes: usize,
}

#[derive(Debug)]
struct CodeManagerMetrics {
    process_cache_hits: AtomicU64,
    process_cache_misses: AtomicU64,
    compile_waits: AtomicU64,
    duplicate_compiles_avoided: AtomicU64,
    compile_count: AtomicU64,
    bytes_live: AtomicUsize,
    bytes_retired: AtomicUsize,
    generations_live: AtomicUsize,
    active_handles: AtomicUsize,
    evictions: AtomicU64,
}

impl Default for CodeManagerMetrics {
    fn default() -> Self {
        Self {
            process_cache_hits: AtomicU64::new(0),
            process_cache_misses: AtomicU64::new(0),
            compile_waits: AtomicU64::new(0),
            duplicate_compiles_avoided: AtomicU64::new(0),
            compile_count: AtomicU64::new(0),
            bytes_live: AtomicUsize::new(0),
            bytes_retired: AtomicUsize::new(0),
            generations_live: AtomicUsize::new(0),
            active_handles: AtomicUsize::new(0),
            evictions: AtomicU64::new(0),
        }
    }
}

/// Snapshot of process-wide Cranelift code ownership and cache activity.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CraneliftCodeManagerStats {
    pub process_cache_hits: u64,
    pub process_cache_misses: u64,
    pub compile_waits: u64,
    pub duplicate_compiles_avoided: u64,
    pub compile_count: u64,
    pub code_bytes_live: usize,
    pub code_bytes_retired: usize,
    pub code_generations: usize,
    pub active_handles: usize,
    pub evictions: u64,
    pub eviction_candidates: usize,
}

/// Exact per-request process-cache event plus post-operation ownership gauges.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CraneliftCodeManagerEvent {
    pub process_cache_hits: u64,
    pub process_cache_misses: u64,
    pub compile_waits: u64,
    pub duplicate_compiles_avoided: u64,
    pub code_bytes_live: usize,
    pub code_bytes_retired: usize,
    pub code_generations: usize,
    pub evictions: u64,
}

struct CodeGeneration {
    id: u64,
    module: Mutex<Option<JITModule>>,
    bytes: AtomicUsize,
    metrics: Arc<CodeManagerMetrics>,
}

impl fmt::Debug for CodeGeneration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodeGeneration")
            .field("id", &self.id)
            .field("bytes", &self.bytes.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl Drop for CodeGeneration {
    fn drop(&mut self) {
        let bytes = self.bytes.load(Ordering::Relaxed);
        if let Ok(module) = self.module.get_mut()
            && let Some(module) = module.take()
        {
            // SAFETY: the generation is dropped only after its manager/cache
            // owner and every published handle have released their `Arc`.
            unsafe { module.free_memory() };
        }
        self.metrics.bytes_live.fetch_sub(bytes, Ordering::Relaxed);
        self.metrics
            .generations_live
            .fetch_sub(1, Ordering::Relaxed);
    }
}

/// Generation-bound lifetime token for an immutable native entry.
pub struct SharedJitCodeHandle {
    generation: Arc<CodeGeneration>,
    entry: usize,
    metadata: Arc<CompiledRegionMetadata>,
}

impl SharedJitCodeHandle {
    fn new(
        generation: Arc<CodeGeneration>,
        entry: usize,
        metadata: Arc<CompiledRegionMetadata>,
    ) -> Self {
        generation
            .metrics
            .active_handles
            .fetch_add(1, Ordering::Relaxed);
        Self {
            generation,
            entry,
            metadata,
        }
    }

    /// Generation that owns the executable allocation.
    #[must_use]
    pub fn generation_id(&self) -> u64 {
        self.generation.id
    }

    /// Published entry address, represented as an integer to remain `Send`.
    #[must_use]
    pub fn entry_address(&self) -> usize {
        self.entry
    }

    /// Stable metadata for the compiled region.
    #[must_use]
    pub fn metadata(&self) -> &CompiledRegionMetadata {
        &self.metadata
    }
}

impl Clone for SharedJitCodeHandle {
    fn clone(&self) -> Self {
        Self::new(
            Arc::clone(&self.generation),
            self.entry,
            Arc::clone(&self.metadata),
        )
    }
}

impl Drop for SharedJitCodeHandle {
    fn drop(&mut self) {
        self.generation
            .metrics
            .active_handles
            .fetch_sub(1, Ordering::Relaxed);
    }
}

impl fmt::Debug for SharedJitCodeHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SharedJitCodeHandle")
            .field("generation", &self.generation.id)
            .field("entry", &format_args!("0x{:x}", self.entry))
            .field("metadata", &self.metadata)
            .finish()
    }
}

impl PartialEq for SharedJitCodeHandle {
    fn eq(&self, other: &Self) -> bool {
        self.generation.id == other.generation.id
            && self.entry == other.entry
            && self.metadata == other.metadata
    }
}

impl Eq for SharedJitCodeHandle {}

/// Whether a compile request produced code or reused an existing publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CraneliftCodeCacheDisposition {
    Hit,
    Compiled,
}

/// Result returned by the process-level compile-once boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedJitFunction {
    pub handle: JitFunctionHandle,
    pub code_bytes: u64,
    pub disposition: CraneliftCodeCacheDisposition,
}

/// Typed failures from code-manager ownership and synchronization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CraneliftCodeManagerError {
    NativeTarget(String),
    Flags(String),
    Poisoned(&'static str),
    HelperAddressConflict { symbol: String },
    CodeLimit { limit: usize, live: usize },
}

impl fmt::Display for CraneliftCodeManagerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NativeTarget(message) | Self::Flags(message) => formatter.write_str(message),
            Self::Poisoned(name) => {
                write!(formatter, "Cranelift code-manager {name} lock is poisoned")
            }
            Self::HelperAddressConflict { symbol } => {
                write!(formatter, "runtime helper `{symbol}` changed address")
            }
            Self::CodeLimit { limit, live } => write!(
                formatter,
                "Cranelift code limit is exhausted: limit={limit} live={live}"
            ),
        }
    }
}

impl std::error::Error for CraneliftCodeManagerError {}

struct ManagerState {
    next_generation: u64,
    active: Arc<CodeGeneration>,
    generations: VecDeque<Arc<CodeGeneration>>,
    cache: HashMap<CraneliftCodeKey, JitFunctionHandle>,
}

/// Process-level compiler owner and bounded code-generation cache.
pub struct CraneliftCodeManager {
    state: Mutex<ManagerState>,
    helpers: Arc<RwLock<HashMap<String, usize>>>,
    metrics: Arc<CodeManagerMetrics>,
    code_limit: usize,
    generation_limit: usize,
}

impl fmt::Debug for CraneliftCodeManager {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CraneliftCodeManager")
            .field("code_limit", &self.code_limit)
            .field("generation_limit", &self.generation_limit)
            .field("stats", &self.stats())
            .finish_non_exhaustive()
    }
}

impl CraneliftCodeManager {
    /// Constructs an isolated manager. Production callers use [`global_code_manager`].
    pub fn new(
        code_limit: usize,
        generation_limit: usize,
    ) -> Result<Self, CraneliftCodeManagerError> {
        let code_limit = code_limit.max(1);
        let generation_limit = generation_limit.max(1).min(code_limit);
        let helpers = Arc::new(RwLock::new(HashMap::new()));
        let metrics = Arc::new(CodeManagerMetrics::default());
        let active = Self::new_generation(1, &helpers, &metrics)?;
        Ok(Self {
            state: Mutex::new(ManagerState {
                next_generation: 2,
                active: Arc::clone(&active),
                generations: VecDeque::from([active]),
                cache: HashMap::new(),
            }),
            helpers,
            metrics,
            code_limit,
            generation_limit,
        })
    }

    fn new_generation(
        id: u64,
        helpers: &Arc<RwLock<HashMap<String, usize>>>,
        metrics: &Arc<CodeManagerMetrics>,
    ) -> Result<Arc<CodeGeneration>, CraneliftCodeManagerError> {
        let mut flags = settings::builder();
        flags
            .set("use_colocated_libcalls", "false")
            .map_err(|error| CraneliftCodeManagerError::Flags(error.to_string()))?;
        flags
            .set("is_pic", "false")
            .map_err(|error| CraneliftCodeManagerError::Flags(error.to_string()))?;
        let isa = cranelift_native::builder()
            .map_err(|error| CraneliftCodeManagerError::NativeTarget(error.to_string()))?
            .finish(settings::Flags::new(flags))
            .map_err(|error| CraneliftCodeManagerError::NativeTarget(error.to_string()))?;
        let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
        let helper_lookup = Arc::clone(helpers);
        builder.symbol_lookup_fn(Box::new(move |symbol| {
            helper_lookup
                .read()
                .ok()
                .and_then(|helpers| helpers.get(symbol).copied())
                .map(|address| address as *const u8)
        }));
        metrics.generations_live.fetch_add(1, Ordering::Relaxed);
        Ok(Arc::new(CodeGeneration {
            id,
            module: Mutex::new(Some(JITModule::new(builder))),
            bytes: AtomicUsize::new(0),
            metrics: Arc::clone(metrics),
        }))
    }

    fn lock_state(
        &self,
    ) -> Result<(MutexGuard<'_, ManagerState>, bool), CraneliftCodeManagerError> {
        match self.state.try_lock() {
            Ok(state) => Ok((state, false)),
            Err(TryLockError::WouldBlock) => {
                self.metrics.compile_waits.fetch_add(1, Ordering::Relaxed);
                self.state
                    .lock()
                    .map(|state| (state, true))
                    .map_err(|_| CraneliftCodeManagerError::Poisoned("state"))
            }
            Err(TryLockError::Poisoned(_)) => Err(CraneliftCodeManagerError::Poisoned("state")),
        }
    }

    fn register_helpers(&self, helpers: &[(&str, usize)]) -> Result<(), CraneliftCodeManagerError> {
        let mut registry = self
            .helpers
            .write()
            .map_err(|_| CraneliftCodeManagerError::Poisoned("helper registry"))?;
        for (symbol, address) in helpers.iter().copied().filter(|(_, address)| *address != 0) {
            // Relocations in already-finalized code are immutable. Updating the
            // lookup registry affects only subsequently finalized functions and
            // allows isolated runtimes/tests with different helper tables to
            // coexist; helper addresses also participate in the cache key.
            registry.insert(symbol.to_owned(), address);
        }
        Ok(())
    }

    /// Compiles and publishes exactly once for `key`, serializing Cranelift mutation.
    pub(crate) fn compile_once<E>(
        &self,
        key: CraneliftCodeKey,
        helpers: &[(&str, usize)],
        compile: impl FnOnce(&mut JITModule, &str) -> Result<(JitFunctionHandle, u64), E>,
    ) -> Result<ManagedJitFunction, ManagedCompileError<E>> {
        let (mut state, waited) = self.lock_state().map_err(ManagedCompileError::Manager)?;
        self.register_helpers(helpers)
            .map_err(ManagedCompileError::Manager)?;
        if let Some(handle) = state.cache.get(&key) {
            self.metrics
                .process_cache_hits
                .fetch_add(1, Ordering::Relaxed);
            if waited {
                self.metrics
                    .duplicate_compiles_avoided
                    .fetch_add(1, Ordering::Relaxed);
            }
            let mut handle = handle.clone();
            let stats = self.stats();
            handle.bind_code_manager_event(CraneliftCodeManagerEvent {
                process_cache_hits: 1,
                compile_waits: u64::from(waited),
                duplicate_compiles_avoided: u64::from(waited),
                code_bytes_live: stats.code_bytes_live,
                code_bytes_retired: stats.code_bytes_retired,
                code_generations: stats.code_generations,
                ..CraneliftCodeManagerEvent::default()
            });
            return Ok(ManagedJitFunction {
                code_bytes: handle.code_bytes(),
                handle,
                disposition: CraneliftCodeCacheDisposition::Hit,
            });
        }
        self.metrics
            .process_cache_misses
            .fetch_add(1, Ordering::Relaxed);
        let evictions_before = self.metrics.evictions.load(Ordering::Relaxed);
        self.evict_retired_generations(&mut state);
        let live = self.metrics.bytes_live.load(Ordering::Relaxed);
        if live >= self.code_limit {
            return Err(ManagedCompileError::Manager(
                CraneliftCodeManagerError::CodeLimit {
                    limit: self.code_limit,
                    live,
                },
            ));
        }

        let generation = Arc::clone(&state.active);
        let symbol = key.symbol();
        let (mut handle, code_bytes) = {
            let mut module = generation.module.lock().map_err(|_| {
                ManagedCompileError::Manager(CraneliftCodeManagerError::Poisoned("generation"))
            })?;
            let module = module.as_mut().ok_or_else(|| {
                ManagedCompileError::Manager(CraneliftCodeManagerError::Poisoned(
                    "retired generation",
                ))
            })?;
            compile(module, &symbol).map_err(ManagedCompileError::Compile)?
        };
        let code_bytes_usize = usize::try_from(code_bytes).unwrap_or(usize::MAX);
        generation
            .bytes
            .fetch_add(code_bytes_usize, Ordering::Relaxed);
        self.metrics
            .bytes_live
            .fetch_add(code_bytes_usize, Ordering::Relaxed);
        self.metrics.compile_count.fetch_add(1, Ordering::Relaxed);
        let metadata = Arc::new(CompiledRegionMetadata {
            key: key.clone(),
            code_bytes: code_bytes_usize,
        });
        let entry = handle.native_entry_address().unwrap_or(0);
        handle.bind_code_lifetime(SharedJitCodeHandle::new(
            Arc::clone(&generation),
            entry,
            metadata,
        ));
        state.cache.insert(key, handle.clone());

        if generation.bytes.load(Ordering::Relaxed) >= self.generation_limit {
            let id = state.next_generation;
            state.next_generation = state.next_generation.saturating_add(1);
            let next = Self::new_generation(id, &self.helpers, &self.metrics)
                .map_err(ManagedCompileError::Manager)?;
            state.active = Arc::clone(&next);
            state.generations.push_back(next);
            self.evict_retired_generations(&mut state);
        }

        let stats = self.stats();
        handle.bind_code_manager_event(CraneliftCodeManagerEvent {
            process_cache_misses: 1,
            compile_waits: u64::from(waited),
            code_bytes_live: stats.code_bytes_live,
            code_bytes_retired: stats.code_bytes_retired,
            code_generations: stats.code_generations,
            evictions: stats.evictions.saturating_sub(evictions_before),
            ..CraneliftCodeManagerEvent::default()
        });

        Ok(ManagedJitFunction {
            handle,
            code_bytes,
            disposition: CraneliftCodeCacheDisposition::Compiled,
        })
    }

    fn evict_retired_generations(&self, state: &mut ManagerState) {
        while self.metrics.bytes_live.load(Ordering::Relaxed) >= self.code_limit
            && state.generations.len() > 1
        {
            let Some(generation) = state.generations.pop_front() else {
                break;
            };
            if generation.id == state.active.id {
                state.generations.push_front(generation);
                break;
            }
            let retired_id = generation.id;
            let retired_bytes = generation.bytes.load(Ordering::Relaxed);
            state
                .cache
                .retain(|_, handle| handle.code_generation_id() != Some(retired_id));
            self.metrics
                .bytes_retired
                .fetch_add(retired_bytes, Ordering::Relaxed);
            self.metrics.evictions.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Returns an atomic snapshot without blocking compilation.
    #[must_use]
    pub fn stats(&self) -> CraneliftCodeManagerStats {
        let eviction_candidates = self
            .state
            .try_lock()
            .ok()
            .map(|state| state.generations.len().saturating_sub(1))
            .unwrap_or(0);
        CraneliftCodeManagerStats {
            process_cache_hits: self.metrics.process_cache_hits.load(Ordering::Relaxed),
            process_cache_misses: self.metrics.process_cache_misses.load(Ordering::Relaxed),
            compile_waits: self.metrics.compile_waits.load(Ordering::Relaxed),
            duplicate_compiles_avoided: self
                .metrics
                .duplicate_compiles_avoided
                .load(Ordering::Relaxed),
            compile_count: self.metrics.compile_count.load(Ordering::Relaxed),
            code_bytes_live: self.metrics.bytes_live.load(Ordering::Relaxed),
            code_bytes_retired: self.metrics.bytes_retired.load(Ordering::Relaxed),
            code_generations: self.metrics.generations_live.load(Ordering::Relaxed),
            active_handles: self.metrics.active_handles.load(Ordering::Relaxed),
            evictions: self.metrics.evictions.load(Ordering::Relaxed),
            eviction_candidates,
        }
    }
}

/// Distinguishes manager failures from lowering failures inside the compile closure.
#[derive(Debug)]
pub(crate) enum ManagedCompileError<E> {
    Manager(CraneliftCodeManagerError),
    Compile(E),
}

static GLOBAL_CODE_MANAGER: OnceLock<Result<CraneliftCodeManager, CraneliftCodeManagerError>> =
    OnceLock::new();

/// Returns the process-level Cranelift compiler/code owner.
pub fn global_code_manager() -> Result<&'static CraneliftCodeManager, CraneliftCodeManagerError> {
    GLOBAL_CODE_MANAGER
        .get_or_init(|| CraneliftCodeManager::new(DEFAULT_CODE_LIMIT, DEFAULT_GENERATION_LIMIT))
        .as_ref()
        .map_err(Clone::clone)
}

/// Returns process code-manager counters, or zeroes when initialization failed.
#[must_use]
pub fn cranelift_code_manager_stats() -> CraneliftCodeManagerStats {
    global_code_manager()
        .map(CraneliftCodeManager::stats)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{
        CraneliftCodeCacheDisposition, CraneliftCodeKey, CraneliftCodeManager, ManagedCompileError,
    };
    use crate::{CraneliftCompilerIdentity, JIT_RUNTIME_ABI_HASH, JitFunctionHandle};
    use cranelift_codegen::ir::{AbiParam, InstBuilder, UserFuncName, types};
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
    use cranelift_module::{Linkage, Module};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};

    fn key(index: u64, generation: u64) -> CraneliftCodeKey {
        CraneliftCodeKey {
            compiled_unit: format!("unit-{index}"),
            region: format!("function-{index}"),
            abi_hash: JIT_RUNTIME_ABI_HASH,
            config_hash: 7,
            invalidation_generation: generation,
            specialization: "test-constant-v1".to_owned(),
        }
    }

    #[test]
    fn compiler_owner_and_published_handles_match_worker_thread_traits() {
        fn assert_send<T: Send>() {}
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send::<cranelift_jit::JITModule>();
        assert_send_sync::<CraneliftCodeManager>();
        assert_send_sync::<JitFunctionHandle>();
    }

    fn compile_constant(
        module: &mut cranelift_jit::JITModule,
        symbol: &str,
        value: i64,
    ) -> Result<(JitFunctionHandle, u64), String> {
        let mut signature = module.make_signature();
        signature.returns.push(AbiParam::new(types::I64));
        let function = module
            .declare_function(symbol, Linkage::Local, &signature)
            .map_err(|error| error.to_string())?;
        let mut context = module.make_context();
        context.func.signature = signature;
        context.func.name = UserFuncName::user(0, function.as_u32());
        let mut builder_context = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
            let entry = builder.create_block();
            builder.switch_to_block(entry);
            builder.seal_block(entry);
            let value = builder.ins().iconst(types::I64, value);
            builder.ins().return_(&[value]);
            builder.finalize();
        }
        module
            .define_function(function, &mut context)
            .map_err(|error| error.to_string())?;
        let code_bytes = context
            .compiled_code()
            .map(|code| code.code_buffer().len() as u64)
            .unwrap_or(0);
        module.clear_context(&mut context);
        module
            .finalize_definitions()
            .map_err(|error| error.to_string())?;
        let address = module.get_finalized_function(function) as usize;
        Ok((
            JitFunctionHandle::i64_native(
                value as u64,
                symbol.to_owned(),
                CraneliftCompilerIdentity,
                address,
                0,
                code_bytes,
            ),
            code_bytes,
        ))
    }

    #[test]
    fn many_functions_share_bounded_generations_without_module_leaks() {
        let manager = CraneliftCodeManager::new(1024 * 1024, 1024 * 1024).unwrap();
        for index in 0..128_u64 {
            let compiled = manager
                .compile_once(key(index, 0), &[], |module, symbol| {
                    compile_constant(module, symbol, index as i64)
                })
                .unwrap();
            assert_eq!(
                compiled.handle.invoke_i64(&[], JIT_RUNTIME_ABI_HASH),
                Ok(index as i64)
            );
        }
        let stats = manager.stats();
        assert_eq!(stats.compile_count, 128);
        assert_eq!(stats.code_generations, 1);
        assert!(stats.code_bytes_live < 1024 * 1024);
    }

    #[test]
    fn published_code_executes_from_multiple_worker_threads() {
        let manager = CraneliftCodeManager::new(1024 * 1024, 1024 * 1024).unwrap();
        let handle = manager
            .compile_once(key(1, 0), &[], |module, symbol| {
                compile_constant(module, symbol, 42)
            })
            .unwrap()
            .handle;
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let handle = handle.clone();
                scope.spawn(move || {
                    assert_eq!(handle.invoke_i64(&[], JIT_RUNTIME_ABI_HASH), Ok(42));
                });
            }
        });
    }

    #[test]
    fn published_code_executes_while_compiler_extends_active_generation() {
        let manager = Arc::new(CraneliftCodeManager::new(1024 * 1024, 1024 * 1024).unwrap());
        let first = manager
            .compile_once(key(1, 0), &[], |module, symbol| {
                compile_constant(module, symbol, 42)
            })
            .unwrap()
            .handle;
        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let worker_running = Arc::clone(&running);
        let worker = std::thread::spawn(move || {
            while worker_running.load(Ordering::Acquire) {
                assert_eq!(first.invoke_i64(&[], JIT_RUNTIME_ABI_HASH), Ok(42));
            }
        });
        for index in 2..64_u64 {
            manager
                .compile_once(key(index, 0), &[], |module, symbol| {
                    compile_constant(module, symbol, index as i64)
                })
                .unwrap();
        }
        running.store(false, Ordering::Release);
        worker.join().unwrap();
    }

    #[test]
    fn invalidation_publishes_new_generation_and_old_handle_stays_live() {
        let manager = CraneliftCodeManager::new(1024 * 1024, 1).unwrap();
        let old = manager
            .compile_once(key(1, 1), &[], |module, symbol| {
                compile_constant(module, symbol, 10)
            })
            .unwrap()
            .handle;
        let new = manager
            .compile_once(key(1, 2), &[], |module, symbol| {
                compile_constant(module, symbol, 20)
            })
            .unwrap()
            .handle;
        assert_ne!(old.code_generation_id(), new.code_generation_id());
        assert_eq!(old.invoke_i64(&[], JIT_RUNTIME_ABI_HASH), Ok(10));
        assert_eq!(new.invoke_i64(&[], JIT_RUNTIME_ABI_HASH), Ok(20));
    }

    #[test]
    fn code_limit_evicts_safely_and_refuses_growth_while_old_code_is_active() {
        let probe = CraneliftCodeManager::new(1024, 1).unwrap();
        let first = probe
            .compile_once(key(1, 0), &[], |module, symbol| {
                compile_constant(module, symbol, 1)
            })
            .unwrap()
            .handle;
        let bytes = first.code_bytes() as usize;
        drop(probe);

        let manager = CraneliftCodeManager::new(bytes.max(1), 1).unwrap();
        let held = manager
            .compile_once(key(2, 0), &[], |module, symbol| {
                compile_constant(module, symbol, 2)
            })
            .unwrap()
            .handle;
        let rejected = manager.compile_once(key(3, 0), &[], |module, symbol| {
            compile_constant(module, symbol, 3)
        });
        assert!(matches!(rejected, Err(ManagedCompileError::Manager(_))));
        assert!(manager.stats().evictions >= 1);
        assert_eq!(held.invoke_i64(&[], JIT_RUNTIME_ABI_HASH), Ok(2));
    }

    #[test]
    fn concurrent_same_key_compiles_once() {
        let manager = Arc::new(CraneliftCodeManager::new(1024 * 1024, 1024 * 1024).unwrap());
        let barrier = Arc::new(Barrier::new(8));
        let compile_count = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let manager = Arc::clone(&manager);
            let barrier = Arc::clone(&barrier);
            let compile_count = Arc::clone(&compile_count);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                manager
                    .compile_once(key(9, 0), &[], |module, symbol| {
                        compile_count.fetch_add(1, Ordering::SeqCst);
                        compile_constant(module, symbol, 99)
                    })
                    .unwrap()
            }));
        }
        let outcomes = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(compile_count.load(Ordering::SeqCst), 1);
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| outcome.disposition == CraneliftCodeCacheDisposition::Compiled)
                .count(),
            1
        );
        assert!(manager.stats().duplicate_compiles_avoided > 0);
    }
}
