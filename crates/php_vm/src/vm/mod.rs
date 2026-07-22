//! Native PHP execution coordinator.

mod jit_abi;
mod native_compile_cache;
mod native_entry;
mod options;
mod result;

pub use native_compile_cache::NativeCompileCacheStats;
pub use options::{NativeBlacklistMode, NativeOptimizationPolicy, VmOptions};
pub use result::VmResult;

use crate::compiled_unit::CompiledUnit;
use jit_abi::{
    NativeRequestOwner, activate_native_context, jit_baseline_native_builtin_dispatch_abi,
    jit_baseline_native_builtin_dispatch_diagnostic_abi, jit_native_argument_check_abi,
    jit_native_array_fetch_abi, jit_native_array_insert_abi, jit_native_array_insert_local_abi,
    jit_native_array_new_abi, jit_native_array_spread_abi, jit_native_array_unset_abi,
    jit_native_basename_abi, jit_native_binary_abi, jit_native_call_dispatch_abi,
    jit_native_call_dispatch_diagnostic_abi, jit_native_cast_abi, jit_native_class_exists_abi,
    jit_native_compare_abi, jit_native_constant_fetch_abi, jit_native_defined_abi,
    jit_native_dirname_abi, jit_native_dynamic_code_abi, jit_native_echo_abi,
    jit_native_echo_bytes_abi, jit_native_echo_float_abi, jit_native_echo_int_abi,
    jit_native_enum_exists_abi, jit_native_exception_new_abi, jit_native_execution_poll_abi,
    jit_native_file_exists_abi, jit_native_float_to_int_abi, jit_native_float_to_string_abi,
    jit_native_foreach_cleanup_abi, jit_native_foreach_init_abi, jit_native_foreach_next_abi,
    jit_native_frame_alloc_abi, jit_native_frame_release_abi, jit_native_function_exists_abi,
    jit_native_function_resolve_abi, jit_native_interface_exists_abi, jit_native_json_decode_abi,
    jit_native_json_encode_abi, jit_native_json_last_error_abi, jit_native_json_last_error_msg_abi,
    jit_native_json_validate_abi, jit_native_local_fetch_abi, jit_native_local_store_abi,
    jit_native_method_exists_abi, jit_native_object_class_name_abi, jit_native_object_clone_abi,
    jit_native_object_clone_with_abi, jit_native_object_new_abi, jit_native_plain_object_clone_abi,
    jit_native_preg_filter_abi, jit_native_preg_grep_abi, jit_native_preg_last_error_abi,
    jit_native_preg_last_error_msg_abi, jit_native_preg_match_abi, jit_native_preg_match_all_abi,
    jit_native_preg_quote_abi, jit_native_preg_replace_abi, jit_native_preg_split_abi,
    jit_native_prepared_object_new_abi, jit_native_printf_abi, jit_native_property_assign_abi,
    jit_native_property_exists_abi, jit_native_property_fetch_abi, jit_native_realpath_abi,
    jit_native_reference_bind_abi, jit_native_return_check_abi, jit_native_runtime_fatal_abi,
    jit_native_semantic_dispatch_abi, jit_native_semantic_dispatch_diagnostic_abi,
    jit_native_sprintf_abi, jit_native_stable_length_abi, jit_native_string_predicate_abi,
    jit_native_trait_exists_abi, jit_native_truthy_abi, jit_native_type_predicate_abi,
    jit_native_unary_abi, jit_native_value_release_abi, jit_native_vprintf_abi,
    jit_native_vsprintf_abi, resume_native_optimizing_exit,
};
use php_runtime::api::{OutputBuffer, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, mpsc};
use std::time::{Duration, Instant};

/// Process-owned state shared by native request coordinators.
#[derive(Clone, Debug)]
pub struct VmWorkerState {
    native_compiles: Arc<native_compile_cache::NativeCompileCache>,
    loaded_native_units: Arc<native_compile_cache::LoadedNativeUnitRegistry>,
    resolved_native_entries: Arc<native_compile_cache::ResolvedNativeEntryCache>,
    background_tiering: bool,
    tiering_options: crate::tiering::TieringOptions,
    tiering_state: Arc<Mutex<BackgroundTieringState>>,
}

#[derive(Debug, Default)]
struct BackgroundTieringState {
    entries: HashMap<native_compile_cache::NativeCompileCacheKey, u64>,
    scheduled: HashSet<native_compile_cache::NativeCompileCacheKey>,
    failed: HashSet<native_compile_cache::NativeCompileCacheKey>,
    stats: crate::tiering::TieringStats,
}

#[derive(Clone, Copy, Debug)]
struct BackgroundTieringDecision {
    key: native_compile_cache::NativeCompileCacheKey,
    entries: u64,
}

static PROCESS_LOADED_NATIVE_UNITS: std::sync::OnceLock<
    Arc<native_compile_cache::LoadedNativeUnitRegistry>,
> = std::sync::OnceLock::new();

type NativeOptimizationJob = Box<dyn FnOnce() + Send + 'static>;

const NATIVE_OPTIMIZATION_WORKERS: usize = 2;
const NATIVE_OPTIMIZATION_QUEUE_CAPACITY: usize = 128;

static NATIVE_OPTIMIZATION_QUEUE: OnceLock<mpsc::SyncSender<NativeOptimizationJob>> =
    OnceLock::new();

fn submit_native_optimization_job(job: impl FnOnce() + Send + 'static) -> bool {
    let sender = NATIVE_OPTIMIZATION_QUEUE.get_or_init(|| {
        let (sender, receiver) =
            mpsc::sync_channel::<NativeOptimizationJob>(NATIVE_OPTIMIZATION_QUEUE_CAPACITY);
        let receiver = Arc::new(Mutex::new(receiver));
        for index in 0..NATIVE_OPTIMIZATION_WORKERS {
            let receiver = Arc::clone(&receiver);
            std::thread::Builder::new()
                .name(format!("phrust-optimize-{index}"))
                .spawn(move || {
                    loop {
                        let job = lock_unpoisoned(&receiver).recv();
                        let Ok(job) = job else {
                            break;
                        };
                        job();
                    }
                })
                .expect("native optimization worker must start");
        }
        sender
    });
    // A full queue blocks only the cold compile-miss edge. Published warm
    // calls never enter this service, and the fixed queue prevents one thread
    // allocation per reached PHP function.
    sender.send(Box::new(job)).is_ok()
}

impl Default for VmWorkerState {
    fn default() -> Self {
        let tiering_options = crate::tiering::TieringOptions::default();
        Self {
            native_compiles: Arc::new(native_compile_cache::NativeCompileCache::default()),
            loaded_native_units: Arc::clone(PROCESS_LOADED_NATIVE_UNITS.get_or_init(|| {
                Arc::new(native_compile_cache::LoadedNativeUnitRegistry::default())
            })),
            resolved_native_entries: Arc::new(
                native_compile_cache::ResolvedNativeEntryCache::default(),
            ),
            background_tiering: false,
            tiering_options,
            tiering_state: Arc::new(Mutex::new(BackgroundTieringState::default())),
        }
    }
}

impl VmWorkerState {
    #[must_use]
    pub fn new(tiering: crate::tiering::TieringOptions) -> Self {
        Self {
            tiering_options: tiering,
            ..Self::default()
        }
    }

    /// Creates a server worker that may publish optimizing code after a hot
    /// baseline threshold without making a request wait for that compilation.
    #[must_use]
    pub fn new_with_background_tiering(tiering: crate::tiering::TieringOptions) -> Self {
        Self {
            background_tiering: true,
            ..Self::new(tiering)
        }
    }

    #[cfg(test)]
    fn isolated_for_restart_test() -> Self {
        Self {
            native_compiles: Arc::new(native_compile_cache::NativeCompileCache::default()),
            loaded_native_units: Arc::new(native_compile_cache::LoadedNativeUnitRegistry::default()),
            resolved_native_entries: Arc::new(
                native_compile_cache::ResolvedNativeEntryCache::default(),
            ),
            background_tiering: false,
            tiering_options: crate::tiering::TieringOptions::default(),
            tiering_state: Arc::new(Mutex::new(BackgroundTieringState::default())),
        }
    }

    /// Returns worker-stable native compile-record cache counters.
    #[must_use]
    pub fn native_compile_cache_stats(&self) -> NativeCompileCacheStats {
        self.native_compiles.stats()
    }

    /// Returns process-worker threshold and background publication counters.
    #[must_use]
    pub fn tiering_stats(&self) -> crate::tiering::TieringStats {
        lock_unpoisoned(&self.tiering_state).stats.clone()
    }

    fn get_or_load_native_unit(
        &self,
        identity: &php_jit::NativeCacheIdentity,
        load: impl FnOnce() -> Result<Option<php_jit::NativeLoadedArtifact>, php_jit::NativeCacheError>,
    ) -> Result<Option<Arc<native_compile_cache::LoadedNativeUnit>>, php_jit::NativeCacheError>
    {
        self.loaded_native_units.get_or_load(identity, load)
    }

    fn loaded_native_unit_stats(&self) -> native_compile_cache::LoadedNativeUnitRegistryStats {
        self.loaded_native_units.stats()
    }

    #[cfg(test)]
    fn resolved_native_entry_hits(&self) -> u64 {
        self.resolved_native_entries.hits()
    }

    fn compile_native(
        &self,
        unit: &CompiledUnit,
        function: php_ir::FunctionId,
        options: &VmOptions,
        external_signatures: &[php_jit::JitExternalFunctionSignature],
    ) -> Result<
        (
            Arc<[php_jit::JitUnitCompileRecord]>,
            native_compile_cache::NativeCompileCacheDisposition,
        ),
        String,
    > {
        self.compile_native_with_priority(unit, function, options, external_signatures, false)
    }

    fn compile_native_with_priority(
        &self,
        unit: &CompiledUnit,
        function: php_ir::FunctionId,
        options: &VmOptions,
        external_signatures: &[php_jit::JitExternalFunctionSignature],
        background: bool,
    ) -> Result<
        (
            Arc<[php_jit::JitUnitCompileRecord]>,
            native_compile_cache::NativeCompileCacheDisposition,
        ),
        String,
    > {
        let function_metadata = unit
            .unit()
            .functions
            .get(function.index())
            .ok_or_else(|| format!("native function {} is missing", function.raw()))?;
        let function_ir_fingerprint = unit
            .prepared_function_ir_fingerprint(function)
            .ok_or_else(|| format!("native function {} has no cache identity", function.raw()))?;
        let function_key = php_jit::native_function_key(
            unit.prepared_ir_fingerprint().to_owned(),
            function.raw(),
            function_metadata.params.len(),
            function_metadata.local_count,
            options.native_optimization.is_optimizing(),
            0,
        );
        let external_signatures_hash = external_function_signatures_hash(external_signatures);
        let key = native_compile_cache::NativeCompileCacheKey::new(
            unit.cache_identity(),
            function,
            options.native_optimization.opt_level(),
            external_signatures_hash,
        );
        let compile = || {
            if options.native_optimization != NativeOptimizationPolicy::Optimizing
                && let Ok(manager) = php_jit::global_code_manager()
                && let Some((cell, handle)) = manager.published_function(&function_key)
                && cell
                    .resolve(
                        function_key.signature_hash,
                        function_key.invalidation_generation,
                    )
                    .is_some()
            {
                return Ok(vec![php_jit::JitUnitCompileRecord {
                    function,
                    result: php_jit::JitCompileResult {
                        status: php_jit::JitCompileStatus::Compiled,
                        handle: Some(handle),
                        diagnostics: vec![format!(
                            "native function {} resolved through its published indirection cell",
                            function.raw()
                        )],
                        stats: php_jit::JitStats::default(),
                    },
                }]);
            }
            compile_native_function_graph(
                unit.unit(),
                function,
                options,
                function_ir_fingerprint,
                unit.prepared_ir_fingerprint(),
                unit.artifact_identity(),
                &format!(
                    "{}-external-signatures-{external_signatures_hash:016x}",
                    unit.prepared_dependency_identity()
                ),
                external_signatures,
            )
        };
        let compiled = if background {
            self.native_compiles.get_or_compile_background(key, compile)
        } else {
            self.native_compiles.get_or_compile(key, compile)
        };
        if std::env::var_os("PHRUST_NATIVE_COMPILE_FUNCTION_LOG").is_some()
            && let Ok((records, disposition)) = &compiled
            && disposition.compiled()
            && let Some(record) = records.first()
        {
            let source = unit
                .unit()
                .files
                .get(function_metadata.span.file.index())
                .map_or("<unknown>", |file| file.path.as_str());
            eprintln!(
                "native_compile_function source={} function={} function_id={} entry_address={:#x} compile_time_nanos={} code_bytes={}",
                source,
                function_metadata.name,
                function.raw(),
                record
                    .result
                    .handle
                    .as_ref()
                    .and_then(php_jit::JitFunctionHandle::native_entry_address)
                    .unwrap_or_default(),
                record.result.stats.native_compile_time_nanos,
                record.result.stats.native_code_bytes,
            );
        }
        compiled
    }

    fn background_tiering_decision(
        &self,
        unit: &CompiledUnit,
        function: php_ir::FunctionId,
        options: &VmOptions,
        external_signatures: &[php_jit::JitExternalFunctionSignature],
    ) -> Option<BackgroundTieringDecision> {
        if !self.background_tiering
            || !self.tiering_options.enabled
            || self.tiering_options.native_eager
            || options.native_optimization != NativeOptimizationPolicy::Optimizing
        {
            return None;
        }
        let key = native_compile_cache::NativeCompileCacheKey::new(
            unit.cache_identity(),
            function,
            NativeOptimizationPolicy::Optimizing.opt_level(),
            external_function_signatures_hash(external_signatures),
        );
        let mut state = lock_unpoisoned(&self.tiering_state);
        state.stats.function_entry_count = state.stats.function_entry_count.saturating_add(1);
        if self.native_compiles.contains(key) {
            return None;
        }
        let entries = state.entries.entry(key).or_default();
        *entries = entries.saturating_add(1);
        let entries = *entries;
        state.stats.baseline_entries = state.stats.baseline_entries.saturating_add(1);
        Some(BackgroundTieringDecision { key, entries })
    }

    fn schedule_background_optimization(
        &self,
        decision: BackgroundTieringDecision,
        unit: CompiledUnit,
        function: php_ir::FunctionId,
        external_signatures: Vec<php_jit::JitExternalFunctionSignature>,
    ) {
        if decision.entries < self.tiering_options.function_entry_threshold.max(1) {
            return;
        }
        {
            let mut state = lock_unpoisoned(&self.tiering_state);
            if state.scheduled.contains(&decision.key) || state.failed.contains(&decision.key) {
                return;
            }
            if state.stats.native_compiled_functions >= self.tiering_options.native_max_functions
                || state.stats.native_compile_budget_used_us
                    >= self.tiering_options.native_max_compile_us
            {
                state.stats.native_compile_budget_rejections = state
                    .stats
                    .native_compile_budget_rejections
                    .saturating_add(1);
                return;
            }
            state.scheduled.insert(decision.key);
            state.stats.optimized_candidates = state.stats.optimized_candidates.saturating_add(1);
        }

        let worker = self.clone();
        let submitted = submit_native_optimization_job(move || {
            let started = Instant::now();
            let mut options = VmOptions::default();
            options.native_optimization = NativeOptimizationPolicy::Optimizing;
            options.tiering.enabled = false;
            let result = worker.compile_native_with_priority(
                &unit,
                function,
                &options,
                &external_signatures,
                true,
            );
            let published_optimizing_entry = result
                .as_ref()
                .ok()
                .and_then(|(records, _)| {
                    records
                        .iter()
                        .find(|record| record.function == function)
                        .and_then(|record| record.result.handle.as_ref())
                })
                .filter(|handle| {
                    handle.region_state_metadata().is_some_and(|metadata| {
                        metadata.compiler_tier == php_jit::region_ir::NativeCompilerTier::Optimizing
                    })
                })
                .and_then(php_jit::JitFunctionHandle::native_entry_address);
            if let Some(address) = published_optimizing_entry
                && let Some(cell) = unit
                    .prepared_deployment_image()
                    .optimizing_function_entries
                    .get(function.index())
            {
                cell.store(address, std::sync::atomic::Ordering::Release);
            }
            let elapsed_us = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
            let mut state = lock_unpoisoned(&worker.tiering_state);
            state.scheduled.remove(&decision.key);
            state.stats.native_compile_budget_used_us = state
                .stats
                .native_compile_budget_used_us
                .saturating_add(elapsed_us);
            if published_optimizing_entry.is_some() {
                state.stats.native_compiled_functions =
                    state.stats.native_compiled_functions.saturating_add(1);
            } else {
                state.failed.insert(decision.key);
            }
        });
        if !submitted {
            let mut state = lock_unpoisoned(&self.tiering_state);
            state.scheduled.remove(&decision.key);
            state.stats.native_compile_budget_rejections = state
                .stats
                .native_compile_budget_rejections
                .saturating_add(1);
        }
    }

