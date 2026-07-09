use crate::config::{BUILTIN_SERVER_REWRITE_PREFIX_QUERY_ENV, ServerPerfAblation};
use crate::{
    access_log::AccessLogger,
    metrics::ServerMetrics,
    perf_trace::PerfTraceWriter,
    persistent_metadata::{PersistentMetadataStats, PersistentMetadataStore},
    request_profile::RequestProfileWriter,
    server::ServerError,
};
use crate::{multipart::MultipartConfig, routing::RouteConfig, session_store::SessionStore};
use php_diagnostics::DiagnosticOutputFormat;
use php_executor::{
    CompiledScriptCache, CompiledScriptCacheLookup, EngineProfileName, IncludeCache, IncludeLoader,
    OptimizationLevel, PhpExecutionError, PhpExecutor, PhpExecutorOptions, PhpScriptCacheInput,
};
use php_vm::api::{
    DenseIncludeMode, DenseJumpThreadingMode, InlineCacheMode, JitMode, QuickeningMode,
    QuickeningSiteSnapshot,
};
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
    pub(crate) max_body_bytes: usize,
    pub(crate) multipart_config: MultipartConfig,
    pub(crate) request_timeout: Duration,
    pub(crate) execution_time_limit: Option<Duration>,
    pub(crate) in_flight: Arc<Semaphore>,
    pub(crate) max_in_flight: usize,
    pub(crate) metrics: Arc<ServerMetrics>,
    pub(crate) engine: Arc<ServerEngineState>,
    pub(crate) metrics_token: Option<String>,
    pub(crate) access_log: Option<Arc<AccessLogger>>,
    pub(crate) perf_trace: Option<Arc<PerfTraceWriter>>,
    pub(crate) perf_trace_vm_counters: bool,
    pub(crate) request_profile: Option<Arc<RequestProfileWriter>>,
    pub(crate) request_profile_vm_counters: bool,
    pub(crate) request_profile_source_attribution: bool,
    pub(crate) request_profile_trigger_header: bool,
    pub(crate) network_requests_enabled: bool,
    pub(crate) env_snapshot: Arc<Vec<(String, String)>>,
    pub(crate) debug: bool,
    pub(crate) error_format: DiagnosticOutputFormat,
    pub(crate) debug_log: Option<PathBuf>,
    pub(crate) request_counter: Arc<AtomicU64>,
    pub(crate) session_config: SessionConfig,
    pub(crate) session_store: Arc<SessionStore>,
    pub(crate) local_addr: SocketAddr,
    pub(crate) request_scheme: &'static str,
    pub(crate) http3_alt_svc: Option<String>,
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
    pub(crate) max_vm_steps: usize,
    pub(crate) script_cache: Arc<CompiledScriptCache>,
    pub(crate) include_cache: Arc<IncludeCache>,
    pub(crate) compile_optimization_level: OptimizationLevel,
    persistent_metadata: Arc<PersistentMetadataStore>,
    dense_includes: Option<DenseIncludeMode>,
    perf_ablation: ServerPerfAblation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RequestExecutorCacheKey {
    engine_profile: EngineProfileName,
    max_vm_steps: usize,
    include_cache_addr: usize,
    compile_optimization_level: OptimizationLevel,
    dense_includes: Option<DenseIncludeMode>,
    perf_ablation: ServerPerfAblation,
}

impl ServerEngineState {
    pub(crate) fn new(
        engine_profile: EngineProfileName,
        max_vm_steps: usize,
        script_cache: Arc<CompiledScriptCache>,
        include_cache: Arc<IncludeCache>,
        dense_includes: Option<DenseIncludeMode>,
        perf_ablation: ServerPerfAblation,
    ) -> Self {
        let base_options = if engine_profile == EngineProfileName::Default {
            PhpExecutorOptions::managed_fast_runtime()
        } else {
            PhpExecutorOptions::for_profile(engine_profile)
        };
        let compile_optimization_level = if perf_ablation.disable_include_o2 {
            OptimizationLevel::O0
        } else {
            base_options.optimization_level
        };
        Self {
            engine_profile,
            max_vm_steps,
            script_cache,
            include_cache,
            compile_optimization_level,
            persistent_metadata: Arc::new(PersistentMetadataStore::default()),
            dense_includes,
            perf_ablation,
        }
    }

    pub(crate) fn executor_options(&self) -> PhpExecutorOptions {
        let mut options = if self.engine_profile == EngineProfileName::Default {
            let mut options = PhpExecutorOptions::managed_fast_runtime();
            options.vm_options.max_steps = self.max_vm_steps;
            options
        } else {
            let mut options = PhpExecutorOptions::for_profile(self.engine_profile);
            options.vm_options.max_steps = self.max_vm_steps;
            options
        };
        self.apply_engine_overrides(&mut options);
        options
    }

