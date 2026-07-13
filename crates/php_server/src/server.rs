use crate::{
    access_log::AccessLogger,
    config::{ConfigError, ServerConfig},
    http3::build_http3_endpoint,
    metrics::ServerMetrics,
    multipart::MultipartConfig,
    routing::RouteConfig,
    serve::serve_until_shutdown,
    session_store::SessionStore,
    state::{
        AppState, CapabilityState, ConcurrencyServices, ObservabilityState, RequestRuntimeConfig,
        RequestTransport, RuntimeServices, ServerEngineState, SessionConfig, SessionServices,
        preload_script_cache, server_env_snapshot,
    },
    tls::build_tls_acceptor,
};
use php_diagnostics::{
    DiagnosticCause, DiagnosticEnvelope, DiagnosticLayer, DiagnosticPhase, DiagnosticSeverity,
    DiagnosticSuggestion,
};
use php_executor::{
    CompiledScriptCache, DeploymentRootFingerprint, IncludeCache, PhpExecutionError,
    SERVER_INCLUDE_REVALIDATION_INTERVAL, include_revalidation_interval_from_env,
};
use php_vm::api::VmError;
use std::{
    collections::BTreeMap,
    fmt,
    path::Path,
    sync::{Arc, atomic::AtomicU64},
    time::Duration,
};
use tokio::{net::TcpListener, sync::Semaphore};
use tracing::debug;

#[derive(Debug)]
pub enum ServerError {
    Config(Box<ConfigError>),
    Io(std::io::Error),
    Preload(Box<PreloadError>),
    Tls(String),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => write!(f, "{error}"),
            Self::Io(error) => write!(f, "{error}"),
            Self::Preload(error) => write!(f, "{error}"),
            Self::Tls(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ServerError {}

impl ServerError {
    #[must_use]
    pub fn diagnostic(&self) -> DiagnosticEnvelope {
        match self {
            Self::Config(error) => error.diagnostic().clone(),
            Self::Io(error) => {
                let cwd = std::env::current_dir()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|cwd_error| format!("<unavailable: {cwd_error}>"));
                let mut diagnostic = DiagnosticEnvelope::new(
                    "E_PHRUST_SERVER_IO",
                    DiagnosticLayer::server(),
                    DiagnosticPhase::new("startup"),
                    DiagnosticSeverity::Error,
                    format!("server startup I/O failed: {error}"),
                )
                .with_context(BTreeMap::from([
                    ("operation".to_string(), "server startup".to_string()),
                    ("cwd".to_string(), cwd),
                ]));
                diagnostic.cause = Some(DiagnosticCause::new(
                    error.to_string(),
                    Some("std::io::Error"),
                ));
                diagnostic.suggestion = Some(DiagnosticSuggestion::new(
                    "check listen address availability and filesystem permissions",
                ));
                diagnostic
            }
            Self::Preload(error) => error.diagnostic().clone(),
            Self::Tls(message) => {
                let mut diagnostic = DiagnosticEnvelope::new(
                    "E_PHRUST_SERVER_TLS",
                    DiagnosticLayer::server(),
                    DiagnosticPhase::new("tls"),
                    DiagnosticSeverity::Error,
                    message.clone(),
                );
                diagnostic.suggestion = Some(DiagnosticSuggestion::new(
                    "provide matching --tls-cert and --tls-key PEM files",
                ));
                diagnostic
            }
        }
    }
}

#[derive(Debug)]
pub struct PreloadError {
    message: String,
    diagnostic: DiagnosticEnvelope,
}

impl PreloadError {
    pub(crate) fn manifest_read(path: &Path, error: &std::io::Error) -> Self {
        let message = format!(
            "script cache preload file `{}` cannot be read: {error}",
            path.display()
        );
        let diagnostic = DiagnosticEnvelope::new(
            "E_PHRUST_SERVER_PRELOAD_READ",
            DiagnosticLayer::server(),
            DiagnosticPhase::new("preload_manifest"),
            DiagnosticSeverity::Error,
            message.clone(),
        )
        .with_context(BTreeMap::from([
            ("preload_file".to_string(), path.display().to_string()),
            ("stage".to_string(), "manifest_read".to_string()),
        ]));
        Self {
            message,
            diagnostic,
        }
    }

