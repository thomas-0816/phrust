use crate::config::{BUILTIN_SERVER_REWRITE_PREFIX_QUERY_ENV, ServerPerfAblation};
use crate::{
    access_log::AccessLogger,
    metrics::ServerMetrics,
    perf_trace::PerfTraceWriter,
    persistent_metadata::PersistentMetadataStats,
    request_profile::RequestProfileWriter,
    server::{PreloadError, ServerError},
};
use crate::{multipart::MultipartConfig, routing::RouteConfig, session_store::SessionStore};
use php_diagnostics::DiagnosticOutputFormat;
use php_executor::{
    CompiledScriptCache, CompiledScriptCacheLookup, EngineProfileName, ExecutorIncludeCompiler,
    IncludeCache, IncludeLoader, OptimizationLevel, PhpExecutionError, PhpExecutor,
    PhpExecutorOptions, PhpScriptCacheInput,
};
use php_vm::api::{CacheInstanceId, InlineCacheMode, VmError, VmWorkerState};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::Semaphore;
use tracing::warn;

#[derive(Clone, Debug)]
pub(crate) struct AppState {
    pub(crate) route_config: RouteConfig,
    pub(crate) request: RequestRuntimeConfig,
    pub(crate) concurrency: ConcurrencyServices,
    pub(crate) observability: ObservabilityState,
    pub(crate) capabilities: CapabilityState,
    pub(crate) sessions: SessionServices,
    pub(crate) transport: RequestTransport,
    pub(crate) services: RuntimeServices,
}

#[derive(Clone, Debug)]
pub(crate) struct RequestRuntimeConfig {
    pub(crate) max_body_bytes: usize,
    pub(crate) multipart_config: MultipartConfig,
    pub(crate) request_timeout: Duration,
    pub(crate) execution_time_limit: Option<Duration>,
}

#[derive(Clone, Debug)]
pub(crate) struct ConcurrencyServices {
    pub(crate) in_flight: Arc<Semaphore>,
    pub(crate) max_in_flight: usize,
    pub(crate) cpu_execution: Arc<Semaphore>,
    pub(crate) cpu_execution_limit: usize,
    /// Pinned PHP execution threads; sized to `cpu_execution_limit` so the
    /// pool and the CPU semaphore describe the same concurrency budget.
    pub(crate) php_workers: Arc<crate::worker_pool::PhpWorkerPool>,
}

#[derive(Clone, Debug)]
pub(crate) struct ObservabilityState {
    pub(crate) metrics_token: Option<String>,
    pub(crate) access_log: Option<Arc<AccessLogger>>,
    pub(crate) perf_trace: Option<Arc<PerfTraceWriter>>,
    pub(crate) perf_trace_vm_counters: bool,
    pub(crate) request_profile: Option<Arc<RequestProfileWriter>>,
    pub(crate) request_profile_vm_counters: bool,
    pub(crate) request_profile_trigger_header: bool,
    pub(crate) debug: bool,
    pub(crate) error_format: DiagnosticOutputFormat,
    pub(crate) debug_log: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub(crate) struct CapabilityState {
    pub(crate) network_requests_enabled: bool,
    pub(crate) env_snapshot: Arc<Vec<(String, String)>>,
}

#[derive(Clone, Debug)]
pub(crate) struct SessionServices {
    pub(crate) config: SessionConfig,
    pub(crate) session_store: Arc<SessionStore>,
}

#[derive(Clone, Debug)]
pub(crate) struct RequestTransport {
    pub(crate) local_addr: SocketAddr,
    pub(crate) request_scheme: &'static str,
    pub(crate) http3_alt_svc: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct RuntimeServices {
    pub(crate) metrics: Arc<ServerMetrics>,
    pub(crate) engine: Arc<ServerEngineState>,
    pub(crate) request_counter: Arc<AtomicU64>,
}

pub(crate) fn server_env_snapshot<I>(env: I) -> Arc<Vec<(String, String)>>
where
    I: IntoIterator<Item = (String, String)>,
{
    let mut env = env
        .into_iter()
        .filter(|(name, _)| name != BUILTIN_SERVER_REWRITE_PREFIX_QUERY_ENV)
        .collect::<Vec<_>>();
    env.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    Arc::new(env)
}
#[derive(Clone, Debug)]
pub(crate) struct ServerEngineState {
    pub(crate) engine_profile: EngineProfileName,
    pub(crate) native_cache: php_vm::api::NativeCacheMode,
    pub(crate) native_cache_dir: PathBuf,
    pub(crate) script_cache: Arc<CompiledScriptCache>,
    pub(crate) include_cache: Arc<IncludeCache>,
    pub(crate) compile_optimization_level: OptimizationLevel,
    perf_ablation: ServerPerfAblation,
    worker_state: VmWorkerState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RequestExecutorCacheKey {
    engine_profile: EngineProfileName,
    native_cache: php_vm::api::NativeCacheMode,
    native_cache_dir: PathBuf,
    include_cache_id: CacheInstanceId,
    compile_optimization_level: OptimizationLevel,
    perf_ablation: ServerPerfAblation,
}

impl ServerEngineState {
    pub(crate) fn new(
        engine_profile: EngineProfileName,
        native_cache: php_vm::api::NativeCacheMode,
        native_cache_dir: PathBuf,
        script_cache: Arc<CompiledScriptCache>,
        include_cache: Arc<IncludeCache>,
        perf_ablation: ServerPerfAblation,
    ) -> Self {
        let base_options = if engine_profile == EngineProfileName::Default {
            PhpExecutorOptions::default_native_runtime()
        } else {
            PhpExecutorOptions::for_profile(engine_profile)
        };
        let compile_optimization_level = if perf_ablation.disable_include_o2 {
            OptimizationLevel::O0
        } else {
            base_options.optimization_level
        };
        let worker_state =
            VmWorkerState::new_with_background_tiering(base_options.vm_options.tiering.clone());
        Self {
            engine_profile,
            native_cache,
            native_cache_dir,
            script_cache,
            include_cache,
            compile_optimization_level,
            perf_ablation,
            worker_state,
        }
    }

