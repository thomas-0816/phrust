use crate::{access_log::AccessLogger, metrics::ServerMetrics, server::ServerError};
use crate::{multipart::MultipartConfig, routing::RouteConfig, session_store::SessionStore};
use php_diagnostics::DiagnosticOutputFormat;
use php_executor::{
    CompiledScriptCache, CompiledScriptCacheLookup, EngineProfileName, IncludeCache,
    OptimizationLevel, PhpExecutionError, PhpExecutor, PhpExecutorOptions, PhpScriptCacheInput,
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
    pub(crate) debug: bool,
    pub(crate) error_format: DiagnosticOutputFormat,
    pub(crate) debug_log: Option<PathBuf>,
    pub(crate) request_counter: Arc<AtomicU64>,
    pub(crate) session_config: SessionConfig,
    pub(crate) session_store: Arc<SessionStore>,
    pub(crate) local_addr: SocketAddr,
    pub(crate) request_scheme: &'static str,
}
#[derive(Clone, Debug)]
pub(crate) struct ServerEngineState {
    pub(crate) engine_profile: EngineProfileName,
    pub(crate) script_cache: Arc<CompiledScriptCache>,
    pub(crate) include_cache: Arc<IncludeCache>,
    pub(crate) compile_optimization_level: OptimizationLevel,
}

impl ServerEngineState {
    pub(crate) fn new(
        engine_profile: EngineProfileName,
        script_cache: Arc<CompiledScriptCache>,
        include_cache: Arc<IncludeCache>,
    ) -> Self {
        let base_options = if engine_profile == EngineProfileName::Default {
            PhpExecutorOptions::managed_fast_runtime()
        } else {
            PhpExecutorOptions::for_profile(engine_profile)
        };
        Self {
            engine_profile,
            script_cache,
            include_cache,
            compile_optimization_level: base_options.optimization_level,
        }
    }

    pub(crate) fn executor_options(&self) -> PhpExecutorOptions {
        if self.engine_profile == EngineProfileName::Default {
            PhpExecutorOptions::managed_fast_runtime()
        } else {
            PhpExecutorOptions::for_profile(self.engine_profile)
        }
    }

    pub(crate) fn executor_options_with_include_cache(&self) -> PhpExecutorOptions {
        let mut options = self.executor_options();
        options.vm_options.include_cache = Some(Arc::clone(&self.include_cache));
        options
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
        match state.compile_script(&script_path) {
            Ok(_) => {
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
