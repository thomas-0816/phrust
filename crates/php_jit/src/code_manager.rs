//! Process-wide, bounded ownership for Cranelift executable code.

use crate::{JitFunctionHandle, NativeFunctionKey, NativeFunctionTier, NativeIndirectionCell};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::FunctionBuilderContext;
use cranelift_jit::{ArenaMemoryProvider, JITBuilder, JITModule};
use cranelift_module::{Module, default_libcall_names};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, RwLock, TryLockError};

// Function-on-demand publication keeps dormant declarations code-free. The
// process owner remains bounded because distinct requested functions may stay
// live across many source units and requests.
const DEFAULT_CODE_LIMIT: usize = 512 * 1024 * 1024;
const DEFAULT_GENERATION_LIMIT: usize = 16 * 1024 * 1024;
const GENERATION_ARENA_RESERVE: usize = 32 * 1024 * 1024;
const DEFAULT_MAX_CONCURRENT_COMPILES: usize = 1;
const DEFAULT_MAX_COMPILE_QUEUE: usize = 64;

/// Stable process-cache identity for one compiled specialization.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CraneliftCodeKey {
    /// Compiled-unit identity or IR fingerprint.
    pub compiled_unit: String,
    /// Function/region identity chosen by the frontend.
    pub region: String,
    /// Runtime ABI hash embedded in the native entry.
    pub abi_hash: u64,
    /// Compiler tier that produced the code.
    pub compiler_tier: String,
    /// Process-independent runtime-helper ABI identity.
    pub helper_abi_hash: u64,
    /// Process-local helper bindings. This participates only in the in-memory
    /// code cache and is never part of persistent artifact compatibility.
    pub helper_binding_hash: u64,
    /// Exact target and host-CPU feature identity.
    pub target_cpu: String,
    /// PHP-visible compiler semantic configuration identity.
    pub semantic_config_hash: u64,
    /// Linked source/dependency identity.
    pub dependency_identity: String,
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
            self.compiler_tier.as_bytes(),
            &self.helper_abi_hash.to_le_bytes(),
            &self.helper_binding_hash.to_le_bytes(),
            self.target_cpu.as_bytes(),
            &self.semantic_config_hash.to_le_bytes(),
            self.dependency_identity.as_bytes(),
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
    function_body_compile_count: AtomicU64,
    optimized_function_publications: AtomicU64,
    duplicate_function_publications: AtomicU64,
    bytes_live: AtomicUsize,
    bytes_retired: AtomicUsize,
    generations_live: AtomicUsize,
    active_handles: AtomicUsize,
    evictions: AtomicU64,
    compile_queue_rejections: AtomicU64,
}

impl Default for CodeManagerMetrics {
    fn default() -> Self {
        Self {
            process_cache_hits: AtomicU64::new(0),
            process_cache_misses: AtomicU64::new(0),
            compile_waits: AtomicU64::new(0),
            duplicate_compiles_avoided: AtomicU64::new(0),
            compile_count: AtomicU64::new(0),
            function_body_compile_count: AtomicU64::new(0),
            optimized_function_publications: AtomicU64::new(0),
            duplicate_function_publications: AtomicU64::new(0),
            bytes_live: AtomicUsize::new(0),
            bytes_retired: AtomicUsize::new(0),
            generations_live: AtomicUsize::new(0),
            active_handles: AtomicUsize::new(0),
            evictions: AtomicU64::new(0),
            compile_queue_rejections: AtomicU64::new(0),
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
    pub function_body_compile_count: u64,
    pub optimized_function_publications: u64,
    pub duplicate_function_publications: u64,
    pub function_cells: usize,
    pub code_bytes_live: usize,
    pub code_bytes_retired: usize,
    pub code_generations: usize,
    pub active_handles: usize,
    pub evictions: u64,
    pub eviction_candidates: usize,
    pub active_compiles: usize,
    pub queued_compiles: usize,
    pub compile_queue_rejections: u64,
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
    compiler: Mutex<GenerationCompiler>,
    bytes: AtomicUsize,
    metrics: Arc<CodeManagerMetrics>,
}

struct GenerationCompiler {
    module: Option<JITModule>,
    context: cranelift_codegen::Context,
    builder_context: FunctionBuilderContext,
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
        if let Ok(compiler) = self.compiler.get_mut()
            && let Some(module) = compiler.module.take()
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
    CompileFailed { detail: String },
    CompileQueueFull { limit: usize },
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
            Self::CompileFailed { detail } => {
                write!(formatter, "native compilation previously failed: {detail}")
            }
            Self::CompileQueueFull { limit } => {
                write!(formatter, "native compile queue is full: limit={limit}")
            }
        }
    }
}