    pub(crate) fn compile_entry(
        preload_file: &Path,
        line: usize,
        script_path: &Path,
        error: PhpExecutionError,
    ) -> Self {
        let message = format!(
            "script cache preload entry {line} in `{}` failed for `{}`",
            preload_file.display(),
            script_path.display()
        );
        let mut diagnostic = match error {
            PhpExecutionError::Compile(output) => {
                output.diagnostics.first().cloned().unwrap_or_else(|| {
                    DiagnosticEnvelope::new(
                        "E_PHRUST_SERVER_PRELOAD_COMPILE",
                        DiagnosticLayer::server(),
                        DiagnosticPhase::new("preload_compile"),
                        DiagnosticSeverity::Error,
                        output.diagnostics_text,
                    )
                })
            }
            PhpExecutionError::Engine(error) => DiagnosticEnvelope::new(
                "E_PHRUST_SERVER_PRELOAD_ENGINE",
                DiagnosticLayer::server(),
                DiagnosticPhase::new("preload_compile"),
                DiagnosticSeverity::Error,
                error,
            ),
        };
        diagnostic
            .context
            .extend(preload_context(preload_file, line, script_path, "compile"));
        diagnostic.suggestion = Some(DiagnosticSuggestion::new(
            "fix the preload entry or run without --strict-preload",
        ));
        Self {
            message,
            diagnostic,
        }
    }

    pub(crate) fn include_entry(
        preload_file: &Path,
        line: usize,
        script_path: &Path,
        error: VmError,
    ) -> Self {
        let message = format!(
            "script cache preload entry {line} in `{}` failed for `{}`",
            preload_file.display(),
            script_path.display()
        );
        let mut diagnostic = error.to_diagnostic_envelope();
        diagnostic.context.extend(preload_context(
            preload_file,
            line,
            script_path,
            "include_compile",
        ));
        diagnostic.suggestion = Some(DiagnosticSuggestion::new(
            "fix the preload entry or run without --strict-preload",
        ));
        Self {
            message,
            diagnostic,
        }
    }

    pub(crate) fn diagnostic(&self) -> &DiagnosticEnvelope {
        &self.diagnostic
    }
}

impl fmt::Display for PreloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

fn preload_context(
    preload_file: &Path,
    line: usize,
    script_path: &Path,
    stage: &str,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "preload_file".to_string(),
            preload_file.display().to_string(),
        ),
        ("preload_line".to_string(), line.to_string()),
        ("script_path".to_string(), script_path.display().to_string()),
        ("stage".to_string(), stage.to_string()),
    ])
}

impl From<ConfigError> for ServerError {
    fn from(error: ConfigError) -> Self {
        Self::Config(Box::new(error))
    }
}

