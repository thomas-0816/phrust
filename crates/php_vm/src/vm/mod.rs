//! Native PHP execution coordinator.

pub(crate) mod jit_abi;
mod native_compile_cache;
mod native_entry;
mod options;
mod result;

pub use native_compile_cache::NativeCompileCacheStats;
pub use options::{NativeBlacklistMode, NativeOptimizationPolicy, VmOptions};
pub use result::VmResult;

use crate::compiled_unit::CompiledUnit;
pub(crate) use jit_abi::native_fixed_callable_plan;
use jit_abi::{
    NativeRequestOwner, activate_native_context, jit_baseline_native_binary_abi,
    jit_baseline_native_builtin_dispatch_abi, jit_baseline_native_builtin_dispatch_diagnostic_abi,
    jit_baseline_native_call_dispatch_abi, jit_baseline_native_call_dispatch_diagnostic_abi,
    jit_baseline_native_cast_abi, jit_baseline_native_compare_abi,
    jit_baseline_native_semantic_dispatch_abi,
    jit_baseline_native_semantic_dispatch_diagnostic_abi, jit_baseline_native_unary_abi,
    jit_native_acos_f64_abi, jit_native_acosh_f64_abi, jit_native_acquire_callable_abi,
    jit_native_argument_check_abi, jit_native_array_cast_abi, jit_native_array_fetch_abi,
    jit_native_array_insert_abi, jit_native_array_insert_local_abi, jit_native_array_new_abi,
    jit_native_array_spread_abi, jit_native_array_union_abi, jit_native_array_unset_abi,
    jit_native_asin_f64_abi, jit_native_asinh_f64_abi, jit_native_atan_f64_abi,
    jit_native_atan2_f64_abi, jit_native_atanh_f64_abi, jit_native_base_convert_abi,
    jit_native_basename_abi, jit_native_bindec_abi, jit_native_bit_and_abi, jit_native_bit_not_abi,
    jit_native_bit_or_abi, jit_native_bit_xor_abi, jit_native_callback_return_string_abi,
    jit_native_chmod_abi, jit_native_class_exists_abi, jit_native_closedir_abi,
    jit_native_concat_abi, jit_native_constant_abi, jit_native_constant_fetch_abi,
    jit_native_cos_f64_abi, jit_native_cosh_f64_abi, jit_native_decbin_abi, jit_native_dechex_abi,
    jit_native_decoct_abi, jit_native_define_abi, jit_native_defined_abi,
    jit_native_deg2rad_f64_abi, jit_native_dirname_abi, jit_native_disk_free_space_abi,
    jit_native_disk_total_space_abi, jit_native_dynamic_code_abi,
    jit_native_dynamic_property_slot_abi, jit_native_dynamic_property_test_slot_abi,
    jit_native_echo_abi, jit_native_echo_bytes_abi, jit_native_enum_exists_abi,
    jit_native_equal_abi, jit_native_exception_new_abi, jit_native_execution_poll_abi,
    jit_native_exp_f64_abi, jit_native_expm1_f64_abi, jit_native_fclose_abi, jit_native_feof_abi,
    jit_native_fflush_abi, jit_native_fgetc_abi, jit_native_fgets_abi, jit_native_file_abi,
    jit_native_file_exists_abi, jit_native_file_get_contents_abi, jit_native_file_put_contents_abi,
    jit_native_filegroup_abi, jit_native_filemtime_abi, jit_native_fileowner_abi,
    jit_native_fileperms_abi, jit_native_filesize_abi, jit_native_filetype_abi,
    jit_native_float_cast_abi, jit_native_float_to_string_abi, jit_native_fmod_f64_abi,
    jit_native_fopen_abi, jit_native_foreach_cleanup_abi, jit_native_foreach_init_abi,
    jit_native_foreach_next_abi, jit_native_fpow_f64_abi, jit_native_frame_alloc_abi,
    jit_native_frame_release_abi, jit_native_fread_abi, jit_native_fseek_abi, jit_native_ftell_abi,
    jit_native_ftruncate_abi, jit_native_function_exists_abi, jit_native_function_resolve_abi,
    jit_native_fwrite_abi, jit_native_get_exception_handler_abi, jit_native_glob_abi,
    jit_native_greater_abi, jit_native_greater_equal_abi, jit_native_gzcompress_abi,
    jit_native_gzdecode_abi, jit_native_gzdeflate_abi, jit_native_gzencode_abi,
    jit_native_gzinflate_abi, jit_native_gzuncompress_abi, jit_native_hash_abi,
    jit_native_hash_equals_abi, jit_native_hash_hmac_abi, jit_native_hexdec_abi,
    jit_native_hypot_f64_abi, jit_native_identical_abi, jit_native_inet_ntop_abi,
    jit_native_inet_pton_abi, jit_native_int_cast_abi, jit_native_interface_exists_abi,
    jit_native_intval_base_abi, jit_native_ip2long_abi, jit_native_is_callable_abi,
    jit_native_is_dir_abi, jit_native_is_file_abi, jit_native_is_link_abi,
    jit_native_is_readable_abi, jit_native_is_uploaded_file_abi, jit_native_is_writable_abi,
    jit_native_json_decode_abi, jit_native_json_encode_abi, jit_native_json_last_error_abi,
    jit_native_json_last_error_msg_abi, jit_native_json_validate_abi, jit_native_less_abi,
    jit_native_less_equal_abi, jit_native_local_fetch_abi, jit_native_local_store_abi,
    jit_native_log_f64_abi, jit_native_log1p_f64_abi, jit_native_log10_f64_abi,
    jit_native_long2ip_abi, jit_native_lstat_abi, jit_native_md5_abi, jit_native_method_exists_abi,
    jit_native_mkdir_abi, jit_native_not_equal_abi, jit_native_not_identical_abi,
    jit_native_number_format_abi, jit_native_numeric_string_abi, jit_native_ob_end_clean_abi,
    jit_native_ob_end_flush_abi, jit_native_ob_get_clean_abi, jit_native_ob_get_contents_abi,
    jit_native_ob_get_flush_abi, jit_native_ob_get_length_abi, jit_native_ob_get_level_abi,
    jit_native_ob_start_abi, jit_native_object_cast_abi, jit_native_object_class_name_abi,
    jit_native_object_clone_abi, jit_native_object_clone_with_abi, jit_native_object_new_abi,
    jit_native_octdec_abi, jit_native_opendir_abi, jit_native_pack_abi, jit_native_pathinfo_abi,
    jit_native_plain_object_clone_abi, jit_native_preg_callback_assemble_abi,
    jit_native_preg_callback_plan_abi, jit_native_preg_filter_abi, jit_native_preg_grep_abi,
    jit_native_preg_last_error_abi, jit_native_preg_last_error_msg_abi, jit_native_preg_match_abi,
    jit_native_preg_match_all_abi, jit_native_preg_quote_abi, jit_native_preg_replace_abi,
    jit_native_preg_split_abi, jit_native_prepared_closure_new_abi,
    jit_native_prepared_exception_new_abi, jit_native_prepared_object_new_abi,
    jit_native_printf_abi, jit_native_property_assign_abi, jit_native_property_exists_abi,
    jit_native_property_fetch_abi, jit_native_rad2deg_f64_abi, jit_native_readdir_abi,
    jit_native_readfile_abi, jit_native_realpath_abi, jit_native_reference_bind_abi,
    jit_native_register_shutdown_function_abi, jit_native_rename_abi,
    jit_native_resolve_callable_abi, jit_native_restore_error_handler_abi,
    jit_native_restore_exception_handler_abi, jit_native_return_check_abi, jit_native_rewind_abi,
    jit_native_rewinddir_abi, jit_native_rmdir_abi, jit_native_round_f64_abi,
    jit_native_runtime_fatal_abi, jit_native_scandir_abi, jit_native_set_error_handler_abi,
    jit_native_set_exception_handler_abi, jit_native_sha1_abi, jit_native_sin_f64_abi,
    jit_native_sinh_f64_abi, jit_native_spaceship_abi, jit_native_spl_autoload_functions_abi,
    jit_native_spl_autoload_register_abi, jit_native_spl_autoload_unregister_abi,
    jit_native_sprintf_abi, jit_native_stable_length_abi, jit_native_stat_abi,
    jit_native_stream_copy_to_stream_abi, jit_native_stream_get_contents_abi,
    jit_native_string_cast_abi, jit_native_string_predicate_abi, jit_native_symlink_abi,
    jit_native_tan_f64_abi, jit_native_tanh_f64_abi, jit_native_tempnam_abi,
    jit_native_tmpfile_abi, jit_native_touch_abi, jit_native_trait_exists_abi,
    jit_native_truthy_abi, jit_native_type_predicate_abi, jit_native_unary_minus_abi,
    jit_native_unary_plus_abi, jit_native_unlink_abi, jit_native_unpack_abi,
    jit_native_value_release_abi, jit_native_vprintf_abi, jit_native_vsprintf_abi,
    jit_native_zlib_decode_abi, jit_native_zlib_encode_abi, resume_native_optimizing_exit,
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
    native_request_pool: Arc<Mutex<jit_abi::NativeRequestPool>>,
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

struct BackgroundOptimizationWork {
    decision: BackgroundTieringDecision,
    unit: CompiledUnit,
    function: php_ir::FunctionId,
    compile_options: BackgroundCompileOptions,
    external_signatures: Vec<php_jit::JitExternalFunctionSignature>,
}

#[derive(Clone, Debug)]
struct BackgroundCompileOptions {
    collect_counters: bool,
    native_cache: php_jit::NativeCacheMode,
    native_cache_dir: std::path::PathBuf,
}

impl BackgroundCompileOptions {
    fn from_vm_options(options: &VmOptions) -> Self {
        Self {
            collect_counters: options.collect_counters,
            native_cache: options.native_cache,
            native_cache_dir: options.native_cache_dir.clone(),
        }
    }

    fn optimizing_vm_options(&self) -> VmOptions {
        VmOptions {
            collect_counters: self.collect_counters,
            native_cache: self.native_cache,
            native_cache_dir: self.native_cache_dir.clone(),
            native_optimization: NativeOptimizationPolicy::Optimizing,
            tiering: crate::tiering::TieringOptions {
                enabled: false,
                ..crate::tiering::TieringOptions::default()
            },
            ..VmOptions::default()
        }
    }
}

static PROCESS_LOADED_NATIVE_UNITS: std::sync::OnceLock<
    Arc<native_compile_cache::LoadedNativeUnitRegistry>,
> = std::sync::OnceLock::new();

type NativeOptimizationJob = Box<dyn FnOnce() + Send + 'static>;

/// Optimizing Cranelift compilation has a large transient working set. One
/// process-wide compiler preserves request parallelism while preventing two
/// independent code generators from doubling RSS and competing for the same
/// CPU after a request boundary.
const NATIVE_OPTIMIZATION_WORKERS: usize = 1;
const NATIVE_OPTIMIZATION_QUEUE_CAPACITY: usize = 1;
const NATIVE_OPTIMIZATION_BATCH_CAPACITY: usize = 128;

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
    // Optimization is never allowed to delay a request. Request completion
    // submits the hottest candidates first; a full queue rejects the colder
    // tail and lets a later request reconsider it after more entry evidence.
    sender.try_send(Box::new(job)).is_ok()
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
            native_request_pool: Arc::new(Mutex::new(jit_abi::NativeRequestPool::default())),
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
            native_request_pool: Arc::new(Mutex::new(jit_abi::NativeRequestPool::default())),
        }
    }

    fn checkout_native_request_buffers(
        &self,
        argument_capacity: usize,
    ) -> jit_abi::NativeRequestBuffers {
        lock_unpoisoned(&self.native_request_pool).checkout(argument_capacity)
    }

    fn recycle_native_request_buffers(&self, buffers: jit_abi::NativeRequestBuffers) {
        lock_unpoisoned(&self.native_request_pool).recycle(buffers);
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
        let compiled =
            self.compile_native_with_priority(unit, function, options, external_signatures, false)?;
        if let Some(handle) = compiled
            .0
            .iter()
            .find(|record| record.function == function)
            .and_then(|record| record.result.handle.as_ref())
        {
            self.prepare_and_publish_optimizing_entry(
                unit,
                function,
                options,
                external_signatures,
                handle,
                false,
            )?;
        }
        Ok(compiled)
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
        // Every statically named non-local call owns an immutable source-unit
        // link slot, even when its declaration is not visible yet. Root units
        // are compiled before request-time includes execute, so relying only
        // on the currently visible signature set would permanently lower
        // those calls to the generic baseline dispatcher. Complete the
        // compile-time set with late-bound link records here; publication
        // fills the same slots once the target unit is declared.
        let external_signatures =
            linked_external_function_signatures(unit, function, external_signatures);
        let external_signatures = external_signatures.as_slice();
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
        let external_signatures_hash = native_dependency_signature_hash(
            unit,
            function,
            external_signatures,
            options.native_optimization.is_optimizing(),
        );
        let key = native_compile_cache::NativeCompileCacheKey::new(
            unit.cache_identity(),
            function,
            options.native_optimization.opt_level(),
            external_signatures_hash,
        );
        let method_specializations = if options.native_optimization.is_optimizing() {
            unit.prepared_method_specializations(function)
        } else {
            Vec::new()
        };
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
                &method_specializations,
            )
        };
        let compiled = if background {
            self.native_compiles.get_or_compile_background(key, compile)
        } else {
            self.native_compiles.get_or_compile(key, compile)
        }?;
        if std::env::var_os("PHRUST_NATIVE_COMPILE_FUNCTION_LOG").is_some()
            && compiled.1.compiled()
            && let Some(record) = compiled.0.first()
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
        Ok(compiled)
    }

    fn prepare_native_baseline_entry(
        &self,
        unit: &CompiledUnit,
        function: php_ir::FunctionId,
        options: &VmOptions,
        external_signatures: &[php_jit::JitExternalFunctionSignature],
    ) -> Result<usize, String> {
        let deployment = unit.prepared_deployment_image();
        let baseline_cell = deployment
            .native_function_entries
            .get(function.index())
            .ok_or_else(|| {
                format!(
                    "native function {} has no baseline publication cell",
                    function.raw()
                )
            })?;
        let preferred = deployment
            .preferred_function_entries
            .get(function.index())
            .ok_or_else(|| {
                format!(
                    "native function {} has no preferred publication cell",
                    function.raw()
                )
            })?;
        let previous_address = baseline_cell.load(std::sync::atomic::Ordering::Acquire);
        if previous_address != 0 && external_signatures.is_empty() {
            // A baseline artifact has no external ABI specialization when
            // this function owns no linked dependencies. Its deployment cell
            // is therefore the complete publication proof; rebuilding a
            // worker cache key here adds one identity lookup (and, across
            // baseline policies, an unnecessary third artifact) to every
            // optimizing publication.
            let _ = preferred.compare_exchange(
                0,
                previous_address,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            );
            return Ok(previous_address);
        }
        let mut baseline_options = options.clone();
        baseline_options.native_optimization = match options.native_optimization {
            NativeOptimizationPolicy::Optimizing
                if options.tiering.enabled && !options.tiering.native_eager =>
            {
                NativeOptimizationPolicy::TieredBaseline
            }
            NativeOptimizationPolicy::Optimizing | NativeOptimizationPolicy::Baseline => {
                NativeOptimizationPolicy::Baseline
            }
            NativeOptimizationPolicy::TieredBaseline => NativeOptimizationPolicy::TieredBaseline,
        };
        baseline_options.tiering.enabled = false;
        let key = native_compile_cache_key(
            unit,
            function,
            baseline_options.native_optimization.opt_level(),
            external_signatures,
        );
        let mut newly_resolved = false;
        let baseline = if let Some(handle) = self.resolved_native_entries.get(key) {
            handle
        } else {
            let handle = self.resolve_native_function_cold(
                unit,
                function,
                &baseline_options,
                external_signatures,
                false,
            )?;
            // Install the generation-owning handle before descending through
            // direct callees. Recursive and mutually recursive graphs can now
            // terminate on this exact signature variant even when an older
            // variant had already populated the shared publication cell.
            self.resolved_native_entries.insert(key, handle.clone());
            newly_resolved = true;
            handle
        };
        let address = baseline.native_entry_address().ok_or_else(|| {
            format!(
                "native function {} has no baseline entry address",
                function.raw()
            )
        })?;
        baseline_cell.store(address, std::sync::atomic::Ordering::Release);
        if newly_resolved && previous_address != 0 {
            // A newly resolved dependency key may reuse identical baseline
            // machine code, so address equality cannot prove ABI identity.
            // Invalidate both tiers rather than leave an optimizer compiled
            // against the previous linked ABI in the preferred cell.
            unit.publish_preferred_function_metadata(function, &baseline);
            preferred.store(address, std::sync::atomic::Ordering::Release);
        }
        if newly_resolved && let Some(metadata) = baseline.region_state_metadata() {
            // Publish the caller cell before descending so recursive and
            // mutually recursive call graphs terminate on the already
            // installed address. Every statically known native call can then
            // load its exact target directly on the caller's first entry.
            for callee in metadata
                .direct_callees
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
            {
                let callee_signatures =
                    linked_external_function_signatures(unit, callee, external_signatures);
                self.prepare_native_baseline_entry(unit, callee, options, &callee_signatures)?;
            }
        }
        if preferred
            .compare_exchange(
                0,
                address,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
        {
            unit.publish_preferred_function_metadata(function, &baseline);
        }
        Ok(address)
    }

    fn defers_optimizing_compilation(&self, options: &VmOptions) -> bool {
        self.background_tiering
            && self.tiering_options.enabled
            && !self.tiering_options.native_eager
            && options.tiering.enabled
            && !options.tiering.native_eager
            && options.native_optimization == NativeOptimizationPolicy::Optimizing
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
        let key = native_compile_cache_key(
            unit,
            function,
            NativeOptimizationPolicy::Optimizing.opt_level(),
            external_signatures,
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
        options: &VmOptions,
        external_signatures: Vec<php_jit::JitExternalFunctionSignature>,
    ) {
        if !self.claim_background_optimization(decision) {
            return;
        }
        let work = BackgroundOptimizationWork {
            decision,
            unit,
            function,
            compile_options: BackgroundCompileOptions::from_vm_options(options),
            external_signatures,
        };
        let worker = self.clone();
        let submitted = submit_native_optimization_job(move || {
            worker.run_background_optimization(work);
        });
        if !submitted {
            self.reject_submitted_optimizations(std::slice::from_ref(&decision));
        }
    }

    fn claim_background_optimization(&self, decision: BackgroundTieringDecision) -> bool {
        if decision.entries < self.tiering_options.function_entry_threshold.max(1) {
            return false;
        }
        if self.native_compiles.contains(decision.key) {
            return false;
        }
        let mut state = lock_unpoisoned(&self.tiering_state);
        if state.scheduled.contains(&decision.key) || state.failed.contains(&decision.key) {
            return false;
        }
        let reserved_functions = u64::try_from(state.scheduled.len()).unwrap_or(u64::MAX);
        if state
            .stats
            .native_compiled_functions
            .saturating_add(reserved_functions)
            >= self.tiering_options.native_max_functions
            || state.stats.native_compile_budget_used_us
                >= self.tiering_options.native_max_compile_us
        {
            state.stats.native_compile_budget_rejections = state
                .stats
                .native_compile_budget_rejections
                .saturating_add(1);
            return false;
        }
        state.scheduled.insert(decision.key);
        state.stats.optimized_candidates = state.stats.optimized_candidates.saturating_add(1);
        true
    }

    fn reject_submitted_optimizations(&self, decisions: &[BackgroundTieringDecision]) {
        if decisions.is_empty() {
            return;
        }
        let mut state = lock_unpoisoned(&self.tiering_state);
        for decision in decisions {
            state.scheduled.remove(&decision.key);
        }
        state.stats.native_compile_budget_rejections = state
            .stats
            .native_compile_budget_rejections
            .saturating_add(u64::try_from(decisions.len()).unwrap_or(u64::MAX));
    }

    fn run_background_optimization(&self, work: BackgroundOptimizationWork) {
        {
            let mut state = lock_unpoisoned(&self.tiering_state);
            if !state.scheduled.contains(&work.decision.key) {
                return;
            }
            if state.stats.native_compiled_functions >= self.tiering_options.native_max_functions
                || state.stats.native_compile_budget_used_us
                    >= self.tiering_options.native_max_compile_us
            {
                state.scheduled.remove(&work.decision.key);
                state.stats.native_compile_budget_rejections = state
                    .stats
                    .native_compile_budget_rejections
                    .saturating_add(1);
                return;
            }
        }
        let started = Instant::now();
        let options = work.compile_options.optimizing_vm_options();
        let result = self.resolve_native_function_with_priority(
            &work.unit,
            work.function,
            &options,
            &work.external_signatures,
            true,
        );
        let completed_optimizing_entry = result
            .as_ref()
            .ok()
            .filter(|handle| {
                handle.region_state_metadata().is_some_and(|metadata| {
                    metadata.compiler_tier == php_jit::region_ir::NativeCompilerTier::Optimizing
                })
            })
            .and_then(php_jit::JitFunctionHandle::native_entry_address);
        // A caller with immutable external links may only become preferred at
        // a request-owned declaration boundary. The background worker has no
        // live runtime view in which it could prepare and validate those target
        // cells; publishing here would let an in-flight request observe new
        // code against its older link graph. Link-free and same-unit products
        // are already fully prepared by `prepare_and_publish_optimizing_entry`
        // and remain safe to publish immediately.
        let publishable_optimizing_entry = work
            .external_signatures
            .is_empty()
            .then_some(completed_optimizing_entry)
            .flatten();
        if let Some(address) = publishable_optimizing_entry
            && let Some(cell) = work
                .unit
                .prepared_deployment_image()
                .preferred_function_entries
                .get(work.function.index())
        {
            if let Ok(handle) = result.as_ref() {
                work.unit
                    .publish_preferred_function_metadata(work.function, handle);
            }
            cell.store(address, std::sync::atomic::Ordering::Release);
        }
        let elapsed_us = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        let mut state = lock_unpoisoned(&self.tiering_state);
        state.scheduled.remove(&work.decision.key);
        state.stats.native_compile_budget_used_us = state
            .stats
            .native_compile_budget_used_us
            .saturating_add(elapsed_us);
        if completed_optimizing_entry.is_some() {
            state.stats.native_compiled_functions =
                state.stats.native_compiled_functions.saturating_add(1);
        } else {
            state.failed.insert(work.decision.key);
        }
    }

    /// Requests one serialized optimizing batch for baseline functions whose
    /// direct process-owned entry counters prove they are hot.
    fn schedule_hot_on_demand_optimizations(
        &self,
        options: &VmOptions,
        candidates: Vec<(
            CompiledUnit,
            php_ir::FunctionId,
            Vec<php_jit::JitExternalFunctionSignature>,
            u64,
        )>,
    ) {
        if !self.background_tiering
            || !self.tiering_options.enabled
            || self.tiering_options.native_eager
        {
            return;
        }
        let mut work = Vec::with_capacity(candidates.len());
        for (unit, function, external_signatures, entries) in candidates {
            let decision = BackgroundTieringDecision {
                key: native_compile_cache_key(
                    &unit,
                    function,
                    NativeOptimizationPolicy::Optimizing.opt_level(),
                    &external_signatures,
                ),
                entries,
            };
            if self.claim_background_optimization(decision) {
                work.push(BackgroundOptimizationWork {
                    decision,
                    unit,
                    function,
                    compile_options: BackgroundCompileOptions::from_vm_options(options),
                    external_signatures,
                });
            }
        }
        if work.is_empty() {
            return;
        }
        let decisions = work.iter().map(|work| work.decision).collect::<Vec<_>>();
        let worker = self.clone();
        let submitted = submit_native_optimization_job(move || {
            for work in work {
                worker.run_background_optimization(work);
            }
        });
        if !submitted {
            self.reject_submitted_optimizations(&decisions);
        }
    }

    fn resolve_native_function(
        &self,
        unit: &CompiledUnit,
        function: php_ir::FunctionId,
        options: &VmOptions,
        external_signatures: &[php_jit::JitExternalFunctionSignature],
    ) -> Result<php_jit::JitFunctionHandle, String> {
        self.resolve_native_function_with_priority(
            unit,
            function,
            options,
            external_signatures,
            false,
        )
    }

    fn resolve_native_function_with_priority(
        &self,
        unit: &CompiledUnit,
        function: php_ir::FunctionId,
        options: &VmOptions,
        external_signatures: &[php_jit::JitExternalFunctionSignature],
        background: bool,
    ) -> Result<php_jit::JitFunctionHandle, String> {
        let linked_external_signatures =
            linked_external_function_signatures(unit, function, external_signatures);
        let fast_key = native_compile_cache_key(
            unit,
            function,
            options.native_optimization.opt_level(),
            &linked_external_signatures,
        );
        if let Some(handle) = self.resolved_native_entries.get(fast_key) {
            self.prepare_and_publish_optimizing_entry(
                unit,
                function,
                options,
                external_signatures,
                &handle,
                background,
            )?;
            return Ok(handle);
        }
        let handle = self.resolve_native_function_cold(
            unit,
            function,
            options,
            external_signatures,
            background,
        )?;
        self.prepare_and_publish_optimizing_entry(
            unit,
            function,
            options,
            external_signatures,
            &handle,
            background,
        )?;
        // Cross-unit background products are compilation-complete but not yet
        // publication-ready: only a request-owned declaration boundary can
        // prepare their exact linked runtime views. Keeping them out of the
        // resolved-entry fast map prevents an in-flight request from adopting
        // one through dynamic dispatch before that boundary. The compile cache
        // retains the handle for zero-recompile adoption by the next request.
        if !background || linked_external_signatures.is_empty() {
            self.resolved_native_entries
                .insert(fast_key, handle.clone());
        }
        Ok(handle)
    }

    fn has_compiled_optimizing_function(
        &self,
        unit: &CompiledUnit,
        function: php_ir::FunctionId,
        external_signatures: &[php_jit::JitExternalFunctionSignature],
    ) -> bool {
        self.native_compiles.contains(native_compile_cache_key(
            unit,
            function,
            NativeOptimizationPolicy::Optimizing.opt_level(),
            external_signatures,
        ))
    }

    fn resolved_native_function(
        &self,
        unit: &CompiledUnit,
        function: php_ir::FunctionId,
        options: &VmOptions,
        external_signatures: &[php_jit::JitExternalFunctionSignature],
    ) -> Option<php_jit::JitFunctionHandle> {
        self.resolved_native_entries.get(native_compile_cache_key(
            unit,
            function,
            options.native_optimization.opt_level(),
            external_signatures,
        ))
    }

    fn resolve_native_function_cold(
        &self,
        unit: &CompiledUnit,
        function: php_ir::FunctionId,
        options: &VmOptions,
        external_signatures: &[php_jit::JitExternalFunctionSignature],
        background: bool,
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
                                    .compile_native_with_priority(
                                        unit,
                                        function,
                                        options,
                                        external_signatures,
                                        background,
                                    )
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

        let (records, _) = self.compile_native_with_priority(
            unit,
            function,
            options,
            external_signatures,
            background,
        )?;
        jit_abi::native_entries_from_records(&records)?
            .remove(&function)
            .ok_or_else(|| format!("native function entry {} was not published", function.raw()))
    }

    /// Loads one already persisted native function without compiling on a
    /// miss. Request-time dynamic-unit publication uses this to adopt a warm
    /// optimizing artifact while retaining the baseline continuation for a
    /// genuinely cold deployment.
    fn load_cached_native_function(
        &self,
        unit: &CompiledUnit,
        function: php_ir::FunctionId,
        options: &VmOptions,
        external_signatures: &[php_jit::JitExternalFunctionSignature],
    ) -> Result<Option<php_jit::JitFunctionHandle>, String> {
        let key = native_compile_cache_key(
            unit,
            function,
            options.native_optimization.opt_level(),
            external_signatures,
        );
        if let Some(handle) = self.resolved_native_entries.get(key) {
            return Ok(Some(handle));
        }
        if options.native_cache == php_jit::NativeCacheMode::Off {
            return Ok(None);
        }
        let cache = php_jit::NativeArtifactCache::new(php_jit::NativeCacheConfig {
            mode: options.native_cache,
            directory: options.native_cache_dir.clone(),
            ..php_jit::NativeCacheConfig::default()
        })
        .map_err(|error| format!("E_NATIVE_CACHE_SETUP: {error}"))?;
        if !cache.config().mode.can_read() {
            return Ok(None);
        }
        let identity = native_cache_identity(unit, function, options, external_signatures)
            .map_err(|error| format!("E_NATIVE_CACHE_IDENTITY: {error}"))?;
        let loaded = self
            .get_or_load_native_unit(&identity, || {
                cache.load(&identity, |stable_id| {
                    resolve_native_cache_helper(stable_id, options.collect_counters)
                })
            })
            .map_err(|error| format!("E_NATIVE_CACHE_ARTIFACT: {error}"))?;
        let Some(handle) =
            loaded.and_then(|loaded| loaded.native_entries().get(&function).cloned())
        else {
            return Ok(None);
        };
        self.resolved_native_entries.insert(key, handle.clone());
        Ok(Some(handle))
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
        hash = (hash ^ u64::from(signature.link_index)).wrapping_mul(0x0000_0100_0000_01b3);
        hash = (hash ^ u64::from(signature.published)).wrapping_mul(0x0000_0100_0000_01b3);
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
        hash = (hash ^ u64::from(signature.native_arity)).wrapping_mul(0x0000_0100_0000_01b3);
        hash = (hash ^ u64::from(signature.requires_non_reference_trampoline))
            .wrapping_mul(0x0000_0100_0000_01b3);
        hash =
            (hash ^ u64::from(signature.returns_by_reference)).wrapping_mul(0x0000_0100_0000_01b3);
        hash = (hash
            ^ signature
                .exception_routes
                .map_or(u64::MAX, |function| u64::from(function.raw())))
        .wrapping_mul(0x0000_0100_0000_01b3);
        for byte in format!("{:?}", signature.native_params).bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        for default in &signature.native_default_constant_indices {
            let encoded = default.map_or(u64::MAX, u64::from);
            hash = (hash ^ encoded).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

fn native_dependency_signature_hash(
    unit: &CompiledUnit,
    function: php_ir::FunctionId,
    external_signatures: &[php_jit::JitExternalFunctionSignature],
    include_method_specializations: bool,
) -> u64 {
    let mut hash = external_function_signatures_hash(external_signatures);
    if !include_method_specializations {
        return hash;
    }
    for specialization in unit.prepared_method_specializations(function) {
        hash =
            (hash ^ u64::from(specialization.instruction_id)).wrapping_mul(0x0000_0100_0000_01b3);
        hash = (hash ^ specialization.receiver_layout_id).wrapping_mul(0x0000_0100_0000_01b3);
        match specialization.target {
            php_jit::JitMethodSpecializationTarget::Local(function) => {
                hash = (hash ^ u64::from(function.raw())).wrapping_mul(0x0000_0100_0000_01b3);
            }
            php_jit::JitMethodSpecializationTarget::Linked(signature) => {
                hash = (hash ^ external_function_signatures_hash(std::slice::from_ref(&signature)))
                    .wrapping_mul(0x0000_0100_0000_01b3);
            }
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
        let deployment = unit.prepared_deployment_image();
        if self.options.native_optimization == NativeOptimizationPolicy::Optimizing {
            let mut baseline_options = self.options.clone();
            baseline_options.native_optimization =
                if self.options.tiering.enabled && !self.options.tiering.native_eager {
                    NativeOptimizationPolicy::TieredBaseline
                } else {
                    NativeOptimizationPolicy::Baseline
                };
            baseline_options.tiering.enabled = false;
            let Ok(baseline) =
                self.worker_state
                    .resolve_native_function(unit, entry, &baseline_options, &[])
            else {
                return 0;
            };
            let Some(baseline_address) = baseline.native_entry_address() else {
                return 0;
            };
            if let Some(cell) = deployment.native_function_entries.get(entry.index()) {
                cell.store(baseline_address, std::sync::atomic::Ordering::Release);
            }
            let Ok(optimizing) =
                self.worker_state
                    .resolve_native_function(unit, entry, &self.options, &[])
            else {
                return 0;
            };
            let Some(address) = optimizing.native_entry_address() else {
                return 0;
            };
            if !optimizing.region_state_metadata().is_some_and(|metadata| {
                metadata.compiler_tier == php_jit::region_ir::NativeCompilerTier::Optimizing
            }) {
                return 0;
            }
            if let Some(preferred) = deployment.preferred_function_entries.get(entry.index()) {
                unit.publish_preferred_function_metadata(entry, &optimizing);
                preferred.store(address, std::sync::atomic::Ordering::Release);
            }
        } else {
            let Ok(handle) =
                self.worker_state
                    .resolve_native_function(unit, entry, &self.options, &[])
            else {
                return 0;
            };
            let Some(address) = handle.native_entry_address() else {
                return 0;
            };
            if let Some(baseline) = deployment.native_function_entries.get(entry.index()) {
                baseline.store(address, std::sync::atomic::Ordering::Release);
            }
            if let Some(preferred) = deployment.preferred_function_entries.get(entry.index()) {
                if preferred
                    .compare_exchange(
                        0,
                        address,
                        std::sync::atomic::Ordering::AcqRel,
                        std::sync::atomic::Ordering::Acquire,
                    )
                    .is_ok()
                {
                    unit.publish_preferred_function_metadata(entry, &handle);
                }
            }
        }
        1
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
                    &self.options,
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
                                // The loaded-unit registry still owns its cold
                                // publication lock while this cache producer
                                // runs. Compile the artifact without preparing
                                // nested baseline artifacts here; the loaded
                                // entry performs that publication transaction
                                // after the registry lock has been released.
                                let (records, disposition) =
                                    match self.worker_state.compile_native_with_priority(
                                        &unit,
                                        entry,
                                        &self.options,
                                        external_signatures,
                                        false,
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
                        let result = self.execute_cached_entry(
                            &unit,
                            loaded,
                            entry,
                            external_signatures,
                            output,
                        );
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
                    let result = self.execute_cached_entry(
                        &unit,
                        loaded,
                        entry,
                        external_signatures,
                        output,
                    );
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
                    .materialize_outer_result(value)
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
        let outcome = resume_native_optimizing_exit(&mut context, handle.clone(), outcome);
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
        let publish_error = context
            .materialize_native_session_state()
            .and_then(|()| context.publish_include_globals())
            .err();
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
                    match context.materialize_outer_result(value) {
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
                    let exit_code = match context.materialize_outer_result(value) {
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
                    let throwable = context.materialize_outer_result(value).ok();
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
        context.recycle_native_request_buffers();
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

fn linked_external_function_signatures(
    unit: &CompiledUnit,
    function: php_ir::FunctionId,
    published: &[php_jit::JitExternalFunctionSignature],
) -> Vec<php_jit::JitExternalFunctionSignature> {
    unit.prepared_external_function_calls(function)
        .iter()
        .map(|call| {
            published
                .iter()
                .find(|signature| {
                    signature
                        .name
                        .trim_start_matches('\\')
                        .eq_ignore_ascii_case(call.source_name.trim_start_matches('\\'))
                })
                .cloned()
                .unwrap_or_else(|| php_jit::JitExternalFunctionSignature {
                    name: call.source_name.to_string(),
                    link_index: call.link_index,
                    published: false,
                    params: Vec::new(),
                    native_params: Vec::new(),
                    native_default_constant_indices: Vec::new(),
                    native_arity: 0,
                    requires_non_reference_trampoline: false,
                    returns_by_reference: false,
                    exception_routes: None,
                })
        })
        .collect()
}

/// Builds every worker-local native key from the immutable source call-link
/// set. Callers may supply no currently visible declarations, but compilation
/// still lowers one late-bound slot per prepared external call. Hashing the
/// raw visibility slice here made tiering look for a different artifact than
/// the compiler stored and forced every subsequent request back through the
/// baseline tier.
fn native_compile_cache_key(
    unit: &CompiledUnit,
    function: php_ir::FunctionId,
    optimization_level: u8,
    external_signatures: &[php_jit::JitExternalFunctionSignature],
) -> native_compile_cache::NativeCompileCacheKey {
    let external_signatures =
        linked_external_function_signatures(unit, function, external_signatures);
    native_compile_cache::NativeCompileCacheKey::new(
        unit.cache_identity(),
        function,
        optimization_level,
        native_dependency_signature_hash(
            unit,
            function,
            &external_signatures,
            optimization_level >= NativeOptimizationPolicy::Optimizing.opt_level(),
        ),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn compile_native_function_graph(
    unit: &php_ir::IrUnit,
    function: php_ir::FunctionId,
    options: &VmOptions,
    ir_fingerprint: &str,
    deployment_identity: &str,
    deployment_runtime_identity: u64,
    dependency_identity: &str,
    external_signatures: &[php_jit::JitExternalFunctionSignature],
    method_specializations: &[php_jit::JitMethodSpecialization],
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
                .with_method_specializations(method_specializations.to_vec())
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
    let key = |name: &str| {
        php_runtime::api::ArrayKey::String(php_runtime::api::PhpString::from_bytes(
            name.as_bytes().to_vec(),
        ))
    };
    let field = |name: &str| match value {
        Value::Array(exception) => exception.get(&key(name)).cloned(),
        Value::Object(exception) => exception.get_property(name),
        _ => None,
    };
    let Value::Int(line) = field("line")? else {
        return None;
    };
    let line = usize::try_from(line).ok()?;
    let trace = match field("trace") {
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
        _ if class.eq_ignore_ascii_case("ArgumentCountError") => {
            format!("{message} in {file}:{line}")
        }
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
    let external_signatures =
        linked_external_function_signatures(unit, function, external_signatures);
    let external_signatures_hash = native_dependency_signature_hash(
        unit,
        function,
        &external_signatures,
        options.native_optimization.is_optimizing(),
    );
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
        baseline_call_dispatch: helper_address!(
            jit_baseline_native_call_dispatch_abi,
            jit_baseline_native_call_dispatch_diagnostic_abi
        ),
        baseline_builtin_dispatch: helper_address!(
            jit_baseline_native_builtin_dispatch_abi,
            jit_baseline_native_builtin_dispatch_diagnostic_abi
        ),
        native_define: jit_native_define_abi as *const () as usize,
        native_defined: jit_native_defined_abi as *const () as usize,
        native_constant: jit_native_constant_abi as *const () as usize,
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
        native_preg_callback_plan: jit_native_preg_callback_plan_abi as *const () as usize,
        native_preg_callback_assemble: jit_native_preg_callback_assemble_abi as *const () as usize,
        native_json_encode: jit_native_json_encode_abi as *const () as usize,
        native_json_decode: jit_native_json_decode_abi as *const () as usize,
        native_json_validate: jit_native_json_validate_abi as *const () as usize,
        native_json_last_error: jit_native_json_last_error_abi as *const () as usize,
        native_json_last_error_msg: jit_native_json_last_error_msg_abi as *const () as usize,
        native_sprintf: jit_native_sprintf_abi as *const () as usize,
        native_printf: jit_native_printf_abi as *const () as usize,
        native_vsprintf: jit_native_vsprintf_abi as *const () as usize,
        native_vprintf: jit_native_vprintf_abi as *const () as usize,
        native_number_format: jit_native_number_format_abi as *const () as usize,
        native_md5: jit_native_md5_abi as *const () as usize,
        native_sha1: jit_native_sha1_abi as *const () as usize,
        native_crc32: jit_abi::jit_native_crc32_abi as *const () as usize,
        native_hash: jit_native_hash_abi as *const () as usize,
        native_hash_hmac: jit_native_hash_hmac_abi as *const () as usize,
        native_hash_equals: jit_native_hash_equals_abi as *const () as usize,
        native_base64_encode: jit_abi::jit_native_base64_encode_abi as *const () as usize,
        native_base64_decode: jit_abi::jit_native_base64_decode_abi as *const () as usize,
        native_bin2hex: jit_abi::jit_native_bin2hex_abi as *const () as usize,
        native_hex2bin: jit_abi::jit_native_hex2bin_abi as *const () as usize,
        native_quoted_printable_decode: jit_abi::jit_native_quoted_printable_decode_abi as *const ()
            as usize,
        native_urlencode: jit_abi::jit_native_urlencode_abi as *const () as usize,
        native_rawurlencode: jit_abi::jit_native_rawurlencode_abi as *const () as usize,
        native_urldecode: jit_abi::jit_native_urldecode_abi as *const () as usize,
        native_rawurldecode: jit_abi::jit_native_rawurldecode_abi as *const () as usize,
        native_convert_uuencode: jit_abi::jit_native_convert_uuencode_abi as *const () as usize,
        native_convert_uudecode: jit_abi::jit_native_convert_uudecode_abi as *const () as usize,
        native_addcslashes: jit_abi::jit_native_addcslashes_abi as *const () as usize,
        native_stripcslashes: jit_abi::jit_native_stripcslashes_abi as *const () as usize,
        native_stripslashes: jit_abi::jit_native_stripslashes_abi as *const () as usize,
        native_quotemeta: jit_abi::jit_native_quotemeta_abi as *const () as usize,
        native_pack: jit_native_pack_abi as *const () as usize,
        native_unpack: jit_native_unpack_abi as *const () as usize,
        native_basename: jit_native_basename_abi as *const () as usize,
        native_dirname: jit_native_dirname_abi as *const () as usize,
        native_realpath: jit_native_realpath_abi as *const () as usize,
        native_file_exists: jit_native_file_exists_abi as *const () as usize,
        native_is_file: jit_native_is_file_abi as *const () as usize,
        native_is_dir: jit_native_is_dir_abi as *const () as usize,
        native_is_readable: jit_native_is_readable_abi as *const () as usize,
        native_is_writable: jit_native_is_writable_abi as *const () as usize,
        native_is_link: jit_native_is_link_abi as *const () as usize,
        native_fileperms: jit_native_fileperms_abi as *const () as usize,
        native_fileowner: jit_native_fileowner_abi as *const () as usize,
        native_filegroup: jit_native_filegroup_abi as *const () as usize,
        native_filetype: jit_native_filetype_abi as *const () as usize,
        native_disk_free_space: jit_native_disk_free_space_abi as *const () as usize,
        native_disk_total_space: jit_native_disk_total_space_abi as *const () as usize,
        native_pathinfo: jit_native_pathinfo_abi as *const () as usize,
        native_stat: jit_native_stat_abi as *const () as usize,
        native_lstat: jit_native_lstat_abi as *const () as usize,
        native_file: jit_native_file_abi as *const () as usize,
        native_glob: jit_native_glob_abi as *const () as usize,
        native_opendir: jit_native_opendir_abi as *const () as usize,
        native_readdir: jit_native_readdir_abi as *const () as usize,
        native_rewinddir: jit_native_rewinddir_abi as *const () as usize,
        native_closedir: jit_native_closedir_abi as *const () as usize,
        native_scandir: jit_native_scandir_abi as *const () as usize,
        native_stream_get_meta_data: jit_abi::jit_native_stream_get_meta_data_abi as *const ()
            as usize,
        native_stream_get_wrappers: jit_abi::jit_native_stream_get_wrappers_abi as *const ()
            as usize,
        native_stream_is_local: jit_abi::jit_native_stream_is_local_abi as *const () as usize,
        native_stream_resolve_include_path: jit_abi::jit_native_stream_resolve_include_path_abi
            as *const () as usize,
        native_stream_context_create: jit_abi::jit_native_stream_context_create_abi as *const ()
            as usize,
        native_stream_context_get_default: jit_abi::jit_native_stream_context_get_default_abi
            as *const () as usize,
        native_stream_context_get_options: jit_abi::jit_native_stream_context_get_options_abi
            as *const () as usize,
        native_stream_context_set_default: jit_abi::jit_native_stream_context_set_default_abi
            as *const () as usize,
        native_stream_context_set_option: jit_abi::jit_native_stream_context_set_option_abi
            as *const () as usize,
        native_stream_context_set_options: jit_abi::jit_native_stream_context_set_options_abi
            as *const () as usize,
        native_stream_filter_append: jit_abi::jit_native_stream_filter_append_abi as *const ()
            as usize,
        native_stream_filter_prepend: jit_abi::jit_native_stream_filter_prepend_abi as *const ()
            as usize,
        native_stream_filter_remove: jit_abi::jit_native_stream_filter_remove_abi as *const ()
            as usize,
        native_stream_isatty: jit_abi::jit_native_stream_isatty_abi as *const () as usize,
        native_stream_set_timeout: jit_abi::jit_native_stream_set_timeout_abi as *const () as usize,
        native_chmod: jit_native_chmod_abi as *const () as usize,
        native_symlink: jit_native_symlink_abi as *const () as usize,
        native_readfile: jit_native_readfile_abi as *const () as usize,
        native_is_uploaded_file: jit_native_is_uploaded_file_abi as *const () as usize,
        native_tempnam: jit_native_tempnam_abi as *const () as usize,
        native_tmpfile: jit_native_tmpfile_abi as *const () as usize,
        native_filesize: jit_native_filesize_abi as *const () as usize,
        native_filemtime: jit_native_filemtime_abi as *const () as usize,
        native_file_get_contents: jit_native_file_get_contents_abi as *const () as usize,
        native_file_put_contents: jit_native_file_put_contents_abi as *const () as usize,
        native_rename: jit_native_rename_abi as *const () as usize,
        native_unlink: jit_native_unlink_abi as *const () as usize,
        native_mkdir: jit_native_mkdir_abi as *const () as usize,
        native_rmdir: jit_native_rmdir_abi as *const () as usize,
        native_touch: jit_native_touch_abi as *const () as usize,
        native_fopen: jit_native_fopen_abi as *const () as usize,
        native_fwrite: jit_native_fwrite_abi as *const () as usize,
        native_fclose: jit_native_fclose_abi as *const () as usize,
        native_fread: jit_native_fread_abi as *const () as usize,
        native_fgets: jit_native_fgets_abi as *const () as usize,
        native_fgetc: jit_native_fgetc_abi as *const () as usize,
        native_feof: jit_native_feof_abi as *const () as usize,
        native_fflush: jit_native_fflush_abi as *const () as usize,
        native_fseek: jit_native_fseek_abi as *const () as usize,
        native_ftell: jit_native_ftell_abi as *const () as usize,
        native_ftruncate: jit_native_ftruncate_abi as *const () as usize,
        native_rewind: jit_native_rewind_abi as *const () as usize,
        native_stream_get_contents: jit_native_stream_get_contents_abi as *const () as usize,
        native_stream_copy_to_stream: jit_native_stream_copy_to_stream_abi as *const () as usize,
        native_output_buffer: [
            jit_native_ob_start_abi as *const () as usize,
            jit_native_ob_get_clean_abi as *const () as usize,
            jit_native_ob_get_contents_abi as *const () as usize,
            jit_native_ob_get_flush_abi as *const () as usize,
            jit_native_ob_get_length_abi as *const () as usize,
            jit_native_ob_get_level_abi as *const () as usize,
            jit_native_ob_end_flush_abi as *const () as usize,
            jit_native_ob_end_clean_abi as *const () as usize,
        ],
        baseline_semantic_dispatch: helper_address!(
            jit_baseline_native_semantic_dispatch_abi,
            jit_baseline_native_semantic_dispatch_diagnostic_abi
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
        baseline_unary: helper_address!(
            jit_baseline_native_unary_abi,
            jit_abi::jit_baseline_native_unary_diagnostic_abi
        ),
        native_exact_unary: [
            jit_native_unary_plus_abi as *const () as usize,
            jit_native_unary_minus_abi as *const () as usize,
            jit_native_bit_not_abi as *const () as usize,
        ],
        baseline_binary: helper_address!(
            jit_baseline_native_binary_abi,
            jit_abi::jit_baseline_native_binary_diagnostic_abi
        ),
        native_array_union: jit_native_array_union_abi as *const () as usize,
        native_concat: jit_native_concat_abi as *const () as usize,
        native_string_bitwise: [
            jit_native_bit_and_abi as *const () as usize,
            jit_native_bit_or_abi as *const () as usize,
            jit_native_bit_xor_abi as *const () as usize,
        ],
        baseline_compare: helper_address!(
            jit_baseline_native_compare_abi,
            jit_abi::jit_baseline_native_compare_diagnostic_abi
        ),
        native_exact_compare: [
            jit_native_equal_abi as *const () as usize,
            jit_native_not_equal_abi as *const () as usize,
            jit_native_identical_abi as *const () as usize,
            jit_native_not_identical_abi as *const () as usize,
            jit_native_less_abi as *const () as usize,
            jit_native_less_equal_abi as *const () as usize,
            jit_native_greater_abi as *const () as usize,
            jit_native_greater_equal_abi as *const () as usize,
            jit_native_spaceship_abi as *const () as usize,
        ],
        baseline_cast: helper_address!(
            jit_baseline_native_cast_abi,
            jit_abi::jit_baseline_native_cast_diagnostic_abi
        ),
        native_echo: helper_address!(jit_native_echo_abi, jit_abi::jit_native_echo_diagnostic_abi),
        native_echo_bytes: jit_native_echo_bytes_abi as *const () as usize,
        native_float_to_string: jit_native_float_to_string_abi as *const () as usize,
        native_numeric_string: jit_native_numeric_string_abi as *const () as usize,
        native_fmod_f64: jit_native_fmod_f64_abi as *const () as usize,
        native_round_f64: jit_native_round_f64_abi as *const () as usize,
        native_pure_math: [
            jit_native_acos_f64_abi as *const () as usize,
            jit_native_acosh_f64_abi as *const () as usize,
            jit_native_asin_f64_abi as *const () as usize,
            jit_native_asinh_f64_abi as *const () as usize,
            jit_native_atan_f64_abi as *const () as usize,
            jit_native_atan2_f64_abi as *const () as usize,
            jit_native_atanh_f64_abi as *const () as usize,
            jit_native_cos_f64_abi as *const () as usize,
            jit_native_cosh_f64_abi as *const () as usize,
            jit_native_deg2rad_f64_abi as *const () as usize,
            jit_native_exp_f64_abi as *const () as usize,
            jit_native_expm1_f64_abi as *const () as usize,
            jit_native_fpow_f64_abi as *const () as usize,
            jit_native_hypot_f64_abi as *const () as usize,
            jit_native_log_f64_abi as *const () as usize,
            jit_native_log10_f64_abi as *const () as usize,
            jit_native_log1p_f64_abi as *const () as usize,
            jit_native_rad2deg_f64_abi as *const () as usize,
            jit_native_sin_f64_abi as *const () as usize,
            jit_native_sinh_f64_abi as *const () as usize,
            jit_native_tan_f64_abi as *const () as usize,
            jit_native_tanh_f64_abi as *const () as usize,
        ],
        native_base_conversion: [
            jit_native_base_convert_abi as *const () as usize,
            jit_native_bindec_abi as *const () as usize,
            jit_native_decbin_abi as *const () as usize,
            jit_native_dechex_abi as *const () as usize,
            jit_native_decoct_abi as *const () as usize,
            jit_native_hexdec_abi as *const () as usize,
            jit_native_octdec_abi as *const () as usize,
        ],
        native_intval_base: jit_native_intval_base_abi as *const () as usize,
        native_string_search_compare: [
            jit_abi::jit_native_strstr_abi as *const () as usize,
            jit_abi::jit_native_stristr_abi as *const () as usize,
            jit_abi::jit_native_strrchr_abi as *const () as usize,
            jit_abi::jit_native_strpbrk_abi as *const () as usize,
            jit_abi::jit_native_substr_compare_abi as *const () as usize,
            jit_abi::jit_native_strnatcmp_abi as *const () as usize,
            jit_abi::jit_native_strnatcasecmp_abi as *const () as usize,
        ],
        native_string_rewrite: [
            jit_abi::jit_native_ucwords_abi as *const () as usize,
            jit_abi::jit_native_str_pad_abi as *const () as usize,
            jit_abi::jit_native_strtr_abi as *const () as usize,
            jit_abi::jit_native_strip_tags_abi as *const () as usize,
            jit_abi::jit_native_substr_replace_abi as *const () as usize,
            jit_abi::jit_native_str_split_abi as *const () as usize,
            jit_abi::jit_native_version_compare_abi as *const () as usize,
        ],
        native_html_codec: [
            jit_abi::jit_native_htmlspecialchars_abi as *const () as usize,
            jit_abi::jit_native_htmlentities_abi as *const () as usize,
            jit_abi::jit_native_html_entity_decode_abi as *const () as usize,
            jit_abi::jit_native_htmlspecialchars_decode_abi as *const () as usize,
        ],
        native_url_query: [
            jit_abi::jit_native_parse_url_abi as *const () as usize,
            jit_abi::jit_native_parse_str_abi as *const () as usize,
            jit_abi::jit_native_http_build_query_abi as *const () as usize,
        ],
        native_array_aggregate: [
            jit_abi::jit_native_array_sum_abi as *const () as usize,
            jit_abi::jit_native_count_abi as *const () as usize,
            jit_abi::jit_native_sizeof_abi as *const () as usize,
        ],
        native_recursive_array: [
            jit_abi::jit_native_array_merge_recursive_abi as *const () as usize,
            jit_abi::jit_native_array_replace_recursive_abi as *const () as usize,
        ],
        native_array_sort: [
            jit_abi::jit_native_asort_abi as *const () as usize,
            jit_abi::jit_native_arsort_abi as *const () as usize,
            jit_abi::jit_native_ksort_abi as *const () as usize,
            jit_abi::jit_native_krsort_abi as *const () as usize,
            jit_abi::jit_native_natsort_abi as *const () as usize,
            jit_abi::jit_native_natcasesort_abi as *const () as usize,
            jit_abi::jit_native_sort_abi as *const () as usize,
            jit_abi::jit_native_rsort_abi as *const () as usize,
        ],
        native_array_multisort: jit_abi::jit_native_array_multisort_abi as *const () as usize,
        native_object_identity: [
            jit_abi::jit_native_spl_object_hash_abi as *const () as usize,
            jit_abi::jit_native_spl_object_id_abi as *const () as usize,
        ],
        native_serialization: [
            jit_abi::jit_native_serialize_abi as *const () as usize,
            jit_abi::jit_native_unserialize_abi as *const () as usize,
        ],
        native_tokenizer: [
            jit_abi::jit_native_token_get_all_abi as *const () as usize,
            jit_abi::jit_native_token_name_abi as *const () as usize,
        ],
        native_mbstring: [
            jit_abi::jit_native_mb_detect_encoding_abi as *const () as usize,
            jit_abi::jit_native_mb_check_encoding_abi as *const () as usize,
            jit_abi::jit_native_mb_convert_encoding_abi as *const () as usize,
            jit_abi::jit_native_mb_internal_encoding_abi as *const () as usize,
            jit_abi::jit_native_mb_list_encodings_abi as *const () as usize,
            jit_abi::jit_native_mb_encoding_aliases_abi as *const () as usize,
            jit_abi::jit_native_mb_substitute_character_abi as *const () as usize,
            jit_abi::jit_native_mb_strlen_abi as *const () as usize,
            jit_abi::jit_native_mb_strtolower_abi as *const () as usize,
            jit_abi::jit_native_mb_strtoupper_abi as *const () as usize,
            jit_abi::jit_native_mb_stripos_abi as *const () as usize,
            jit_abi::jit_native_mb_strpos_abi as *const () as usize,
            jit_abi::jit_native_mb_strripos_abi as *const () as usize,
            jit_abi::jit_native_mb_strrpos_abi as *const () as usize,
            jit_abi::jit_native_mb_substr_count_abi as *const () as usize,
            jit_abi::jit_native_mb_substr_abi as *const () as usize,
            jit_abi::jit_native_mb_strcut_abi as *const () as usize,
            jit_abi::jit_native_mb_strwidth_abi as *const () as usize,
            jit_abi::jit_native_mb_strimwidth_abi as *const () as usize,
            jit_abi::jit_native_mb_convert_case_abi as *const () as usize,
            jit_abi::jit_native_mb_ucfirst_abi as *const () as usize,
            jit_abi::jit_native_mb_lcfirst_abi as *const () as usize,
            jit_abi::jit_native_mb_ord_abi as *const () as usize,
            jit_abi::jit_native_mb_chr_abi as *const () as usize,
            jit_abi::jit_native_mb_parse_str_abi as *const () as usize,
        ],
        native_bcmath: [
            jit_abi::jit_native_bcadd_abi as *const () as usize,
            jit_abi::jit_native_bccomp_abi as *const () as usize,
            jit_abi::jit_native_bcdiv_abi as *const () as usize,
            jit_abi::jit_native_bcmod_abi as *const () as usize,
            jit_abi::jit_native_bcmul_abi as *const () as usize,
            jit_abi::jit_native_bcpow_abi as *const () as usize,
            jit_abi::jit_native_bcpowmod_abi as *const () as usize,
            jit_abi::jit_native_bcscale_abi as *const () as usize,
            jit_abi::jit_native_bcsqrt_abi as *const () as usize,
            jit_abi::jit_native_bcsub_abi as *const () as usize,
        ],
        native_filter: [
            jit_abi::jit_native_filter_input_abi as *const () as usize,
            jit_abi::jit_native_filter_has_var_abi as *const () as usize,
            jit_abi::jit_native_filter_input_array_abi as *const () as usize,
            jit_abi::jit_native_filter_var_array_abi as *const () as usize,
            jit_abi::jit_native_filter_list_abi as *const () as usize,
            jit_abi::jit_native_filter_id_abi as *const () as usize,
            jit_abi::jit_native_filter_var_abi as *const () as usize,
        ],
        native_session: [
            jit_abi::jit_native_session_abort_abi as *const () as usize,
            jit_abi::jit_native_session_cache_expire_abi as *const () as usize,
            jit_abi::jit_native_session_cache_limiter_abi as *const () as usize,
            jit_abi::jit_native_session_commit_abi as *const () as usize,
            jit_abi::jit_native_session_destroy_abi as *const () as usize,
            jit_abi::jit_native_session_gc_abi as *const () as usize,
            jit_abi::jit_native_session_decode_abi as *const () as usize,
            jit_abi::jit_native_session_encode_abi as *const () as usize,
            jit_abi::jit_native_session_create_id_abi as *const () as usize,
            jit_abi::jit_native_session_get_cookie_params_abi as *const () as usize,
            jit_abi::jit_native_session_id_abi as *const () as usize,
            jit_abi::jit_native_session_module_name_abi as *const () as usize,
            jit_abi::jit_native_session_name_abi as *const () as usize,
            jit_abi::jit_native_session_regenerate_id_abi as *const () as usize,
            jit_abi::jit_native_session_register_shutdown_abi as *const () as usize,
            jit_abi::jit_native_session_reset_abi as *const () as usize,
            jit_abi::jit_native_session_save_path_abi as *const () as usize,
            jit_abi::jit_native_session_set_cookie_params_abi as *const () as usize,
            jit_abi::jit_native_session_set_save_handler_abi as *const () as usize,
            jit_abi::jit_native_session_start_abi as *const () as usize,
            jit_abi::jit_native_session_status_abi as *const () as usize,
            jit_abi::jit_native_session_unset_abi as *const () as usize,
            jit_abi::jit_native_session_write_close_abi as *const () as usize,
        ],
        native_object_vars: [
            jit_abi::jit_native_get_object_vars_abi as *const () as usize,
            jit_abi::jit_native_get_mangled_object_vars_abi as *const () as usize,
        ],
        native_class_metadata: [
            jit_abi::jit_native_get_class_methods_abi as *const () as usize,
            jit_abi::jit_native_get_class_vars_abi as *const () as usize,
        ],
        native_class_lineage: [
            jit_abi::jit_native_get_parent_class_abi as *const () as usize,
            jit_abi::jit_native_is_subclass_of_abi as *const () as usize,
            jit_abi::jit_native_is_a_abi as *const () as usize,
            jit_abi::jit_native_class_implements_abi as *const () as usize,
        ],
        native_extension_query: [
            jit_abi::jit_native_extension_loaded_abi as *const () as usize,
            jit_abi::jit_native_get_loaded_extensions_abi as *const () as usize,
        ],
        native_memory_query: [
            jit_abi::jit_native_memory_get_usage_abi as *const () as usize,
            jit_abi::jit_native_memory_get_peak_usage_abi as *const () as usize,
        ],
        native_gc: [
            jit_abi::jit_native_gc_collect_cycles_abi as *const () as usize,
            jit_abi::jit_native_gc_disable_abi as *const () as usize,
            jit_abi::jit_native_gc_enable_abi as *const () as usize,
            jit_abi::jit_native_gc_enabled_abi as *const () as usize,
            jit_abi::jit_native_gc_mem_caches_abi as *const () as usize,
            jit_abi::jit_native_gc_status_abi as *const () as usize,
        ],
        native_resource_query: [
            jit_abi::jit_native_get_resource_id_abi as *const () as usize,
            jit_abi::jit_native_get_resource_type_abi as *const () as usize,
            jit_abi::jit_native_get_resources_abi as *const () as usize,
        ],
        native_error_state: [
            jit_abi::jit_native_error_get_last_abi as *const () as usize,
            jit_abi::jit_native_error_clear_last_abi as *const () as usize,
        ],
        native_settype: jit_abi::jit_native_settype_abi as *const () as usize,
        native_configuration: [
            jit_abi::jit_native_ini_get_abi as *const () as usize,
            jit_abi::jit_native_ini_get_all_abi as *const () as usize,
            jit_abi::jit_native_get_cfg_var_abi as *const () as usize,
            jit_abi::jit_native_get_include_path_abi as *const () as usize,
            jit_abi::jit_native_ini_set_abi as *const () as usize,
            jit_abi::jit_native_set_include_path_abi as *const () as usize,
            jit_abi::jit_native_date_default_timezone_get_abi as *const () as usize,
            jit_abi::jit_native_date_default_timezone_set_abi as *const () as usize,
        ],
        native_http_response: [
            jit_abi::jit_native_header_abi as *const () as usize,
            jit_abi::jit_native_header_remove_abi as *const () as usize,
            jit_abi::jit_native_headers_list_abi as *const () as usize,
            jit_abi::jit_native_headers_sent_abi as *const () as usize,
            jit_abi::jit_native_http_response_code_abi as *const () as usize,
        ],
        native_cookie: [
            jit_abi::jit_native_setcookie_abi as *const () as usize,
            jit_abi::jit_native_setrawcookie_abi as *const () as usize,
        ],
        native_clock: [
            jit_abi::jit_native_time_abi as *const () as usize,
            jit_abi::jit_native_microtime_abi as *const () as usize,
            jit_abi::jit_native_hrtime_abi as *const () as usize,
        ],
        native_date: [
            jit_abi::jit_native_checkdate_abi as *const () as usize,
            jit_abi::jit_native_date_abi as *const () as usize,
            jit_abi::jit_native_gmdate_abi as *const () as usize,
            jit_abi::jit_native_strtotime_abi as *const () as usize,
            jit_abi::jit_native_mktime_abi as *const () as usize,
            jit_abi::jit_native_gmmktime_abi as *const () as usize,
            jit_abi::jit_native_timezone_identifiers_list_abi as *const () as usize,
        ],
        native_random: [
            jit_abi::jit_native_random_bytes_abi as *const () as usize,
            jit_abi::jit_native_random_int_abi as *const () as usize,
            jit_abi::jit_native_rand_abi as *const () as usize,
            jit_abi::jit_native_mt_rand_abi as *const () as usize,
            jit_abi::jit_native_getrandmax_abi as *const () as usize,
            jit_abi::jit_native_mt_getrandmax_abi as *const () as usize,
            jit_abi::jit_native_array_rand_abi as *const () as usize,
            jit_abi::jit_native_shuffle_abi as *const () as usize,
        ],
        native_request_query: [
            jit_abi::jit_native_sys_get_temp_dir_abi as *const () as usize,
            jit_abi::jit_native_getcwd_abi as *const () as usize,
            jit_abi::jit_native_getenv_abi as *const () as usize,
            jit_abi::jit_native_php_sapi_name_abi as *const () as usize,
            jit_abi::jit_native_php_uname_abi as *const () as usize,
            jit_abi::jit_native_get_current_user_abi as *const () as usize,
            jit_abi::jit_native_get_included_files_abi as *const () as usize,
            jit_abi::jit_native_chdir_abi as *const () as usize,
            jit_abi::jit_native_umask_abi as *const () as usize,
            jit_abi::jit_native_clearstatcache_abi as *const () as usize,
        ],
        native_declaration_inventory: [
            jit_abi::jit_native_get_defined_functions_abi as *const () as usize,
            jit_abi::jit_native_get_declared_classes_abi as *const () as usize,
            jit_abi::jit_native_get_declared_interfaces_abi as *const () as usize,
            jit_abi::jit_native_get_declared_traits_abi as *const () as usize,
        ],
        native_constant_inventory: jit_abi::jit_native_get_defined_constants_abi as *const ()
            as usize,
        native_compact: jit_abi::jit_native_compact_abi as *const () as usize,
        native_frame_introspection: [
            jit_abi::jit_native_func_num_args_abi as *const () as usize,
            jit_abi::jit_native_func_get_arg_abi as *const () as usize,
            jit_abi::jit_native_func_get_args_abi as *const () as usize,
        ],
        native_network_address: [
            jit_native_ip2long_abi as *const () as usize,
            jit_native_long2ip_abi as *const () as usize,
            jit_native_inet_pton_abi as *const () as usize,
            jit_native_inet_ntop_abi as *const () as usize,
        ],
        native_compression_codec: [
            jit_native_gzencode_abi as *const () as usize,
            jit_native_gzcompress_abi as *const () as usize,
            jit_native_gzdeflate_abi as *const () as usize,
            jit_native_gzdecode_abi as *const () as usize,
            jit_native_gzuncompress_abi as *const () as usize,
            jit_native_gzinflate_abi as *const () as usize,
            jit_native_zlib_decode_abi as *const () as usize,
            jit_native_zlib_encode_abi as *const () as usize,
        ],
        native_array_cast: jit_native_array_cast_abi as *const () as usize,
        native_int_cast: jit_native_int_cast_abi as *const () as usize,
        native_float_cast: jit_native_float_cast_abi as *const () as usize,
        native_string_cast: jit_native_string_cast_abi as *const () as usize,
        native_callback_return_string: jit_native_callback_return_string_abi as *const () as usize,
        native_object_cast: jit_native_object_cast_abi as *const () as usize,
        native_object_class_name: jit_native_object_class_name_abi as *const () as usize,
        native_acquire_callable: jit_native_acquire_callable_abi as *const () as usize,
        native_is_callable: jit_native_is_callable_abi as *const () as usize,
        native_callback_handler: [
            jit_native_set_error_handler_abi as *const () as usize,
            jit_native_restore_error_handler_abi as *const () as usize,
            jit_native_set_exception_handler_abi as *const () as usize,
            jit_native_restore_exception_handler_abi as *const () as usize,
            jit_native_get_exception_handler_abi as *const () as usize,
        ],
        native_autoload_callback: [
            jit_native_spl_autoload_register_abi as *const () as usize,
            jit_native_spl_autoload_unregister_abi as *const () as usize,
            jit_native_spl_autoload_functions_abi as *const () as usize,
        ],
        native_register_shutdown_function: jit_native_register_shutdown_function_abi as *const ()
            as usize,
        native_resolve_callable: jit_native_resolve_callable_abi as *const () as usize,
        native_prepared_object_new: jit_native_prepared_object_new_abi as *const () as usize,
        native_prepared_exception_new: jit_native_prepared_exception_new_abi as *const () as usize,
        native_prepared_closure_new: jit_native_prepared_closure_new_abi as *const () as usize,
        native_plain_object_clone: jit_native_plain_object_clone_abi as *const () as usize,
        native_dynamic_property_slot: jit_native_dynamic_property_slot_abi as *const () as usize,
        native_dynamic_property_test_slot: jit_native_dynamic_property_test_slot_abi as *const ()
            as usize,
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
#[path = "tests.rs"]
mod tests;