impl std::error::Error for CraneliftCodeManagerError {}

struct ManagerState {
    next_generation: u64,
    active: Arc<CodeGeneration>,
    generations: VecDeque<Arc<CodeGeneration>>,
    cache: HashMap<CraneliftCodeKey, JitFunctionHandle>,
    function_cells: HashMap<NativeFunctionKey, Arc<NativeIndirectionCell>>,
    function_publications: HashMap<NativeFunctionKey, JitFunctionHandle>,
    compiling: HashSet<CraneliftCodeKey>,
    compile_failures: HashMap<CraneliftCodeKey, String>,
}

#[derive(Default)]
struct CompilerState {
    active: usize,
    queued: usize,
}

/// Process-level compiler owner and bounded code-generation cache.
pub struct CraneliftCodeManager {
    state: Mutex<ManagerState>,
    state_changed: Condvar,
    compiler_state: Mutex<CompilerState>,
    compiler_changed: Condvar,
    helpers: Arc<RwLock<HashMap<String, usize>>>,
    metrics: Arc<CodeManagerMetrics>,
    code_limit: usize,
    generation_limit: usize,
}

struct CompilerPermit<'a> {
    manager: &'a CraneliftCodeManager,
}

impl Drop for CompilerPermit<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.manager.compiler_state.lock() {
            state.active = state.active.saturating_sub(1);
            self.manager.compiler_changed.notify_one();
        }
    }
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
                function_cells: HashMap::new(),
                function_publications: HashMap::new(),
                compiling: HashSet::new(),
                compile_failures: HashMap::new(),
            }),
            state_changed: Condvar::new(),
            compiler_state: Mutex::new(CompilerState::default()),
            compiler_changed: Condvar::new(),
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
        flags
            .set("preserve_frame_pointers", "true")
            .map_err(|error| CraneliftCodeManagerError::Flags(error.to_string()))?;
        let isa = cranelift_native::builder()
            .map_err(|error| CraneliftCodeManagerError::NativeTarget(error.to_string()))?
            .finish(settings::Flags::new(flags))
            .map_err(|error| CraneliftCodeManagerError::NativeTarget(error.to_string()))?;
        let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
        // Cranelift encodes x86-64 calls between locally defined functions as
        // rel32. Its default memory provider may mmap individual functions
        // more than 2 GiB apart, which makes finalization panic for large
        // dynamic applications. One bounded contiguous arena per generation
        // keeps every local target in range; imported helpers use absolute
        // indirect calls in the lowering layer.
        let arena =
            ArenaMemoryProvider::new_with_size(GENERATION_ARENA_RESERVE).map_err(|error| {
                CraneliftCodeManagerError::NativeTarget(format!(
                    "failed to reserve contiguous Cranelift code arena: {error}"
                ))
            })?;
        builder.memory_provider(Box::new(arena));
        let helper_lookup = Arc::clone(helpers);
        builder.symbol_lookup_fn(Box::new(move |symbol| {
            helper_lookup
                .read()
                .ok()
                .and_then(|helpers| helpers.get(symbol).copied())
                .map(|address| address as *const u8)
        }));
        metrics.generations_live.fetch_add(1, Ordering::Relaxed);
        let module = JITModule::new(builder);
        let context = module.make_context();
        Ok(Arc::new(CodeGeneration {
            id,
            compiler: Mutex::new(GenerationCompiler {
                module: Some(module),
                context,
                builder_context: FunctionBuilderContext::new(),
            }),
            bytes: AtomicUsize::new(0),
            metrics: Arc::clone(metrics),
        }))
    }

    fn acquire_compiler(&self) -> Result<CompilerPermit<'_>, CraneliftCodeManagerError> {
        let mut state = self
            .compiler_state
            .lock()
            .map_err(|_| CraneliftCodeManagerError::Poisoned("compiler scheduler"))?;
        if state.active < DEFAULT_MAX_CONCURRENT_COMPILES {
            state.active = state.active.saturating_add(1);
            return Ok(CompilerPermit { manager: self });
        }
        if state.queued >= DEFAULT_MAX_COMPILE_QUEUE {
            self.metrics
                .compile_queue_rejections
                .fetch_add(1, Ordering::Relaxed);
            return Err(CraneliftCodeManagerError::CompileQueueFull {
                limit: DEFAULT_MAX_COMPILE_QUEUE,
            });
        }
        state.queued = state.queued.saturating_add(1);
        loop {
            state = self
                .compiler_changed
                .wait(state)
                .map_err(|_| CraneliftCodeManagerError::Poisoned("compiler scheduler"))?;
            if state.active < DEFAULT_MAX_CONCURRENT_COMPILES {
                state.queued = state.queued.saturating_sub(1);
                state.active = state.active.saturating_add(1);
                return Ok(CompilerPermit { manager: self });
            }
        }
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

    /// Declares symbolic PHP functions without compiling or publishing code.
    /// Repeated declarations of the exact key reuse the stable cell.
    pub fn declare_function_cells(
        &self,
        keys: impl IntoIterator<Item = NativeFunctionKey>,
    ) -> Result<Vec<Arc<NativeIndirectionCell>>, CraneliftCodeManagerError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| CraneliftCodeManagerError::Poisoned("state"))?;
        Ok(keys
            .into_iter()
            .map(|key| {
                state
                    .function_cells
                    .entry(key.clone())
                    .or_insert_with(|| Arc::new(NativeIndirectionCell::new(key)))
                    .clone()
            })
            .collect())
    }

    /// Returns a declared cell whether or not machine code is published.
    #[must_use]
    pub fn function_cell(&self, key: &NativeFunctionKey) -> Option<Arc<NativeIndirectionCell>> {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.function_cells.get(key).cloned())
    }

    /// Compiles and publishes exactly once for `key`.
    ///
    /// The manager mutex protects admission and publication only. Cranelift
    /// lowering, register allocation, and finalization run after that mutex is
    /// released; unrelated cache reads and declarations therefore cannot be
    /// serialized behind a pathological compile. The generation-local module
    /// lock remains the bounded mutable-codegen owner.
    #[cfg(test)]
    pub(crate) fn compile_once<E>(
        &self,
        key: CraneliftCodeKey,
        helpers: &[(&str, usize)],
        compile: impl FnOnce(&mut JITModule, &str) -> Result<(JitFunctionHandle, u64), E>,
    ) -> Result<ManagedJitFunction, ManagedCompileError<E>>
    where
        E: fmt::Display,
    {
        self.compile_once_with_scratch(
            key,
            None,
            helpers,
            |module, _context, _builder_context, symbol| compile(module, symbol),
        )
    }

    pub(crate) fn compile_once_with_scratch<E>(
        &self,
        key: CraneliftCodeKey,
        function_key: Option<NativeFunctionKey>,
        helpers: &[(&str, usize)],
        compile: impl FnOnce(
            &mut JITModule,
            &mut cranelift_codegen::Context,
            &mut FunctionBuilderContext,
            &str,
        ) -> Result<(JitFunctionHandle, u64), E>,
    ) -> Result<ManagedJitFunction, ManagedCompileError<E>>
    where
        E: fmt::Display,
    {
        self.register_helpers(helpers)
            .map_err(ManagedCompileError::Manager)?;
        let mut waited = false;
        let (generation, evictions_before, function_cell) = {
            let (mut state, lock_waited) =
                self.lock_state().map_err(ManagedCompileError::Manager)?;
            waited |= lock_waited;
            loop {
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
                    handle.bind_code_manager_event(CraneliftCodeManagerEvent {
                        process_cache_hits: 1,
                        compile_waits: u64::from(waited),
                        duplicate_compiles_avoided: u64::from(waited),
                        code_bytes_live: self.metrics.bytes_live.load(Ordering::Relaxed),
                        code_bytes_retired: self.metrics.bytes_retired.load(Ordering::Relaxed),
                        code_generations: self.metrics.generations_live.load(Ordering::Relaxed),
                        ..CraneliftCodeManagerEvent::default()
                    });
                    return Ok(ManagedJitFunction {
                        code_bytes: handle.code_bytes(),
                        handle,
                        disposition: CraneliftCodeCacheDisposition::Hit,
                    });
                }
                if let Some(detail) = state.compile_failures.get(&key) {
                    return Err(ManagedCompileError::Manager(
                        CraneliftCodeManagerError::CompileFailed {
                            detail: detail.clone(),
                        },
                    ));
                }
                if state.compiling.insert(key.clone()) {
                    let function_cell = function_key.as_ref().map(|function_key| {
                        state
                            .function_cells
                            .entry(function_key.clone())
                            .or_insert_with(|| {
                                Arc::new(NativeIndirectionCell::new(function_key.clone()))
                            })
                            .clone()
                    });
                    if let Some(cell) = &function_cell {
                        cell.mark_queued();
                    }
                    self.metrics
                        .process_cache_misses
                        .fetch_add(1, Ordering::Relaxed);
                    let evictions_before = self.metrics.evictions.load(Ordering::Relaxed);
                    self.evict_retired_generations(&mut state);
                    let live = self.metrics.bytes_live.load(Ordering::Relaxed);
                    if live >= self.code_limit {
                        if let Some(cell) = &function_cell {
                            cell.reset_declared();
                        }
                        state.compiling.remove(&key);
                        self.state_changed.notify_all();
                        return Err(ManagedCompileError::Manager(
                            CraneliftCodeManagerError::CodeLimit {
                                limit: self.code_limit,
                                live,
                            },
                        ));
                    }
                    break (Arc::clone(&state.active), evictions_before, function_cell);
                }
                waited = true;
                self.metrics.compile_waits.fetch_add(1, Ordering::Relaxed);
                state = self.state_changed.wait(state).map_err(|_| {
                    ManagedCompileError::Manager(CraneliftCodeManagerError::Poisoned("state"))
                })?;
            }
        };
        let symbol = key.symbol();
        let compiler_permit = match self.acquire_compiler() {
            Ok(permit) => permit,
            Err(error) => {
                if let Some(cell) = &function_cell {
                    cell.reset_declared();
                }
                if let Ok(mut state) = self.state.lock() {
                    state.compiling.remove(&key);
                    self.state_changed.notify_all();
                }
                return Err(ManagedCompileError::Manager(error));
            }
        };
        if let Some(cell) = &function_cell {
            cell.mark_compiling();
        }
        let compiled = match generation.compiler.lock() {
            Ok(mut compiler) => {
                let GenerationCompiler {
                    module,
                    context,
                    builder_context,
                } = &mut *compiler;
                match module.as_mut() {
                    Some(module) => compile(module, context, builder_context, &symbol)
                        .map_err(ManagedCompileError::Compile),
                    None => Err(ManagedCompileError::Manager(
                        CraneliftCodeManagerError::Poisoned("retired generation"),
                    )),
                }
            }
            Err(_) => Err(ManagedCompileError::Manager(
                CraneliftCodeManagerError::Poisoned("generation"),
            )),
        };
        drop(compiler_permit);
        let (mut handle, code_bytes) = match compiled {
            Ok(compiled) => compiled,
            Err(error) => {
                let detail = match &error {
                    ManagedCompileError::Manager(error) => error.to_string(),
                    ManagedCompileError::Compile(error) => error.to_string(),
                };
                if let Ok(mut state) = self.state.lock() {
                    state.compiling.remove(&key);
                    state.compile_failures.insert(key.clone(), detail);
                    self.state_changed.notify_all();
                }
                if let Some(cell) = &function_cell {
                    cell.mark_failed();
                }
                return Err(error);
            }
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
        let mut state = self.state.lock().map_err(|_| {
            ManagedCompileError::Manager(CraneliftCodeManagerError::Poisoned("state"))
        })?;
        self.publish_function_entries(&mut state, &key, &handle);
        state.cache.insert(key.clone(), handle.clone());
        state.compile_failures.remove(&key);

        if generation.bytes.load(Ordering::Relaxed) >= self.generation_limit {
            let id = state.next_generation;
            state.next_generation = state.next_generation.saturating_add(1);
            if let Ok(next) = Self::new_generation(id, &self.helpers, &self.metrics) {
                state.active = Arc::clone(&next);
                state.generations.push_back(next);
                self.evict_retired_generations(&mut state);
            }
        }
        state.compiling.remove(&key);
        self.state_changed.notify_all();
        drop(state);
        handle.bind_code_manager_event(CraneliftCodeManagerEvent {
            process_cache_misses: 1,
            compile_waits: u64::from(waited),
            code_bytes_live: self.metrics.bytes_live.load(Ordering::Relaxed),
            code_bytes_retired: self.metrics.bytes_retired.load(Ordering::Relaxed),
            code_generations: self.metrics.generations_live.load(Ordering::Relaxed),
            evictions: self
                .metrics
                .evictions
                .load(Ordering::Relaxed)
                .saturating_sub(evictions_before),
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
            let retired_keys = state
                .function_publications
                .iter()
                .filter_map(|(key, handle)| {
                    (handle.code_generation_id() == Some(retired_id)).then_some(key.clone())
                })
                .collect::<Vec<_>>();
            for key in retired_keys {
                if let Some(cell) = state.function_cells.remove(&key) {
                    cell.retire();
                }
                state.function_publications.remove(&key);
            }
            self.metrics
                .bytes_retired
                .fetch_add(retired_bytes, Ordering::Relaxed);
            self.metrics.evictions.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn publish_function_entries(
        &self,
        state: &mut ManagerState,
        code_key: &CraneliftCodeKey,
        handle: &JitFunctionHandle,
    ) {
        let Some(metadata) = handle.region_state_metadata() else {
            return;
        };
        let tier = if code_key.compiler_tier == "optimizing" {
            NativeFunctionTier::Optimized
        } else {
            NativeFunctionTier::Baseline
        };
        let entries = metadata.function_entries.clone();
        self.metrics
            .function_body_compile_count
            .fetch_add(entries.len() as u64, Ordering::Relaxed);
        if tier == NativeFunctionTier::Optimized {
            self.metrics
                .optimized_function_publications
                .fetch_add(entries.len() as u64, Ordering::Relaxed);
        }
        for entry in entries {
            let key = crate::native_function_key(
                code_key.compiled_unit.clone(),
                entry.function.raw(),
                entry.arity as usize,
                entry.local_count,
                code_key.compiler_tier == "optimizing",
                code_key.invalidation_generation,
            );
            let cell = state
                .function_cells
                .entry(key.clone())
                .or_insert_with(|| Arc::new(NativeIndirectionCell::new(key.clone())))
                .clone();
            if state.function_publications.contains_key(&key) {
                self.metrics
                    .duplicate_function_publications
                    .fetch_add(1, Ordering::Relaxed);
            }
            let publication = handle
                .clone_for_function_entry(entry.function)
                .unwrap_or_else(|| handle.clone());
            cell.publish(tier, code_key.invalidation_generation, entry.address);
            state.function_publications.insert(key, publication);
        }
    }

    /// Returns a stable cell and its generation-owning publication handle.
    /// Callers retain the handle for at least as long as they may invoke the
    /// resolved address.
    #[must_use]
    pub fn published_function(
        &self,
        key: &NativeFunctionKey,
    ) -> Option<(Arc<NativeIndirectionCell>, JitFunctionHandle)> {
        let state = self.state.lock().ok()?;
        Some((
            Arc::clone(state.function_cells.get(key)?),
            state.function_publications.get(key)?.clone(),
        ))
    }

    /// Returns an atomic snapshot without blocking compilation.
    #[must_use]
    pub fn stats(&self) -> CraneliftCodeManagerStats {
        let state_snapshot = self
            .state
            .try_lock()
            .ok()
            .map(|state| {
                (
                    state.generations.len().saturating_sub(1),
                    state.function_cells.len(),
                )
            })
            .unwrap_or((0, 0));
        let compiler_snapshot = self
            .compiler_state
            .try_lock()
            .ok()
            .map(|state| (state.active, state.queued))
            .unwrap_or((0, 0));
        CraneliftCodeManagerStats {
            process_cache_hits: self.metrics.process_cache_hits.load(Ordering::Relaxed),
            process_cache_misses: self.metrics.process_cache_misses.load(Ordering::Relaxed),
            compile_waits: self.metrics.compile_waits.load(Ordering::Relaxed),
            duplicate_compiles_avoided: self
                .metrics
                .duplicate_compiles_avoided
                .load(Ordering::Relaxed),
            compile_count: self.metrics.compile_count.load(Ordering::Relaxed),
            function_body_compile_count: self
                .metrics
                .function_body_compile_count
                .load(Ordering::Relaxed),
            optimized_function_publications: self
                .metrics
                .optimized_function_publications
                .load(Ordering::Relaxed),
            duplicate_function_publications: self
                .metrics
                .duplicate_function_publications
                .load(Ordering::Relaxed),
            function_cells: state_snapshot.1,
            code_bytes_live: self.metrics.bytes_live.load(Ordering::Relaxed),
            code_bytes_retired: self.metrics.bytes_retired.load(Ordering::Relaxed),
            code_generations: self.metrics.generations_live.load(Ordering::Relaxed),
            active_handles: self.metrics.active_handles.load(Ordering::Relaxed),
            evictions: self.metrics.evictions.load(Ordering::Relaxed),
            eviction_candidates: state_snapshot.0,
            active_compiles: compiler_snapshot.0,
            queued_compiles: compiler_snapshot.1,
            compile_queue_rejections: self
                .metrics
                .compile_queue_rejections
                .load(Ordering::Relaxed),
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
        CraneliftCodeCacheDisposition, CraneliftCodeKey, CraneliftCodeManager,
        CraneliftCodeManagerError, ManagedCompileError,
    };
    use crate::{
        CraneliftCompilerIdentity, JIT_RUNTIME_ABI_HASH, JitFunctionHandle, NativeFunctionKey,
    };
    use cranelift_codegen::ir::{AbiParam, InstBuilder, UserFuncName, types};
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
    use cranelift_module::{Linkage, Module};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier, mpsc};
    use std::time::{Duration, Instant};

    fn key(index: u64, generation: u64) -> CraneliftCodeKey {
        CraneliftCodeKey {
            compiled_unit: format!("unit-{index}"),
            region: format!("function-{index}"),
            abi_hash: JIT_RUNTIME_ABI_HASH,
            compiler_tier: "baseline".to_owned(),
            helper_abi_hash: JIT_RUNTIME_ABI_HASH,
            helper_binding_hash: 0,
            target_cpu: "test-target:test-cpu".to_owned(),
            semantic_config_hash: 7,
            dependency_identity: format!("dependency-{index}"),
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

    #[test]
    fn manager_state_is_not_locked_during_codegen() {
        let manager = Arc::new(CraneliftCodeManager::new(1024 * 1024, 1024 * 1024).unwrap());
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let compiler = {
            let manager = Arc::clone(&manager);
            std::thread::spawn(move || {
                manager
                    .compile_once(key(40, 0), &[], |module, symbol| {
                        started_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                        compile_constant(module, symbol, 40)
                    })
                    .unwrap()
            })
        };
        started_rx.recv().unwrap();
        let start = Instant::now();
        manager
            .declare_function_cells([NativeFunctionKey {
                deployment_unit: "lock-free-diagnostic".to_owned(),
                function_id: 1,
                signature_hash: 2,
                compiler_tier: "baseline".to_owned(),
                version: "test".to_owned(),
                invalidation_generation: 0,
            }])
            .unwrap();
        assert!(
            start.elapsed() < Duration::from_millis(100),
            "manager declaration was serialized behind Cranelift"
        );
        release_tx.send(()).unwrap();
        compiler.join().unwrap();
    }

    #[test]
    fn failed_compile_is_sticky_for_the_exact_key() {
        let manager = CraneliftCodeManager::new(1024 * 1024, 1024 * 1024).unwrap();
        let attempts = AtomicUsize::new(0);
        let first = manager.compile_once(key(41, 0), &[], |_module, _symbol| {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err::<(JitFunctionHandle, u64), _>("deterministic failure".to_owned())
        });
        assert!(matches!(first, Err(ManagedCompileError::Compile(_))));
        let second = manager.compile_once(key(41, 0), &[], |module, symbol| {
            attempts.fetch_add(1, Ordering::SeqCst);
            compile_constant(module, symbol, 41)
        });
        assert!(matches!(
            second,
            Err(ManagedCompileError::Manager(
                CraneliftCodeManagerError::CompileFailed { .. }
            ))
        ));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn compiler_scheduler_bounds_distinct_keys() {
        let manager = Arc::new(CraneliftCodeManager::new(1024 * 1024, 1024 * 1024).unwrap());
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let first = {
            let manager = Arc::clone(&manager);
            std::thread::spawn(move || {
                manager
                    .compile_once(key(42, 0), &[], |module, symbol| {
                        started_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                        compile_constant(module, symbol, 42)
                    })
                    .unwrap()
            })
        };
        started_rx.recv().unwrap();
        let (second_started_tx, second_started_rx) = mpsc::channel();
        let second = {
            let manager = Arc::clone(&manager);
            std::thread::spawn(move || {
                second_started_tx.send(()).unwrap();
                manager
                    .compile_once(key(43, 0), &[], |module, symbol| {
                        compile_constant(module, symbol, 43)
                    })
                    .unwrap()
            })
        };
        second_started_rx.recv().unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let stats = manager.stats();
            if stats.queued_compiles == 1 {
                assert_eq!(stats.active_compiles, 1);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "second compile never entered the bounded queue"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(manager.stats().active_compiles, 1);
        assert_eq!(manager.stats().queued_compiles, 1);
        release_tx.send(()).unwrap();
        first.join().unwrap();
        second.join().unwrap();
    }
}