    /// Requests the optimizing product for one function that has already
    /// acquired a callable baseline body through compile-on-demand.
    ///
    /// This is invoked only on the cold unpublished-cell edge. It never runs
    /// from a published warm call: generated baseline and optimizing callers
    /// both load the deployment's atomic function cell directly.
    fn schedule_on_demand_optimization(
        &self,
        unit: CompiledUnit,
        function: php_ir::FunctionId,
        external_signatures: Vec<php_jit::JitExternalFunctionSignature>,
    ) {
        if !self.tiering_options.enabled || self.tiering_options.native_eager {
            return;
        }
        let key = native_compile_cache::NativeCompileCacheKey::new(
            unit.cache_identity(),
            function,
            NativeOptimizationPolicy::Optimizing.opt_level(),
            external_function_signatures_hash(&external_signatures),
        );
        if self.native_compiles.contains(key) {
            return;
        }
        self.schedule_background_optimization(
            BackgroundTieringDecision {
                key,
                entries: self.tiering_options.function_entry_threshold.max(1),
            },
            unit,
            function,
            external_signatures,
        );
    }

    fn resolve_native_function(
        &self,
        unit: &CompiledUnit,
        function: php_ir::FunctionId,
        options: &VmOptions,
        external_signatures: &[php_jit::JitExternalFunctionSignature],
    ) -> Result<php_jit::JitFunctionHandle, String> {
        let external_signatures_hash = external_function_signatures_hash(external_signatures);
        let fast_key = native_compile_cache::NativeCompileCacheKey::new(
            unit.cache_identity(),
            function,
            options.native_optimization.opt_level(),
            external_signatures_hash,
        );
        if let Some(handle) = self.resolved_native_entries.get(fast_key) {
            return Ok(handle);
        }
        let handle =
            self.resolve_native_function_cold(unit, function, options, external_signatures)?;
        self.resolved_native_entries
            .insert(fast_key, handle.clone());
        Ok(handle)
    }

    fn resolved_native_function(
        &self,
        unit: &CompiledUnit,
        function: php_ir::FunctionId,
        options: &VmOptions,
        external_signatures: &[php_jit::JitExternalFunctionSignature],
    ) -> Option<php_jit::JitFunctionHandle> {
        self.resolved_native_entries
            .get(native_compile_cache::NativeCompileCacheKey::new(
                unit.cache_identity(),
                function,
                options.native_optimization.opt_level(),
                external_function_signatures_hash(external_signatures),
            ))
    }