    fn apply_engine_overrides(&self, options: &mut PhpExecutorOptions) {
        let quickening_seed = self.persistent_metadata.quickening_templates();
        if !quickening_seed.is_empty() {
            options.vm_options.quickening_seed = quickening_seed;
        }
        if let Some(mode) = self.dense_includes {
            options.vm_options.dense_include_execution = mode;
        }
        options.vm_options.include_optimization_level = self.compile_optimization_level;
        if self.perf_ablation.disable_dense_includes {
            options.vm_options.dense_include_execution = DenseIncludeMode::Off;
        }
        if self.perf_ablation.disable_quickening {
            options.vm_options.quickening = QuickeningMode::Off;
        }
        if self.perf_ablation.disable_inline_caches {
            options.vm_options.inline_caches = InlineCacheMode::Off;
        }
        if self.perf_ablation.disable_builtin_ic {
            options.vm_options.internal_function_dispatch_cache = false;
        }
        if self.perf_ablation.disable_jit {
            options.vm_options.jit = JitMode::Off;
            options.vm_options.tiering.enabled = false;
        }
        if self.perf_ablation.disable_include_o2 {
            options.vm_options.include_optimization_level = OptimizationLevel::O0;
        }
        if self.perf_ablation.disable_dense_jump_threading {
            options.vm_options.dense_jump_threading = DenseJumpThreadingMode::Off;
        }
    }

    pub(crate) fn executor_options_with_include_cache(&self) -> PhpExecutorOptions {
        let mut options = self.executor_options();
        options.vm_options.include_cache = Some(Arc::clone(&self.include_cache));
        options
    }

    pub(crate) fn executor_options_for_request(
        &self,
        metrics: &ServerMetrics,
    ) -> PhpExecutorOptions {
        let options = self.executor_options_with_include_cache();
        let instantiated = options.vm_options.quickening_seed.len() as u64;
        if instantiated > 0 {
            metrics
                .persistent_engine_feedback_template_instantiations
                .fetch_add(instantiated, Ordering::Relaxed);
        }
        options
    }

    pub(crate) fn request_executor_cache_key(&self) -> RequestExecutorCacheKey {
        RequestExecutorCacheKey {
            engine_profile: self.engine_profile,
            max_vm_steps: self.max_vm_steps,
            include_cache_addr: Arc::as_ptr(&self.include_cache) as usize,
            compile_optimization_level: self.compile_optimization_level,
            dense_includes: self.dense_includes,
            perf_ablation: self.perf_ablation.clone(),
        }
    }

    pub(crate) fn absorb_quickening_feedback(
        &self,
        feedback: Vec<QuickeningSiteSnapshot>,
    ) -> usize {
        self.persistent_metadata
            .absorb_quickening_feedback(feedback)
    }

    pub(crate) fn persistent_metadata_stats(&self) -> PersistentMetadataStats {
        self.persistent_metadata.stats()
    }

    pub(crate) fn compile_script(
        &self,
        script_path: &Path,
    ) -> Result<CompiledScriptCacheLookup, PhpExecutionError> {
        let executor = PhpExecutor::with_options(self.executor_options());
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
        let id = self.request_counter.fetch_add(1, Ordering::Relaxed) + 1;
        format!("req-{id:08}")
    }

    pub(crate) fn compile_script(
        &self,
        script_path: &Path,
    ) -> Result<CompiledScriptCacheLookup, PhpExecutionError> {
        let lookup = self.engine.compile_script(script_path)?;
        self.metrics
            .persistent_engine_policy_reuses
            .fetch_add(1, Ordering::Relaxed);
        if lookup.hit {
            self.metrics
                .persistent_engine_immutable_metadata_reuses
                .fetch_add(1, Ordering::Relaxed);
        } else {
            self.metrics
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
    let Some(preload_file) = preload_file else {
        return Ok(());
    };
    let contents = match std::fs::read_to_string(preload_file) {
        Ok(contents) => contents,
        Err(error) => {
            state
                .metrics
                .script_cache_preload_failures
                .fetch_add(1, Ordering::Relaxed);
            let message = format!(
                "script cache preload file `{}` cannot be read: {error}",
                preload_file.display()
            );
            if strict {
                return Err(ServerError::Preload(message));
            }
            warn!("{message}");
            return Ok(());
        }
    };
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
        match state
            .compile_script(&script_path)
            .and_then(|_| preload_include_cache_entry(state, &script_path))
        {
            Ok(()) => {
                state
                    .metrics
                    .script_cache_preload_successes
                    .fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => {
                state
                    .metrics
                    .script_cache_preload_failures
                    .fetch_add(1, Ordering::Relaxed);
                let message = format!(
                    "script cache preload entry {} in `{}` failed for `{}`: {error:?}",
                    line_index + 1,
                    preload_file.display(),
                    script_path.display()
                );
                if strict {
                    return Err(ServerError::Preload(message));
                }
                warn!("{message}");
            }
        }
    }
    Ok(())
}

fn preload_include_cache_entry(
    state: &AppState,
    script_path: &Path,
) -> Result<(), PhpExecutionError> {
    let loader = IncludeLoader::for_root(&state.route_config.docroot)
        .map_err(|error| PhpExecutionError::Engine(format!("{error:?}")))?;
    let resolved = state
        .engine
        .include_cache
        .resolve_with_include_path(
            &loader,
            None,
            &script_path.to_string_lossy(),
            &[],
            Some(&state.route_config.docroot),
        )
        .map_err(|error| PhpExecutionError::Engine(format!("{error:?}")))?;
    state
        .engine
        .include_cache
        .get_or_compile_include(&loader, &resolved, state.engine.compile_optimization_level)
        .map_err(|error| PhpExecutionError::Engine(format!("{error:?}")))?;
    Ok(())
}
