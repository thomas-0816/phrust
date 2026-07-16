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
    NativeExecutionContext, activate_native_context, jit_native_argument_check_abi,
    jit_native_array_fetch_abi, jit_native_array_insert_abi, jit_native_array_new_abi,
    jit_native_array_spread_abi, jit_native_array_unset_abi, jit_native_binary_abi,
    jit_native_call_dispatch_abi, jit_native_cast_abi, jit_native_compare_abi,
    jit_native_constant_fetch_abi, jit_native_dynamic_code_abi, jit_native_echo_abi,
    jit_native_exception_new_abi, jit_native_execution_poll_abi, jit_native_foreach_cleanup_abi,
    jit_native_foreach_init_abi, jit_native_foreach_next_abi, jit_native_frame_alloc_abi,
    jit_native_frame_release_abi, jit_native_function_resolve_abi, jit_native_local_fetch_abi,
    jit_native_local_store_abi, jit_native_object_clone_abi, jit_native_object_clone_with_abi,
    jit_native_object_new_abi, jit_native_property_assign_abi, jit_native_property_fetch_abi,
    jit_native_reference_bind_abi, jit_native_return_check_abi, jit_native_runtime_fatal_abi,
    jit_native_truthy_abi, jit_native_unary_abi, jit_native_value_lifecycle_abi,
};
use php_runtime::api::{OutputBuffer, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn validate_native_class_table(unit: &php_ir::IrUnit) -> Result<(), String> {
    let find_class = |name: &str| {
        let normalized = php_ir::module::normalize_class_name(name);
        unit.classes.iter().find(|class| class.name == normalized)
    };
    for class in unit
        .classes
        .iter()
        .filter(|class| !class.flags.is_conditional)
    {
        if let Some(parent_name) = class.parent.as_deref()
            && let Some(parent) = find_class(parent_name)
        {
            if parent.flags.is_final || parent.flags.is_enum {
                return Err(format!(
                    "Class {} cannot extend final class {}",
                    class.display_name, parent.display_name
                ));
            }
            for method in &class.methods {
                let mut ancestor = Some(parent);
                while let Some(current) = ancestor {
                    if current.methods.iter().any(|candidate| {
                        candidate.name.eq_ignore_ascii_case(&method.name)
                            && candidate.flags.is_final
                    }) {
                        return Err(format!(
                            "Cannot override final method {}::{}()",
                            current.display_name, method.name
                        ));
                    }
                    ancestor = current.parent.as_deref().and_then(&find_class);
                }
            }
        }

        if class.flags.is_abstract || class.flags.is_interface || class.flags.is_trait {
            continue;
        }
        let implements = |name: &str| {
            let mut current = Some(class);
            while let Some(candidate) = current {
                if let Some(method) = candidate
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case(name))
                {
                    return Some(method);
                }
                current = candidate.parent.as_deref().and_then(&find_class);
            }
            None
        };
        let mut required = Vec::new();
        let mut ancestor = class.parent.as_deref().and_then(&find_class);
        while let Some(current) = ancestor {
            required.extend(
                current
                    .methods
                    .iter()
                    .filter(|method| method.flags.is_abstract)
                    .map(|method| (current, method)),
            );
            ancestor = current.parent.as_deref().and_then(&find_class);
        }
        for interface_name in &class.interfaces {
            if let Some(interface) = find_class(interface_name) {
                required.extend(interface.methods.iter().map(|method| (interface, method)));
            }
        }
        for (owner, method) in required {
            let Some(implementation) = implements(&method.name) else {
                return Err(format!(
                    "Class {} contains an abstract method {}::{}()",
                    class.display_name, owner.display_name, method.name
                ));
            };
            if implementation.flags.is_abstract {
                return Err(format!(
                    "Class {} contains an abstract method {}::{}()",
                    class.display_name, owner.display_name, method.name
                ));
            }
            if owner.flags.is_interface
                && (implementation.flags.is_private || implementation.flags.is_protected)
            {
                return Err(format!(
                    "Access level to {}::{}() must be public",
                    class.display_name, method.name
                ));
            }
        }
    }
    Ok(())
}

/// Process-owned state shared by native request coordinators.
#[derive(Clone, Debug)]
pub struct VmWorkerState {
    native_compiles: Arc<native_compile_cache::NativeCompileCache>,
    loaded_native_units: Arc<native_compile_cache::LoadedNativeUnitRegistry>,
}