    fn resolve_native_function_cold(
        &self,
        unit: &CompiledUnit,
        function: php_ir::FunctionId,
        options: &VmOptions,
        external_signatures: &[php_jit::JitExternalFunctionSignature],
    ) -> Result<php_jit::JitFunctionHandle, String> {
        let cache = if options.native_cache == php_jit::NativeCacheMode::Off {
            None
        } else {
            Some(
                php_jit::NativeArtifactCache::new(php_jit::NativeCacheConfig {
                    mode: options.native_cache,
                    directory: options.native_cache_dir.clone(),
                    ..php_jit::NativeCacheConfig::default()
                })
                .map_err(|error| format!("E_NATIVE_CACHE_SETUP: {error}"))?,
            )
        };
        if let Some(cache) = &cache {
            let identity = native_cache_identity(unit, function, options, external_signatures)
                .map_err(|error| format!("E_NATIVE_CACHE_IDENTITY: {error}"))?;
            let mut compiled_records = None;
            let loaded = if cache.config().mode.can_write() {
                self.get_or_load_native_unit(&identity, || {
                    cache
                        .get_or_compile(
                            &identity,
                            |stable_id| {
                                resolve_native_cache_helper(stable_id, options.collect_counters)
                            },
                            || {
                                let (records, _) = self
                                    .compile_native(unit, function, options, external_signatures)
                                    .map_err(php_jit::NativeCacheError::InvalidHeader)?;
                                let image = cache_image(identity.clone(), function, &records)?;
                                compiled_records = Some(records);
                                Ok(image)
                            },
                        )
                        .map(|(artifact, _)| Some(artifact))
                })
            } else {
                self.get_or_load_native_unit(&identity, || {
                    cache.load(&identity, |stable_id| {
                        resolve_native_cache_helper(stable_id, options.collect_counters)
                    })
                })
            };
            let loaded = match loaded {
                Ok(loaded) => loaded,
                Err(error) if compiled_records.is_some() => {
                    if options.collect_counters {
                        let function_name = unit
                            .unit()
                            .functions
                            .get(function.index())
                            .map_or("<missing>", |function| function.name.as_str());
                        eprintln!(
                            "native_cache_persist_failed function={} function_id={} error={error}",
                            function_name,
                            function.raw(),
                        );
                    }
                    None
                }
                Err(error) => return Err(format!("E_NATIVE_CACHE_ARTIFACT: {error}")),
            };
            if let Some(loaded) = loaded {
                return loaded
                    .native_entries()
                    .get(&function)
                    .cloned()
                    .ok_or_else(|| {
                        format!(
                            "cached native function entry {} was not published",
                            function.raw()
                        )
                    });
            }
            if let Some(records) = compiled_records {
                return jit_abi::native_entries_from_records(&records)?
                    .remove(&function)
                    .ok_or_else(|| {
                        format!("native function entry {} was not published", function.raw())
                    });
            }
        }

        let (records, _) = self.compile_native(unit, function, options, external_signatures)?;
        jit_abi::native_entries_from_records(&records)?
            .remove(&function)
            .ok_or_else(|| format!("native function entry {} was not published", function.raw()))
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn external_function_signatures_hash(signatures: &[php_jit::JitExternalFunctionSignature]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for signature in signatures {
        for byte in signature.name.bytes() {
            hash =
                (hash ^ u64::from(byte.to_ascii_lowercase())).wrapping_mul(0x0000_0100_0000_01b3);
        }
        for parameter in &signature.params {
            for byte in parameter.name.bytes() {
                hash = (hash ^ u64::from(byte.to_ascii_lowercase()))
                    .wrapping_mul(0x0000_0100_0000_01b3);
            }
            hash = (hash ^ u64::from(parameter.by_ref)).wrapping_mul(0x0000_0100_0000_01b3);
            hash = (hash ^ u64::from(parameter.variadic)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

/// Coordinates mandatory native compilation and outer result assembly.
pub struct Vm {
    options: VmOptions,
    worker_state: VmWorkerState,
}

/// Native-only compilation result for one selected authoritative IR function.
#[derive(Clone, Debug)]
pub struct NativeCompileProbeReport {
    pub function: php_ir::FunctionId,
    pub function_name: String,
    pub result: php_jit::JitCompileResult,
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

impl Vm {
    #[must_use]
    pub fn new() -> Self {
        Self::with_options(VmOptions::default())
    }

    #[must_use]
    pub fn with_options(options: VmOptions) -> Self {
        let worker_state = VmWorkerState::new(options.tiering.clone());
        Self::with_options_and_worker_state(options, worker_state)
    }

    #[must_use]
    pub fn with_options_and_worker_state(options: VmOptions, worker_state: VmWorkerState) -> Self {
        Self {
            options,
            worker_state,
        }
    }

    /// Compile and publish native entries without entering application code.
    #[must_use]
    pub fn prewarm_cranelift(&self, unit: &CompiledUnit) -> u64 {
        let entry = unit.unit().entry;
        if unit.unit().functions.get(entry.index()).is_none() {
            return 0;
        }
        self.compile_native(unit, entry).map_or(0, |records| {
            records
                .0
                .iter()
                .filter(|record| {
                    matches!(record.result.status, php_jit::JitCompileStatus::Compiled)
                })
                .count() as u64
        })
    }

    /// Compiles one selected function with the production Cranelift helper ABI
    /// without entering PHP code.
    pub fn probe_cranelift(
        &self,
        unit: &CompiledUnit,
        function_name: Option<&str>,
    ) -> Result<NativeCompileProbeReport, String> {
        if self.options.verify_ir && unit.prepared_ir_verification_errors() > 0 {
            return Err(format!(
                "IR verifier failed with {} error(s)",
                unit.prepared_ir_verification_errors()
            ));
        }
        match unit.prepared_class_validation() {
            crate::compiled_unit::PreparedClassValidation::Valid => {}
            crate::compiled_unit::PreparedClassValidation::Invalid(diagnostic) => {
                return Err(diagnostic.to_string());
            }
        }
        let selected_function = if let Some(name) = function_name {
            Some(
                unit.unit()
                    .functions
                    .iter()
                    .position(|function| function.name.eq_ignore_ascii_case(name))
                    .map(|index| php_ir::FunctionId::new(index as u32))
                    .ok_or_else(|| format!("native compile probe function not found: {name}"))?,
            )
        } else {
            None
        };
        let function = selected_function.unwrap_or(unit.unit().entry);
        let function_entry = unit.unit().functions.get(function.index()).ok_or_else(|| {
            format!(
                "native compile probe function {} is missing",
                function.raw()
            )
        })?;
        let function_name = function_entry.name.clone();
        let mut compiler = php_jit::JitEngine::new();
        let request = php_jit::JitCompileRequest::new(format!(
            "probe.unit.{}.function.{}",
            unit.unit().id.raw(),
            function.raw()
        ))
        .with_function_name(function_name.clone())
        .with_opt_level(self.options.native_optimization.opt_level());
        let result = compiler
            .compile_function_with_runtime_helpers(
                unit.unit(),
                function,
                request,
                runtime_helper_addresses(false),
            )
            .map_err(|error| error.to_string())?;
        Ok(NativeCompileProbeReport {
            function,
            function_name,
            result,
        })
    }

    /// Compile the entry function from authoritative IR and enter it. Other
    /// declared functions compile through native dispatch on first execution.
    #[must_use]
    pub fn execute(&self, unit: impl Into<CompiledUnit>) -> VmResult {
        self.execute_with_external_function_signatures(unit, &[])
    }

    pub(super) fn execute_with_external_function_signatures(
        &self,
        unit: impl Into<CompiledUnit>,
        external_signatures: &[php_jit::JitExternalFunctionSignature],
    ) -> VmResult {
        let unit = unit.into();
        let output = OutputBuffer::default();
        let entry = unit.unit().entry;
        let Some(function) = unit.unit().functions.get(entry.index()) else {
            return VmResult::compile_error(output, "entry function is missing");
        };
        if self.options.verify_ir && unit.prepared_ir_verification_errors() > 0 {
            return VmResult::compile_error(
                output,
                format!(
                    "IR verifier failed with {} error(s)",
                    unit.prepared_ir_verification_errors()
                ),
            );
        }
        if let crate::compiled_unit::PreparedClassValidation::Invalid(diagnostic) =
            unit.prepared_class_validation()
        {
            return VmResult::compile_error(output, diagnostic.to_string());
        }

        if let Some(decision) = self.worker_state.background_tiering_decision(
            &unit,
            entry,
            &self.options,
            external_signatures,
        ) {
            let mut baseline_options = self.options.clone();
            baseline_options.native_optimization = NativeOptimizationPolicy::TieredBaseline;
            baseline_options.tiering.enabled = false;
            let mut result =
                Vm::with_options_and_worker_state(baseline_options, self.worker_state.clone())
                    .execute_with_external_function_signatures(unit.clone(), external_signatures);
            if result.status.is_success() {
                self.worker_state.schedule_background_optimization(
                    decision,
                    unit,
                    entry,
                    external_signatures.to_vec(),
                );
            }
            if self.options.tiering.collect_stats {
                result.tiering_stats = Some(Box::new(self.worker_state.tiering_stats()));
            }
            return result;
        }

        let worker_cache_before = self.worker_state.native_compile_cache_stats();
        let mut cache_load_time = Duration::ZERO;
        let mut native_compile_time = Duration::ZERO;
        let cache_candidate = native_cache_candidate(unit.unit(), entry);
        let cache = match cache_candidate {
            true => match self.native_cache() {
                Ok(cache) => cache,
                Err(error) => {
                    return VmResult::compile_error(
                        output,
                        format!("E_NATIVE_CACHE_SETUP: {error}"),
                    );
                }
            },
            false => None,
        };
        let cache_identity = cache.as_ref().and_then(|_| {
            native_cache_identity(&unit, entry, &self.options, external_signatures).ok()
        });
        let mut cached_compile_records = None;
        let mut cached_compile_error = None;

        if let (Some(cache), Some(identity)) = (&cache, &cache_identity) {
            if cache.config().mode.can_write() {
                let cache_started = Instant::now();
                let result = self.worker_state.get_or_load_native_unit(identity, || {
                    cache
                        .get_or_compile(
                            identity,
                            |stable_id| {
                                resolve_native_cache_helper(
                                    stable_id,
                                    self.options.collect_counters,
                                )
                            },
                            || {
                                let compile_started = Instant::now();
                                let (records, disposition) = match self
                                    .compile_native_with_external_function_signatures(
                                        &unit,
                                        entry,
                                        external_signatures,
                                    ) {
                                    Ok(records) => records,
                                    Err(error) => {
                                        native_compile_time += compile_started.elapsed();
                                        cached_compile_error = Some(error.clone());
                                        return Err(php_jit::NativeCacheError::InvalidHeader(
                                            error,
                                        ));
                                    }
                                };
                                if disposition.compiled() {
                                    native_compile_time += compile_started.elapsed();
                                }
                                let image = cache_image(identity.clone(), entry, &records);
                                cached_compile_records = Some(records);
                                image
                            },
                        )
                        .map(|(artifact, _)| Some(artifact))
                });
                cache_load_time += cache_started.elapsed().saturating_sub(native_compile_time);
                let cache_error = match result {
                    Ok(Some(loaded)) => {
                        let result = self.execute_cached_entry(&unit, loaded, entry, output);
                        return self.attach_native_cache_metrics(
                            result,
                            cache,
                            cache_load_time,
                            native_compile_time,
                            worker_cache_before,
                        );
                    }
                    Ok(None) => php_jit::NativeCacheError::InvalidHeader(
                        "native cache write produced no loaded unit".to_owned(),
                    ),
                    Err(error) => error,
                };
                if let Some(error) = cached_compile_error {
                    let result =
                        VmResult::compile_error(output, format!("E_NATIVE_COMPILE_SETUP: {error}"));
                    return self.attach_native_cache_metrics(
                        result,
                        cache,
                        cache_load_time,
                        native_compile_time,
                        worker_cache_before,
                    );
                }
                if cached_compile_records.is_none() {
                    let result = VmResult::compile_error(
                        output,
                        format!("E_NATIVE_CACHE_ARTIFACT: {cache_error}"),
                    );
                    return self.attach_native_cache_metrics(
                        result,
                        cache,
                        cache_load_time,
                        native_compile_time,
                        worker_cache_before,
                    );
                }
            } else if cache.config().mode.can_read() {
                let cache_started = Instant::now();
                let loaded = self.worker_state.get_or_load_native_unit(identity, || {
                    cache.load(identity, |stable_id| {
                        resolve_native_cache_helper(stable_id, self.options.collect_counters)
                    })
                });
                cache_load_time += cache_started.elapsed();
                if let Ok(Some(loaded)) = loaded {
                    let result = self.execute_cached_entry(&unit, loaded, entry, output);
                    return self.attach_native_cache_metrics(
                        result,
                        cache,
                        cache_load_time,
                        native_compile_time,
                        worker_cache_before,
                    );
                }
            }
        }

        let compile_started = Instant::now();
        let records = match cached_compile_records {
            Some(records) => records,
            None => match self.compile_native_with_external_function_signatures(
                &unit,
                entry,
                external_signatures,
            ) {
                Ok((records, disposition)) => {
                    if disposition.compiled() {
                        native_compile_time += compile_started.elapsed();
                    }
                    records
                }
                Err(error) => {
                    native_compile_time += compile_started.elapsed();
                    let result =
                        VmResult::compile_error(output, format!("E_NATIVE_COMPILE_SETUP: {error}"));
                    return self.attach_optional_native_cache_metrics(
                        result,
                        cache.as_ref(),
                        cache_load_time,
                        native_compile_time,
                        worker_cache_before,
                    );
                }
            },
        };
        let Some(entry_record) = records.iter().find(|record| record.function == entry) else {
            let result =
                VmResult::compile_error(output, "E_NATIVE_COMPILE_SETUP: entry record missing");
            return self.attach_optional_native_cache_metrics(
                result,
                cache.as_ref(),
                cache_load_time,
                native_compile_time,
                worker_cache_before,
            );
        };
        if let Some(rejected) = records
            .iter()
            .find(|record| !matches!(&record.result.status, php_jit::JitCompileStatus::Compiled))
        {
            let name = unit
                .unit()
                .functions
                .get(rejected.function.index())
                .map_or("<missing>", |function| function.name.as_str());
            let reason = match &rejected.result.status {
                php_jit::JitCompileStatus::Rejected { reason } => reason.as_str(),
                php_jit::JitCompileStatus::Compiled => "compiler reported no native code",
            };
            let detail = rejected
                .result
                .diagnostics
                .first()
                .map_or("", String::as_str);
            let result = VmResult::compile_error(
                output,
                format!("E_NATIVE_UNSUPPORTED_LOWERING: function={name}: {reason}: {detail}"),
            );
            return self.attach_optional_native_cache_metrics(
                result,
                cache.as_ref(),
                cache_load_time,
                native_compile_time,
                worker_cache_before,
            );
        }
        let compiled = &entry_record.result;
        let Some(handle) = compiled.handle.as_ref() else {
            let reason = match &compiled.status {
                php_jit::JitCompileStatus::Rejected { reason } => reason.clone(),
                php_jit::JitCompileStatus::Compiled => {
                    "compiler reported success without a native entry".to_owned()
                }
            };
            let result = VmResult::compile_error(output, format!("E_NATIVE_COMPILE: {reason}"));
            return self.attach_optional_native_cache_metrics(
                result,
                cache.as_ref(),
                cache_load_time,
                native_compile_time,
                worker_cache_before,
            );
        };
        let native_entries = records
            .iter()
            .filter_map(|record| {
                record
                    .result
                    .handle
                    .as_ref()
                    .cloned()
                    .map(|handle| (record.function, handle))
            })
            .collect();
        let native_entries = Arc::new(native_entries);
        let mut context = NativeRequestOwner::new(
            &unit,
            unit.artifact_identity(),
            &self.options,
            &self.worker_state,
            output,
            native_entries,
        );
        context.attach_root_deployment_image(unit.clone());
        let native_execution_started_at =
            self.options.collect_counters.then(std::time::Instant::now);
        context.record_native_direct_calls(handle);
        let guard = activate_native_context(&mut context);
        let runtime = context.native_runtime_ptr();
        let outcome = handle.invoke_i64_with_native_unwind_runtime(
            &[],
            php_jit::JIT_RUNTIME_ABI_HASH,
            runtime,
            |types, value| {
                let class = context
                    .decode_result(value)
                    .ok()
                    .and_then(native_exception_fields)
                    .map(|(class, _, _)| class);
                class.is_some_and(|class| {
                    types.iter().any(|type_| {
                        type_.eq_ignore_ascii_case(&class)
                            || type_.eq_ignore_ascii_case("Throwable")
                            || (type_.eq_ignore_ascii_case("Exception")
                                && class.ends_with("Exception"))
                            || (type_.eq_ignore_ascii_case("Error")
                                && (class == "Error" || class.ends_with("Error")))
                    })
                })
            },
        );
        let outcome = resume_native_optimizing_exit(&mut context, outcome);
        let (exception_handled, exception_handler_error) = match &outcome {
            Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
                if *status == php_jit::JitCallStatus::THROW.0 as i32 =>
            {
                match context.handle_uncaught_throwable(*value) {
                    Ok(handled) => (handled, None),
                    Err(error) => (false, Some(error)),
                }
            }
            _ => (false, None),
        };
        let mut shutdown_throwable = None;
        let shutdown_error = exception_handler_error.or_else(|| {
            context.run_shutdown_callbacks().err().and_then(|error| {
                if error == "E_PHP_RETHROW"
                    && let Some(throwable) = context.take_pending_throwable()
                {
                    shutdown_throwable = Some(throwable);
                    None
                } else {
                    Some(error)
                }
            })
        });
        context.output.flush_all_buffers();
        drop(guard);
        let publish_error = context.publish_include_globals().err();
        let native_execution_time_nanos = native_execution_started_at.map_or(0, |started_at| {
            started_at.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
        });
        let runtime_counters = self.options.collect_counters.then(|| {
            let mut counters = context.runtime_counters();
            counters.native_execution_entries = counters.native_execution_entries.saturating_add(1);
            counters.native_region_entries = counters.native_region_entries.saturating_add(1);
            counters.native_execution_time_nanos = native_execution_time_nanos;
            counters
        });
        let http_response = std::mem::take(&mut context.http_response);
        let upload_registry = std::mem::take(&mut context.upload_registry);
        let session = std::mem::take(&mut context.session);
        let process_exit_terminates_process = context.process_exit_terminates_process();
        let mut result = if let Some(throwable) = shutdown_throwable {
            native_uncaught_throwable_result(std::mem::take(&mut context.output), Some(throwable))
        } else if let Some(error) = shutdown_error.or(publish_error) {
            VmResult::runtime_error(
                std::mem::take(&mut context.output),
                context.diagnostic.take(),
                format!("E_NATIVE_SHUTDOWN: {error}"),
            )
        } else if exception_handled {
            VmResult::success(std::mem::take(&mut context.output), Some(Value::Null))
        } else {
            match outcome {
                Ok(php_jit::JitI64InvokeOutcome::Returned(value)) => {
                    match context.decode_result(value) {
                        Ok(value) => {
                            let mut result =
                                VmResult::success(std::mem::take(&mut context.output), Some(value));
                            result.diagnostics.extend(context.diagnostic.take());
                            result
                        }
                        Err(error) => VmResult::runtime_error(
                            std::mem::take(&mut context.output),
                            context.diagnostic.take(),
                            format!("E_NATIVE_VALUE: {error}"),
                        ),
                    }
                }
                Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
                    if status == php_jit::JitCallStatus::EXIT.0 as i32 =>
                {
                    let exit_code = match context.decode_result(value) {
                        Ok(Value::String(value)) => {
                            context.output.write_bytes(value.as_bytes());
                            0
                        }
                        Ok(Value::Int(value)) => i32::try_from(value).unwrap_or(0),
                        Ok(Value::Bool(value)) => i32::from(value),
                        _ => 0,
                    };
                    VmResult::success_exit(std::mem::take(&mut context.output), exit_code)
                }
                Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
                    if status == php_jit::JitCallStatus::THROW.0 as i32 =>
                {
                    let throwable = context.decode_result(value).ok();
                    native_uncaught_throwable_result(std::mem::take(&mut context.output), throwable)
                }
                Ok(php_jit::JitI64InvokeOutcome::SideExit { status, state, .. })
                    if status == php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32 =>
                {
                    let operation =
                        context.instruction_kind_debug(state.function_id, state.continuation_id);
                    let message = context
                        .diagnostic
                        .as_ref()
                        .map_or_else(
                            || {
                                format!(
                                    "native runtime operation failed at function {} continuation {} ({}) native version {} control {} marker {:#x} value {}",
                                    state.function_id,
                                    state.continuation_id,
                                    operation,
                                    state.native_version,
                                    state.control_status.0,
                                    state.control_reserved,
                                    state.control_value,
                                )
                            },
                            |diagnostic| diagnostic.message().to_owned(),
                        );
                    if context.diagnostic.as_ref().is_some_and(|diagnostic| {
                        diagnostic.severity() == php_runtime::api::RuntimeSeverity::FatalError
                    }) && context
                        .output
                        .as_bytes()
                        .windows(b"Fatal error".len())
                        .any(|window| window == b"Fatal error")
                    {
                        VmResult::fatal(
                            std::mem::take(&mut context.output),
                            context.diagnostic.take(),
                            message,
                        )
                    } else {
                        VmResult::runtime_error(
                            std::mem::take(&mut context.output),
                            context.diagnostic.take(),
                            message,
                        )
                    }
                }
                Ok(php_jit::JitI64InvokeOutcome::SideExit { status, .. })
                    if status == php_jit::JitCallStatus::RETURN_REFERENCE.0 as i32 =>
                {
                    VmResult::success(std::mem::take(&mut context.output), None)
                }
                Ok(php_jit::JitI64InvokeOutcome::SideExit { status, .. }) => {
                    VmResult::runtime_error(
                        std::mem::take(&mut context.output),
                        context.diagnostic.take(),
                        format!("native entry returned status {status}"),
                    )
                }
                Err(error) => VmResult::compile_error(
                    std::mem::take(&mut context.output),
                    format!("E_NATIVE_ENTRY: native entry invocation failed: {error:?}"),
                ),
            }
        };
        context.recycle_native_value_arena();
        result.process_exit_terminates_process = process_exit_terminates_process;
        result.http_response = Some(Box::new(http_response));
        result.upload_registry = Some(Box::new(upload_registry));
        result.session = Some(Box::new(session));
        if let Some(runtime_counters) = runtime_counters {
            result.counters = Some(Box::new(runtime_counters));
        }
        if self.options.trace {
            result.trace.push(format!(
                "vm-trace: function={}({}) native_entry=cranelift output_len={}",
                function.name,
                entry.raw(),
                result.output.as_bytes().len()
            ));
        }
        self.attach_optional_native_cache_metrics(
            result,
            cache.as_ref(),
            cache_load_time,
            native_compile_time,
            worker_cache_before,
        )
    }

    fn compile_native(
        &self,
        unit: &CompiledUnit,
        function: php_ir::FunctionId,
    ) -> Result<
        (
            Arc<[php_jit::JitUnitCompileRecord]>,
            native_compile_cache::NativeCompileCacheDisposition,
        ),
        String,
    > {
        self.worker_state
            .compile_native(unit, function, &self.options, &[])
    }

    fn compile_native_with_external_function_signatures(
        &self,
        unit: &CompiledUnit,
        function: php_ir::FunctionId,
        external_signatures: &[php_jit::JitExternalFunctionSignature],
    ) -> Result<
        (
            Arc<[php_jit::JitUnitCompileRecord]>,
            native_compile_cache::NativeCompileCacheDisposition,
        ),
        String,
    > {
        self.worker_state
            .compile_native(unit, function, &self.options, external_signatures)
    }

    fn native_cache(
        &self,
    ) -> Result<Option<php_jit::NativeArtifactCache>, php_jit::NativeCacheError> {
        if self.options.native_cache == php_jit::NativeCacheMode::Off {
            return Ok(None);
        }
        php_jit::NativeArtifactCache::new(php_jit::NativeCacheConfig {
            mode: self.options.native_cache,
            directory: self.options.native_cache_dir.clone(),
            ..php_jit::NativeCacheConfig::default()
        })
        .map(Some)
    }

    fn attach_optional_native_cache_metrics(
        &self,
        result: VmResult,
        cache: Option<&php_jit::NativeArtifactCache>,
        cache_load_time: Duration,
        native_compile_time: Duration,
        worker_cache_before: NativeCompileCacheStats,
    ) -> VmResult {
        self.attach_native_metrics(
            result,
            cache.map(php_jit::NativeArtifactCache::stats),
            cache_load_time,
            native_compile_time,
            worker_cache_before,
        )
    }

    fn attach_native_cache_metrics(
        &self,
        result: VmResult,
        cache: &php_jit::NativeArtifactCache,
        cache_load_time: Duration,
        native_compile_time: Duration,
        worker_cache_before: NativeCompileCacheStats,
    ) -> VmResult {
        self.attach_native_metrics(
            result,
            Some(cache.stats()),
            cache_load_time,
            native_compile_time,
            worker_cache_before,
        )
    }

    fn attach_native_metrics(
        &self,
        mut result: VmResult,
        cache_stats: Option<php_jit::NativeCacheStats>,
        cache_load_time: Duration,
        native_compile_time: Duration,
        worker_cache_before: NativeCompileCacheStats,
    ) -> VmResult {
        let worker_cache = self
            .worker_state
            .native_compile_cache_stats()
            .saturating_delta(worker_cache_before);
        result.native_cache_load_nanos =
            cache_load_time.as_nanos().min(u128::from(u64::MAX)) as u64;
        result.native_compile_nanos = worker_cache
            .compile_time_nanos
            .max(native_compile_time.as_nanos().min(u128::from(u64::MAX)) as u64);
        if self.options.native_cache_stats
            && let Some(stats) = cache_stats
        {
            result.native_cache_stats = Some(Box::new(stats));
        }
        if self.options.collect_counters {
            let mut counters = result
                .counters
                .take()
                .map_or_else(crate::counters::VmCounters::default, |counters| *counters);
            let executed = result.status.is_success();
            counters.native_compile_attempts = worker_cache.misses;
            counters.native_compile_successes = worker_cache.insertions;
            counters.native_compile_failures = worker_cache.compile_failures;
            counters.native_compile_time_nanos = result.native_compile_nanos;
            counters.native_execution_entries =
                counters.native_execution_entries.max(u64::from(executed));
            counters.native_region_entries =
                counters.native_region_entries.max(u64::from(executed));
            counters.native_version_published = worker_cache.insertions;
            let code_stats = php_jit::cranelift_code_manager_stats();
            counters.native_function_body_compile_count = code_stats.function_body_compile_count;
            counters.native_duplicate_function_body_count =
                code_stats.duplicate_function_publications;
            let loaded_stats = self.worker_state.loaded_native_unit_stats();
            counters.native_loaded_artifact_registry_hits = loaded_stats.hits;
            counters.native_loaded_artifact_maps = loaded_stats.maps;
            counters.native_loaded_entry_table_constructions =
                loaded_stats.entry_table_constructions;
            counters.native_mapped_executable_bytes = loaded_stats.mapped_executable_bytes;
            if let Some(stats) = cache_stats {
                counters.native_cache_hits = stats.hits;
                counters.native_cache_misses = stats.misses;
                counters.native_cache_writes = stats.writes;
                counters.native_cache_rebuilds = stats.rebuilds;
                counters.native_cache_invalid_artifacts = stats.invalid_artifacts;
                counters.native_cache_compile_waits = stats.compile_waits;
                counters.native_cache_bytes_loaded = stats.bytes_loaded;
                counters.native_cache_bytes_written = stats.bytes_written;
            }
            result.counters = Some(Box::new(counters));
        }
        if self.options.tiering.collect_stats {
            result.tiering_stats = Some(Box::new(self.worker_state.tiering_stats()));
        }
        result
    }
}

pub(super) fn compile_native_function_graph(
    unit: &php_ir::IrUnit,
    function: php_ir::FunctionId,
    options: &VmOptions,
    ir_fingerprint: &str,
    deployment_identity: &str,
    deployment_runtime_identity: u64,
    dependency_identity: &str,
    external_signatures: &[php_jit::JitExternalFunctionSignature],
) -> Result<Vec<php_jit::JitUnitCompileRecord>, String> {
    let function_name = unit
        .functions
        .get(function.index())
        .ok_or_else(|| format!("native function {} is missing", function.raw()))?
        .name
        .clone();
    let mut compiler = php_jit::JitEngine::new();
    let result = compiler
        .compile_function_with_runtime_helpers(
            unit,
            function,
            php_jit::JitCompileRequest::new(format!("unit.{}", unit.id.raw()))
                .with_function_name(function_name)
                .with_ir_fingerprint(ir_fingerprint)
                .with_deployment_identity(deployment_identity)
                .with_deployment_runtime_identity(deployment_runtime_identity)
                .with_dependency_identity(dependency_identity)
                .with_external_function_signatures(external_signatures.to_vec())
                .with_opt_level(options.native_optimization.opt_level()),
            runtime_helper_addresses(options.collect_counters),
        )
        .map_err(|error| error.to_string())?;
    Ok(vec![php_jit::JitUnitCompileRecord { function, result }])
}

fn native_exception_fields(value: Value) -> Option<(String, String, String)> {
    if let Value::Object(object) = value {
        let string = |value: Value| match value {
            Value::String(value) => Some(String::from_utf8_lossy(value.as_bytes()).into_owned()),
            Value::Null => Some(String::new()),
            _ => None,
        };
        let message = object
            .get_property("message")
            .and_then(string)
            .unwrap_or_default();
        let file = object
            .get_property("file")
            .and_then(string)
            .unwrap_or_else(|| "<unknown>".to_owned());
        return Some((object.display_name(), message, file));
    }
    let Value::Array(array) = value else {
        return None;
    };
    let field = |name: &str| {
        array.get(&php_runtime::api::ArrayKey::String(
            php_runtime::api::PhpString::from_bytes(name.as_bytes().to_vec()),
        ))
    };
    let string = |value: &Value| match value {
        Value::String(value) => Some(String::from_utf8_lossy(value.as_bytes()).into_owned()),
        Value::Null => Some(String::new()),
        _ => None,
    };
    let raw_class = string(field("class")?)?;
    let class = match raw_class.to_ascii_lowercase().as_str() {
        "exception" => "Exception".to_owned(),
        "runtimeexception" => "RuntimeException".to_owned(),
        "error" => "Error".to_owned(),
        "typeerror" => "TypeError".to_owned(),
        "valueerror" => "ValueError".to_owned(),
        "argumentcounterror" => "ArgumentCountError".to_owned(),
        "divisionbyzeroerror" => "DivisionByZeroError".to_owned(),
        _ => raw_class,
    };
    Some((class, string(field("message")?)?, string(field("file")?)?))
}

fn native_exception_detailed_output(
    value: &Value,
    class: &str,
    message: &str,
    file: &str,
) -> Option<String> {
    let Value::Array(exception) = value else {
        return None;
    };
    let key = |name: &str| {
        php_runtime::api::ArrayKey::String(php_runtime::api::PhpString::from_bytes(
            name.as_bytes().to_vec(),
        ))
    };
    let Value::Int(line) = exception.get(&key("line"))? else {
        return None;
    };
    let line = usize::try_from(*line).ok()?;
    let trace = match exception.get(&key("trace")) {
        Some(Value::Array(trace)) => trace,
        _ => {
            return Some(format!(
                "\nFatal error: Uncaught {class}: {message} in {file}:{line}\nStack trace:\n#0 {{main}}\n  thrown in {file} on line {line}\n"
            ));
        }
    };
    let frames = trace
        .iter()
        .filter_map(|(_, value)| {
            let Value::Array(frame) = value else {
                return None;
            };
            let string = |name: &str| match frame.get(&key(name)) {
                Some(Value::String(value)) => {
                    Some(String::from_utf8_lossy(value.as_bytes()).into_owned())
                }
                _ => None,
            };
            let internal = matches!(frame.get(&key("internal")), Some(Value::Bool(true)));
            let frame_line = match frame.get(&key("line")) {
                Some(Value::Int(value)) => usize::try_from(*value).ok(),
                _ => None,
            };
            let function = string("function")?;
            let frame_file = string("file");
            let args = match frame.get(&key("args")) {
                Some(Value::Array(args)) => args
                    .iter()
                    .map(|(_, value)| native_trace_argument(value))
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => String::new(),
            };
            if !internal && (frame_file.is_none() || frame_line.is_none()) {
                return None;
            }
            Some((frame_file, frame_line, function, args, internal))
        })
        .collect::<Vec<_>>();
    if frames.is_empty() {
        return Some(format!(
            "\nFatal error: Uncaught {class}: {message} in {file}:{line}\nStack trace:\n#0 {{main}}\n  thrown in {file} on line {line}\n"
        ));
    }
    let detailed_message = match &frames[0] {
        (_, _, _, _, true) => format!("{message} in {file}:{line}"),
        (Some(call_file), Some(call_line), _, _, false) => format!(
            "{message}, called in {call_file} on line {call_line} and defined in {file}:{line}"
        ),
        _ => format!("{message} in {file}:{line}"),
    };
    let mut output = format!("\nFatal error: Uncaught {class}: {detailed_message}\nStack trace:\n");
    for (index, (frame_file, frame_line, function, args, internal)) in frames.iter().enumerate() {
        if *internal {
            output.push_str(&format!(
                "#{index} [internal function]: {function}({args})\n"
            ));
        } else if let (Some(frame_file), Some(frame_line)) = (frame_file, frame_line) {
            output.push_str(&format!(
                "#{index} {frame_file}({frame_line}): {function}({args})\n"
            ));
        }
    }
    output.push_str(&format!(
        "#{} {{main}}\n  thrown in {file} on line {line}\n",
        frames.len()
    ));
    Some(output)
}

fn native_uncaught_throwable_result(
    mut output: php_runtime::api::OutputBuffer,
    throwable: Option<Value>,
) -> VmResult {
    let (class, message, file) = throwable
        .clone()
        .and_then(native_exception_fields)
        .unwrap_or_else(|| {
            (
                "Exception".to_owned(),
                "unknown exception".to_owned(),
                "<unknown>".to_owned(),
            )
        });
    let rendered = throwable
        .as_ref()
        .and_then(|value| native_exception_detailed_output(value, &class, &message, &file))
        .unwrap_or_else(|| {
            format!(
                "\nFatal error: Uncaught {class}: {message}\nStack trace:\n#0 {{main}}\n  thrown in {file}\n"
            )
        });
    output.write_bytes(rendered);
    let diagnostic = php_runtime::api::RuntimeDiagnostic::new(
        "E_PHP_VM_UNCAUGHT_THROWABLE",
        php_runtime::api::RuntimeSeverity::FatalError,
        format!("Uncaught {class}: {message}"),
        php_runtime::api::RuntimeSourceSpan {
            file: Some(file),
            start: 0,
            end: 0,
        },
        Vec::new(),
        None,
    );
    VmResult::fatal(output, Some(diagnostic), "uncaught throwable")
}

fn native_trace_argument(value: &Value) -> String {
    match value {
        Value::String(value) => format!("'{}'", String::from_utf8_lossy(value.as_bytes())),
        Value::Array(_) => "Array".to_owned(),
        Value::Int(value) => value.to_string(),
        Value::Float(value) => value.to_f64().to_string(),
        Value::Bool(true) => "true".to_owned(),
        Value::Bool(false) => "false".to_owned(),
        Value::Null | Value::Uninitialized => "NULL".to_owned(),
        Value::Object(object) => format!("Object({})", object.display_name()),
        _ => "...".to_owned(),
    }
}

fn native_cache_candidate(unit: &php_ir::IrUnit, entry: php_ir::FunctionId) -> bool {
    // Each persistent image is rooted at exactly one PHP function. Dormant
    // declarations never contribute code, relocations, or cache bytes.
    unit.functions.get(entry.index()).is_some()
}

fn native_cache_identity(
    unit: &CompiledUnit,
    function: php_ir::FunctionId,
    options: &VmOptions,
    external_signatures: &[php_jit::JitExternalFunctionSignature],
) -> Result<php_jit::NativeCacheIdentity, php_jit::CraneliftHostIsaError> {
    let isa = php_jit::cranelift_host_isa_identity()?;
    let optimization_tier = format!(
        "{}:{}",
        options.native_optimization.as_str(),
        php_jit::native_compiler_mode_identity(options.native_optimization.is_optimizing())
    );
    let function_ir_hash = unit
        .prepared_function_ir_fingerprint(function)
        .map(str::to_owned)
        .unwrap_or_else(|| php_jit::stable_function_ir_fingerprint(unit.unit(), function));
    let external_signatures_hash = external_function_signatures_hash(external_signatures);
    Ok(php_jit::NativeCacheIdentity {
        source_hash: format!("compiled-function-source-v3-{function_ir_hash}"),
        ir_hash: format!(
            "{function_ir_hash}:fragment-plan-schema-v{}",
            php_jit::NATIVE_FRAGMENT_PLAN_SCHEMA_VERSION
        ),
        dependency_graph_hash: format!(
            "{}:external-signatures-{external_signatures_hash:016x}",
            unit.prepared_dependency_identity()
        ),
        build_id: option_env!("PHRUST_BUILD_ID")
            .unwrap_or(env!("PHRUST_AUTO_BUILD_ID"))
            .to_owned(),
        cranelift_version: php_jit::CRANELIFT_VERSION.to_owned(),
        cranelift_settings_hash: isa.feature_fingerprint,
        region_ir_schema_version: php_jit::region_ir::REGION_IR_SCHEMA_VERSION,
        runtime_abi_hash: php_jit::JIT_RUNTIME_ABI_HASH
            ^ php_runtime::api::NATIVE_OPERATION_ABI_HASH,
        helper_abi_hash: php_jit::JIT_HELPER_REGISTRY_ABI_HASH,
        target_triple: isa.target_triple,
        pointer_width: usize::BITS as u8,
        cpu_feature_fingerprint: isa.feature_fingerprint,
        optimization_tier,
        optimization_config_hash: u64::from(options.native_optimization.opt_level())
            | (u64::from(options.collect_counters) << 8),
        php_semantic_config_hash: 0x0008_0005_0007,
    })
}

fn cache_image(
    identity: php_jit::NativeCacheIdentity,
    _entry: php_ir::FunctionId,
    records: &[php_jit::JitUnitCompileRecord],
) -> Result<php_jit::NativeArtifactImage, php_jit::NativeCacheError> {
    php_jit::NativeArtifactImage::from_compile_records(identity, records)
}

fn runtime_helper_addresses(diagnostic: bool) -> php_jit::JitRuntimeHelperAddresses {
    macro_rules! helper_address {
        ($production:path, $diagnostic:path) => {
            if diagnostic {
                $diagnostic as *const () as usize
            } else {
                $production as *const () as usize
            }
        };
    }
    php_jit::JitRuntimeHelperAddresses {
        native_call_dispatch: helper_address!(
            jit_native_call_dispatch_abi,
            jit_native_call_dispatch_diagnostic_abi
        ),
        native_builtin_dispatch: helper_address!(
            jit_baseline_native_builtin_dispatch_abi,
            jit_baseline_native_builtin_dispatch_diagnostic_abi
        ),
        native_defined: jit_native_defined_abi as *const () as usize,
        native_function_exists: jit_native_function_exists_abi as *const () as usize,
        native_class_exists: jit_native_class_exists_abi as *const () as usize,
        native_interface_exists: jit_native_interface_exists_abi as *const () as usize,
        native_trait_exists: jit_native_trait_exists_abi as *const () as usize,
        native_enum_exists: jit_native_enum_exists_abi as *const () as usize,
        native_method_exists: jit_native_method_exists_abi as *const () as usize,
        native_property_exists: jit_native_property_exists_abi as *const () as usize,
        native_preg_match: jit_native_preg_match_abi as *const () as usize,
        native_preg_match_all: jit_native_preg_match_all_abi as *const () as usize,
        native_preg_replace: jit_native_preg_replace_abi as *const () as usize,
        native_preg_filter: jit_native_preg_filter_abi as *const () as usize,
        native_preg_split: jit_native_preg_split_abi as *const () as usize,
        native_preg_grep: jit_native_preg_grep_abi as *const () as usize,
        native_preg_quote: jit_native_preg_quote_abi as *const () as usize,
        native_preg_last_error: jit_native_preg_last_error_abi as *const () as usize,
        native_preg_last_error_msg: jit_native_preg_last_error_msg_abi as *const () as usize,
        native_json_encode: jit_native_json_encode_abi as *const () as usize,
        native_json_decode: jit_native_json_decode_abi as *const () as usize,
        native_json_validate: jit_native_json_validate_abi as *const () as usize,
        native_json_last_error: jit_native_json_last_error_abi as *const () as usize,
        native_json_last_error_msg: jit_native_json_last_error_msg_abi as *const () as usize,
        native_sprintf: jit_native_sprintf_abi as *const () as usize,
        native_printf: jit_native_printf_abi as *const () as usize,
        native_vsprintf: jit_native_vsprintf_abi as *const () as usize,
        native_vprintf: jit_native_vprintf_abi as *const () as usize,
        native_basename: jit_native_basename_abi as *const () as usize,
        native_dirname: jit_native_dirname_abi as *const () as usize,
        native_realpath: jit_native_realpath_abi as *const () as usize,
        native_file_exists: jit_native_file_exists_abi as *const () as usize,
        native_semantic_dispatch: helper_address!(
            jit_native_semantic_dispatch_abi,
            jit_native_semantic_dispatch_diagnostic_abi
        ),
        native_function_resolve: helper_address!(
            jit_native_function_resolve_abi,
            jit_abi::jit_native_function_resolve_diagnostic_abi
        ),
        native_frame_alloc: helper_address!(
            jit_native_frame_alloc_abi,
            jit_abi::jit_native_frame_alloc_diagnostic_abi
        ),
        native_frame_release: helper_address!(
            jit_native_frame_release_abi,
            jit_abi::jit_native_frame_release_diagnostic_abi
        ),
        native_dynamic_code: helper_address!(
            jit_native_dynamic_code_abi,
            jit_abi::jit_native_dynamic_code_diagnostic_abi
        ),
        native_unary: helper_address!(
            jit_native_unary_abi,
            jit_abi::jit_native_unary_diagnostic_abi
        ),
        native_binary: helper_address!(
            jit_native_binary_abi,
            jit_abi::jit_native_binary_diagnostic_abi
        ),
        native_compare: helper_address!(
            jit_native_compare_abi,
            jit_abi::jit_native_compare_diagnostic_abi
        ),
        native_cast: helper_address!(jit_native_cast_abi, jit_abi::jit_native_cast_diagnostic_abi),
        native_echo: helper_address!(jit_native_echo_abi, jit_abi::jit_native_echo_diagnostic_abi),
        native_echo_bytes: jit_native_echo_bytes_abi as *const () as usize,
        native_echo_int: jit_native_echo_int_abi as *const () as usize,
        native_echo_float: jit_native_echo_float_abi as *const () as usize,
        native_float_to_string: jit_native_float_to_string_abi as *const () as usize,
        native_float_to_int: jit_native_float_to_int_abi as *const () as usize,
        native_object_class_name: jit_native_object_class_name_abi as *const () as usize,
        native_prepared_object_new: jit_native_prepared_object_new_abi as *const () as usize,
        native_plain_object_clone: jit_native_plain_object_clone_abi as *const () as usize,
        native_local_fetch: helper_address!(
            jit_native_local_fetch_abi,
            jit_abi::jit_native_local_fetch_diagnostic_abi
        ),
        native_local_store: helper_address!(
            jit_native_local_store_abi,
            jit_abi::jit_native_local_store_diagnostic_abi
        ),
        native_value_release: helper_address!(
            jit_native_value_release_abi,
            jit_abi::jit_native_value_release_diagnostic_abi
        ),
        native_reference_bind: helper_address!(
            jit_native_reference_bind_abi,
            jit_abi::jit_native_reference_bind_diagnostic_abi
        ),
        native_argument_check: helper_address!(
            jit_native_argument_check_abi,
            jit_abi::jit_native_argument_check_diagnostic_abi
        ),
        native_return_check: helper_address!(
            jit_native_return_check_abi,
            jit_abi::jit_native_return_check_diagnostic_abi
        ),
        native_exception_new: helper_address!(
            jit_native_exception_new_abi,
            jit_abi::jit_native_exception_new_diagnostic_abi
        ),
        native_array_new: helper_address!(
            jit_native_array_new_abi,
            jit_abi::jit_native_array_new_diagnostic_abi
        ),
        native_object_new: helper_address!(
            jit_native_object_new_abi,
            jit_abi::jit_native_object_new_diagnostic_abi
        ),
        native_property_fetch: helper_address!(
            jit_native_property_fetch_abi,
            jit_abi::jit_native_property_fetch_diagnostic_abi
        ),
        native_property_assign: helper_address!(
            jit_native_property_assign_abi,
            jit_abi::jit_native_property_assign_diagnostic_abi
        ),
        native_object_clone: helper_address!(
            jit_native_object_clone_abi,
            jit_abi::jit_native_object_clone_diagnostic_abi
        ),
        native_object_clone_with: helper_address!(
            jit_native_object_clone_with_abi,
            jit_abi::jit_native_object_clone_with_diagnostic_abi
        ),
        native_array_insert: helper_address!(
            jit_native_array_insert_abi,
            jit_abi::jit_native_array_insert_diagnostic_abi
        ),
        native_array_insert_local: helper_address!(
            jit_native_array_insert_local_abi,
            jit_abi::jit_native_array_insert_local_diagnostic_abi
        ),
        native_array_fetch: helper_address!(
            jit_native_array_fetch_abi,
            jit_abi::jit_native_array_fetch_diagnostic_abi
        ),
        native_array_unset: helper_address!(
            jit_native_array_unset_abi,
            jit_abi::jit_native_array_unset_diagnostic_abi
        ),
        native_array_spread: helper_address!(
            jit_native_array_spread_abi,
            jit_abi::jit_native_array_spread_diagnostic_abi
        ),
        native_foreach_init: helper_address!(
            jit_native_foreach_init_abi,
            jit_abi::jit_native_foreach_init_diagnostic_abi
        ),
        native_foreach_next: helper_address!(
            jit_native_foreach_next_abi,
            jit_abi::jit_native_foreach_next_diagnostic_abi
        ),
        native_foreach_cleanup: helper_address!(
            jit_native_foreach_cleanup_abi,
            jit_abi::jit_native_foreach_cleanup_diagnostic_abi
        ),
        native_constant_fetch: helper_address!(
            jit_native_constant_fetch_abi,
            jit_abi::jit_native_constant_fetch_diagnostic_abi
        ),
        native_truthy: helper_address!(
            jit_native_truthy_abi,
            jit_abi::jit_native_truthy_diagnostic_abi
        ),
        native_type_predicate: helper_address!(
            jit_native_type_predicate_abi,
            jit_abi::jit_native_type_predicate_diagnostic_abi
        ),
        native_stable_length: helper_address!(
            jit_native_stable_length_abi,
            jit_abi::jit_native_stable_length_diagnostic_abi
        ),
        native_string_predicate: helper_address!(
            jit_native_string_predicate_abi,
            jit_abi::jit_native_string_predicate_diagnostic_abi
        ),
        native_runtime_fatal: helper_address!(
            jit_native_runtime_fatal_abi,
            jit_abi::jit_native_runtime_fatal_diagnostic_abi
        ),
        native_execution_poll: helper_address!(
            jit_native_execution_poll_abi,
            jit_abi::jit_native_execution_poll_diagnostic_abi
        ),
    }
}

fn resolve_native_cache_helper(stable_id: u32, diagnostic: bool) -> Option<usize> {
    php_jit::resolve_helper_address(
        php_runtime::api::JitHelperId(stable_id),
        runtime_helper_addresses(diagnostic),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use php_ir::builder::IrBuilder;
    use php_ir::{
        BinaryOp, ClassEntry, ClassFlags, ClassId, ClassMethodEntry, ClassMethodFlags,
        FunctionFlags, InstructionKind, IrConstant, IrParam, IrReturnType, IrSpan, Operand, UnitId,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn production_and_diagnostic_helpers_use_distinct_tables() {
        let production = runtime_helper_addresses(false);
        let diagnostic = runtime_helper_addresses(true);

        assert_ne!(production.native_array_fetch, diagnostic.native_array_fetch);
        assert_ne!(
            production.native_value_release,
            diagnostic.native_value_release
        );
        assert_ne!(
            production.native_foreach_next,
            diagnostic.native_foreach_next
        );
        assert_ne!(
            production.native_call_dispatch,
            diagnostic.native_call_dispatch
        );
        assert_ne!(
            production.native_builtin_dispatch,
            diagnostic.native_builtin_dispatch
        );
        assert_ne!(
            production.native_semantic_dispatch,
            diagnostic.native_semantic_dispatch
        );

        let array_fetch = php_jit::lookup_helper_by_name("phrust_native_array_fetch")
            .expect("array-fetch helper is registered")
            .id
            .0;
        assert_eq!(
            resolve_native_cache_helper(array_fetch, false),
            Some(production.native_array_fetch)
        );
        assert_eq!(
            resolve_native_cache_helper(array_fetch, true),
            Some(diagnostic.native_array_fetch)
        );
    }

    fn returning_unit(value: i64) -> CompiledUnit {
        let mut builder = IrBuilder::new(UnitId::new(991));
        let file = builder.add_file("native-cache-vm.php");
        let span = IrSpan::new(file, 0, 20);
        let constant = builder.intern_constant(IrConstant::Int(value));
        let function = builder.start_function("main", FunctionFlags::default(), span);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let block = builder.append_block(function);
        let register = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadConst {
                dst: register,
                constant,
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(register)), span);
        builder.set_entry(function);
        CompiledUnit::new(builder.finish())
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn server_worker_publishes_optimized_entry_after_hot_baseline_threshold() {
        let mut tiering = crate::tiering::TieringOptions::default();
        tiering.collect_stats = true;
        tiering.function_entry_threshold = 2;
        tiering.native_max_functions = 1;
        let worker = VmWorkerState::new_with_background_tiering(tiering.clone());
        let options = VmOptions {
            native_optimization: NativeOptimizationPolicy::Optimizing,
            native_cache: php_jit::NativeCacheMode::Off,
            tiering,
            collect_counters: true,
            ..VmOptions::default()
        };
        let unit = returning_unit(7_301);

        let first = Vm::with_options_and_worker_state(options.clone(), worker.clone())
            .execute(unit.clone());
        assert_eq!(first.return_value, Some(Value::Int(7_301)), "{first:#?}");
        let function = unit.unit().entry;
        let metadata = &unit.unit().functions[function.index()];
        let function_key = php_jit::native_function_key(
            unit.prepared_ir_fingerprint().to_owned(),
            function.raw(),
            metadata.params.len(),
            metadata.local_count,
            true,
            0,
        );
        let (baseline_cell, _) = php_jit::global_code_manager()
            .unwrap()
            .published_function(&function_key)
            .unwrap_or_else(|| {
                panic!("tiered baseline publication missing for {function_key:?}: {first:#?}")
            });
        let baseline_address = baseline_cell
            .resolve(function_key.signature_hash, 0)
            .expect("tiered baseline address");

        let second = Vm::with_options_and_worker_state(options.clone(), worker.clone())
            .execute(unit.clone());
        assert_eq!(second.return_value, Some(Value::Int(7_301)), "{second:#?}");

        let deadline = Instant::now() + Duration::from_secs(10);
        while worker.tiering_stats().native_compiled_functions == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let published = worker.tiering_stats();
        assert_eq!(published.baseline_entries, 2);
        assert_eq!(published.optimized_candidates, 1);
        assert_eq!(published.native_compiled_functions, 1);
        let (optimized_cell, _) = php_jit::global_code_manager()
            .unwrap()
            .published_function(&function_key)
            .expect("optimized publication");
        assert!(Arc::ptr_eq(&baseline_cell, &optimized_cell));
        let optimized_address = optimized_cell
            .resolve(function_key.signature_hash, 0)
            .expect("optimized address");
        assert_ne!(
            Some(optimized_address),
            Some(baseline_address),
            "optimized code must atomically replace the less-specialized target"
        );
        assert_eq!(
            unit.prepared_deployment_image().native_function_entries[function.index()]
                .load(std::sync::atomic::Ordering::Acquire),
            baseline_address,
            "nested compiled calls must retain a side-exit-free baseline target"
        );
        assert_eq!(
            unit.prepared_deployment_image().optimizing_function_entries[function.index()]
                .load(std::sync::atomic::Ordering::Acquire),
            optimized_address,
            "optimizing callers must observe the independently published optimizing target"
        );
    }

    fn declaration_heavy_unit() -> CompiledUnit {
        let mut builder = IrBuilder::new(UnitId::new(9_901));
        let file = builder.add_file("function-on-demand-breadth.php");
        let span = IrSpan::new(file, 0, 32);
        let constant = builder.intern_constant(IrConstant::Int(17));
        for index in 0..121 {
            let function = builder.start_function(
                format!("breadth_function_{index}"),
                FunctionFlags::default(),
                span,
            );
            builder.set_return_type(function, Some(IrReturnType::Int));
            let block = builder.append_block(function);
            let value = builder.alloc_register(function);
            builder.emit(
                function,
                block,
                InstructionKind::LoadConst {
                    dst: value,
                    constant,
                },
                span,
            );
            builder.terminate_return(function, block, Some(Operand::Register(value)), span);
            if index == 0 {
                builder.set_entry(function);
            }
        }
        CompiledUnit::new(builder.finish())
    }

    fn looping_unit() -> CompiledUnit {
        let mut builder = IrBuilder::new(UnitId::new(992));
        let file = builder.add_file("native-deadline-vm.php");
        let span = IrSpan::new(file, 0, 24);
        let function = builder.start_function("main", FunctionFlags::default(), span);
        let block = builder.append_block(function);
        builder.terminate_jump(function, block, block, span);
        builder.set_entry(function);
        CompiledUnit::new(builder.finish())
    }

    fn direct_call_unit() -> CompiledUnit {
        direct_call_unit_with_identity(993, "native-direct-counter.php")
    }

    fn direct_call_unit_with_identity(unit_id: u32, source: &str) -> CompiledUnit {
        let mut builder = IrBuilder::new(UnitId::new(unit_id));
        let file = builder.add_file(source);
        let span = IrSpan::new(file, 0, 24);
        let constant = builder.intern_constant(IrConstant::Int(42));
        let callee = builder.start_function("callee", FunctionFlags::default(), span);
        builder.set_return_type(callee, Some(IrReturnType::Int));
        let callee_block = builder.append_block(callee);
        let value = builder.alloc_register(callee);
        builder.emit(
            callee,
            callee_block,
            InstructionKind::LoadConst {
                dst: value,
                constant,
            },
            span,
        );
        builder.terminate_return(callee, callee_block, Some(Operand::Register(value)), span);
        builder.register_function_name("callee", callee);

        let entry = builder.start_function("main", FunctionFlags::default(), span);
        builder.set_return_type(entry, Some(IrReturnType::Int));
        let entry_block = builder.append_block(entry);
        let result = builder.alloc_register(entry);
        builder.emit(
            entry,
            entry_block,
            InstructionKind::CallFunction {
                dst: result,
                name: "callee".to_owned(),
                args: Vec::new(),
            },
            span,
        );
        builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
        builder.set_entry(entry);
        CompiledUnit::new(builder.finish())
    }

    fn optimizing_array_to_baseline_mutation_unit() -> (CompiledUnit, php_ir::FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(9_934));
        let file = builder.add_file("native-direct-array-baseline-mutation.php");
        let span = IrSpan::new(file, 0, 32);
        let nine = builder.intern_constant(IrConstant::Int(9));

        let callee = builder.start_function("append_value", FunctionFlags::default(), span);
        builder.set_return_type(callee, Some(IrReturnType::Int));
        let items = builder.intern_local(callee, "items");
        builder.push_required_param(callee, "items", items);
        let callee_block = builder.append_block(callee);
        let appended = builder.alloc_register(callee);
        builder.emit(
            callee,
            callee_block,
            InstructionKind::AppendDim {
                dst: appended,
                local: items,
                dims: Vec::new(),
                value: Operand::Constant(nine),
            },
            span,
        );
        builder.terminate_return(
            callee,
            callee_block,
            Some(Operand::Register(appended)),
            span,
        );
        builder.register_function_name("append_value", callee);

        let entry = builder.start_function("main", FunctionFlags::default(), span);
        builder.set_return_type(entry, Some(IrReturnType::Int));
        let entry_block = builder.append_block(entry);
        let array = builder.alloc_register(entry);
        builder.emit(
            entry,
            entry_block,
            InstructionKind::NewArray { dst: array },
            span,
        );
        let result = builder.alloc_register(entry);
        builder.emit(
            entry,
            entry_block,
            InstructionKind::CallFunction {
                dst: result,
                name: "append_value".to_owned(),
                args: vec![php_ir::instruction::IrCallArg {
                    name: None,
                    value: Operand::Register(array),
                    unpack: false,
                    value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
        builder.set_entry(entry);
        (CompiledUnit::new(builder.finish()), callee)
    }

    fn optimizing_nested_callee_transition_unit() -> (CompiledUnit, php_ir::FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(9_938));
        let file = builder.add_file("native-nested-optimizing-transition.php");
        let span = IrSpan::new(file, 0, 32);
        let negative_nine = builder.intern_constant(IrConstant::Int(-9));
        let one = builder.intern_constant(IrConstant::Int(1));

        let callee = builder.start_function("absolute_value", FunctionFlags::default(), span);
        builder.set_return_type(callee, Some(IrReturnType::Int));
        let callee_block = builder.append_block(callee);
        let absolute = builder.alloc_register(callee);
        builder.emit(
            callee,
            callee_block,
            InstructionKind::CallFunction {
                dst: absolute,
                name: "abs".to_owned(),
                args: vec![php_ir::instruction::IrCallArg {
                    name: None,
                    value: Operand::Constant(negative_nine),
                    unpack: false,
                    value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        builder.terminate_return(
            callee,
            callee_block,
            Some(Operand::Register(absolute)),
            span,
        );
        builder.register_function_name("absolute_value", callee);

        let entry = builder.start_function("main", FunctionFlags::default(), span);
        builder.set_return_type(entry, Some(IrReturnType::Int));
        let entry_block = builder.append_block(entry);
        let called = builder.alloc_register(entry);
        builder.emit(
            entry,
            entry_block,
            InstructionKind::CallFunction {
                dst: called,
                name: "absolute_value".to_owned(),
                args: Vec::new(),
            },
            span,
        );
        let result = builder.alloc_register(entry);
        builder.emit(
            entry,
            entry_block,
            InstructionKind::Binary {
                dst: result,
                op: BinaryOp::Add,
                lhs: Operand::Register(called),
                rhs: Operand::Constant(one),
            },
            span,
        );
        builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
        builder.set_entry(entry);
        (CompiledUnit::new(builder.finish()), callee)
    }

    fn optimizing_nested_constant_key_array_transition_unit() -> (CompiledUnit, php_ir::FunctionId)
    {
        let mut builder = IrBuilder::new(UnitId::new(9_938_1));
        let file = builder.add_file("native-nested-constant-key-array-transition.php");
        let span = IrSpan::new(file, 0, 48);
        let first_key_constant = builder.intern_constant(IrConstant::String("path".to_owned()));
        let second_key_constant =
            builder.intern_constant(IrConstant::String("selector".to_owned()));
        let nested_value = builder.intern_constant(IrConstant::Int(41));
        let null = builder.intern_constant(IrConstant::Null);

        let callee = builder.start_function("build_array", FunctionFlags::default(), span);
        let callee_block = builder.append_block(callee);
        let array = builder.alloc_register(callee);
        builder.emit(
            callee,
            callee_block,
            InstructionKind::NewArray { dst: array },
            span,
        );
        let first_key = builder.alloc_register(callee);
        builder.emit_load_const(callee, callee_block, first_key, first_key_constant, span);
        let nested = builder.alloc_register(callee);
        builder.emit(
            callee,
            callee_block,
            InstructionKind::NewArray { dst: nested },
            span,
        );
        builder.emit(
            callee,
            callee_block,
            InstructionKind::ArrayInsert {
                array: nested,
                key: None,
                value: Operand::Constant(nested_value),
                by_ref_local: None,
            },
            span,
        );
        builder.emit(
            callee,
            callee_block,
            InstructionKind::ArrayInsert {
                array,
                key: Some(Operand::Register(first_key)),
                value: Operand::Register(nested),
                by_ref_local: None,
            },
            span,
        );
        builder.emit(
            callee,
            callee_block,
            InstructionKind::Discard {
                src: Operand::Register(first_key),
            },
            span,
        );
        builder.emit(
            callee,
            callee_block,
            InstructionKind::Discard {
                src: Operand::Register(nested),
            },
            span,
        );
        let second_key = builder.alloc_register(callee);
        builder.emit_load_const(callee, callee_block, second_key, second_key_constant, span);
        builder.emit(
            callee,
            callee_block,
            InstructionKind::ArrayInsert {
                array,
                key: Some(Operand::Register(second_key)),
                value: Operand::Constant(null),
                by_ref_local: None,
            },
            span,
        );
        builder.terminate_return(callee, callee_block, Some(Operand::Register(array)), span);
        builder.register_function_name("build_array", callee);

        let entry = builder.start_function("main", FunctionFlags::default(), span);
        let entry_block = builder.append_block(entry);
        let result = builder.alloc_register(entry);
        builder.emit(
            entry,
            entry_block,
            InstructionKind::CallFunction {
                dst: result,
                name: "build_array".to_owned(),
                args: Vec::new(),
            },
            span,
        );
        builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
        builder.set_entry(entry);
        (CompiledUnit::new(builder.finish()), callee)
    }

    fn optimizing_nested_builtin_constants_unit() -> (CompiledUnit, php_ir::FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(9_939));
        let file = builder.add_file("native-nested-builtin-constants.php");
        let span = IrSpan::new(file, 0, 64);
        let pattern = builder.intern_constant(IrConstant::String("/-[0-9]+$/".to_owned()));
        let replacement = builder.intern_constant(IrConstant::String(String::new()));
        let subject = builder.intern_constant(IrConstant::String("widget-12".to_owned()));
        let suffix = builder.intern_constant(IrConstant::String("!".to_owned()));

        let callee = builder.start_function("strip_widget_id", FunctionFlags::default(), span);
        builder.set_return_type(callee, Some(IrReturnType::String));
        let id = builder.intern_local(callee, "id");
        builder.push_required_param(callee, "id", id);
        let callee_block = builder.append_block(callee);
        let pattern_value = builder.alloc_register(callee);
        builder.emit(
            callee,
            callee_block,
            InstructionKind::LoadConst {
                dst: pattern_value,
                constant: pattern,
            },
            span,
        );
        let replacement_value = builder.alloc_register(callee);
        builder.emit(
            callee,
            callee_block,
            InstructionKind::LoadConst {
                dst: replacement_value,
                constant: replacement,
            },
            span,
        );
        let subject_value = builder.alloc_register(callee);
        builder.emit(
            callee,
            callee_block,
            InstructionKind::LoadLocal {
                dst: subject_value,
                local: id,
            },
            span,
        );
        let value = builder.alloc_register(callee);
        builder.emit(
            callee,
            callee_block,
            InstructionKind::CallFunction {
                dst: value,
                name: "preg_replace".to_owned(),
                args: [
                    Operand::Register(pattern_value),
                    Operand::Register(replacement_value),
                    Operand::Register(subject_value),
                ]
                .into_iter()
                .map(|value| php_ir::instruction::IrCallArg {
                    name: None,
                    value,
                    unpack: false,
                    value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                })
                .collect(),
            },
            span,
        );
        builder.terminate_return(callee, callee_block, Some(Operand::Register(value)), span);
        builder.register_function_name("strip_widget_id", callee);

        let entry = builder.start_function("main", FunctionFlags::default(), span);
        builder.set_return_type(entry, Some(IrReturnType::String));
        let entry_block = builder.append_block(entry);
        let called = builder.alloc_register(entry);
        builder.emit(
            entry,
            entry_block,
            InstructionKind::CallFunction {
                dst: called,
                name: "strip_widget_id".to_owned(),
                args: vec![php_ir::instruction::IrCallArg {
                    name: None,
                    value: Operand::Constant(subject),
                    unpack: false,
                    value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        let result = builder.alloc_register(entry);
        builder.emit(
            entry,
            entry_block,
            InstructionKind::Binary {
                dst: result,
                op: BinaryOp::Concat,
                lhs: Operand::Register(called),
                rhs: Operand::Constant(suffix),
            },
            span,
        );
        builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
        builder.set_entry(entry);
        (CompiledUnit::new(builder.finish()), callee)
    }

    fn direct_method_on_demand_unit() -> CompiledUnit {
        let mut builder = IrBuilder::new(UnitId::new(9_931));
        let file = builder.add_file("native-direct-method.php");
        let span = IrSpan::new(file, 0, 32);
        let constant = builder.intern_constant(IrConstant::Int(42));
        let method = builder.start_function(
            "Widget::value",
            FunctionFlags {
                is_method: true,
                ..FunctionFlags::default()
            },
            span,
        );
        builder.intern_local(method, "this");
        builder.set_return_type(method, Some(IrReturnType::Int));
        let method_block = builder.append_block(method);
        let value = builder.alloc_register(method);
        builder.emit(
            method,
            method_block,
            InstructionKind::LoadConst {
                dst: value,
                constant,
            },
            span,
        );
        builder.terminate_return(method, method_block, Some(Operand::Register(value)), span);
        builder.push_class(ClassEntry {
            id: ClassId::new(0),
            name: "widget".to_owned(),
            display_name: "Widget".to_owned(),
            parent: None,
            parent_display_name: None,
            interfaces: Vec::new(),
            methods: vec![ClassMethodEntry {
                name: "value".to_owned(),
                origin_class: "widget".to_owned(),
                function: method,
                flags: ClassMethodFlags {
                    has_body: true,
                    ..ClassMethodFlags::default()
                },
                attributes: Vec::new(),
            }],
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor: None,
            flags: ClassFlags::default(),
            span,
        });

        let entry = builder.start_function("main", FunctionFlags::default(), span);
        builder.set_return_type(entry, Some(IrReturnType::Int));
        let entry_block = builder.append_block(entry);
        let object = builder.alloc_register(entry);
        builder.emit(
            entry,
            entry_block,
            InstructionKind::NewObject {
                dst: object,
                display_class_name: "Widget".to_owned(),
                class_name: "widget".to_owned(),
                args: Vec::new(),
            },
            span,
        );
        let result = builder.alloc_register(entry);
        builder.emit(
            entry,
            entry_block,
            InstructionKind::CallMethod {
                dst: result,
                object: Operand::Register(object),
                method: "value".to_owned(),
                args: Vec::new(),
            },
            span,
        );
        builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
        builder.set_entry(entry);
        CompiledUnit::new(builder.finish())
    }

    fn typed_direct_call_unit(strict_types: bool) -> CompiledUnit {
        let mut builder = IrBuilder::new(UnitId::new(998));
        let file = builder.add_file("native-typed-direct.php");
        builder.set_file_strict_types(file, strict_types);
        builder.set_strict_types(strict_types);
        let span = IrSpan::new(file, 0, 32);
        let callee = builder.start_function("typed_callee", FunctionFlags::default(), span);
        builder.set_return_type(callee, Some(IrReturnType::Int));
        let parameter = builder.intern_local(callee, "value");
        builder.push_param(
            callee,
            IrParam {
                name: "value".to_owned(),
                local: parameter,
                required: true,
                default: None,
                type_: Some(IrReturnType::Int),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            },
        );
        let callee_block = builder.append_block(callee);
        let value = builder.alloc_register(callee);
        builder.emit(
            callee,
            callee_block,
            InstructionKind::LoadLocal {
                dst: value,
                local: parameter,
            },
            span,
        );
        builder.terminate_return(callee, callee_block, Some(Operand::Register(value)), span);
        builder.register_function_name("typed_callee", callee);

        let argument = builder.intern_constant(IrConstant::String("42".to_owned()));
        let entry = builder.start_function("main", FunctionFlags::default(), span);
        builder.set_return_type(entry, Some(IrReturnType::Int));
        let entry_block = builder.append_block(entry);
        let result = builder.alloc_register(entry);
        builder.emit(
            entry,
            entry_block,
            InstructionKind::CallFunction {
                dst: result,
                name: "typed_callee".to_owned(),
                args: vec![php_ir::instruction::IrCallArg {
                    name: None,
                    value: Operand::Constant(argument),
                    unpack: false,
                    value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
        builder.set_entry(entry);
        CompiledUnit::new(builder.finish())
    }

    fn invalid_return_type_on_demand_unit() -> CompiledUnit {
        let mut builder = IrBuilder::new(UnitId::new(9_999));
        let file = builder.add_file("native-return-type-on-demand.php");
        let span = IrSpan::new(file, 0, 48);
        let invalid = builder.intern_constant(IrConstant::Array(Vec::new()));
        let callee = builder.start_function("invalid_return", FunctionFlags::default(), span);
        builder.set_return_type(callee, Some(IrReturnType::String));
        let callee_block = builder.append_block(callee);
        let value = builder.alloc_register(callee);
        builder.emit(
            callee,
            callee_block,
            InstructionKind::LoadConst {
                dst: value,
                constant: invalid,
            },
            span,
        );
        builder.terminate_return(callee, callee_block, Some(Operand::Register(value)), span);
        builder.register_function_name("invalid_return", callee);

        let entry = builder.start_function("main", FunctionFlags::default(), span);
        let entry_block = builder.append_block(entry);
        let result = builder.alloc_register(entry);
        builder.emit(
            entry,
            entry_block,
            InstructionKind::CallFunction {
                dst: result,
                name: "invalid_return".to_owned(),
                args: Vec::new(),
            },
            span,
        );
        builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
        builder.set_entry(entry);
        CompiledUnit::new(builder.finish())
    }

    fn direct_builtin_unit() -> CompiledUnit {
        let mut builder = IrBuilder::new(UnitId::new(994));
        let file = builder.add_file("native-direct-builtin.php");
        let span = IrSpan::new(file, 0, 32);
        let value = builder.intern_constant(IrConstant::Int(-6));
        let function = builder.start_function("main", FunctionFlags::default(), span);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let block = builder.append_block(function);
        let result = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::CallFunction {
                dst: result,
                name: "abs".to_owned(),
                args: vec![php_ir::instruction::IrCallArg {
                    name: None,
                    value: Operand::Constant(value),
                    unpack: false,
                    value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(result)), span);
        builder.set_entry(function);
        CompiledUnit::new(builder.finish())
    }

    fn direct_type_predicate_unit() -> CompiledUnit {
        let mut builder = IrBuilder::new(UnitId::new(993));
        let file = builder.add_file("native-direct-type-predicate.php");
        let span = IrSpan::new(file, 0, 32);
        let string = builder.intern_constant(IrConstant::String("phrust".to_owned()));
        let function = builder.start_function("main", FunctionFlags::default(), span);
        builder.set_return_type(function, Some(IrReturnType::Bool));
        let block = builder.append_block(function);
        let result = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::CallFunction {
                dst: result,
                name: "is_string".to_owned(),
                args: vec![php_ir::instruction::IrCallArg {
                    name: None,
                    value: Operand::Constant(string),
                    unpack: false,
                    value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(result)), span);
        builder.set_entry(function);
        CompiledUnit::new(builder.finish())
    }

    fn bounded_inline_unit() -> CompiledUnit {
        let mut builder = IrBuilder::new(UnitId::new(997));
        let file = builder.add_file("native-inline-constant.php");
        let span = IrSpan::new(file, 0, 32);
        let constant = builder.intern_constant(IrConstant::Int(19));
        let callee = builder.start_function("constant_wrapper", FunctionFlags::default(), span);
        let callee_block = builder.append_block(callee);
        let value = builder.alloc_register(callee);
        builder.emit(
            callee,
            callee_block,
            InstructionKind::LoadConst {
                dst: value,
                constant,
            },
            span,
        );
        builder.terminate_return(callee, callee_block, Some(Operand::Register(value)), span);
        builder.register_function_name("constant_wrapper", callee);

        let main = builder.start_function("main", FunctionFlags::default(), span);
        let main_block = builder.append_block(main);
        let result = builder.alloc_register(main);
        builder.emit(
            main,
            main_block,
            InstructionKind::CallFunction {
                dst: result,
                name: "constant_wrapper".to_owned(),
                args: Vec::new(),
            },
            span,
        );
        builder.terminate_return(main, main_block, Some(Operand::Register(result)), span);
        builder.set_entry(main);
        CompiledUnit::new(builder.finish())
    }

    fn unbounded_recursive_unit() -> CompiledUnit {
        let mut builder = IrBuilder::new(UnitId::new(995));
        let file = builder.add_file("native-frame-depth.php");
        let span = IrSpan::new(file, 0, 32);
        let function = builder.start_function("recurse", FunctionFlags::default(), span);
        builder.register_function_name("recurse", function);
        let block = builder.append_block(function);
        let result = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::CallFunction {
                dst: result,
                name: "recurse".to_owned(),
                args: Vec::new(),
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(result)), span);
        builder.set_entry(function);
        CompiledUnit::new(builder.finish())
    }

    fn polymorphic_method_pic_unit() -> CompiledUnit {
        let mut builder = IrBuilder::new(UnitId::new(996));
        let file = builder.add_file("native-method-pic.php");
        let span = IrSpan::new(file, 0, 64);
        let seven = builder.intern_constant(IrConstant::Int(7));

        let method = builder.start_function(
            "Widget::value",
            FunctionFlags {
                is_method: true,
                ..FunctionFlags::default()
            },
            span,
        );
        builder.intern_local(method, "this");
        builder.set_return_type(method, Some(IrReturnType::Int));
        let method_block = builder.append_block(method);
        let method_value = builder.alloc_register(method);
        builder.emit(
            method,
            method_block,
            InstructionKind::LoadConst {
                dst: method_value,
                constant: seven,
            },
            span,
        );
        builder.terminate_return(
            method,
            method_block,
            Some(Operand::Register(method_value)),
            span,
        );

        let factory = builder.start_function("make_widget", FunctionFlags::default(), span);
        let factory_block = builder.append_block(factory);
        let object = builder.alloc_register(factory);
        builder.emit(
            factory,
            factory_block,
            InstructionKind::NewObject {
                dst: object,
                display_class_name: "Widget".to_owned(),
                class_name: "widget".to_owned(),
                args: Vec::new(),
            },
            span,
        );
        builder.terminate_return(
            factory,
            factory_block,
            Some(Operand::Register(object)),
            span,
        );
        builder.register_function_name("make_widget", factory);

        let call_value = builder.start_function("call_value", FunctionFlags::default(), span);
        builder.set_return_type(call_value, Some(IrReturnType::Int));
        let receiver_local = builder.intern_local(call_value, "receiver");
        builder.push_required_param(call_value, "receiver", receiver_local);
        let call_value_block = builder.append_block(call_value);
        let receiver_value = builder.alloc_register(call_value);
        builder.emit(
            call_value,
            call_value_block,
            InstructionKind::LoadLocal {
                dst: receiver_value,
                local: receiver_local,
            },
            span,
        );
        let call_value_result = builder.alloc_register(call_value);
        builder.emit(
            call_value,
            call_value_block,
            InstructionKind::CallMethod {
                dst: call_value_result,
                object: Operand::Register(receiver_value),
                method: "value".to_owned(),
                args: Vec::new(),
            },
            span,
        );
        builder.terminate_return(
            call_value,
            call_value_block,
            Some(Operand::Register(call_value_result)),
            span,
        );
        builder.register_function_name("call_value", call_value);

        let main = builder.start_function("main", FunctionFlags::default(), span);
        builder.set_return_type(main, Some(IrReturnType::Int));
        let main_block = builder.append_block(main);
        let receiver = builder.alloc_register(main);
        builder.emit(
            main,
            main_block,
            InstructionKind::CallFunction {
                dst: receiver,
                name: "make_widget".to_owned(),
                args: Vec::new(),
            },
            span,
        );
        let first = builder.alloc_register(main);
        builder.emit(
            main,
            main_block,
            InstructionKind::CallFunction {
                dst: first,
                name: "call_value".to_owned(),
                args: vec![php_ir::instruction::IrCallArg {
                    name: None,
                    value: Operand::Register(receiver),
                    unpack: false,
                    value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        let second = builder.alloc_register(main);
        builder.emit(
            main,
            main_block,
            InstructionKind::CallFunction {
                dst: second,
                name: "call_value".to_owned(),
                args: vec![php_ir::instruction::IrCallArg {
                    name: None,
                    value: Operand::Register(receiver),
                    unpack: false,
                    value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        builder.terminate_return(main, main_block, Some(Operand::Register(second)), span);
        builder.push_class(ClassEntry {
            id: ClassId::new(0),
            name: "widget".to_owned(),
            display_name: "Widget".to_owned(),
            parent: None,
            parent_display_name: None,
            interfaces: Vec::new(),
            methods: vec![ClassMethodEntry {
                name: "value".to_owned(),
                origin_class: "widget".to_owned(),
                function: method,
                flags: ClassMethodFlags {
                    has_body: true,
                    ..ClassMethodFlags::default()
                },
                attributes: Vec::new(),
            }],
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor: None,
            flags: ClassFlags::default(),
            span,
        });
        builder.set_entry(main);
        CompiledUnit::new(builder.finish())
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn same_unit_call_resolves_on_demand_then_calls_native() {
        let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
        let result = Vm::with_options_and_worker_state(
            VmOptions {
                collect_counters: true,
                ..VmOptions::default()
            },
            worker.clone(),
        )
        .execute(direct_call_unit());

        assert_eq!(result.return_value, Some(Value::Int(42)), "{result:#?}");
        let counters = result.counters.expect("diagnostic counters");
        assert_eq!(counters.native_call_direct, 1);
        assert_eq!(counters.native_same_unit_direct_executed, 1);
        assert_eq!(counters.native_call_dynamic, 0);
        assert_eq!(counters.native_transition_count, 0);
        assert_eq!(counters.native_tail_calls, 0);
        assert!(counters.native_frame_arena_high_water_bytes > 0);
        let compile_stats = worker.native_compile_cache_stats();
        assert_eq!(compile_stats.entries, 2);
        assert_eq!(compile_stats.misses, 2);
        assert_eq!(compile_stats.insertions, 2);
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn tiered_baseline_call_miss_cannot_publish_an_optimizing_callee() {
        let unit = direct_call_unit_with_identity(9_935, "native-tiered-baseline-firewall.php");
        let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
        let result = Vm::with_options_and_worker_state(
            VmOptions {
                native_optimization: NativeOptimizationPolicy::TieredBaseline,
                native_cache: php_jit::NativeCacheMode::Off,
                collect_counters: true,
                ..VmOptions::default()
            },
            worker,
        )
        .execute(unit.clone());

        assert_eq!(result.return_value, Some(Value::Int(42)), "{result:#?}");
        let callee = unit
            .unit()
            .function_table
            .iter()
            .find_map(|entry| (entry.name == "callee").then_some(entry.function))
            .expect("callee function id");
        let metadata = &unit.unit().functions[callee.index()];
        let key = |optimizing| {
            php_jit::native_function_key(
                unit.prepared_ir_fingerprint().to_owned(),
                callee.raw(),
                metadata.params.len(),
                metadata.local_count,
                optimizing,
                0,
            )
        };
        let manager = php_jit::global_code_manager().expect("global code manager");
        assert!(
            manager.published_function_exact(&key(false)).is_some(),
            "baseline resolver must publish the baseline callee"
        );
        assert!(
            manager.published_function_exact(&key(true)).is_none(),
            "baseline resolver must never compile or publish an optimizing callee"
        );
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn optimizing_call_miss_keeps_nested_warm_cell_on_baseline_entry() {
        let unit = direct_call_unit_with_identity(9_936, "native-on-demand-optimizer-cell.php");
        let mut tiering = crate::tiering::TieringOptions::default();
        tiering.collect_stats = true;
        let worker = VmWorkerState::new(tiering.clone());
        let result = Vm::with_options_and_worker_state(
            VmOptions {
                native_optimization: NativeOptimizationPolicy::Optimizing,
                native_cache: php_jit::NativeCacheMode::Off,
                collect_counters: true,
                tiering,
                ..VmOptions::default()
            },
            worker.clone(),
        )
        .execute(unit.clone());
        assert_eq!(result.return_value, Some(Value::Int(42)), "{result:#?}");

        let callee = unit
            .unit()
            .function_table
            .iter()
            .find_map(|entry| (entry.name == "callee").then_some(entry.function))
            .expect("callee function id");
        let metadata = &unit.unit().functions[callee.index()];
        let optimizing_key = php_jit::native_function_key(
            unit.prepared_ir_fingerprint().to_owned(),
            callee.raw(),
            metadata.params.len(),
            metadata.local_count,
            true,
            0,
        );
        let manager = php_jit::global_code_manager().expect("global code manager");
        let deadline = Instant::now() + Duration::from_secs(10);
        let optimizing_address = loop {
            if let Some((cell, handle)) = manager.published_function_exact(&optimizing_key)
                && handle.region_state_metadata().is_some_and(|metadata| {
                    metadata.compiler_tier == php_jit::region_ir::NativeCompilerTier::Optimizing
                })
                && let Some(address) = cell.resolve(optimizing_key.signature_hash, 0)
            {
                break address;
            }
            assert!(
                Instant::now() < deadline,
                "on-demand callee optimization was not published"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        let baseline_key = php_jit::native_function_key(
            unit.prepared_ir_fingerprint().to_owned(),
            callee.raw(),
            metadata.params.len(),
            metadata.local_count,
            false,
            0,
        );
        let baseline_address = manager
            .published_function_exact(&baseline_key)
            .and_then(|(_, handle)| handle.native_entry_address())
            .expect("on-demand baseline callee publication");
        let nested_address = unit.prepared_deployment_image().native_function_entries
            [callee.index()]
        .load(std::sync::atomic::Ordering::Acquire);
        assert_eq!(nested_address, baseline_address);
        assert_ne!(nested_address, optimizing_address);
        assert_eq!(
            unit.prepared_deployment_image().optimizing_function_entries[callee.index()]
                .load(std::sync::atomic::Ordering::Acquire),
            optimizing_address
        );
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn optimizing_direct_array_can_cross_into_baseline_array_mutation() {
        let (unit, callee) = optimizing_array_to_baseline_mutation_unit();
        let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
        let baseline = VmOptions {
            native_optimization: NativeOptimizationPolicy::Baseline,
            native_cache: php_jit::NativeCacheMode::Off,
            collect_counters: true,
            ..VmOptions::default()
        };
        worker
            .resolve_native_function(&unit, callee, &baseline, &[])
            .expect("baseline mutation callee must be published before optimizer execution");

        let result = Vm::with_options_and_worker_state(
            VmOptions {
                native_optimization: NativeOptimizationPolicy::Optimizing,
                native_cache: php_jit::NativeCacheMode::Off,
                collect_counters: true,
                ..VmOptions::default()
            },
            worker,
        )
        .execute(unit);

        assert_eq!(result.return_value, Some(Value::Int(9)), "{result:#?}");
        let counters = result.counters.expect("diagnostic counters");
        assert_eq!(counters.native_call_dynamic, 0);
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn compiled_caller_resumes_rejected_optimizing_callee_and_continues() {
        let (unit, callee) = optimizing_nested_callee_transition_unit();
        let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
        let baseline = VmOptions {
            native_optimization: NativeOptimizationPolicy::Baseline,
            native_cache: php_jit::NativeCacheMode::Off,
            collect_counters: true,
            ..VmOptions::default()
        };
        let baseline_handle = worker
            .resolve_native_function(&unit, callee, &baseline, &[])
            .expect("baseline callee");
        let optimizing = VmOptions {
            native_optimization: NativeOptimizationPolicy::Optimizing,
            native_cache: php_jit::NativeCacheMode::Off,
            collect_counters: true,
            ..VmOptions::default()
        };
        let optimizing_handle = worker
            .resolve_native_function(&unit, callee, &optimizing, &[])
            .expect("optimizing callee");
        unit.prepared_deployment_image().native_function_entries[callee.index()].store(
            baseline_handle
                .native_entry_address()
                .expect("baseline address"),
            std::sync::atomic::Ordering::Release,
        );
        unit.prepared_deployment_image().optimizing_function_entries[callee.index()].store(
            optimizing_handle
                .native_entry_address()
                .expect("optimizing address"),
            std::sync::atomic::Ordering::Release,
        );

        let result = Vm::with_options_and_worker_state(baseline, worker).execute(unit);
        assert_eq!(result.return_value, Some(Value::Int(10)), "{result:#?}");
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn compiled_caller_preserves_array_across_constant_key_callee_transition() {
        let (unit, callee) = optimizing_nested_constant_key_array_transition_unit();
        let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
        let baseline = VmOptions {
            native_optimization: NativeOptimizationPolicy::Baseline,
            native_cache: php_jit::NativeCacheMode::Off,
            ..VmOptions::default()
        };
        let baseline_handle = worker
            .resolve_native_function(&unit, callee, &baseline, &[])
            .expect("baseline callee");
        let optimizing = VmOptions {
            native_optimization: NativeOptimizationPolicy::Optimizing,
            native_cache: php_jit::NativeCacheMode::Off,
            ..VmOptions::default()
        };
        let optimizing_handle = worker
            .resolve_native_function(&unit, callee, &optimizing, &[])
            .expect("optimizing callee");
        unit.prepared_deployment_image().native_function_entries[callee.index()].store(
            baseline_handle
                .native_entry_address()
                .expect("baseline address"),
            std::sync::atomic::Ordering::Release,
        );
        unit.prepared_deployment_image().optimizing_function_entries[callee.index()].store(
            optimizing_handle
                .native_entry_address()
                .expect("optimizing address"),
            std::sync::atomic::Ordering::Release,
        );

        let result = Vm::with_options_and_worker_state(optimizing, worker).execute(unit);
        let Some(Value::Array(array)) = result.return_value else {
            panic!("nested transition did not return an array: {result:#?}");
        };
        assert!(
            array
                .get(&php_runtime::api::ArrayKey::String("path".into()))
                .is_some(),
            "first constant-key insert was lost"
        );
        assert!(
            array
                .get(&php_runtime::api::ArrayKey::String("selector".into()))
                .is_some(),
            "second constant-key insert was lost"
        );
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn compiled_caller_preserves_builtin_constants_across_callee_transition() {
        let (unit, callee) = optimizing_nested_builtin_constants_unit();
        let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
        let baseline = VmOptions {
            native_optimization: NativeOptimizationPolicy::Baseline,
            native_cache: php_jit::NativeCacheMode::Off,
            collect_counters: true,
            ..VmOptions::default()
        };
        let baseline_handle = worker
            .resolve_native_function(&unit, callee, &baseline, &[])
            .expect("baseline callee");
        let optimizing = VmOptions {
            native_optimization: NativeOptimizationPolicy::Optimizing,
            native_cache: php_jit::NativeCacheMode::Off,
            collect_counters: true,
            ..VmOptions::default()
        };
        let optimizing_handle = worker
            .resolve_native_function(&unit, callee, &optimizing, &[])
            .expect("optimizing callee");
        unit.prepared_deployment_image().native_function_entries[callee.index()].store(
            baseline_handle
                .native_entry_address()
                .expect("baseline address"),
            std::sync::atomic::Ordering::Release,
        );
        unit.prepared_deployment_image().optimizing_function_entries[callee.index()].store(
            optimizing_handle
                .native_entry_address()
                .expect("optimizing address"),
            std::sync::atomic::Ordering::Release,
        );

        let result = Vm::with_options_and_worker_state(baseline, worker).execute(unit);
        assert_eq!(
            result.return_value,
            Some(Value::String(php_runtime::api::PhpString::from_bytes(
                b"widget!".to_vec()
            ))),
            "{result:#?}"
        );
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn instance_method_resolver_uses_exact_packed_entry_arity() {
        let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
        let result = Vm::with_options_and_worker_state(
            VmOptions {
                collect_counters: true,
                ..VmOptions::default()
            },
            worker.clone(),
        )
        .execute(direct_method_on_demand_unit());

        assert_eq!(result.return_value, Some(Value::Int(42)), "{result:#?}");
        let counters = result.counters.expect("diagnostic counters");
        assert_eq!(counters.native_call_direct, 1);
        assert_eq!(counters.native_same_unit_direct_executed, 1);
        assert_eq!(counters.native_call_dynamic, 0);
        assert_eq!(counters.native_transition_count, 0);
        let compile_stats = worker.native_compile_cache_stats();
        assert_eq!(compile_stats.entries, 2);
        assert_eq!(compile_stats.misses, 2);
        assert_eq!(compile_stats.insertions, 2);
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn typed_function_on_demand_call_preserves_coercion() {
        let result = Vm::with_options(VmOptions {
            collect_counters: true,
            ..VmOptions::default()
        })
        .execute(typed_direct_call_unit(false));

        assert_eq!(result.return_value, Some(Value::Int(42)), "{result:#?}");
        let counters = result.counters.expect("diagnostic counters");
        assert_eq!(counters.native_call_direct, 1);
        assert_eq!(counters.native_same_unit_direct_executed, 1);
        assert_eq!(counters.native_call_dynamic, 0);
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn typed_function_on_demand_call_preserves_throw() {
        let result = Vm::with_options(VmOptions {
            collect_counters: true,
            ..VmOptions::default()
        })
        .execute(typed_direct_call_unit(true));

        assert_eq!(
            result.status.exit_status(),
            php_runtime::api::ExitStatus::Fatal,
            "{result:#?}"
        );
        assert!(
            String::from_utf8_lossy(result.output.as_bytes()).contains(
                "Uncaught TypeError: typed_callee(): Argument #1 ($value) must be of type int, string given"
            ),
            "{result:#?}"
        );
        let counters = result.counters.expect("diagnostic counters");
        assert_eq!(counters.native_call_direct, 1);
        assert_eq!(counters.native_same_unit_direct_executed, 1);
        assert_eq!(counters.native_call_dynamic, 0);
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn function_on_demand_call_preserves_runtime_diagnostic() {
        let result = Vm::new().execute(invalid_return_type_on_demand_unit());

        assert_eq!(
            result.status.exit_status(),
            php_runtime::api::ExitStatus::RuntimeError,
            "{result:#?}"
        );
        assert_eq!(
            result.diagnostics.first().map(|diagnostic| diagnostic.id()),
            Some("E_PHP_VM_RETURN_TYPE_MISMATCH"),
            "{result:#?}"
        );
        assert!(
            result.status.message().is_some_and(|message| message
                .contains("invalid_return(): Return value must be of type string, array returned")),
            "{result:#?}"
        );
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn stable_builtin_uses_helper_id_without_generic_dynamic_count() {
        let result = Vm::with_options(VmOptions {
            collect_counters: true,
            ..VmOptions::default()
        })
        .execute(direct_builtin_unit());

        assert_eq!(result.return_value, Some(Value::Int(6)), "{result:#?}");
        let counters = result.counters.expect("diagnostic counters");
        assert_eq!(counters.native_call_direct, 1);
        assert_eq!(counters.native_builtin_direct_eligible, 1);
        assert_eq!(counters.native_builtin_direct_executed, 1);
        assert_eq!(counters.native_call_dynamic, 0);
        assert_eq!(counters.native_call_argument_allocation_bytes, 0);
        assert_eq!(
            counters.native_call_frame_bytes,
            std::mem::size_of::<i64>() as u64
        );
        assert_eq!(counters.native_frame_arena_high_water_bytes, 0);
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn type_predicate_bypasses_the_generic_call_frame() {
        let result = Vm::with_options(VmOptions {
            collect_counters: true,
            ..VmOptions::default()
        })
        .execute(direct_type_predicate_unit());

        assert_eq!(result.return_value, Some(Value::Bool(true)), "{result:#?}");
        let counters = result.counters.expect("diagnostic counters");
        assert_eq!(counters.native_call_direct, 0);
        assert_eq!(counters.native_builtin_direct_executed, 0);
        assert_eq!(counters.native_call_argument_allocation_bytes, 0);
        assert_eq!(counters.native_frame_arena_high_water_bytes, 0);
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn baseline_does_not_inline_or_widen_for_constant_wrapper() {
        let result = Vm::with_options(VmOptions {
            collect_counters: true,
            ..VmOptions::default()
        })
        .execute(bounded_inline_unit());

        assert_eq!(result.return_value, Some(Value::Int(19)), "{result:#?}");
        let counters = result.counters.expect("diagnostic counters");
        assert_eq!(counters.native_inlined_calls, 0);
        assert_eq!(counters.native_inline_calls_removed, 0);
        assert_eq!(counters.native_call_direct, 1);
        assert_eq!(counters.native_call_dynamic, 0);
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn warmed_method_pic_reclassifies_stable_call_as_direct() {
        let vm = Vm::with_options(VmOptions {
            collect_counters: true,
            ..VmOptions::default()
        });
        let unit = polymorphic_method_pic_unit();
        let result = vm.execute(unit.clone());

        assert_eq!(result.return_value, Some(Value::Int(7)), "{result:#?}");
        let counters = result.counters.expect("diagnostic counters");
        assert!(counters.native_method_monomorphic_eligible >= 2);
        assert!(counters.native_method_monomorphic_executed >= 1);
        assert!(counters.native_call_direct >= 2);

        // The descriptor is compiled-unit metadata shared across requests.
        // Both calls in the second request should therefore hit the persistent
        // monomorphic entry rather than warming another request-local table.
        let warm = vm.execute(unit);
        assert_eq!(warm.return_value, Some(Value::Int(7)), "{warm:#?}");
        let warm_counters = warm.counters.expect("diagnostic counters");
        assert!(warm_counters.native_method_monomorphic_executed >= 2);
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn reached_method_is_published_to_the_optimizing_entry_table() {
        let mut tiering = crate::tiering::TieringOptions::default();
        tiering.collect_stats = true;
        let worker = VmWorkerState::new(tiering.clone());
        let unit = polymorphic_method_pic_unit();
        let method = unit.unit().classes[0].methods[0].function;
        let result = Vm::with_options_and_worker_state(
            VmOptions {
                native_optimization: NativeOptimizationPolicy::Optimizing,
                native_cache: php_jit::NativeCacheMode::Off,
                collect_counters: true,
                tiering,
                ..VmOptions::default()
            },
            worker,
        )
        .execute(unit.clone());
        assert_eq!(result.return_value, Some(Value::Int(7)), "{result:#?}");

        let deadline = Instant::now() + Duration::from_secs(10);
        while unit.prepared_deployment_image().optimizing_function_entries[method.index()]
            .load(std::sync::atomic::Ordering::Acquire)
            == 0
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_ne!(
            unit.prepared_deployment_image().optimizing_function_entries[method.index()]
                .load(std::sync::atomic::Ordering::Acquire),
            0,
            "a reached method must not remain permanently baseline-only"
        );
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn deep_direct_recursion_hits_php_frame_limit_without_stack_abort() {
        let result = Vm::new().execute(unbounded_recursive_unit());

        assert!(!result.status.is_success(), "{result:#?}");
        assert_eq!(
            result.diagnostics.first().map(|diagnostic| diagnostic.id()),
            Some("E_PHP_VM_NATIVE_FRAME_LIMIT"),
            "{result:#?}"
        );
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn native_compile_probe_uses_production_helpers_without_execution() {
        let report = Vm::new()
            .probe_cranelift(&returning_unit(42), Some("main"))
            .expect("native compile probe");
        assert_eq!(report.function_name, "main");
        assert!(matches!(
            report.result.status,
            php_jit::JitCompileStatus::Compiled
        ));
        assert!(
            Vm::new()
                .probe_cranelift(&returning_unit(42), Some("missing"))
                .expect_err("unknown function must be strict")
                .contains("function not found")
        );
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn native_loop_poll_reports_stable_execution_timeout() {
        let result = Vm::with_options(VmOptions {
            runtime_context: php_runtime::api::RuntimeContext::controlled_cli(
                "native-deadline-vm.php",
                Vec::new(),
            )
            .with_execution_time_limit(Some(Duration::ZERO)),
            ..VmOptions::default()
        })
        .execute(looping_unit());

        assert_eq!(
            result.status.exit_status(),
            php_runtime::api::ExitStatus::RuntimeError
        );
        assert_eq!(result.diagnostics.len(), 1, "{result:#?}");
        assert_eq!(result.diagnostics[0].id(), "E_PHP_VM_EXECUTION_TIMEOUT");
    }

    #[test]
    fn declaration_units_are_native_cache_candidates() {
        let unit = returning_unit(42);
        assert!(native_cache_candidate(unit.unit(), unit.unit().entry));

        let mut declaration_unit = unit.unit().clone();
        declaration_unit.classes.push(ClassEntry {
            id: ClassId::new(0),
            name: "cacheddeclaration".to_owned(),
            display_name: "CachedDeclaration".to_owned(),
            parent: None,
            parent_display_name: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor: None,
            flags: ClassFlags::default(),
            span: IrSpan::new(declaration_unit.files[0].id, 6, 32),
        });
        assert!(native_cache_candidate(
            &declaration_unit,
            declaration_unit.entry
        ));
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn worker_cache_skips_region_rebuild_and_invalidates_exactly() {
        let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
        let unit = returning_unit(73);
        let options = VmOptions::default();

        let first = Vm::with_options_and_worker_state(options.clone(), worker.clone())
            .execute(unit.clone());
        let second = Vm::with_options_and_worker_state(options.clone(), worker.clone())
            .execute(unit.clone());
        assert_eq!(first.return_value, Some(Value::Int(73)));
        assert_eq!(second.return_value, Some(Value::Int(73)));
        assert_eq!(second.native_compile_nanos, 0);
        let warm_stats = worker.native_compile_cache_stats();
        assert_eq!(warm_stats.entries, 1);
        assert_eq!(warm_stats.hits, 1);
        assert_eq!(warm_stats.misses, 1);
        assert_eq!(warm_stats.insertions, 1);
        assert_eq!(warm_stats.evictions, 0);
        assert_eq!(warm_stats.compile_waits, 0);
        assert_eq!(warm_stats.compile_failures, 0);
        assert!(warm_stats.compile_time_nanos > 0);

        // A separately built artifact must not borrow handles merely because
        // its source and IR happen to be equal.
        let replacement = returning_unit(73);
        let replacement_result =
            Vm::with_options_and_worker_state(options, worker.clone()).execute(replacement);
        assert_eq!(replacement_result.return_value, Some(Value::Int(73)));

        // Optimization policy is part of the cache key even for the same
        // immutable compiled-unit allocation.
        let optimizing = VmOptions {
            native_optimization: NativeOptimizationPolicy::Optimizing,
            ..VmOptions::default()
        };
        let optimizing_result =
            Vm::with_options_and_worker_state(optimizing, worker.clone()).execute(unit);
        assert_eq!(optimizing_result.return_value, Some(Value::Int(73)));
        let stats = worker.native_compile_cache_stats();
        assert_eq!(stats.entries, 3);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 3);
        assert_eq!(stats.insertions, 3);
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn loading_declaration_heavy_unit_compiles_only_entry_and_declares_other_cells() {
        let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
        let unit = declaration_heavy_unit();
        let result = Vm::with_options_and_worker_state(VmOptions::default(), worker.clone())
            .execute(unit.clone());

        assert_eq!(result.return_value, Some(Value::Int(17)), "{result:#?}");
        let stats = worker.native_compile_cache_stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.insertions, 1);

        let manager = php_jit::global_code_manager().expect("global code manager");
        let mut published = 0;
        let mut unpublished = 0;
        for (index, function) in unit.unit().functions.iter().enumerate() {
            let key = php_jit::native_function_key(
                unit.prepared_ir_fingerprint().to_owned(),
                index as u32,
                function.params.len(),
                function.local_count,
                false,
                0,
            );
            let cell = manager.function_cell(&key).expect("declared function cell");
            match cell.state() {
                php_jit::NativeIndirectionState::Published => published += 1,
                php_jit::NativeIndirectionState::Declared
                | php_jit::NativeIndirectionState::Queued
                | php_jit::NativeIndirectionState::Compiling
                | php_jit::NativeIndirectionState::Failed => unpublished += 1,
                php_jit::NativeIndirectionState::Retired => panic!("fresh cell was retired"),
            }
        }
        assert_eq!(published, 1);
        assert_eq!(unpublished, 120);
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn vm_reloads_native_artifact_without_compilation() {
        let directory = std::env::temp_dir().join(format!(
            "phrust-vm-pna-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let unit = returning_unit(42);
        let first = Vm::with_options_and_worker_state(
            VmOptions {
                native_cache: php_jit::NativeCacheMode::ReadWrite,
                native_cache_dir: directory.clone(),
                native_cache_stats: true,
                ..VmOptions::default()
            },
            VmWorkerState::isolated_for_restart_test(),
        )
        .execute(unit.clone());
        assert_eq!(
            first.return_value,
            Some(Value::Int(42)),
            "cache population result: {first:#?}"
        );
        assert_eq!(first.native_cache_stats.unwrap().writes, 1);

        let second = Vm::with_options_and_worker_state(
            VmOptions {
                native_cache: php_jit::NativeCacheMode::Read,
                native_cache_dir: directory.clone(),
                native_cache_stats: true,
                ..VmOptions::default()
            },
            VmWorkerState::isolated_for_restart_test(),
        )
        .execute(unit);
        assert_eq!(
            second.return_value,
            Some(Value::Int(42)),
            "cached execution result: {second:#?}"
        );
        assert_eq!(second.native_cache_stats.unwrap().hits, 1);
        assert_eq!(second.native_compile_nanos, 0);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn function_on_demand_callee_reloads_without_compilation() {
        let directory = std::env::temp_dir().join(format!(
            "phrust-vm-pna-callee-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let unit = direct_call_unit();
        let first_worker = VmWorkerState::isolated_for_restart_test();
        let first = Vm::with_options_and_worker_state(
            VmOptions {
                native_cache: php_jit::NativeCacheMode::ReadWrite,
                native_cache_dir: directory.clone(),
                ..VmOptions::default()
            },
            first_worker.clone(),
        )
        .execute(unit.clone());
        assert_eq!(first.return_value, Some(Value::Int(42)), "{first:#?}");
        assert_eq!(first_worker.native_compile_cache_stats().misses, 2);
        let artifacts = std::fs::read_dir(&directory)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "pna")
            })
            .count();
        assert_eq!(
            artifacts, 2,
            "root and demanded callee must persist separately"
        );

        let second_worker = VmWorkerState::isolated_for_restart_test();
        let second = Vm::with_options_and_worker_state(
            VmOptions {
                native_cache: php_jit::NativeCacheMode::Read,
                native_cache_dir: directory.clone(),
                ..VmOptions::default()
            },
            second_worker.clone(),
        )
        .execute(unit);
        assert_eq!(second.return_value, Some(Value::Int(42)), "{second:#?}");
        assert_eq!(second.native_compile_nanos, 0);
        assert_eq!(second_worker.native_compile_cache_stats().misses, 0);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn worker_fast_entry_cache_reuses_loaded_artifact_without_identity_rebuild() {
        let directory = std::env::temp_dir().join(format!(
            "phrust-vm-loaded-unit-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
        let before = worker.loaded_native_unit_stats();
        let entry_hits_before = worker.resolved_native_entry_hits();
        let unit = direct_call_unit();
        let options = VmOptions {
            native_cache: php_jit::NativeCacheMode::ReadWrite,
            native_cache_dir: directory.clone(),
            ..VmOptions::default()
        };
        let first = Vm::with_options_and_worker_state(options.clone(), worker.clone())
            .execute(unit.clone());
        assert_eq!(first.return_value, Some(Value::Int(42)), "{first:#?}");

        for entry in std::fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|extension| extension == "pna") {
                std::fs::remove_file(path).unwrap();
            }
        }

        let second = Vm::with_options_and_worker_state(options, worker.clone()).execute(unit);
        assert_eq!(second.return_value, Some(Value::Int(42)), "{second:#?}");
        assert_eq!(second.native_compile_nanos, 0);
        let loaded = worker.loaded_native_unit_stats();
        assert_eq!(loaded.maps.saturating_sub(before.maps), 2);
        assert_eq!(
            loaded
                .entry_table_constructions
                .saturating_sub(before.entry_table_constructions),
            2
        );
        // The root entry still follows the top-level cache path. Its demanded
        // callee now comes directly from the deployment-owned atomic cell;
        // the deleted worker entry cache must receive no warm lookup at all.
        assert_eq!(loaded.hits.saturating_sub(before.hits), 1);
        assert_eq!(
            worker
                .resolved_native_entry_hits()
                .saturating_sub(entry_hits_before),
            0
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn vm_reloads_helper_using_native_artifact_without_compilation() {
        let directory = std::env::temp_dir().join(format!(
            "phrust-vm-pna-helper-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut ir = returning_unit(91).unit().clone();
        ir.functions[ir.entry.index()].return_type = None;
        let unit = CompiledUnit::from(ir);
        let first = Vm::with_options_and_worker_state(
            VmOptions {
                native_cache: php_jit::NativeCacheMode::ReadWrite,
                native_cache_dir: directory.clone(),
                native_cache_stats: true,
                ..VmOptions::default()
            },
            VmWorkerState::isolated_for_restart_test(),
        )
        .execute(unit.clone());
        assert_eq!(first.return_value, Some(Value::Int(91)), "{first:#?}");
        assert_eq!(first.native_cache_stats.unwrap().writes, 1);

        let second = Vm::with_options_and_worker_state(
            VmOptions {
                native_cache: php_jit::NativeCacheMode::Read,
                native_cache_dir: directory.clone(),
                native_cache_stats: true,
                ..VmOptions::default()
            },
            VmWorkerState::isolated_for_restart_test(),
        )
        .execute(unit);
        assert_eq!(second.return_value, Some(Value::Int(91)), "{second:#?}");
        assert_eq!(second.native_cache_stats.unwrap().hits, 1);
        assert_eq!(second.native_compile_nanos, 0);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