    pub(crate) fn executor_options(&self) -> PhpExecutorOptions {
        let mut options = if self.engine_profile == EngineProfileName::Default {
            PhpExecutorOptions::default_native_runtime()
        } else {
            PhpExecutorOptions::for_profile(self.engine_profile)
        };
        self.apply_engine_overrides(&mut options);
        options
    }

    fn apply_engine_overrides(&self, options: &mut PhpExecutorOptions) {
        options.include_optimization_level = self.compile_optimization_level;
        options.vm_options.native_cache = self.native_cache;
        options.vm_options.native_cache_dir = self.native_cache_dir.clone();
        if self.perf_ablation.disable_inline_caches {
            options.vm_options.inline_caches = InlineCacheMode::Off;
        }
        if self.perf_ablation.disable_include_o2 {
            options.include_optimization_level = OptimizationLevel::O0;
        }
    }

    pub(crate) fn executor_options_with_include_cache(&self) -> PhpExecutorOptions {
        let mut options = self.executor_options();
        options.vm_options.include_cache = Some(Arc::clone(&self.include_cache));
        options
    }

    pub(crate) fn executor_options_for_request(
        &self,
        _script: &str,
        _metrics: &ServerMetrics,
    ) -> PhpExecutorOptions {
        self.executor_options_with_include_cache()
    }

    pub(crate) fn request_executor_cache_key(&self) -> RequestExecutorCacheKey {
        RequestExecutorCacheKey {
            engine_profile: self.engine_profile,
            native_cache: self.native_cache,
            native_cache_dir: self.native_cache_dir.clone(),
            include_cache_id: self.include_cache.instance_id(),
            compile_optimization_level: self.compile_optimization_level,
            perf_ablation: self.perf_ablation.clone(),
        }
    }

    pub(crate) fn persistent_metadata_stats(&self) -> PersistentMetadataStats {
        PersistentMetadataStats::default()
    }

    pub(crate) fn executor(&self, options: PhpExecutorOptions) -> PhpExecutor {
        PhpExecutor::with_options_and_worker_state(options, self.worker_state.clone())
    }

    pub(crate) fn compile_script(
        &self,
        script_path: &Path,
    ) -> Result<CompiledScriptCacheLookup, PhpExecutionError> {
        let executor = self.executor(self.executor_options());
        self.script_cache.get_or_compile_script(
            &executor,
            PhpScriptCacheInput {
                path: script_path.to_path_buf(),
                source_path: script_path.to_string_lossy().into_owned(),
                optimization_level: self.compile_optimization_level,
            },
        )
    }
}

impl AppState {
    pub(crate) fn next_request_id(&self) -> String {
        let id = self
            .services
            .request_counter
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        format!("req-{id:08}")
    }