static PROCESS_LOADED_NATIVE_UNITS: std::sync::OnceLock<
    Arc<native_compile_cache::LoadedNativeUnitRegistry>,
> = std::sync::OnceLock::new();

impl Default for VmWorkerState {
    fn default() -> Self {
        Self {
            native_compiles: Arc::new(native_compile_cache::NativeCompileCache::default()),
            loaded_native_units: Arc::clone(PROCESS_LOADED_NATIVE_UNITS.get_or_init(|| {
                Arc::new(native_compile_cache::LoadedNativeUnitRegistry::default())
            })),
        }
    }
}

impl VmWorkerState {
    #[must_use]
    pub fn new(_tiering: crate::tiering::TieringOptions) -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn isolated_for_restart_test() -> Self {
        Self {
            native_compiles: Arc::new(native_compile_cache::NativeCompileCache::default()),
            loaded_native_units: Arc::new(native_compile_cache::LoadedNativeUnitRegistry::default()),
        }
    }

    /// Returns worker-stable native compile-record cache counters.
    #[must_use]
    pub fn native_compile_cache_stats(&self) -> NativeCompileCacheStats {
        self.native_compiles.stats()
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
        let function_metadata = unit
            .unit()
            .functions
            .get(function.index())
            .ok_or_else(|| format!("native function {} is missing", function.raw()))?;
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
            options.native_optimization.is_optimizing(),
            external_signatures_hash,
        );
        self.native_compiles.get_or_compile(key, || {
            if let Ok(manager) = php_jit::global_code_manager()
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
                unit.prepared_ir_fingerprint(),
                &format!(
                    "{}-external-signatures-{external_signatures_hash:016x}",
                    unit.prepared_dependency_identity()
                ),
                external_signatures,
            )
        })
    }
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
        validate_native_class_table(unit.unit())?;
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
        .with_opt_level(if self.options.native_optimization.is_optimizing() {
            2
        } else {
            0
        });
        let result = compiler
            .compile_function_with_runtime_helpers(
                unit.unit(),
                function,
                request,
                runtime_helper_addresses(),
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
        if let Err(error) = validate_native_class_table(unit.unit()) {
            return VmResult::compile_error(output, error);
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
        let cache_identity = cache
            .as_ref()
            .and_then(|_| native_cache_identity(&unit, entry, &self.options).ok());
        let mut cached_compile_records = None;
        let mut cached_compile_error = None;

        if let (Some(cache), Some(identity)) = (&cache, &cache_identity) {
            if cache.config().mode.can_write() {
                let cache_started = Instant::now();
                let result = self.worker_state.get_or_load_native_unit(identity, || {
                    cache
                        .get_or_compile(identity, resolve_native_cache_helper, || {
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
                                    return Err(php_jit::NativeCacheError::InvalidHeader(error));
                                }
                            };
                            if disposition.compiled() {
                                native_compile_time += compile_started.elapsed();
                            }
                            let image = cache_image(identity.clone(), entry, &records);
                            cached_compile_records = Some(records);
                            image
                        })
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
            } else if cache.config().mode.can_read() {
                let cache_started = Instant::now();
                let loaded = self.worker_state.get_or_load_native_unit(identity, || {
                    cache.load(identity, resolve_native_cache_helper)
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
        let mut context = NativeExecutionContext::new(
            &unit,
            unit.cache_identity(),
            &self.options,
            &self.worker_state,
            output,
            native_entries,
        );
        context.install_root_dynamic_unit(unit.clone());
        let native_execution_started_at =
            self.options.collect_counters.then(std::time::Instant::now);
        context.record_native_direct_calls(handle);
        let guard = activate_native_context(&mut context);
        let outcome = handle.invoke_i64_with_native_unwind(
            &[],
            php_jit::JIT_RUNTIME_ABI_HASH,
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
        context.publish_include_globals();
        let native_execution_time_nanos = native_execution_started_at.map_or(0, |started_at| {
            started_at.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
        });
        let mut runtime_counters = context.runtime_counters();
        runtime_counters.native_execution_entries =
            runtime_counters.native_execution_entries.saturating_add(1);
        runtime_counters.native_region_entries =
            runtime_counters.native_region_entries.saturating_add(1);
        runtime_counters.native_execution_time_nanos = native_execution_time_nanos;
        let http_response = std::mem::take(&mut context.http_response);
        let upload_registry = std::mem::take(&mut context.upload_registry);
        let session = std::mem::take(&mut context.session);
        let process_exit_terminates_process = context.process_exit_terminates_process();
        let mut result = if let Some(throwable) = shutdown_throwable {
            native_uncaught_throwable_result(context.output, Some(throwable))
        } else if let Some(error) = shutdown_error {
            VmResult::runtime_error(
                context.output,
                context.diagnostic,
                format!("E_NATIVE_SHUTDOWN: {error}"),
            )
        } else if exception_handled {
            VmResult::success(context.output, Some(Value::Null))
        } else {
            match outcome {
                Ok(php_jit::JitI64InvokeOutcome::Returned(value)) => {
                    match context.decode_result(value) {
                        Ok(value) => {
                            let mut result = VmResult::success(context.output, Some(value));
                            result.diagnostics.extend(context.diagnostic);
                            result
                        }
                        Err(error) => VmResult::runtime_error(
                            context.output,
                            context.diagnostic,
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
                    VmResult::success_exit(context.output, exit_code)
                }
                Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
                    if status == php_jit::JitCallStatus::THROW.0 as i32 =>
                {
                    let throwable = context.decode_result(value).ok();
                    native_uncaught_throwable_result(context.output, throwable)
                }
                Ok(php_jit::JitI64InvokeOutcome::SideExit { status, .. })
                    if status == php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32 =>
                {
                    let message = context
                        .diagnostic
                        .as_ref()
                        .map_or("native runtime operation failed", |diagnostic| {
                            diagnostic.message()
                        })
                        .to_owned();
                    if context.diagnostic.as_ref().is_some_and(|diagnostic| {
                        diagnostic.severity() == php_runtime::api::RuntimeSeverity::FatalError
                    }) && context
                        .output
                        .as_bytes()
                        .windows(b"Fatal error".len())
                        .any(|window| window == b"Fatal error")
                    {
                        VmResult::fatal(context.output, context.diagnostic, message)
                    } else {
                        VmResult::runtime_error(context.output, context.diagnostic, message)
                    }
                }
                Ok(php_jit::JitI64InvokeOutcome::SideExit { status, .. })
                    if status == php_jit::JitCallStatus::RETURN_REFERENCE.0 as i32 =>
                {
                    VmResult::success(context.output, None)
                }
                Ok(php_jit::JitI64InvokeOutcome::SideExit { status, .. }) => {
                    VmResult::runtime_error(
                        context.output,
                        context.diagnostic,
                        format!("native entry returned status {status}"),
                    )
                }
                Err(error) => VmResult::compile_error(
                    context.output,
                    format!("E_NATIVE_ENTRY: native entry invocation failed: {error:?}"),
                ),
            }
        };
        result.process_exit_terminates_process = process_exit_terminates_process;
        result.http_response = Some(Box::new(http_response));
        result.upload_registry = Some(Box::new(upload_registry));
        result.session = Some(Box::new(session));
        if self.options.collect_counters {
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
        result
    }
}

pub(super) fn compile_native_function_graph(
    unit: &php_ir::IrUnit,
    function: php_ir::FunctionId,
    options: &VmOptions,
    ir_fingerprint: &str,
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
                .with_dependency_identity(dependency_identity)
                .with_external_function_signatures(external_signatures.to_vec())
                .with_opt_level(if options.native_optimization.is_optimizing() {
                    2
                } else {
                    0
                }),
            runtime_helper_addresses(),
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
) -> Result<php_jit::NativeCacheIdentity, php_jit::CraneliftHostIsaError> {
    let isa = php_jit::cranelift_host_isa_identity()?;
    let optimization_tier = options.native_optimization.as_str().to_owned();
    Ok(php_jit::NativeCacheIdentity {
        source_hash: format!(
            "compiled-source-v2-{:016x}-function-{}",
            unit.artifact_identity(),
            function.raw()
        ),
        ir_hash: unit.prepared_ir_fingerprint().to_owned(),
        dependency_graph_hash: unit.prepared_dependency_identity().to_owned(),
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
        optimization_config_hash: u64::from(options.native_optimization.is_optimizing()),
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

fn runtime_helper_addresses() -> php_jit::JitRuntimeHelperAddresses {
    php_jit::JitRuntimeHelperAddresses {
        native_call_dispatch: jit_native_call_dispatch_abi as *const () as usize,
        native_function_resolve: jit_native_function_resolve_abi as *const () as usize,
        native_frame_alloc: jit_native_frame_alloc_abi as *const () as usize,
        native_frame_release: jit_native_frame_release_abi as *const () as usize,
        native_dynamic_code: jit_native_dynamic_code_abi as *const () as usize,
        native_unary: jit_native_unary_abi as *const () as usize,
        native_binary: jit_native_binary_abi as *const () as usize,
        native_compare: jit_native_compare_abi as *const () as usize,
        native_cast: jit_native_cast_abi as *const () as usize,
        native_echo: jit_native_echo_abi as *const () as usize,
        native_local_fetch: jit_native_local_fetch_abi as *const () as usize,
        native_local_store: jit_native_local_store_abi as *const () as usize,
        native_value_lifecycle: jit_native_value_lifecycle_abi as *const () as usize,
        native_reference_bind: jit_native_reference_bind_abi as *const () as usize,
        native_argument_check: jit_native_argument_check_abi as *const () as usize,
        native_return_check: jit_native_return_check_abi as *const () as usize,
        native_exception_new: jit_native_exception_new_abi as *const () as usize,
        native_array_new: jit_native_array_new_abi as *const () as usize,
        native_object_new: jit_native_object_new_abi as *const () as usize,
        native_property_fetch: jit_native_property_fetch_abi as *const () as usize,
        native_property_assign: jit_native_property_assign_abi as *const () as usize,
        native_object_clone: jit_native_object_clone_abi as *const () as usize,
        native_object_clone_with: jit_native_object_clone_with_abi as *const () as usize,
        native_array_insert: jit_native_array_insert_abi as *const () as usize,
        native_array_fetch: jit_native_array_fetch_abi as *const () as usize,
        native_array_unset: jit_native_array_unset_abi as *const () as usize,
        native_array_spread: jit_native_array_spread_abi as *const () as usize,
        native_foreach_init: jit_native_foreach_init_abi as *const () as usize,
        native_foreach_next: jit_native_foreach_next_abi as *const () as usize,
        native_foreach_cleanup: jit_native_foreach_cleanup_abi as *const () as usize,
        native_constant_fetch: jit_native_constant_fetch_abi as *const () as usize,
        native_truthy: jit_native_truthy_abi as *const () as usize,
        native_runtime_fatal: jit_native_runtime_fatal_abi as *const () as usize,
        native_execution_poll: jit_native_execution_poll_abi as *const () as usize,
    }
}

fn resolve_native_cache_helper(stable_id: u32) -> Option<usize> {
    php_jit::resolve_helper_address(
        php_runtime::api::JitHelperId(stable_id),
        runtime_helper_addresses(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use php_ir::builder::IrBuilder;
    use php_ir::{
        ClassEntry, ClassFlags, ClassId, ClassMethodEntry, ClassMethodFlags, FunctionFlags,
        InstructionKind, IrConstant, IrParam, IrReturnType, IrSpan, Operand, UnitId,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

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
        let mut builder = IrBuilder::new(UnitId::new(993));
        let file = builder.add_file("native-direct-counter.php");
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
        let string = builder.intern_constant(IrConstant::String("phrust".to_owned()));
        let function = builder.start_function("main", FunctionFlags::default(), span);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let block = builder.append_block(function);
        let result = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::CallFunction {
                dst: result,
                name: "strlen".to_owned(),
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
        assert!(counters.native_frame_arena_high_water_bytes > 0);
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
                php_jit::NativeIndirectionState::Unpublished => unpublished += 1,
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
    fn worker_registry_reuses_loaded_artifact_without_remapping_file() {
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
        let unit = returning_unit(91);
        let options = VmOptions {
            native_cache: php_jit::NativeCacheMode::ReadWrite,
            native_cache_dir: directory.clone(),
            ..VmOptions::default()
        };
        let first = Vm::with_options_and_worker_state(options.clone(), worker.clone())
            .execute(unit.clone());
        assert_eq!(first.return_value, Some(Value::Int(91)), "{first:#?}");

        for entry in std::fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|extension| extension == "pna") {
                std::fs::remove_file(path).unwrap();
            }
        }

        let second = Vm::with_options_and_worker_state(options, worker.clone()).execute(unit);
        assert_eq!(second.return_value, Some(Value::Int(91)), "{second:#?}");
        assert_eq!(second.native_compile_nanos, 0);
        let loaded = worker.loaded_native_unit_stats();
        assert_eq!(loaded.maps.saturating_sub(before.maps), 1);
        assert_eq!(
            loaded
                .entry_table_constructions
                .saturating_sub(before.entry_table_constructions),
            1
        );
        assert_eq!(loaded.hits.saturating_sub(before.hits), 1);
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