impl From<std::io::Error> for ServerError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub async fn run(config: ServerConfig) -> Result<(), ServerError> {
    let docroot = config.validated_docroot()?;
    let listener = TcpListener::bind(config.transport.listen).await?;
    let local_addr = listener.local_addr()?;
    let script_cache_preload = config.engine.script_cache_preload.clone();
    let strict_preload = config.engine.strict_preload;
    let startup_front_controller = config.routing.front_controller.clone();
    let startup_upload_temp_dir = config.sessions_uploads.upload_temp_dir.clone();
    let startup_session_save_path = config.sessions_uploads.session_save_path.clone();
    let startup_script_cache_enabled = config.engine.script_cache_enabled;
    let startup_script_cache_shards = config.engine.script_cache_shards;
    let startup_script_cache_max_entries = config.engine.script_cache_max_entries;
    let startup_metrics_endpoint_enabled = config.routing.metrics_endpoint_enabled;
    let startup_metrics_token_enabled = config.observability.metrics_token.is_some();
    let startup_access_log = config.observability.access_log.clone();
    let startup_perf_trace = config.observability.perf_trace.clone();
    let startup_request_profile = config.observability.request_profile.clone();
    let startup_tls_enabled = config.transport.tls_cert.is_some();
    let startup_http3_enabled = config.transport.http3_enabled;
    let http3_listen = config.transport.http3_listen.unwrap_or(local_addr);
    let engine_profile = config.engine.engine_preset;
    let tls_acceptor = build_tls_acceptor(
        config.transport.tls_cert.as_deref(),
        config.transport.tls_key.as_deref(),
    )?;
    let http3_endpoint = if config.transport.http3_enabled {
        let cert_path = config.transport.tls_cert.as_deref().ok_or_else(|| {
            ConfigError::new(
                "HTTP/3 requires TLS; provide --tls-cert <path> and --tls-key <path> with --enable-http3",
            )
        })?;
        let key_path = config.transport.tls_key.as_deref().ok_or_else(|| {
            ConfigError::new(
                "HTTP/3 requires TLS; provide --tls-cert <path> and --tls-key <path> with --enable-http3",
            )
        })?;
        Some(build_http3_endpoint(cert_path, key_path, http3_listen)?)
    } else {
        None
    };
    let http3_local_addr = http3_endpoint
        .as_ref()
        .map(|endpoint| endpoint.local_addr())
        .transpose()?;
    let http3_alt_svc = http3_local_addr.map(|addr| format!("h3=\":{}\"; ma=86400", addr.port()));
    let access_log = config
        .observability
        .access_log
        .as_deref()
        .map(AccessLogger::open)
        .transpose()?
        .map(Arc::new);
    let perf_trace = config
        .observability
        .perf_trace
        .map(crate::perf_trace::PerfTraceWriter::open)
        .transpose()?
        .map(Arc::new);
    let request_profile = config
        .observability
        .request_profile
        .map(crate::request_profile::RequestProfileWriter::open)
        .transpose()?
        .map(Arc::new);
    let session_store = Arc::new(SessionStore::new(config.sessions_uploads.session_save_path));
    if config.sessions_uploads.sessions_enabled {
        session_store
            .ensure_ready()
            .map_err(std::io::Error::other)?;
    }
    let startup_scheme = if startup_tls_enabled { "https" } else { "http" };
    println!("listening {startup_scheme}://{local_addr}");
    eprintln!(
        "startup docroot={} front_controller={} engine_preset={} script_cache={} script_cache_shards={} script_cache_max_entries={} upload_temp_dir={} session_save_path={} metrics_endpoint={} metrics_token={} access_log={} perf_trace={} request_profile={} tls={} tls_alpn={} http3={} http3_addr={}",
        docroot.display(),
        startup_front_controller
            .as_ref()
            .map_or("-", |path| path.to_str().unwrap_or("<non-utf8>")),
        engine_profile,
        startup_script_cache_enabled,
        startup_script_cache_shards,
        startup_script_cache_max_entries,
        startup_upload_temp_dir.display(),
        startup_session_save_path.display(),
        startup_metrics_endpoint_enabled,
        startup_metrics_token_enabled,
        startup_access_log.as_deref().unwrap_or("-"),
        startup_perf_trace
            .as_ref()
            .map_or("-", |path| path.to_str().unwrap_or("<non-utf8>")),
        startup_request_profile
            .as_ref()
            .map_or("-", |path| path.to_str().unwrap_or("<non-utf8>")),
        startup_tls_enabled,
        if startup_tls_enabled {
            "h2,http/1.1"
        } else {
            "-"
        },
        startup_http3_enabled,
        http3_local_addr
            .map(|addr| addr.to_string())
            .unwrap_or_else(|| "-".to_string()),
    );
    debug!(%local_addr, docroot=%docroot.display(), "starting phrust server");
    let script_cache = Arc::new(if config.engine.script_cache_enabled {
        CompiledScriptCache::new_with_limits(
            config.engine.script_cache_shards,
            config.engine.script_cache_max_entries,
            Duration::from_millis(config.engine.script_cache_check_interval_ms),
        )
    } else {
        CompiledScriptCache::disabled()
    });
    // Server default mirrors an opcache deployment (validate_timestamps=1,
    // revalidate_freq=2): cached includes serve without filesystem probes for
    // two seconds. PHRUST_INCLUDE_REVALIDATE_MS overrides; 0 validates every
    // hit like the reference CLI.
    let include_cache = Arc::new(IncludeCache::new_with_revalidation_interval(
        config.engine.script_cache_shards,
        include_revalidation_interval_from_env().unwrap_or(SERVER_INCLUDE_REVALIDATION_INTERVAL),
    ));
    // Deployment-root fingerprint: metadata + counters only. A root that
    // cannot be observed counts as `deployment_fingerprint_missing` and keeps
    // every fingerprint-gated persistent reuse blocked.
    include_cache.set_deployment_root_fingerprint(DeploymentRootFingerprint::observe(
        &docroot,
        config.engine.deployment_mode,
    ));
    let engine = Arc::new(ServerEngineState::new(
        engine_profile,
        script_cache,
        include_cache,
        config.engine.perf_ablation,
    ));
    let state = Arc::new(AppState {
        route_config: RouteConfig {
            docroot,
            index: config.routing.index,
            front_controller: config.routing.front_controller,
            builtin_router: config.routing.builtin_router,
            request_rewrites: config.routing.request_rewrites,
            metrics_endpoint_enabled: config.routing.metrics_endpoint_enabled,
            cache_clear_endpoint_enabled: config.routing.cache_clear_endpoint_enabled,
        },
        request: RequestRuntimeConfig {
            max_body_bytes: config.limits.max_body_bytes,
            multipart_config: MultipartConfig {
                upload_temp_dir: config.sessions_uploads.upload_temp_dir,
                max_upload_files: config.sessions_uploads.max_upload_files,
                max_upload_file_bytes: config.sessions_uploads.max_upload_file_bytes,
            },
            request_timeout: Duration::from_millis(config.limits.request_timeout_ms),
            execution_time_limit: config
                .limits
                .execution_deadline_enabled
                .then(|| Duration::from_millis(config.limits.max_execution_ms)),
        },
        concurrency: ConcurrencyServices {
            in_flight: Arc::new(Semaphore::new(config.limits.max_in_flight)),
            max_in_flight: config.limits.max_in_flight,
            cpu_execution: Arc::new(Semaphore::new(config.limits.cpu_execution_limit)),
            cpu_execution_limit: config.limits.cpu_execution_limit,
            php_workers: Arc::new(crate::worker_pool::PhpWorkerPool::new(
                config.limits.cpu_execution_limit,
            )),
        },
        observability: ObservabilityState {
            metrics_token: config.observability.metrics_token,
            access_log,
            perf_trace,
            perf_trace_vm_counters: config.observability.perf_trace_vm_counters,
            request_profile,
            request_profile_vm_counters: config.observability.request_profile_vm_counters,
            request_profile_source_attribution: config
                .observability
                .request_profile_source_attribution,
            request_profile_trigger_header: config.observability.request_profile_trigger_header,
            debug: config.observability.debug,
            error_format: config.observability.error_format,
            debug_log: config.observability.debug_log,
        },
        capabilities: CapabilityState {
            network_requests_enabled: config.capabilities.network_requests_enabled,
            env_snapshot: server_env_snapshot(std::env::vars()),
        },
        sessions: SessionServices {
            config: SessionConfig {
                enabled: config.sessions_uploads.sessions_enabled,
                cookie_name: config.sessions_uploads.session_cookie_name,
                cookie_path: config.sessions_uploads.session_cookie_path,
            },
            session_store,
        },
        transport: RequestTransport {
            local_addr,
            request_scheme: if startup_tls_enabled { "https" } else { "http" },
            http3_alt_svc,
        },
        services: RuntimeServices {
            metrics: Arc::new(ServerMetrics::default()),
            engine,
            request_counter: Arc::new(AtomicU64::new(0)),
        },
    });
    preload_script_cache(&state, script_cache_preload.as_deref(), strict_preload)?;
    serve_until_shutdown(listener, state, tls_acceptor, http3_endpoint).await;
    Ok(())
}

pub fn run_blocking(config: ServerConfig) -> Result<(), ServerError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(ServerError::Io)?;
    runtime.block_on(run(config))
}