    pub(crate) fn compile_script(
        &self,
        script_path: &Path,
    ) -> Result<CompiledScriptCacheLookup, PhpExecutionError> {
        let lookup = self.services.engine.compile_script(script_path)?;
        self.services
            .metrics
            .persistent_engine_policy_reuses
            .fetch_add(1, Ordering::Relaxed);
        if lookup.hit {
            self.services
                .metrics
                .persistent_engine_immutable_metadata_reuses
                .fetch_add(1, Ordering::Relaxed);
        } else {
            self.services
                .metrics
                .persistent_engine_misses
                .fetch_add(1, Ordering::Relaxed);
        }
        Ok(lookup)
    }
}
#[derive(Clone, Debug)]
pub(crate) struct SessionConfig {
    pub(crate) enabled: bool,
    pub(crate) cookie_name: String,
    pub(crate) cookie_path: String,
}
pub(crate) fn preload_script_cache(
    state: &AppState,
    preload_file: Option<&Path>,
    strict: bool,
) -> Result<(), ServerError> {
    let started = std::time::Instant::now();
    state
        .services
        .metrics
        .script_cache_ready
        .store(1, Ordering::Release);
    let Some(preload_file) = preload_file else {
        finish_native_prewarm_readiness(state, 0, started.elapsed());
        return Ok(());
    };
    let contents = match std::fs::read_to_string(preload_file) {
        Ok(contents) => contents,
        Err(error) => {
            state
                .services
                .metrics
                .script_cache_preload_failures
                .fetch_add(1, Ordering::Relaxed);
            let error = PreloadError::manifest_read(preload_file, &error);
            if strict {
                return Err(ServerError::Preload(Box::new(error)));
            }
            warn!(%error);
            finish_native_prewarm_readiness(state, 0, started.elapsed());
            return Ok(());
        }
    };
    let mut prewarmed_entries = 0_u64;
    for (line_index, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let raw_path = PathBuf::from(trimmed);
        let script_path = if raw_path.is_absolute() {
            raw_path
        } else {
            state.route_config.docroot.join(raw_path)
        };
        let line = line_index + 1;
        let result = state
            .compile_script(&script_path)
            .map_err(|error| {
                Box::new(PreloadError::compile_entry(
                    preload_file,
                    line,
                    &script_path,
                    error,
                ))
            })
            .and_then(|lookup| {
                let executor = state
                    .services
                    .engine
                    .executor(state.services.engine.executor_options());
                prewarmed_entries =
                    prewarmed_entries.saturating_add(executor.prewarm_compiled(&lookup.compiled));
                preload_include_cache_entry(state, &script_path).map_err(|error| {
                    Box::new(PreloadError::include_entry(
                        preload_file,
                        line,
                        &script_path,
                        error,
                    ))
                })
            });
        match result {
            Ok(()) => {
                state
                    .services
                    .metrics
                    .script_cache_preload_successes
                    .fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => {
                state
                    .services
                    .metrics
                    .script_cache_preload_failures
                    .fetch_add(1, Ordering::Relaxed);
                if strict {
                    return Err(ServerError::Preload(error));
                }
                warn!(%error);
            }
        }
    }
    finish_native_prewarm_readiness(state, prewarmed_entries, started.elapsed());
    Ok(())
}

fn finish_native_prewarm_readiness(state: &AppState, entries: u64, elapsed: Duration) {
    let metrics = &state.services.metrics;
    metrics
        .native_prewarm_entries
        .fetch_add(entries, Ordering::Relaxed);
    metrics.native_prewarm_nanos.fetch_add(
        elapsed.as_nanos().min(u128::from(u64::MAX)) as u64,
        Ordering::Relaxed,
    );
    metrics.native_code_cache_generation.store(
        php_vm::tooling::cranelift_code_cache_generation(),
        Ordering::Release,
    );
    // Compilation is synchronous and serialized by the process code manager;
    // completing this phase therefore also proves that its queue is empty.
    metrics
        .native_compile_queue_empty
        .store(1, Ordering::Release);
    metrics.native_prewarm_complete.store(1, Ordering::Release);
}

fn preload_include_cache_entry(state: &AppState, script_path: &Path) -> Result<(), VmError> {
    let loader = IncludeLoader::for_root(&state.route_config.docroot)?;
    let resolved = state
        .services
        .engine
        .include_cache
        .resolve_with_include_path(
            &loader,
            None,
            &script_path.to_string_lossy(),
            &[],
            Some(&state.route_config.docroot),
        )?;
    state.services.engine.include_cache.get_or_compile_include(
        &loader,
        &resolved,
        &ExecutorIncludeCompiler::new(state.services.engine.compile_optimization_level),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(include_cache: Arc<IncludeCache>) -> ServerEngineState {
        ServerEngineState::new(
            EngineProfileName::Default,
            php_vm::api::NativeCacheMode::Off,
            std::env::temp_dir().join("phrust-server-test-native-cache"),
            Arc::new(CompiledScriptCache::new(1)),
            include_cache,
            ServerPerfAblation::default(),
        )
    }

    #[test]
    fn executor_cache_key_tracks_logical_cache_instance() {
        let shared_cache = Arc::new(IncludeCache::new(1));
        let first = engine(Arc::clone(&shared_cache));
        let same_instance = engine(shared_cache);
        let replacement = engine(Arc::new(IncludeCache::new(1)));

        assert_eq!(
            first.request_executor_cache_key(),
            same_instance.request_executor_cache_key()
        );
        assert_ne!(
            first.request_executor_cache_key(),
            replacement.request_executor_cache_key()
        );
    }
}
